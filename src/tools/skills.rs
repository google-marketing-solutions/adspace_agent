// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Toolset for managing agent skills and their lifecycle.
//!
//! Skills are Markdown documents (`SKILL.md` with YAML front matter) loaded
//! from a local directory (`LOCAL_SKILLS_DIR`, default `./skills`) and/or a
//! Cloud Storage bucket (`SKILLS_BUCKET_NAME`, under the `skills/` prefix).
//! They are exposed to the model through adk-skill's progressive-disclosure
//! tools (`list_skills`, `load_skill`, `load_skill_resource`) plus a
//! `reload_skills` tool that re-reads every source at runtime.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use adk_rust::skill::{
    ResourceAccessPolicy, SkillIndex, SkillToolset, SkillToolsetConfig, load_skill_index,
    parse_skill_markdown,
};
use adk_rust::{
    LlmRequest, ReadonlyContext, Result, Tool, ToolContext, ToolRegistry, Toolset, ValidationMode,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::RwLock;

use crate::gcs::GcsClient;

/// Prefix inside the bucket under which skills live.
pub const GCS_SKILLS_PREFIX: &str = "skills/";

/// Where skills come from.
#[derive(Clone)]
pub struct SkillSources {
    /// Local directory containing one sub-directory per skill.
    pub local_dir: Option<PathBuf>,
    /// GCS bucket (and client) containing `skills/<name>/SKILL.md`.
    pub gcs: Option<(GcsClient, String)>,
    /// Staging directory where skills are materialised for adk-skill.
    pub staging_dir: PathBuf,
}

impl std::fmt::Debug for SkillSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkillSources")
            .field("local_dir", &self.local_dir)
            .field("gcs_bucket", &self.gcs.as_ref().map(|(_, b)| b))
            .field("staging_dir", &self.staging_dir)
            .finish()
    }
}

impl SkillSources {
    /// Builds sources from `LOCAL_SKILLS_DIR` / `SKILLS_BUCKET_NAME`.
    pub fn from_env(gcs: Option<GcsClient>) -> Self {
        let local_dir = std::env::var("LOCAL_SKILLS_DIR")
            .ok()
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| Some(PathBuf::from("skills")));
        let bucket = std::env::var("SKILLS_BUCKET_NAME")
            .ok()
            .filter(|b| !b.is_empty());
        let staging_dir = std::env::temp_dir()
            .join("adspace-agent")
            .join("skills-staging");
        Self {
            local_dir,
            gcs: gcs.zip(bucket),
            staging_dir,
        }
    }
}

/// Result of (re)loading skills.
#[derive(Debug, Default, Clone)]
pub struct SkillLoadReport {
    /// Names of the loaded skills, in index order.
    pub names: Vec<String>,
    /// Human-readable load errors.
    pub errors: Vec<String>,
}

/// A registry with no business tools (skills here only carry instructions).
struct EmptyToolRegistry;

impl ToolRegistry for EmptyToolRegistry {
    fn resolve(&self, _tool_name: &str) -> Option<Arc<dyn Tool>> {
        None
    }
}

/// Shared mutable skill state.
struct SkillsState {
    sources: SkillSources,
    inner: RwLock<Arc<SkillToolset>>,
    index: RwLock<Arc<SkillIndex>>,
    last_report: RwLock<SkillLoadReport>,
}

/// Copies a local skill directory (recursively) into the staging area.
async fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(dst).await?;
    let mut entries = tokio::fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            Box::pin(copy_dir_recursive(&path, &target)).await?;
        } else {
            tokio::fs::copy(&path, &target).await?;
        }
    }
    Ok(())
}

/// Validates a staged `SKILL.md`, returning its declared name.
fn validate_skill_dir(dir: &Path) -> anyhow::Result<String> {
    let skill_md = dir.join("SKILL.md");
    let content =
        std::fs::read_to_string(&skill_md).map_err(|e| anyhow::anyhow!("missing SKILL.md: {e}"))?;
    let parsed = parse_skill_markdown(&skill_md, &content).map_err(|e| anyhow::anyhow!("{e}"))?;
    let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if parsed.name != folder {
        tracing::warn!(
            "Skill folder '{folder}' does not match frontmatter name '{}'",
            parsed.name
        );
    }
    Ok(parsed.name)
}

impl SkillsState {
    /// Rebuilds the staging directory from every source and reloads the index.
    async fn reload(&self) -> SkillLoadReport {
        let mut errors = Vec::new();
        let staging = self.sources.staging_dir.join(".skills");
        if let Err(e) = tokio::fs::remove_dir_all(&staging).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            errors.push(format!(
                "Failed to clear skills staging dir '{}': {e}",
                staging.display()
            ));
        }
        if let Err(e) = tokio::fs::create_dir_all(&staging).await {
            errors.push(format!(
                "Failed to create skills staging dir '{}': {e}",
                staging.display()
            ));
        }

        // Track which skill ids are staged; local skills override GCS ones
        // (Python loaded GCS first, then local, keyed by name).
        let mut staged: BTreeMap<String, String> = BTreeMap::new();

        if let Some((client, bucket)) = &self.sources.gcs {
            match client
                .list_objects(bucket, GCS_SKILLS_PREFIX, Some("/"))
                .await
            {
                Ok((_, prefixes)) => {
                    for prefix in prefixes {
                        let skill_id = prefix
                            .trim_start_matches(GCS_SKILLS_PREFIX)
                            .trim_end_matches('/')
                            .to_string();
                        if skill_id.is_empty() {
                            continue;
                        }
                        match self
                            .stage_gcs_skill(client, bucket, &skill_id, &staging)
                            .await
                        {
                            Ok(name) => {
                                staged.insert(skill_id, name);
                            }
                            Err(e) => {
                                errors.push(format!("GCS skill '{skill_id}' failed to load: {e:#}"))
                            }
                        }
                    }
                }
                Err(e) => errors.push(format!(
                    "Failed to list GCS skills in bucket '{bucket}': {e:#}"
                )),
            }
        }

        if let Some(local_dir) = &self.sources.local_dir
            && local_dir.is_dir()
        {
            match std::fs::read_dir(local_dir) {
                Ok(entries) => {
                    let mut dirs: Vec<PathBuf> = entries
                        .filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| p.is_dir())
                        .collect();
                    dirs.sort();
                    for dir in dirs {
                        let skill_id = dir
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or_default()
                            .to_string();
                        if !dir.join("SKILL.md").is_file() {
                            continue;
                        }
                        let target = staging.join(&skill_id);
                        if target.exists() {
                            let _ = tokio::fs::remove_dir_all(&target).await;
                        }
                        let result = match copy_dir_recursive(&dir, &target).await {
                            Ok(()) => validate_skill_dir(&target),
                            Err(e) => Err(e),
                        };
                        match result {
                            Ok(name) => {
                                staged.insert(skill_id, name);
                            }
                            Err(e) => {
                                errors.push(format!(
                                    "Local skill '{skill_id}' failed to load: {e:#}"
                                ));
                                let _ = tokio::fs::remove_dir_all(&target).await;
                            }
                        }
                    }
                }
                Err(e) => errors.push(format!(
                    "Failed to list local skills in '{}': {e}",
                    local_dir.display()
                )),
            }
        }

        let index = match load_skill_index(&self.sources.staging_dir) {
            Ok(index) => index,
            Err(e) => {
                errors.push(format!("Failed to index skills: {e}"));
                SkillIndex::new(Vec::new())
            }
        };
        let names: Vec<String> = index.skills().iter().map(|s| s.name.clone()).collect();
        let index = Arc::new(index);
        let toolset = Arc::new(SkillToolset::new(
            index.clone(),
            Arc::new(EmptyToolRegistry),
            Self::config(),
        ));
        *self.index.write().await = index;
        *self.inner.write().await = toolset;
        let report = SkillLoadReport { names, errors };
        *self.last_report.write().await = report.clone();
        report
    }

    fn config() -> SkillToolsetConfig {
        SkillToolsetConfig {
            resource_access: ResourceAccessPolicy::ActivatedOnly,
            validation_mode: ValidationMode::Permissive,
            max_active_skills: 8,
            max_instruction_chars: 32_000,
            max_resource_chars: 32_000,
        }
    }

    async fn stage_gcs_skill(
        &self,
        client: &GcsClient,
        bucket: &str,
        skill_id: &str,
        staging: &Path,
    ) -> anyhow::Result<String> {
        let prefix = format!("{GCS_SKILLS_PREFIX}{skill_id}/");
        let (objects, _) = client.list_objects(bucket, &prefix, None).await?;
        if !objects.iter().any(|o| o == &format!("{prefix}SKILL.md")) {
            anyhow::bail!("gs://{bucket}/{prefix}SKILL.md not found");
        }
        let target = staging.join(skill_id);
        tokio::fs::create_dir_all(&target).await?;
        for object in objects {
            let relative = object.trim_start_matches(&prefix);
            if relative.is_empty() || relative.ends_with('/') {
                continue;
            }
            let path = target.join(relative);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let bytes = client.download_bytes(bucket, &object).await?;
            tokio::fs::write(&path, bytes).await?;
        }
        validate_skill_dir(&target)
    }
}

/// Reloads skills from every configured source.
struct ReloadSkillsTool {
    state: Arc<SkillsState>,
}

#[async_trait]
impl Tool for ReloadSkillsTool {
    fn name(&self) -> &str {
        "reload_skills"
    }

    fn description(&self) -> &str {
        "Reloads all skills from the configured local or GCS directories. Use this tool when you have \
         updated, added, or modified any skills on the disk, to ensure you are using the latest definitions."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    async fn execute(&self, _ctx: Arc<dyn ToolContext>, _args: Value) -> Result<Value> {
        let report = self.state.reload().await;
        let names = if report.names.is_empty() {
            "none".to_string()
        } else {
            report.names.join(", ")
        };
        let mut msg = format!("Successfully reloaded skills. Available skills: {names}.");
        if !report.errors.is_empty() {
            let details = report
                .errors
                .iter()
                .map(|e| format!("- {e}"))
                .collect::<Vec<_>>()
                .join("\n");
            msg.push_str(&format!(
                "\n\nWARNING: The following errors occurred while loading skills:\n{details}"
            ));
        }
        Ok(Value::String(msg))
    }
}

/// Toolset exposing skills plus the `reload_skills` tool.
pub struct SkillsToolset {
    state: Arc<SkillsState>,
}

impl SkillsToolset {
    /// Loads skills from `sources` and builds the toolset.
    pub async fn load(sources: SkillSources) -> Self {
        let empty_index = Arc::new(SkillIndex::new(Vec::new()));
        let state = Arc::new(SkillsState {
            inner: RwLock::new(Arc::new(SkillToolset::new(
                empty_index.clone(),
                Arc::new(EmptyToolRegistry),
                SkillsState::config(),
            ))),
            index: RwLock::new(empty_index),
            last_report: RwLock::new(SkillLoadReport::default()),
            sources,
        });
        let report = state.reload().await;
        for err in &report.errors {
            tracing::warn!("WARNING: {err}");
        }
        tracing::info!(
            "Loaded skills: {}",
            if report.names.is_empty() {
                "none".to_string()
            } else {
                report.names.join(", ")
            }
        );
        Self { state }
    }

    /// Names of the currently loaded skills.
    pub async fn skill_names(&self) -> Vec<String> {
        self.state.last_report.read().await.names.clone()
    }

    /// Errors from the most recent load.
    pub async fn load_errors(&self) -> Vec<String> {
        self.state.last_report.read().await.errors.clone()
    }

    /// Reloads skills from every source.
    pub async fn reload(&self) -> SkillLoadReport {
        self.state.reload().await
    }

    async fn available_skills_block(&self) -> Option<String> {
        let index = self.state.index.read().await;
        if index.is_empty() {
            return None;
        }
        let mut block = String::from("<available_skills>\n");
        for skill in index.skills() {
            block.push_str(&format!(
                "- `{}`: {}\n",
                skill.name,
                skill.description.trim()
            ));
        }
        block.push_str("</available_skills>");
        Some(block)
    }
}

#[async_trait]
impl Toolset for SkillsToolset {
    fn name(&self) -> &str {
        "skills"
    }

    async fn tools(&self, ctx: Arc<dyn ReadonlyContext>) -> Result<Vec<Arc<dyn Tool>>> {
        let inner = self.state.inner.read().await.clone();
        let mut tools = inner.tools(ctx).await?;
        tools.push(Arc::new(ReloadSkillsTool {
            state: self.state.clone(),
        }));
        Ok(tools)
    }

    async fn process_llm_request(
        &self,
        ctx: Arc<dyn ReadonlyContext>,
        request: &mut LlmRequest,
    ) -> Result<()> {
        let inner = self.state.inner.read().await.clone();
        inner.process_llm_request(ctx, request).await?;
        if let Some(block) = self.available_skills_block().await {
            request.contents.insert(
                0,
                adk_rust::Content::new("user").with_text(format!(
                    "The following skills are available. Load a skill with `load_skill` only when the user's request matches its description:\n{block}"
                )),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn loads_local_skills_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("skills");
        std::fs::create_dir_all(local.join("demo-skill")).unwrap();
        std::fs::write(
            local.join("demo-skill").join("SKILL.md"),
            "---\nname: demo-skill\ndescription: Demo skill for tests.\n---\n\n# Demo\n\nDo the thing.\n",
        )
        .unwrap();
        // a broken skill (no frontmatter) is reported, not fatal
        std::fs::create_dir_all(local.join("broken")).unwrap();
        std::fs::write(local.join("broken").join("SKILL.md"), "no frontmatter here").unwrap();

        let sources = SkillSources {
            local_dir: Some(local.clone()),
            gcs: None,
            staging_dir: dir.path().join("staging"),
        };
        let toolset = SkillsToolset::load(sources).await;
        assert_eq!(toolset.skill_names().await, vec!["demo-skill"]);
        assert_eq!(toolset.load_errors().await.len(), 1);
        assert!(toolset.load_errors().await[0].contains("broken"));

        // add a second skill and reload
        std::fs::create_dir_all(local.join("another")).unwrap();
        std::fs::write(
            local.join("another").join("SKILL.md"),
            "---\nname: another\ndescription: Another.\n---\n\nBody\n",
        )
        .unwrap();
        let report = toolset.reload().await;
        assert_eq!(report.names, vec!["another", "demo-skill"]);

        let block = toolset.available_skills_block().await.unwrap();
        assert!(block.contains("`demo-skill`: Demo skill for tests."));
    }
}

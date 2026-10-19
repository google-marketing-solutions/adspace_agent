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

//! Deferred ("lazy") toolsets.
//!
//! Large Google API toolsets (Campaign Manager 360 alone is 218 tools and
//! several hundred kilobytes of declarations) would otherwise be sent to the
//! model on every request. A [`LazyToolset`] hides its tools until the model
//! calls [`EnableToolsetsTool`] (`enable_toolsets`), which records the choice
//! in session state. adk-rust re-resolves `Toolset::tools` against the live
//! session state before each model call, so the tools appear on the very next
//! turn and stay enabled for the rest of the session.

use std::sync::Arc;

use adk_rust::{LlmRequest, ReadonlyContext, Result, State, Tool, ToolContext, Toolset};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Legacy session-state key holding the list of enabled lazy toolset names.
///
/// Still honoured when reading (sessions created before the per-toolset keys
/// keep working) but no longer written: a single array is last-writer-wins
/// when the model enables two collections in parallel calls of one turn.
pub const ENABLED_TOOLSETS_STATE_KEY: &str = "enabled_lazy_toolsets";

/// Prefix of the per-toolset session-state keys (`<prefix><toolset name>`
/// holds `true` once the toolset is enabled). One key per toolset lets
/// concurrent `enable_toolsets` calls merge instead of overwrite each other.
pub const ENABLED_TOOLSET_STATE_KEY_PREFIX: &str = "enabled_lazy_toolset:";

/// Name of the activation tool.
pub const ENABLE_TOOLSETS_TOOL_NAME: &str = "enable_toolsets";

/// Describes a deferred toolset to the model.
#[derive(Debug, Clone)]
pub struct LazyToolsetEntry {
    /// Toolset name passed to `enable_toolsets`.
    pub name: String,
    /// One-line summary of what the toolset offers.
    pub summary: String,
    /// Number of tools the toolset contributes once enabled.
    pub tool_count: usize,
    /// Size of the toolset's declarations in bytes (0 when unknown). Shown
    /// to the model as an approximate token cost so it can weigh enabling a
    /// collection the same way it weighs any other context.
    pub declaration_bytes: usize,
}

/// Suffix that marks the write tier of a split collection (see
/// `config::LAZY_WRITE_TIER_SUFFIX`); used only to phrase the description.
const WRITE_TIER_SUFFIX: &str = "_write";

/// Session-state key marking `name` as enabled.
pub fn toolset_state_key(name: &str) -> String {
    format!("{ENABLED_TOOLSET_STATE_KEY_PREFIX}{name}")
}

/// `", ~12k tokens"` for a declaration size, or `""` when unknown.
fn approx_tokens(bytes: usize) -> String {
    if bytes == 0 {
        return String::new();
    }
    let tokens = bytes / 4;
    if tokens >= 1000 {
        format!(", ~{}k tokens", tokens / 1000)
    } else {
        format!(", ~{tokens} tokens")
    }
}

/// Whether `name` is enabled in `state` (per-toolset key or legacy array).
fn is_enabled_in(state: Option<&dyn State>, name: &str) -> bool {
    let Some(state) = state else {
        return false;
    };
    if state
        .get(&toolset_state_key(name))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return true;
    }
    state
        .get(ENABLED_TOOLSETS_STATE_KEY)
        .and_then(|v| {
            v.as_array()
                .map(|a| a.iter().filter_map(Value::as_str).any(|n| n == name))
        })
        .unwrap_or(false)
}

/// A toolset whose tools are only exposed once enabled for the session.
pub struct LazyToolset {
    inner: Arc<dyn Toolset>,
    name: String,
}

impl LazyToolset {
    /// Wraps `inner`.
    pub fn new(inner: Arc<dyn Toolset>) -> Self {
        let name = inner.name().to_string();
        Self { inner, name }
    }

    fn is_enabled(&self, ctx: &dyn ReadonlyContext) -> bool {
        is_enabled_in(ctx.state(), &self.name)
    }
}

#[async_trait]
impl Toolset for LazyToolset {
    fn name(&self) -> &str {
        &self.name
    }

    async fn tools(&self, ctx: Arc<dyn ReadonlyContext>) -> Result<Vec<Arc<dyn Tool>>> {
        if self.is_enabled(ctx.as_ref()) {
            self.inner.tools(ctx).await
        } else {
            Ok(Vec::new())
        }
    }

    async fn process_llm_request(
        &self,
        ctx: Arc<dyn ReadonlyContext>,
        request: &mut LlmRequest,
    ) -> Result<()> {
        if self.is_enabled(ctx.as_ref()) {
            self.inner.process_llm_request(ctx, request).await
        } else {
            Ok(())
        }
    }
}

/// Tool that enables deferred toolsets for the current session.
pub struct EnableToolsetsTool {
    entries: Vec<LazyToolsetEntry>,
    description: String,
}

impl EnableToolsetsTool {
    /// Creates the tool for the given deferred toolsets.
    pub fn new(entries: Vec<LazyToolsetEntry>) -> Self {
        let mut description = String::from(
            "Loads additional tool collections on demand. Their tools are NOT available until this tool has been \
             called with the collection name; afterwards they can be called directly for the rest of the session. \
             Call this first whenever the user's request needs one of these collections, passing only the \
             collections required.",
        );
        if entries.iter().any(|e| e.name.ends_with(WRITE_TIER_SUFFIX)) {
            description.push_str(
                " Collections ending in `_write` hold the create/update/patch/delete tools of the collection with \
                 the same base name and are much larger; enable them only when the user asks to create or change \
                 something, never for reading or reporting.",
            );
        }
        description.push_str(" Available collections:");
        for entry in &entries {
            description.push_str(&format!(
                "\n- `{}` ({} tools{}): {}",
                entry.name,
                entry.tool_count,
                approx_tokens(entry.declaration_bytes),
                entry.summary.trim()
            ));
        }
        Self {
            entries,
            description,
        }
    }

    /// The deferred toolsets this tool knows about.
    pub fn entries(&self) -> &[LazyToolsetEntry] {
        &self.entries
    }
}

#[async_trait]
impl Tool for EnableToolsetsTool {
    fn name(&self) -> &str {
        ENABLE_TOOLSETS_TOOL_NAME
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Option<Value> {
        let names: Vec<&str> = self.entries.iter().map(|e| e.name.as_str()).collect();
        Some(json!({
            "type": "object",
            "properties": {
                "toolset_names": {
                    "type": "array",
                    "items": { "type": "string", "enum": names },
                    "description": "Names of the tool collections to enable."
                }
            },
            "required": ["toolset_names"]
        }))
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        let requested: Vec<String> = args
            .get("toolset_names")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let known = |name: &str| self.entries.iter().any(|e| e.name == name);
        let (valid, unknown): (Vec<String>, Vec<String>) =
            requested.into_iter().partition(|n| known(n));

        // Already enabled: committed session state, plus anything an earlier
        // call in this same turn has staged in the pending state delta.
        let mut actions = ctx.actions();
        let session_state = ctx.session().map(|s| s.state());
        let was_enabled: Vec<String> = self
            .entries
            .iter()
            .map(|e| e.name.clone())
            .filter(|name| {
                is_enabled_in(session_state, name)
                    || actions
                        .state_delta
                        .get(&toolset_state_key(name))
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
            })
            .collect();
        let mut newly_enabled: Vec<String> = Vec::new();
        for name in valid {
            if !was_enabled.contains(&name) && !newly_enabled.contains(&name) {
                newly_enabled.push(name);
            }
        }
        // One key per toolset: parallel calls each add their own keys and the
        // runner merges the deltas, so no call can erase another's choice.
        for name in &newly_enabled {
            actions
                .state_delta
                .insert(toolset_state_key(name), Value::Bool(true));
        }
        ctx.set_actions(actions);
        let enabled: Vec<String> = self
            .entries
            .iter()
            .map(|e| e.name.clone())
            .filter(|name| was_enabled.contains(name) || newly_enabled.contains(name))
            .collect();

        let tool_count: usize = self
            .entries
            .iter()
            .filter(|e| newly_enabled.contains(&e.name))
            .map(|e| e.tool_count)
            .sum();
        let mut response = json!({
            "status": "SUCCESS",
            "enabled": enabled,
            "newly_enabled": newly_enabled,
            "message": format!("{tool_count} additional tools are now available and can be called directly."),
        });
        if !unknown.is_empty() {
            response["unknown"] = json!(unknown);
            response["available"] = json!(
                self.entries
                    .iter()
                    .map(|e| e.name.clone())
                    .collect::<Vec<_>>()
            );
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_rust::tool::{BasicToolset, SimpleToolContext};
    use adk_rust::{CallbackContext, Content, EventActions, MemoryEntry, Session};
    use std::collections::HashMap;

    struct Dummy(&'static str);

    #[async_trait]
    impl Tool for Dummy {
        fn name(&self) -> &str {
            self.0
        }
        fn description(&self) -> &str {
            "dummy"
        }
        async fn execute(&self, _ctx: Arc<dyn ToolContext>, _args: Value) -> Result<Value> {
            Ok(Value::Null)
        }
    }

    /// Read-only context exposing a fixed state.
    struct StateCtx {
        state: HashMap<String, Value>,
        content: Content,
    }

    impl State for StateCtx {
        fn get(&self, key: &str) -> Option<Value> {
            self.state.get(key).cloned()
        }
        fn set(&mut self, key: String, value: Value) {
            self.state.insert(key, value);
        }
        fn all(&self) -> HashMap<String, Value> {
            self.state.clone()
        }
    }

    impl ReadonlyContext for StateCtx {
        fn invocation_id(&self) -> &str {
            "inv"
        }
        fn agent_name(&self) -> &str {
            "agent"
        }
        fn user_id(&self) -> &str {
            "user"
        }
        fn app_name(&self) -> &str {
            "app"
        }
        fn session_id(&self) -> &str {
            "session"
        }
        fn branch(&self) -> &str {
            ""
        }
        fn user_content(&self) -> &Content {
            &self.content
        }
        fn state(&self) -> Option<&dyn State> {
            Some(self)
        }
    }

    fn ctx(enabled: &[&str]) -> Arc<dyn ReadonlyContext> {
        let mut state = HashMap::new();
        for name in enabled {
            state.insert(toolset_state_key(name), Value::Bool(true));
        }
        Arc::new(StateCtx {
            state,
            content: Content::new("user"),
        })
    }

    /// Tool context shaped like the runner's: committed session state is
    /// reachable through `session()`, `ReadonlyContext::state()` is `None`,
    /// and the actions are per call.
    struct SessionCtx {
        state: HashMap<String, Value>,
        actions: std::sync::Mutex<EventActions>,
        content: Content,
    }

    impl State for SessionCtx {
        fn get(&self, key: &str) -> Option<Value> {
            self.state.get(key).cloned()
        }
        fn set(&mut self, key: String, value: Value) {
            self.state.insert(key, value);
        }
        fn all(&self) -> HashMap<String, Value> {
            self.state.clone()
        }
    }

    impl ReadonlyContext for SessionCtx {
        fn invocation_id(&self) -> &str {
            "inv"
        }
        fn agent_name(&self) -> &str {
            "agent"
        }
        fn user_id(&self) -> &str {
            "user"
        }
        fn app_name(&self) -> &str {
            "app"
        }
        fn session_id(&self) -> &str {
            "session"
        }
        fn branch(&self) -> &str {
            ""
        }
        fn user_content(&self) -> &Content {
            &self.content
        }
    }

    impl Session for SessionCtx {
        fn id(&self) -> &str {
            "session"
        }
        fn app_name(&self) -> &str {
            "app"
        }
        fn user_id(&self) -> &str {
            "user"
        }
        fn state(&self) -> &dyn State {
            self
        }
        fn conversation_history(&self) -> Vec<Content> {
            Vec::new()
        }
    }

    #[async_trait]
    impl CallbackContext for SessionCtx {
        fn artifacts(&self) -> Option<Arc<dyn adk_rust::Artifacts>> {
            None
        }
    }

    #[async_trait]
    impl ToolContext for SessionCtx {
        fn function_call_id(&self) -> &str {
            "call"
        }
        fn actions(&self) -> EventActions {
            self.actions.lock().unwrap().clone()
        }
        fn set_actions(&self, actions: EventActions) {
            *self.actions.lock().unwrap() = actions;
        }
        async fn search_memory(&self, _query: &str) -> Result<Vec<MemoryEntry>> {
            Ok(vec![])
        }
        fn session(&self) -> Option<&dyn Session> {
            Some(self)
        }
    }

    fn session_ctx(state: HashMap<String, Value>) -> Arc<SessionCtx> {
        Arc::new(SessionCtx {
            state,
            actions: std::sync::Mutex::new(EventActions::default()),
            content: Content::new("user"),
        })
    }

    fn tool() -> EnableToolsetsTool {
        EnableToolsetsTool::new(vec![
            LazyToolsetEntry {
                name: "cm360".into(),
                summary: "Campaign Manager 360".into(),
                tool_count: 218,
                declaration_bytes: 90_000,
            },
            LazyToolsetEntry {
                name: "dv360".into(),
                summary: "Display & Video 360".into(),
                tool_count: 113,
                declaration_bytes: 0,
            },
        ])
    }

    #[test]
    fn description_mentions_write_tiers_only_when_present() {
        let plain = tool();
        assert!(
            plain
                .description()
                .contains("`cm360` (218 tools, ~22k tokens)")
        );
        assert!(plain.description().contains("`dv360` (113 tools):"));
        assert!(!plain.description().contains("_write"));

        let tiered = EnableToolsetsTool::new(vec![
            LazyToolsetEntry {
                name: "cm360".into(),
                summary: "reads".into(),
                tool_count: 110,
                declaration_bytes: 2_000,
            },
            LazyToolsetEntry {
                name: "cm360_write".into(),
                summary: "writes".into(),
                tool_count: 108,
                declaration_bytes: 600_000,
            },
        ]);
        assert!(
            tiered
                .description()
                .contains("Collections ending in `_write`")
        );
        assert!(
            tiered
                .description()
                .contains("`cm360` (110 tools, ~500 tokens)")
        );
        assert!(
            tiered
                .description()
                .contains("`cm360_write` (108 tools, ~150k tokens)")
        );
    }

    #[tokio::test]
    async fn lazy_toolset_hides_tools_until_enabled() {
        let inner: Arc<dyn Toolset> = Arc::new(BasicToolset::new(
            "cm360",
            vec![Arc::new(Dummy("a")), Arc::new(Dummy("b"))],
        ));
        let lazy = LazyToolset::new(inner);
        assert!(lazy.tools(ctx(&[])).await.unwrap().is_empty());
        assert!(lazy.tools(ctx(&["other"])).await.unwrap().is_empty());
        assert_eq!(lazy.tools(ctx(&["cm360"])).await.unwrap().len(), 2);

        // Sessions written before the per-toolset keys still work.
        let legacy = Arc::new(StateCtx {
            state: HashMap::from([(ENABLED_TOOLSETS_STATE_KEY.to_string(), json!(["cm360"]))]),
            content: Content::new("user"),
        });
        assert_eq!(lazy.tools(legacy).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn enable_tool_records_state_delta_and_reports_unknown_names() {
        let tool = tool();
        assert!(
            tool.description()
                .contains("`cm360` (218 tools, ~22k tokens)")
        );
        let schema = tool.parameters_schema().unwrap();
        assert_eq!(
            schema["properties"]["toolset_names"]["items"]["enum"],
            json!(["cm360", "dv360"])
        );

        let ctx = Arc::new(SimpleToolContext::new("test")) as Arc<dyn ToolContext>;
        let out = tool
            .execute(ctx.clone(), json!({"toolset_names": ["cm360", "nope"]}))
            .await
            .unwrap();
        assert_eq!(out["newly_enabled"], json!(["cm360"]));
        assert_eq!(out["enabled"], json!(["cm360"]));
        assert_eq!(out["unknown"], json!(["nope"]));
        let delta = ctx.actions().state_delta;
        assert_eq!(delta[&toolset_state_key("cm360")], json!(true));
        assert!(
            !delta.contains_key(ENABLED_TOOLSETS_STATE_KEY),
            "the legacy array is no longer written"
        );
    }

    /// Two calls in one turn (Gemini emits parallel calls) must both survive:
    /// the second call sees the first through the pending delta and only adds
    /// its own key, so the merged delta enables both collections.
    #[tokio::test]
    async fn sequential_calls_in_one_turn_merge_instead_of_overwriting() {
        let tool = tool();
        let ctx = session_ctx(HashMap::new());
        let first = tool
            .execute(ctx.clone(), json!({"toolset_names": ["cm360"]}))
            .await
            .unwrap();
        assert_eq!(first["newly_enabled"], json!(["cm360"]));
        let second = tool
            .execute(ctx.clone(), json!({"toolset_names": ["dv360"]}))
            .await
            .unwrap();
        assert_eq!(second["newly_enabled"], json!(["dv360"]));
        assert_eq!(second["enabled"], json!(["cm360", "dv360"]));
        let delta = ctx.actions().state_delta;
        assert_eq!(delta[&toolset_state_key("cm360")], json!(true));
        assert_eq!(delta[&toolset_state_key("dv360")], json!(true));

        // Separate contexts (true parallel calls) each write disjoint keys.
        let a = session_ctx(HashMap::new());
        let b = session_ctx(HashMap::new());
        tool.execute(a.clone(), json!({"toolset_names": ["cm360"]}))
            .await
            .unwrap();
        tool.execute(b.clone(), json!({"toolset_names": ["dv360"]}))
            .await
            .unwrap();
        let mut merged = a.actions().state_delta;
        merged.extend(b.actions().state_delta);
        assert!(is_enabled_in(
            Some(&StateCtx {
                state: merged.clone(),
                content: Content::new("user")
            }),
            "cm360"
        ));
        assert!(is_enabled_in(
            Some(&StateCtx {
                state: merged,
                content: Content::new("user")
            }),
            "dv360"
        ));
    }

    #[tokio::test]
    async fn committed_session_state_counts_as_enabled() {
        let tool = tool();
        let ctx = session_ctx(HashMap::from([(toolset_state_key("cm360"), json!(true))]));
        let out = tool
            .execute(ctx.clone(), json!({"toolset_names": ["cm360", "dv360"]}))
            .await
            .unwrap();
        assert_eq!(out["newly_enabled"], json!(["dv360"]));
        assert_eq!(out["enabled"], json!(["cm360", "dv360"]));
        assert_eq!(
            out["message"],
            json!("113 additional tools are now available and can be called directly.")
        );
        let delta = ctx.actions().state_delta;
        assert_eq!(delta.len(), 1, "only the new toolset is written: {delta:?}");

        // Legacy array in committed state is honoured too.
        let ctx = session_ctx(HashMap::from([(
            ENABLED_TOOLSETS_STATE_KEY.to_string(),
            json!(["dv360"]),
        )]));
        let out = tool
            .execute(ctx.clone(), json!({"toolset_names": ["dv360"]}))
            .await
            .unwrap();
        assert_eq!(out["newly_enabled"], json!([]));
        assert!(ctx.actions().state_delta.is_empty());
    }
}

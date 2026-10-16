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

/// Session-state key holding the list of enabled lazy toolset names.
pub const ENABLED_TOOLSETS_STATE_KEY: &str = "enabled_lazy_toolsets";

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
}

fn enabled_names(state: Option<&dyn State>) -> Vec<String> {
    state
        .and_then(|s| s.get(ENABLED_TOOLSETS_STATE_KEY))
        .and_then(|v| {
            v.as_array().map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
        })
        .unwrap_or_default()
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
        enabled_names(ctx.state()).iter().any(|n| n == &self.name)
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
             collections required. Available collections:",
        );
        for entry in &entries {
            description.push_str(&format!(
                "\n- `{}` ({} tools): {}",
                entry.name,
                entry.tool_count,
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

        let mut enabled = enabled_names(ctx.session().map(|s| s.state()));
        let mut newly_enabled = Vec::new();
        for name in valid {
            if !enabled.contains(&name) {
                enabled.push(name.clone());
                newly_enabled.push(name);
            }
        }
        let mut actions = ctx.actions();
        actions
            .state_delta
            .insert(ENABLED_TOOLSETS_STATE_KEY.to_string(), json!(enabled));
        ctx.set_actions(actions);

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
    use adk_rust::Content;
    use adk_rust::tool::{BasicToolset, SimpleToolContext};
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
        if !enabled.is_empty() {
            state.insert(ENABLED_TOOLSETS_STATE_KEY.to_string(), json!(enabled));
        }
        Arc::new(StateCtx {
            state,
            content: Content::new("user"),
        })
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
    }

    #[tokio::test]
    async fn enable_tool_records_state_delta_and_reports_unknown_names() {
        let tool = EnableToolsetsTool::new(vec![
            LazyToolsetEntry {
                name: "cm360".into(),
                summary: "Campaign Manager 360".into(),
                tool_count: 218,
            },
            LazyToolsetEntry {
                name: "dv360".into(),
                summary: "Display & Video 360".into(),
                tool_count: 113,
            },
        ]);
        assert!(tool.description().contains("`cm360` (218 tools)"));
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
        assert_eq!(out["unknown"], json!(["nope"]));
        assert_eq!(
            ctx.actions().state_delta[ENABLED_TOOLSETS_STATE_KEY],
            json!(["cm360"])
        );
    }
}

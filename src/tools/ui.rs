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

//! Generative UI tools powered by [`adk_ui`] 3.0 and `adk-surface` `genui`.
//!
//! Exposes `DrawToolset` in `adk_ui::catalogs::studio()` (`render_form`,
//! `render_card`, `render_table`, `render_screen`, `patch_surface`,
//! `delete_surface`), ordered sequentially during a turn so UI draws remain
//! ordered relative to state-mutating calls.

use std::sync::Arc;
use std::time::Duration;

use adk_rust::egress::ToolEgress;
use adk_rust::tool::BasicToolset;
use adk_rust::{BatchGroup, Evidence, Result, Tool, ToolContext, ToolEffect};
use async_trait::async_trait;
use serde_json::Value;

/// Wraps a `DrawToolset` tool so ADK's `ToolExecutionStrategy::Auto` executes
/// UI draw calls in model order rather than hoisting them ahead of writes.
struct OrderedDrawTool {
    inner: Arc<dyn Tool>,
}

impl OrderedDrawTool {
    fn new(inner: Arc<dyn Tool>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl Tool for OrderedDrawTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn declaration(&self) -> Value {
        self.inner.declaration()
    }

    fn enhanced_description(&self) -> String {
        self.inner.enhanced_description()
    }

    fn is_long_running(&self) -> bool {
        self.inner.is_long_running()
    }

    fn is_builtin(&self) -> bool {
        self.inner.is_builtin()
    }

    fn parameters_schema(&self) -> Option<Value> {
        self.inner.parameters_schema()
    }

    fn response_schema(&self) -> Option<Value> {
        self.inner.response_schema()
    }

    fn annotations(&self) -> Option<&Value> {
        self.inner.annotations()
    }

    fn origin(&self) -> &str {
        self.inner.origin()
    }

    fn required_scopes(&self) -> &[&str] {
        self.inner.required_scopes()
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn effect(&self) -> ToolEffect {
        self.inner.effect()
    }

    fn timeout_override(&self) -> Option<Option<Duration>> {
        self.inner.timeout_override()
    }

    // Deliberately not concurrency-safe: ADK's `Auto` strategy runs
    // concurrency-safe calls before mutating ones, whereas UI draws (or a
    // `patch_surface` after a prior `render_screen`) should stay in model order.
    fn is_concurrency_safe(&self) -> bool {
        false
    }

    fn batch_group(&self) -> Option<BatchGroup> {
        self.inner.batch_group()
    }

    fn is_agent_delegation(&self) -> bool {
        self.inner.is_agent_delegation()
    }

    fn egress(&self, args: &Value) -> ToolEgress {
        self.inner.egress(args)
    }

    fn approval_evidence(&self, args: &Value) -> Vec<Evidence> {
        self.inner.approval_evidence(args)
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        self.inner.execute(ctx, args).await
    }
}

/// Builds the `ui` toolset with the 6 `adk-surface` `genui` tools bound to the
/// `adk-ui` `studio` catalog (`forms` 1.0 + `insight` 1.0 + `studio` 1.0).
pub fn ui_toolset() -> BasicToolset {
    let tools: Vec<Arc<dyn Tool>> = adk_ui::toolset_for(adk_ui::catalogs::studio())
        .tools()
        .into_iter()
        .map(|tool| Arc::new(OrderedDrawTool::new(tool)) as Arc<dyn Tool>)
        .collect();
    BasicToolset::new("ui", tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use adk_rust::tool::SimpleToolContext;
    use adk_rust::{
        Artifacts, CallbackContext, Content, DRAW_TOOL_NAMES, EventActions, Part, ReadonlyContext,
        Session, State, Toolset,
    };
    use adk_surface::genui::{DRAWN_STATE_KEY, REFUSED_STATE_KEY};
    use serde_json::json;

    struct SessionCtx {
        state: HashMap<String, Value>,
        actions: Mutex<EventActions>,
        history: Mutex<Vec<Content>>,
        user_msg: Content,
    }

    impl SessionCtx {
        fn new() -> Self {
            Self {
                state: HashMap::new(),
                actions: Mutex::new(EventActions::default()),
                history: Mutex::new(Vec::new()),
                user_msg: Content::new("user"),
            }
        }

        fn record_tool_result(
            &self,
            name: &str,
            response: Value,
            state_delta: HashMap<String, Value>,
        ) {
            let mut history = self.history.lock().unwrap();
            history.push(Content {
                role: "function".to_string(),
                parts: vec![Part::FunctionResponse {
                    function_response: adk_rust::FunctionResponseData::new(name, response),
                    id: None,
                    annotations: None,
                }],
            });
            drop(history);
            let _ = state_delta;
        }
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
            "inv-1"
        }
        fn agent_name(&self) -> &str {
            "adspace_agent"
        }
        fn user_id(&self) -> &str {
            "tester"
        }
        fn app_name(&self) -> &str {
            "adspace_agent"
        }
        fn session_id(&self) -> &str {
            "sess-1"
        }
        fn branch(&self) -> &str {
            ""
        }
        fn user_content(&self) -> &Content {
            &self.user_msg
        }
    }

    impl Session for SessionCtx {
        fn id(&self) -> &str {
            "sess-1"
        }
        fn app_name(&self) -> &str {
            "adspace_agent"
        }
        fn user_id(&self) -> &str {
            "tester"
        }
        fn state(&self) -> &dyn State {
            self
        }
        fn conversation_history(&self) -> Vec<Content> {
            self.history.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl CallbackContext for SessionCtx {
        fn artifacts(&self) -> Option<Arc<dyn Artifacts>> {
            None
        }
    }

    #[async_trait]
    impl ToolContext for SessionCtx {
        fn function_call_id(&self) -> &str {
            "call-1"
        }
        fn actions(&self) -> EventActions {
            self.actions.lock().unwrap().clone()
        }
        fn set_actions(&self, actions: EventActions) {
            *self.actions.lock().unwrap() = actions;
        }
        async fn search_memory(&self, _query: &str) -> Result<Vec<adk_rust::MemoryEntry>> {
            Ok(Vec::new())
        }
        fn session(&self) -> Option<&dyn Session> {
            Some(self)
        }
    }

    async fn tool(name: &str) -> Arc<dyn Tool> {
        let ctx = Arc::new(SimpleToolContext::new("ui-test"));
        ui_toolset()
            .tools(ctx)
            .await
            .unwrap()
            .into_iter()
            .find(|t| t.name() == name)
            .unwrap_or_else(|| panic!("missing UI tool {name}"))
    }

    #[tokio::test]
    async fn exposes_six_ordered_draw_tools_for_studio_catalog() {
        let ctx = Arc::new(SimpleToolContext::new("ui-test"));
        let tools = ui_toolset().tools(ctx).await.unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(names, DRAW_TOOL_NAMES);

        for tool in &tools {
            assert!(tool.is_read_only(), "{} should be read-only", tool.name());
            assert_eq!(tool.effect(), ToolEffect::ReadOnly);
            assert!(
                !tool.is_concurrency_safe(),
                "{} must not be concurrency-safe",
                tool.name()
            );
        }
    }

    #[tokio::test]
    async fn valid_draws_emit_drawn_envelopes_and_state_deltas() {
        let ctx = Arc::new(SimpleToolContext::new("ui-test"));

        let card = tool("render_card")
            .await
            .execute(
                ctx.clone(),
                json!({
                    "surfaceId": "summary-card",
                    "title": "Campaign Summary",
                    "rows": [
                        { "label": "Spend", "value": "$1,250.00" },
                        { "label": "CTR", "value": "4.2%" }
                    ],
                    "note": "Last 7 days"
                }),
            )
            .await
            .unwrap();
        assert_eq!(card["status"], "shown");
        assert_eq!(card["drawn"]["kind"], "card");
        assert_eq!(card["drawn"]["catalogId"], adk_ui::catalogs::STUDIO_ID);
        assert!(ctx.actions().state_delta.contains_key(DRAWN_STATE_KEY));

        let table = tool("render_table")
            .await
            .execute(
                ctx.clone(),
                json!({
                    "surfaceId": "campaigns-table",
                    "title": "Top Campaigns",
                    "columns": ["Campaign", "Clicks"],
                    "rows": [["Brand Search", 1200], ["PMax", 850]]
                }),
            )
            .await
            .unwrap();
        assert_eq!(table["status"], "shown");
        assert_eq!(table["drawn"]["kind"], "table");

        let screen = tool("render_screen")
            .await
            .execute(
                ctx.clone(),
                json!({
                    "surfaceId": "perf-screen",
                    "title": "Performance",
                    "components": [
                        { "id": "root", "component": "Column", "children": ["kpi", "chart"] },
                        {
                            "id": "kpi",
                            "component": "Metric",
                            "label": "Total Spend",
                            "value": { "path": "/totalSpend" },
                            "format": "currency",
                            "currency": "USD"
                        },
                        {
                            "id": "chart",
                            "component": "Chart",
                            "kind": "bar",
                            "title": "Spend by Campaign",
                            "data": { "path": "/rows" },
                            "x": { "key": "campaign", "type": "category" },
                            "series": [{ "key": "spend", "label": "Spend" }]
                        }
                    ],
                    "dataModel": {
                        "totalSpend": 2500,
                        "rows": [{ "campaign": "Brand Search", "spend": 2500 }]
                    }
                }),
            )
            .await
            .unwrap();
        assert_eq!(screen["status"], "shown");
        assert_eq!(screen["drawn"]["kind"], "screen");
        assert_eq!(screen["drawn"]["catalogId"], adk_ui::catalogs::STUDIO_ID);

        let form = tool("render_form")
            .await
            .execute(
                ctx,
                json!({
                    "surfaceId": "intake-form",
                    "title": "Campaign Brief",
                    "fields": [
                        { "name": "campaign", "label": "Campaign name", "type": "text", "required": true },
                        { "name": "channel", "label": "Channel", "type": "choice", "options": [{ "label": "Search", "value": "search" }] }
                    ],
                    "actions": [
                        {
                            "id": "submit",
                            "label": "Create draft",
                            "tool": "list_user_files",
                            "args": { "campaign": { "path": "/campaign" } }
                        }
                    ]
                }),
            )
            .await
            .unwrap();
        assert_eq!(form["status"], "shown");
        assert_eq!(form["drawn"]["kind"], "form");
        assert_eq!(form["drawn"]["actions"][0]["id"], "submit");
    }

    #[tokio::test]
    async fn patch_and_delete_surface_read_session_history() {
        let ctx = Arc::new(SessionCtx::new());
        let initial = tool("render_screen")
            .await
            .execute(
                ctx.clone(),
                json!({
                    "surfaceId": "live-progress",
                    "title": "Sync",
                    "components": [
                        {
                            "id": "root",
                            "component": "Progress",
                            "label": "Syncing accounts",
                            "value": { "path": "/pct" }
                        }
                    ],
                    "dataModel": { "pct": 25 }
                }),
            )
            .await
            .unwrap();
        assert_eq!(initial["drawn"]["version"], 1);
        ctx.record_tool_result("render_screen", initial, ctx.actions().state_delta);

        let patched = tool("patch_surface")
            .await
            .execute(
                ctx.clone(),
                json!({
                    "surfaceId": "live-progress",
                    "data": [{ "path": "/pct", "value": 80 }]
                }),
            )
            .await
            .unwrap();
        assert_eq!(patched["status"], "shown");
        assert_eq!(patched["drawn"]["version"], 2);
        assert_eq!(
            patched["drawn"]["messages"][2]["updateDataModel"]["value"]["pct"],
            80
        );
        ctx.record_tool_result("patch_surface", patched, ctx.actions().state_delta);

        let deleted = tool("delete_surface")
            .await
            .execute(ctx.clone(), json!({ "surfaceId": "live-progress" }))
            .await
            .unwrap();
        assert_eq!(deleted["status"], "removed");
        assert_eq!(deleted["drawn"]["deleted"], true);
        assert_eq!(deleted["drawn"]["version"], 3);
    }

    #[tokio::test]
    async fn rule5_refusals_set_refused_state_and_return_error() {
        let ctx = Arc::new(SimpleToolContext::new("ui-test"));
        let err = tool("render_card")
            .await
            .execute(
                ctx.clone(),
                json!({
                    "surfaceId": "bad-card",
                    "title": "Allow once",
                    "rows": [{ "label": "Status", "value": "Waiting" }]
                }),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Not shown"), "{err}");
        assert!(ctx.actions().state_delta.contains_key(REFUSED_STATE_KEY));

        let ctx_secret = Arc::new(SimpleToolContext::new("ui-test"));
        let err_secret = tool("render_form")
            .await
            .execute(
                ctx_secret.clone(),
                json!({
                    "surfaceId": "secret-form",
                    "title": "Auth",
                    "fields": [{ "name": "password", "label": "Password" }],
                    "actions": [{ "id": "go", "label": "Continue", "tool": "list_user_files", "args": {} }]
                }),
            )
            .await
            .unwrap_err();
        assert!(err_secret.to_string().contains("secret"), "{err_secret}");
        assert!(
            ctx_secret
                .actions()
                .state_delta
                .contains_key(REFUSED_STATE_KEY)
        );
    }
}

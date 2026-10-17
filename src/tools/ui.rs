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

//! Generative UI tools powered by [`adk_ui`].
//!
//! Exposes `adk-ui`'s component tools (`render_chart`, `render_table`,
//! `render_layout`, `render_card`, `render_form`, `render_confirm`,
//! `render_modal`, `render_alert`, `render_progress`, `render_toast`) so the
//! agent can render interactive charts, sortable tables, KPI dashboards,
//! campaign intake forms, and trafficking confirmation cards directly inside
//! the web UI (`/ui/`).

use std::sync::Arc;

use adk_rust::tool::BasicToolset;
use adk_rust::{Result, Tool, ToolContext};
use adk_ui::{
    RenderAlertTool, RenderCardTool, RenderChartTool, RenderConfirmTool, RenderFormTool,
    RenderLayoutTool, RenderModalTool, RenderProgressTool, RenderTableTool, RenderToastTool,
};
use async_trait::async_trait;
use serde_json::Value;

/// Protocol-plumbing properties flattened into `adk-ui` tool schemas that we
/// strip so the model sees only the component parameters and always emits the
/// canonical `UiResponse` component tree.
const PROTOCOL_FIELDS: &[&str] = &[
    "protocol",
    "surface_id",
    "kit_id",
    "ag_ui_thread_id",
    "ag_ui_run_id",
    "mcp_apps",
];

struct CleanUiTool {
    inner: Arc<dyn Tool>,
    schema: Option<Value>,
}

impl CleanUiTool {
    fn new(inner: Arc<dyn Tool>) -> Self {
        let mut schema = inner.parameters_schema();
        if let Some(obj) = schema
            .as_mut()
            .and_then(|s| s.get_mut("properties"))
            .and_then(Value::as_object_mut)
        {
            for key in PROTOCOL_FIELDS {
                obj.remove(*key);
            }
        }
        Self { inner, schema }
    }
}

#[async_trait]
impl Tool for CleanUiTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> Option<Value> {
        self.schema.clone()
    }

    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, mut args: Value) -> Result<Value> {
        // Strip every protocol field, consistent with the advertised schema,
        // so a model that echoes one back cannot steer the surface/thread
        // plumbing.
        if let Some(obj) = args.as_object_mut() {
            for key in PROTOCOL_FIELDS {
                obj.remove(*key);
            }
        }
        self.inner.execute(ctx, args).await
    }
}

/// Builds the `ui` toolset with the 10 core `adk-ui` rendering tools.
pub fn ui_toolset() -> BasicToolset {
    let raw: Vec<Arc<dyn Tool>> = vec![
        Arc::new(RenderChartTool::new()),
        Arc::new(RenderTableTool::new()),
        Arc::new(RenderLayoutTool::new()),
        Arc::new(RenderCardTool::new()),
        Arc::new(RenderFormTool::new()),
        Arc::new(RenderConfirmTool::new()),
        Arc::new(RenderModalTool::new()),
        Arc::new(RenderAlertTool::new()),
        Arc::new(RenderProgressTool::new()),
        Arc::new(RenderToastTool::new()),
    ];
    let tools: Vec<Arc<dyn Tool>> = raw
        .into_iter()
        .map(|t| Arc::new(CleanUiTool::new(t)) as Arc<dyn Tool>)
        .collect();
    BasicToolset::new("ui", tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_rust::Toolset;
    use adk_rust::tool::SimpleToolContext;
    use serde_json::json;

    #[tokio::test]
    async fn exposes_ten_clean_ui_tools_and_emits_ui_response() {
        let ts = ui_toolset();
        let ctx = Arc::new(SimpleToolContext::new("ui-test"));
        let tools = ts.tools(ctx.clone()).await.unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            vec![
                "render_chart",
                "render_table",
                "render_layout",
                "render_card",
                "render_form",
                "render_confirm",
                "render_modal",
                "render_alert",
                "render_progress",
                "render_toast",
            ]
        );

        let chart = tools.iter().find(|t| t.name() == "render_chart").unwrap();
        let schema = chart.parameters_schema().unwrap();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("x_key"));
        assert!(props.contains_key("y_keys"));
        assert!(!props.contains_key("ag_ui_thread_id"));

        let out = chart
            .execute(
                ctx,
                json!({
                    "title": "Top Campaigns by Impressions",
                    "type": "bar",
                    "x_key": "campaign",
                    "y_keys": ["impressions"],
                    "data": [
                        { "campaign": "Brand Search", "impressions": 120000 },
                        { "campaign": "PMax Holiday", "impressions": 95000 }
                    ]
                }),
            )
            .await
            .unwrap();
        assert_eq!(out["components"][0]["type"], "chart");
        assert_eq!(
            out["components"][0]["title"],
            "Top Campaigns by Impressions"
        );
    }

    #[tokio::test]
    async fn protocol_fields_in_args_are_dropped_before_rendering() {
        let ts = ui_toolset();
        let ctx = Arc::new(SimpleToolContext::new("ui-test"));
        let tools = ts.tools(ctx.clone()).await.unwrap();
        let alert = tools.iter().find(|t| t.name() == "render_alert").unwrap();
        let out = alert
            .execute(
                ctx,
                json!({
                    "title": "Heads up",
                    "message": "Budget nearly spent",
                    "protocol": "ag_ui",
                    "surface_id": "injected",
                    "kit_id": "injected",
                    "ag_ui_thread_id": "injected",
                    "ag_ui_run_id": "injected",
                    "mcp_apps": [{"id": "injected"}]
                }),
            )
            .await
            .unwrap();
        assert_eq!(out["components"][0]["type"], "alert");
        assert!(
            !out.to_string().contains("injected"),
            "no protocol field leaks into the rendered response: {out}"
        );
    }
}

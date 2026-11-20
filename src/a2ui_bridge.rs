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

//! Wire-boundary bridge that projects UI tool responses (`DrawToolset`
//! `drawn` envelopes and custom `render_band_chart`) into A2UI v0.9.1 surfaces
//! wrapped in Python `google-adk` `UiWidget` envelopes
//! (`Event.actions.render_ui_widgets`).
//!
//! This module is invoked **only** by the Vertex AI Agent Engine / Gemini
//! Enterprise adapter in [`crate::server`]. The Cloud Run `/app/` Studio and
//! `/api/run_sse` stream continue to receive the native tool responses
//! untouched.

use adk_rust::DRAW_TOOL_NAMES;
use adk_surface::genui::{A2UI_VERSION, A2uiMessage, FORMS_CATALOG_ID, encode_jsonl, stable_id};
use serde_json::{Map, Value, json};

use crate::tools::custom_ui::as_number;

/// Media type advertised on Python `google-adk` `UiWidget` A2UI payloads.
pub const A2UI_MIME_TYPE: &str = "application/json+a2ui";
/// A2UI protocol version advertised on projected `UiWidget` payloads.
pub const A2UI_PROTOCOL_VERSION: &str = A2UI_VERSION;

/// Maximum number of chart data points projected as text rows inside a band
/// chart summary card (the full series always remains in `updateDataModel`).
const MAX_PROJECTED_CHART_ROWS: usize = 12;

/// Mutable builder state while projecting `render_band_chart` into A2UI v0.9.1
/// component definitions.
#[derive(Default)]
struct SurfaceBuilder {
    components: Vec<Value>,
    charts: Map<String, Value>,
    seq: usize,
}

impl SurfaceBuilder {
    fn next_id(&mut self, prefix: &str) -> String {
        let idx = self.seq;
        self.seq += 1;
        stable_id(&format!("a2ui:{prefix}:{idx}"))
    }

    fn push_text(&mut self, id: String, content: &str, variant: Option<&str>) -> String {
        let mut text_obj = Map::new();
        text_obj.insert("text".to_string(), Value::String(content.to_string()));
        if let Some(v) = variant.filter(|s| !s.is_empty()) {
            text_obj.insert("variant".to_string(), Value::String(v.to_string()));
        }
        self.components.push(json!({
            "id": id,
            "component": "Text",
            "text": text_obj["text"],
        }));
        if let Some(v) = text_obj.get("variant")
            && let Some(last) = self.components.last_mut().and_then(Value::as_object_mut)
        {
            last.insert("variant".to_string(), v.clone());
        }
        id
    }

    fn push_column(&mut self, id: String, children: Vec<String>) -> String {
        self.components.push(json!({
            "id": id,
            "component": "Column",
            "children": children,
            "justify": "start",
            "align": "stretch",
        }));
        id
    }

    fn push_card(&mut self, id: String, child_id: String) -> String {
        self.components.push(json!({
            "id": id,
            "component": "Card",
            "child": child_id,
        }));
        id
    }

    fn project_band_chart(&mut self, obj: &Map<String, Value>) -> Option<String> {
        let comp_type = obj.get("type").and_then(Value::as_str)?;
        if comp_type != "chart" {
            return None;
        }
        let id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "root")
            .map(str::to_string)
            .unwrap_or_else(|| self.next_id("chart"));
        let kind = obj.get("kind").and_then(Value::as_str).unwrap_or("line");
        let title = obj.get("title").and_then(Value::as_str).unwrap_or("Chart");
        let x_key = obj.get("x_key").and_then(Value::as_str).unwrap_or("x");
        let y_keys: Vec<String> = obj
            .get("y_keys")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let lower_key = obj.get("lower_key").and_then(Value::as_str);
        let upper_key = obj.get("upper_key").and_then(Value::as_str);
        let note_key = obj.get("note_key").and_then(Value::as_str);
        let rows = obj
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        self.charts.insert(id.clone(), Value::Object(obj.clone()));

        let mut inner_ids = Vec::new();
        let title_id = self.next_id("chart-title");
        inner_ids.push(self.push_text(title_id, title, Some("h3")));

        let subtitle = if y_keys.is_empty() {
            format!("{} ({} points)", kind.to_uppercase(), rows.len())
        } else {
            format!(
                "{} — {} by {} ({} points)",
                kind.to_uppercase(),
                y_keys.join(", "),
                x_key,
                rows.len()
            )
        };
        let sub_id = self.next_id("chart-sub");
        inner_ids.push(self.push_text(sub_id, &subtitle, Some("caption")));

        for row in rows.iter().take(MAX_PROJECTED_CHART_ROWS) {
            let Some(row_obj) = row.as_object() else {
                continue;
            };
            let x_val = row_obj
                .get(x_key)
                .map(format_cell_value)
                .unwrap_or_default();
            let mut metrics: Vec<String> = y_keys
                .iter()
                .filter_map(|yk| {
                    let v = row_obj.get(yk)?;
                    Some(format!("{yk}: {}", format_cell_value(v)))
                })
                .collect();

            if let (Some(lk), Some(uk)) = (lower_key, upper_key) {
                let low = row_obj.get(lk).and_then(as_number);
                let high = row_obj.get(uk).and_then(as_number);
                let y_num = y_keys
                    .first()
                    .and_then(|yk| row_obj.get(yk))
                    .and_then(as_number);
                if let (Some(l), Some(u)) = (low, high) {
                    metrics.push(format!("range [{l}..{u}]"));
                    if let Some(y) = y_num
                        && (y < l || y > u)
                    {
                        metrics.push("OUT OF RANGE".to_string());
                    }
                }
            }
            if let Some(nk) = note_key
                && let Some(note) = row_obj
                    .get(nk)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
            {
                metrics.push(format!("({note})"));
            }

            let line = format!("{x_val} — {}", metrics.join(" | "));
            let pt_id = self.next_id("chart-pt");
            inner_ids.push(self.push_text(pt_id, &line, Some("body")));
        }

        if rows.len() > MAX_PROJECTED_CHART_ROWS {
            let more_id = self.next_id("chart-more");
            let rem = rows.len() - MAX_PROJECTED_CHART_ROWS;
            inner_ids.push(self.push_text(
                more_id,
                &format!("… and {rem} more points in dataModel./charts/{id}"),
                Some("caption"),
            ));
        }

        let col_id = format!("{id}-body");
        self.push_column(col_id.clone(), inner_ids);
        Some(self.push_card(id, col_id))
    }
}

fn format_cell_value(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn widget_envelope(
    surface_id: &str,
    catalog_id: &str,
    tool_name: &str,
    messages: &[A2uiMessage],
) -> Option<Value> {
    let messages_json = serde_json::to_value(messages).ok()?;
    let jsonl = encode_jsonl(messages);
    Some(json!({
        "id": surface_id,
        "provider": "a2ui",
        "payload": {
            "surfaceId": surface_id,
            "catalogId": catalog_id,
            "mimeType": A2UI_MIME_TYPE,
            "version": A2UI_PROTOCOL_VERSION,
            "tool": tool_name,
            "messages": messages_json,
            "jsonl": jsonl,
        }
    }))
}

fn project_drawn_envelope(
    tool_name: &str,
    call_id: Option<&str>,
    drawn: &Map<String, Value>,
) -> Option<Value> {
    let raw_messages = drawn.get("messages")?.clone();
    let messages: Vec<A2uiMessage> = serde_json::from_value(raw_messages).ok()?;
    if messages.is_empty() {
        return None;
    }

    let surface_id = drawn
        .get("surfaceId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| stable_id(&format!("{tool_name}:{}", call_id.unwrap_or("main"))));
    let catalog_id = drawn
        .get("catalogId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(FORMS_CATALOG_ID);

    widget_envelope(&surface_id, catalog_id, tool_name, &messages)
}

fn project_band_chart_response(
    tool_name: &str,
    call_id: Option<&str>,
    response: &Value,
) -> Option<Value> {
    let components = response.get("components").and_then(Value::as_array)?;
    if components.is_empty() {
        return None;
    }

    let surface_id = response
        .get("surface_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| stable_id(&format!("{tool_name}:{}", call_id.unwrap_or("main"))));

    let mut builder = SurfaceBuilder::default();
    let mut root_children = Vec::with_capacity(components.len());
    for comp in components {
        if let Some(obj) = comp.as_object()
            && let Some(child_id) = builder.project_band_chart(obj)
        {
            root_children.push(child_id);
        }
    }
    if root_children.is_empty() {
        return None;
    }
    builder.push_column("root".to_string(), root_children);

    let mut messages = vec![
        A2uiMessage::create_surface(surface_id.clone()),
        A2uiMessage::update_components(surface_id.clone(), builder.components),
    ];
    if !builder.charts.is_empty() {
        let mut data_model = Map::new();
        data_model.insert("charts".to_string(), Value::Object(builder.charts));
        messages.push(A2uiMessage::update_data_model(
            surface_id.clone(),
            None,
            Some(Value::Object(data_model)),
        ));
    }

    widget_envelope(&surface_id, FORMS_CATALOG_ID, tool_name, &messages)
}

/// Projects a single UI tool response (`DrawToolset` `"drawn"` envelope or
/// `render_band_chart` `"components"`) into a Python `google-adk` `UiWidget`
/// JSON object (`{"id", "provider", "payload"}`).
pub fn project_ui_tool_to_widget(
    tool_name: &str,
    call_id: Option<&str>,
    response: &Value,
) -> Option<Value> {
    if DRAW_TOOL_NAMES.contains(&tool_name) {
        let drawn = response.get("drawn").and_then(Value::as_object)?;
        return project_drawn_envelope(tool_name, call_id, drawn);
    }
    if tool_name == "render_band_chart" {
        return project_band_chart_response(tool_name, call_id, response);
    }
    None
}

/// Inspects an `Event` JSON object for any UI tool `function_response` parts
/// and returns their projected A2UI `UiWidget` objects for
/// `Event.actions.render_ui_widgets`.
pub fn extract_event_ui_widgets(event: &Value) -> Vec<Value> {
    let Some(parts) = event
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let mut widgets = Vec::new();
    for part in parts {
        let Some(part_obj) = part.as_object() else {
            continue;
        };
        let Some(fr) = part_obj
            .get("function_response")
            .or_else(|| part_obj.get("functionResponse"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        let Some(name) = fr.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(response) = fr.get("response") else {
            continue;
        };
        let call_id = fr
            .get("id")
            .or_else(|| part_obj.get("id"))
            .and_then(Value::as_str);
        if let Some(widget) = project_ui_tool_to_widget(name, call_id, response) {
            widgets.push(widget);
        }
    }
    widgets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use adk_rust::tool::SimpleToolContext;
    use adk_rust::{Tool, Toolset};
    use adk_surface::genui::decode_jsonl;

    fn ctx() -> Arc<SimpleToolContext> {
        Arc::new(SimpleToolContext::new("a2ui-bridge-test"))
    }

    fn assert_valid_ui_widget(widget: &Value, expected_tool: &str) -> Vec<A2uiMessage> {
        let obj = widget.as_object().expect("UiWidget must be an object");
        // Python google-adk's UiWidget has `extra='forbid'` and accepts only
        // `id`, `provider`, and `payload`.
        assert_eq!(obj.len(), 3, "UiWidget must have exactly 3 top-level keys");
        assert!(obj.get("id").and_then(Value::as_str).is_some());
        assert_eq!(obj.get("provider").and_then(Value::as_str), Some("a2ui"));
        let payload = obj
            .get("payload")
            .and_then(Value::as_object)
            .expect("payload must be an object");
        assert_eq!(
            payload.get("surfaceId").and_then(Value::as_str),
            obj.get("id").and_then(Value::as_str)
        );
        assert!(payload.get("catalogId").and_then(Value::as_str).is_some());
        assert_eq!(
            payload.get("mimeType").and_then(Value::as_str),
            Some("application/json+a2ui")
        );
        assert_eq!(
            payload.get("version").and_then(Value::as_str),
            Some("v0.9.1")
        );
        assert_eq!(
            payload.get("tool").and_then(Value::as_str),
            Some(expected_tool)
        );
        let messages = payload
            .get("messages")
            .and_then(Value::as_array)
            .expect("messages must be an array");
        assert!(!messages.is_empty());
        let jsonl = payload
            .get("jsonl")
            .and_then(Value::as_str)
            .expect("jsonl must be a string");
        let decoded = decode_jsonl(jsonl).expect("jsonl must decode as A2uiMessage lines");
        assert_eq!(decoded.len(), messages.len());
        decoded
    }

    #[tokio::test]
    async fn projects_draw_tools_to_valid_a2ui_widgets() {
        let tools = crate::tools::ui::ui_toolset()
            .tools(ctx())
            .await
            .expect("ui tools");
        let by_name = |name: &str| {
            tools
                .iter()
                .find(|t| t.name() == name)
                .unwrap_or_else(|| panic!("missing {name}"))
                .clone()
        };

        let card_resp = by_name("render_card")
            .execute(
                ctx(),
                json!({
                    "surfaceId": "q4-summary",
                    "title": "Q4 Campaign Summary",
                    "rows": [
                        { "label": "Spend", "value": "$42,500" },
                        { "label": "ROAS", "value": "4.2x" }
                    ],
                    "note": "Holiday retail push"
                }),
            )
            .await
            .expect("card output");
        let card_widget =
            project_ui_tool_to_widget("render_card", Some("call-1"), &card_resp).expect("widget");
        let card_msgs = assert_valid_ui_widget(&card_widget, "render_card");
        assert_eq!(
            card_widget["payload"]["catalogId"],
            adk_ui::catalogs::STUDIO_ID
        );
        assert!(matches!(
            card_msgs[0].body,
            adk_surface::genui::A2uiBody::CreateSurface(_)
        ));

        let table_resp = by_name("render_table")
            .execute(
                ctx(),
                json!({
                    "surfaceId": "campaign-table",
                    "title": "Campaigns",
                    "columns": ["Campaign", "Spend"],
                    "rows": [
                        ["Brand Search", "$12,000"],
                        ["YouTube Reach", "$30,500"]
                    ]
                }),
            )
            .await
            .expect("table output");
        let table_widget =
            project_ui_tool_to_widget("render_table", Some("call-2"), &table_resp).expect("widget");
        assert_valid_ui_widget(&table_widget, "render_table");
        assert!(
            table_widget["payload"]["jsonl"]
                .as_str()
                .unwrap()
                .contains("updateDataModel")
        );

        let form_resp = by_name("render_form")
            .execute(
                ctx(),
                json!({
                    "surfaceId": "brief-form",
                    "title": "New Campaign Brief",
                    "fields": [
                        { "name": "campaign_name", "label": "Campaign Name", "type": "text", "required": true },
                        {
                            "name": "objective",
                            "label": "Objective",
                            "type": "choice",
                            "options": [
                                { "label": "Awareness", "value": "awareness" },
                                { "label": "Conversions", "value": "conversions" }
                            ]
                        },
                        { "name": "launch_date", "label": "Launch Date", "type": "date" },
                        { "name": "auto_bid", "label": "Enable Auto-Bidding", "type": "checkbox" }
                    ],
                    "actions": [
                        {
                            "id": "submit",
                            "label": "Save brief",
                            "tool": "list_user_files",
                            "args": { "campaign_name": { "path": "/campaign_name" } }
                        }
                    ]
                }),
            )
            .await
            .expect("form output");
        let form_widget =
            project_ui_tool_to_widget("render_form", Some("call-3"), &form_resp).expect("widget");
        assert_valid_ui_widget(&form_widget, "render_form");
        let form_jsonl = form_widget["payload"]["jsonl"].as_str().unwrap();
        assert!(form_jsonl.contains("TextField"));
        assert!(form_jsonl.contains("ChoicePicker"));
        assert!(form_jsonl.contains("DateTimeInput"));
        assert!(form_jsonl.contains("CheckBox"));

        // `patch_surface` and `delete_surface` are in `DRAW_TOOL_NAMES` and project cleanly too.
        let synthetic_delete = json!({
            "status": "removed",
            "message": "Removed 'brief-form'.",
            "drawn": {
                "surfaceId": "brief-form",
                "version": 2,
                "kind": "form",
                "catalogId": adk_ui::catalogs::STUDIO_ID,
                "messages": [{ "version": "v0.9.1", "deleteSurface": { "surfaceId": "brief-form" } }],
                "actions": [],
                "deleted": true
            }
        });
        let del_widget =
            project_ui_tool_to_widget("delete_surface", Some("call-4"), &synthetic_delete)
                .expect("delete widget");
        assert_valid_ui_widget(&del_widget, "delete_surface");
    }

    #[tokio::test]
    async fn projects_render_band_chart_with_out_of_range_notes() {
        let band_tool = crate::tools::custom_ui::RenderBandChartTool;
        let band_resp = band_tool
            .execute(
                ctx(),
                json!({
                    "title": "Daily Spend vs Expected Range",
                    "x_key": "date",
                    "y_key": "spend",
                    "lower_key": "low",
                    "upper_key": "high",
                    "note_key": "reason",
                    "data": [
                        { "date": "2026-11-01", "spend": 100, "low": 80, "high": 120 },
                        { "date": "2026-11-02", "spend": 210, "low": 80, "high": 120, "reason": "Black Friday spike" }
                    ]
                }),
            )
            .await
            .expect("band chart output");

        let event = json!({
            "content": {
                "role": "user",
                "parts": [{
                    "function_response": {
                        "id": "call-band-1",
                        "name": "render_band_chart",
                        "response": band_resp,
                    }
                }]
            }
        });
        let widgets = extract_event_ui_widgets(&event);
        assert_eq!(widgets.len(), 1);
        assert_valid_ui_widget(&widgets[0], "render_band_chart");
        let jsonl = widgets[0]["payload"]["jsonl"].as_str().unwrap();
        assert!(jsonl.contains("updateDataModel"));
        assert!(jsonl.contains("OUT OF RANGE"));
        assert!(jsonl.contains("Black Friday spike"));
    }
}

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

//! Wire-boundary bridge that projects `adk-ui` and `custom_ui` tool responses
//! (`{"components": [...]}`) into A2UI v0.9 surfaces wrapped in Python
//! `google-adk` `UiWidget` envelopes (`Event.actions.render_ui_widgets`).
//!
//! This module is invoked **only** by the Vertex AI Agent Engine / Gemini
//! Enterprise adapter in [`crate::server`]. The Cloud Run `/app/` Studio and
//! `/api/run_sse` stream continue to receive the native `adk-ui` component JSON
//! untouched.

use adk_ui::{
    A2UI_BASIC_CATALOG_ID, A2UI_MIME_TYPE, A2UI_PROTOCOL_VERSION, A2uiSchemaVersion, A2uiValidator,
    UiSurface,
    a2ui::{stable_id, stable_indexed_id},
};
use serde_json::{Map, Value, json};

use crate::tools::custom_ui::as_number;

/// Maximum number of table rows projected as individual A2UI `Row` components
/// before summarizing the remainder (the full table always remains in
/// `updateDataModel`).
const MAX_PROJECTED_TABLE_ROWS: usize = 20;

/// Maximum number of chart data points projected as text rows inside a chart
/// summary card (the full series always remains in `updateDataModel`).
const MAX_PROJECTED_CHART_ROWS: usize = 12;

/// Mutable builder state while projecting a tree of `adk-ui` components into a
/// flat list of A2UI v0.9 component definitions.
#[derive(Default)]
struct SurfaceBuilder {
    components: Vec<Value>,
    form_defaults: Map<String, Value>,
    tables: Map<String, Value>,
    charts: Map<String, Value>,
    seq: usize,
}

impl SurfaceBuilder {
    fn next_id(&mut self, prefix: &str) -> String {
        let idx = self.seq;
        self.seq += 1;
        stable_indexed_id("a2ui", prefix, idx)
    }

    fn push_text(&mut self, id: String, content: &str, variant: Option<&str>) -> String {
        let mut text_obj = Map::new();
        text_obj.insert("text".to_string(), json!({ "literalString": content }));
        if let Some(v) = variant.filter(|s| !s.is_empty()) {
            text_obj.insert("variant".to_string(), Value::String(v.to_string()));
        }
        self.components.push(json!({
            "id": id,
            "component": {
                "Text": Value::Object(text_obj),
            }
        }));
        id
    }

    fn push_column(&mut self, id: String, children: Vec<String>) -> String {
        self.components.push(json!({
            "id": id,
            "component": {
                "Column": {
                    "children": children,
                    "justify": "start",
                    "align": "stretch",
                }
            }
        }));
        id
    }

    fn push_row(&mut self, id: String, children: Vec<String>) -> String {
        self.components.push(json!({
            "id": id,
            "component": {
                "Row": {
                    "children": children,
                    "justify": "start",
                    "align": "center",
                }
            }
        }));
        id
    }

    fn push_card(&mut self, id: String, child_id: String) -> String {
        self.components.push(json!({
            "id": id,
            "component": {
                "Card": {
                    "child": child_id,
                }
            }
        }));
        id
    }

    fn push_divider(&mut self, id: String) -> String {
        self.components.push(json!({
            "id": id,
            "component": {
                "Divider": {
                    "axis": "horizontal",
                }
            }
        }));
        id
    }

    fn project_component(&mut self, comp: &Value) -> Option<String> {
        let obj = comp.as_object()?;
        let comp_type = obj.get("type").and_then(Value::as_str).unwrap_or("text");
        let explicit_id = obj
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "root")
            .map(str::to_string);

        match comp_type {
            "text" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("text"));
                let content = obj.get("content").and_then(Value::as_str).unwrap_or("");
                let variant = obj.get("variant").and_then(Value::as_str);
                Some(self.push_text(id, content, variant))
            }
            "badge" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("badge"));
                let label = obj.get("label").and_then(Value::as_str).unwrap_or("");
                Some(self.push_text(id, &format!("[{label}]"), Some("caption")))
            }
            "icon" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("icon"));
                let name = obj.get("name").and_then(Value::as_str).unwrap_or("icon");
                Some(self.push_text(id, &format!("({name})"), Some("caption")))
            }
            "image" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("image"));
                let src = obj.get("src").and_then(Value::as_str).unwrap_or("");
                let mut image_obj = Map::new();
                image_obj.insert("url".to_string(), json!({ "literalString": src }));
                if let Some(alt) = obj.get("alt").and_then(Value::as_str) {
                    image_obj.insert("altText".to_string(), json!({ "literalString": alt }));
                }
                self.components.push(json!({
                    "id": id,
                    "component": {
                        "Image": Value::Object(image_obj),
                    }
                }));
                Some(id)
            }
            "divider" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("divider"));
                Some(self.push_divider(id))
            }
            "button" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("btn"));
                let label = obj
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("Continue");
                let action_id = obj
                    .get("action_id")
                    .and_then(Value::as_str)
                    .unwrap_or("click");
                let label_id = format!("{id}-label");
                self.push_text(label_id.clone(), label, None);
                self.components.push(json!({
                    "id": id,
                    "component": {
                        "Button": {
                            "child": label_id,
                            "action": {
                                "event": {
                                    "name": action_id,
                                }
                            }
                        }
                    }
                }));
                Some(id)
            }
            "stack" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("stack"));
                let direction = obj
                    .get("direction")
                    .and_then(Value::as_str)
                    .unwrap_or("vertical");
                let children = self.project_children(obj.get("children"));
                if direction == "horizontal" {
                    Some(self.push_row(id, children))
                } else {
                    Some(self.push_column(id, children))
                }
            }
            "grid" | "container" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id(comp_type));
                let children = self.project_children(obj.get("children"));
                Some(self.push_column(id, children))
            }
            "tabs" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("tabs"));
                let mut tab_ids = Vec::new();
                if let Some(tabs) = obj.get("tabs").and_then(Value::as_array) {
                    for tab in tabs {
                        let Some(tab_obj) = tab.as_object() else {
                            continue;
                        };
                        let mut sec_children = Vec::new();
                        if let Some(label) = tab_obj.get("label").and_then(Value::as_str) {
                            let title_id = self.next_id("tab-title");
                            sec_children.push(self.push_text(title_id, label, Some("h4")));
                        }
                        sec_children.extend(self.project_children(tab_obj.get("content")));
                        if !sec_children.is_empty() {
                            let col_id = self.next_id("tab-col");
                            tab_ids.push(self.push_column(col_id, sec_children));
                        }
                    }
                }
                Some(self.push_column(id, tab_ids))
            }
            "card" | "modal" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id(comp_type));
                let mut inner_ids = Vec::new();
                if let Some(title) = obj
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    let title_id = self.next_id("card-title");
                    inner_ids.push(self.push_text(title_id, title, Some("h3")));
                }
                if let Some(desc) = obj
                    .get("description")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    let desc_id = self.next_id("card-desc");
                    inner_ids.push(self.push_text(desc_id, desc, Some("caption")));
                }
                inner_ids.extend(self.project_children(obj.get("content")));
                let footer_ids = self.project_children(obj.get("footer"));
                if !footer_ids.is_empty() {
                    let footer_row_id = self.next_id("card-footer");
                    inner_ids.push(self.push_row(footer_row_id, footer_ids));
                }
                if inner_ids.is_empty() {
                    let empty_id = self.next_id("card-empty");
                    inner_ids.push(self.push_text(empty_id, "", Some("body")));
                }
                let col_id = format!("{id}-body");
                self.push_column(col_id.clone(), inner_ids);
                Some(self.push_card(id, col_id))
            }
            "alert" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("alert"));
                let variant = obj
                    .get("variant")
                    .and_then(Value::as_str)
                    .unwrap_or("info")
                    .to_ascii_uppercase();
                let title = obj.get("title").and_then(Value::as_str).unwrap_or("Alert");
                let mut inner_ids = Vec::new();
                let title_id = self.next_id("alert-title");
                inner_ids.push(self.push_text(
                    title_id,
                    &format!("[{variant}] {title}"),
                    Some("h4"),
                ));
                if let Some(desc) = obj
                    .get("description")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    let desc_id = self.next_id("alert-desc");
                    inner_ids.push(self.push_text(desc_id, desc, Some("body")));
                }
                let col_id = format!("{id}-body");
                self.push_column(col_id.clone(), inner_ids);
                Some(self.push_card(id, col_id))
            }
            "progress" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("progress"));
                let value = obj.get("value").and_then(Value::as_u64).unwrap_or(0);
                let label = obj
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("Progress");
                Some(self.push_text(id, &format!("{label}: {value}%"), Some("body")))
            }
            "toast" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("toast"));
                let message = obj.get("message").and_then(Value::as_str).unwrap_or("");
                Some(self.push_text(id, message, Some("body")))
            }
            "spinner" | "skeleton" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id(comp_type));
                let label = obj
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("Loading…");
                Some(self.push_text(id, label, Some("caption")))
            }
            "code_block" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("code"));
                let code = obj.get("code").and_then(Value::as_str).unwrap_or("");
                Some(self.push_text(id, code, Some("code")))
            }
            "list" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("list"));
                let ordered = obj.get("ordered").and_then(Value::as_bool).unwrap_or(false);
                let mut item_ids = Vec::new();
                if let Some(items) = obj.get("items").and_then(Value::as_array) {
                    for (idx, item) in items.iter().enumerate() {
                        let text = item.as_str().unwrap_or_default();
                        let line = if ordered {
                            format!("{}. {text}", idx + 1)
                        } else {
                            format!("• {text}")
                        };
                        let item_id = self.next_id("list-item");
                        item_ids.push(self.push_text(item_id, &line, Some("body")));
                    }
                }
                Some(self.push_column(id, item_ids))
            }
            "key_value" => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("kv"));
                let mut row_ids = Vec::new();
                if let Some(pairs) = obj.get("pairs").and_then(Value::as_array) {
                    for pair in pairs {
                        let Some(p) = pair.as_object() else {
                            continue;
                        };
                        let key = p.get("key").and_then(Value::as_str).unwrap_or("");
                        let val = p.get("value").and_then(Value::as_str).unwrap_or("");
                        let k_id = self.next_id("kv-k");
                        let v_id = self.next_id("kv-v");
                        self.push_text(k_id.clone(), &format!("{key}:"), Some("caption"));
                        self.push_text(v_id.clone(), val, Some("body"));
                        let row_id = self.next_id("kv-row");
                        row_ids.push(self.push_row(row_id, vec![k_id, v_id]));
                    }
                }
                Some(self.push_column(id, row_ids))
            }
            "table" => Some(self.project_table(explicit_id, obj)),
            "chart" => Some(self.project_chart(explicit_id, obj)),
            "text_input"
                if matches!(
                    obj.get("input_type").and_then(Value::as_str),
                    Some("date" | "datetime" | "datetime-local" | "time")
                ) =>
            {
                Some(self.project_date_input(explicit_id, obj))
            }
            "text_input" | "textarea" | "number_input" | "slider" => {
                Some(self.project_text_field(explicit_id, obj))
            }
            "select" | "multi_select" => {
                Some(self.project_choice_picker(explicit_id, comp_type == "multi_select", obj))
            }
            "switch" => Some(self.project_checkbox(explicit_id, obj)),
            "date_input" => Some(self.project_date_input(explicit_id, obj)),
            _ => {
                let id = explicit_id.unwrap_or_else(|| self.next_id("comp"));
                let label = obj
                    .get("title")
                    .or_else(|| obj.get("label"))
                    .and_then(Value::as_str)
                    .unwrap_or(comp_type);
                Some(self.push_text(id, label, Some("body")))
            }
        }
    }

    fn project_children(&mut self, children_val: Option<&Value>) -> Vec<String> {
        let Some(arr) = children_val.and_then(Value::as_array) else {
            return Vec::new();
        };
        arr.iter()
            .filter_map(|child| self.project_component(child))
            .collect()
    }

    fn project_table(&mut self, explicit_id: Option<String>, obj: &Map<String, Value>) -> String {
        let id = explicit_id.unwrap_or_else(|| self.next_id("table"));
        let cols: Vec<(String, String)> = obj
            .get("columns")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| {
                        let co = c.as_object()?;
                        let accessor = co.get("accessor_key")?.as_str()?.to_string();
                        let header = co
                            .get("header")
                            .and_then(Value::as_str)
                            .unwrap_or(&accessor)
                            .to_string();
                        Some((header, accessor))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let rows = obj
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        self.tables.insert(
            id.clone(),
            json!({
                "columns": obj.get("columns").cloned().unwrap_or_else(|| json!([])),
                "rows": rows,
            }),
        );

        let mut table_children = Vec::new();
        if !cols.is_empty() {
            let mut header_cells = Vec::with_capacity(cols.len());
            for (header, _) in &cols {
                let cell_id = self.next_id("th");
                header_cells.push(self.push_text(cell_id, header, Some("h4")));
            }
            let header_row_id = self.next_id("thead");
            table_children.push(self.push_row(header_row_id, header_cells));
            let div_id = self.next_id("tdiv");
            table_children.push(self.push_divider(div_id));
        }

        for row in rows.iter().take(MAX_PROJECTED_TABLE_ROWS) {
            let Some(row_obj) = row.as_object() else {
                continue;
            };
            let mut cell_ids = Vec::new();
            if cols.is_empty() {
                for (k, v) in row_obj {
                    let cell_id = self.next_id("td");
                    let val_str = format_cell_value(v);
                    cell_ids.push(self.push_text(
                        cell_id,
                        &format!("{k}: {val_str}"),
                        Some("body"),
                    ));
                }
            } else {
                for (_, accessor) in &cols {
                    let cell_id = self.next_id("td");
                    let val_str = row_obj
                        .get(accessor)
                        .map(format_cell_value)
                        .unwrap_or_default();
                    cell_ids.push(self.push_text(cell_id, &val_str, Some("body")));
                }
            }
            if !cell_ids.is_empty() {
                let tr_id = self.next_id("tr");
                table_children.push(self.push_row(tr_id, cell_ids));
            }
        }

        if rows.len() > MAX_PROJECTED_TABLE_ROWS {
            let more_id = self.next_id("tmore");
            let remaining = rows.len() - MAX_PROJECTED_TABLE_ROWS;
            table_children.push(self.push_text(
                more_id,
                &format!("… and {remaining} more rows in dataModel./tables/{id}"),
                Some("caption"),
            ));
        }

        if table_children.is_empty() {
            let empty_id = self.next_id("tempty");
            table_children.push(self.push_text(empty_id, "No rows", Some("caption")));
        }

        self.push_column(id, table_children)
    }

    fn project_chart(&mut self, explicit_id: Option<String>, obj: &Map<String, Value>) -> String {
        let id = explicit_id.unwrap_or_else(|| self.next_id("chart"));
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
        self.push_card(id, col_id)
    }

    fn project_text_field(
        &mut self,
        explicit_id: Option<String>,
        obj: &Map<String, Value>,
    ) -> String {
        let id = explicit_id.unwrap_or_else(|| self.next_id("field"));
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let label = obj.get("label").and_then(Value::as_str).unwrap_or(&name);
        if let Some(default_val) = obj.get("default_value") {
            self.form_defaults.insert(name.clone(), default_val.clone());
        }
        self.components.push(json!({
            "id": id,
            "component": {
                "TextField": {
                    "label": { "literalString": label },
                    "text": { "path": format!("/form/{name}") },
                }
            }
        }));
        id
    }

    fn project_choice_picker(
        &mut self,
        explicit_id: Option<String>,
        multi: bool,
        obj: &Map<String, Value>,
    ) -> String {
        let id = explicit_id.unwrap_or_else(|| self.next_id("select"));
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let label = obj.get("label").and_then(Value::as_str).unwrap_or(&name);
        let options: Vec<Value> = obj
            .get("options")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|o| {
                        let o_obj = o.as_object()?;
                        let opt_label = o_obj.get("label")?.as_str()?;
                        let opt_val = o_obj.get("value")?.as_str()?;
                        Some(json!({
                            "label": { "literalString": opt_label },
                            "value": opt_val,
                        }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.components.push(json!({
            "id": id,
            "component": {
                "ChoicePicker": {
                    "label": { "literalString": label },
                    "options": options,
                    "multiple": multi,
                    "value": { "path": format!("/form/{name}") },
                }
            }
        }));
        id
    }

    fn project_checkbox(
        &mut self,
        explicit_id: Option<String>,
        obj: &Map<String, Value>,
    ) -> String {
        let id = explicit_id.unwrap_or_else(|| self.next_id("checkbox"));
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let label = obj.get("label").and_then(Value::as_str).unwrap_or(&name);
        let default_checked = obj
            .get("default_checked")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.form_defaults
            .insert(name.clone(), Value::Bool(default_checked));
        self.components.push(json!({
            "id": id,
            "component": {
                "CheckBox": {
                    "label": { "literalString": label },
                    "value": { "path": format!("/form/{name}") },
                }
            }
        }));
        id
    }

    fn project_date_input(
        &mut self,
        explicit_id: Option<String>,
        obj: &Map<String, Value>,
    ) -> String {
        let id = explicit_id.unwrap_or_else(|| self.next_id("date"));
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let label = obj.get("label").and_then(Value::as_str).unwrap_or(&name);
        self.components.push(json!({
            "id": id,
            "component": {
                "DateTimeInput": {
                    "label": { "literalString": label },
                    "enableDate": true,
                    "enableTime": false,
                    "value": { "path": format!("/form/{name}") },
                }
            }
        }));
        id
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

/// Projects a single `render_*` UI tool response (`{"components": [...]}`) into
/// a Python `google-adk` `UiWidget` JSON object (`{"id", "provider", "payload"}`).
///
/// Returns `None` if `tool_name` is not a `render_*` UI tool or `response` does
/// not contain a non-empty `"components"` array.
pub fn project_ui_tool_to_widget(
    tool_name: &str,
    call_id: Option<&str>,
    response: &Value,
) -> Option<Value> {
    if !tool_name.starts_with("render_") {
        return None;
    }
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
        .unwrap_or_else(|| {
            let seed = format!("{tool_name}:{}", call_id.unwrap_or("main"));
            stable_id(&seed)
        });

    let catalog_id = response
        .get("kit_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(A2UI_BASIC_CATALOG_ID)
        .to_string();

    let mut builder = SurfaceBuilder::default();
    let mut root_children = Vec::with_capacity(components.len());
    for comp in components {
        if let Some(child_id) = builder.project_component(comp) {
            root_children.push(child_id);
        }
    }
    if root_children.is_empty() {
        return None;
    }
    builder.push_column("root".to_string(), root_children);

    let mut data_model = Map::new();
    if !builder.form_defaults.is_empty() {
        data_model.insert("form".to_string(), Value::Object(builder.form_defaults));
    }
    if !builder.tables.is_empty() {
        data_model.insert("tables".to_string(), Value::Object(builder.tables));
    }
    if !builder.charts.is_empty() {
        data_model.insert("charts".to_string(), Value::Object(builder.charts));
    }

    let surface = UiSurface::new(surface_id.clone(), catalog_id.clone(), builder.components)
        .with_data_model((!data_model.is_empty()).then_some(Value::Object(data_model)));

    let messages = surface.to_a2ui_messages();
    if let Ok(validator) = A2uiValidator::new() {
        for msg in &messages {
            if validator
                .validate_message(msg, A2uiSchemaVersion::V0_9)
                .is_err()
            {
                return None;
            }
        }
    }

    let messages_json = serde_json::to_value(&messages).ok()?;
    let jsonl = surface.to_a2ui_jsonl().ok()?;

    Some(json!({
        "id": surface_id,
        "provider": "a2ui",
        "payload": {
            "surfaceId": surface.surface_id,
            "catalogId": catalog_id,
            "mimeType": A2UI_MIME_TYPE,
            "version": A2UI_PROTOCOL_VERSION,
            "tool": tool_name,
            "messages": messages_json,
            "jsonl": jsonl,
        }
    }))
}

/// Inspects an `Event` JSON object for any `render_*` `function_response` parts
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
    use adk_rust::tool::SimpleToolContext;
    use adk_rust::{Tool, Toolset};
    use std::sync::Arc;

    fn ctx() -> Arc<SimpleToolContext> {
        Arc::new(SimpleToolContext::new("a2ui-bridge-test"))
    }

    fn assert_valid_ui_widget(widget: &Value) {
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
            payload.get("mimeType").and_then(Value::as_str),
            Some(A2UI_MIME_TYPE)
        );
        let messages = payload
            .get("messages")
            .and_then(Value::as_array)
            .expect("messages must be an array");
        assert!(messages.len() >= 2);
        assert!(messages.first().unwrap().get("createSurface").is_some());
        assert!(messages.last().unwrap().get("updateComponents").is_some());

        let components = messages
            .last()
            .unwrap()
            .get("updateComponents")
            .and_then(|u| u.get("components"))
            .and_then(Value::as_array)
            .expect("updateComponents.components must be an array");
        assert!(
            components
                .iter()
                .any(|c| c.get("id").and_then(Value::as_str) == Some("root")),
            "A2UI components must include a root component"
        );

        let validator = A2uiValidator::new().expect("validator");
        for msg_val in messages {
            validator
                .validate_value(msg_val, A2uiSchemaVersion::V0_9)
                .expect("every A2UI message must pass v0.9 validation");
        }
    }

    #[tokio::test]
    async fn projects_render_card_and_table_and_form_to_valid_a2ui_widgets() {
        let tools = crate::tools::ui::ui_toolset()
            .tools(ctx())
            .await
            .expect("ui tools");

        let card_tool = tools
            .iter()
            .find(|t| t.name() == "render_card")
            .expect("render_card");
        let card_resp = card_tool
            .execute(
                ctx(),
                json!({
                    "title": "Q4 Campaign Summary",
                    "description": "Holiday retail push",
                    "content": "Total spend of $42,500 delivered a 4.2x ROAS.",
                    "actions": [
                        { "label": "Approve Plan", "action_id": "approve_plan" }
                    ]
                }),
            )
            .await
            .expect("card output");
        let card_widget =
            project_ui_tool_to_widget("render_card", Some("call-1"), &card_resp).expect("widget");
        assert_valid_ui_widget(&card_widget);

        let table_tool = tools
            .iter()
            .find(|t| t.name() == "render_table")
            .expect("render_table");
        let table_resp = table_tool
            .execute(
                ctx(),
                json!({
                    "columns": [
                        { "header": "Campaign", "accessor_key": "campaign" },
                        { "header": "Spend", "accessor_key": "spend" }
                    ],
                    "data": [
                        { "campaign": "Brand Search", "spend": "$12,000" },
                        { "campaign": "YouTube Reach", "spend": "$30,500" }
                    ]
                }),
            )
            .await
            .expect("table output");
        let table_widget =
            project_ui_tool_to_widget("render_table", Some("call-2"), &table_resp).expect("widget");
        assert_valid_ui_widget(&table_widget);
        assert!(
            table_widget["payload"]["jsonl"]
                .as_str()
                .unwrap()
                .contains("updateDataModel"),
            "table projection should include updateDataModel"
        );

        let form_tool = tools
            .iter()
            .find(|t| t.name() == "render_form")
            .expect("render_form");
        let form_resp = form_tool
            .execute(
                ctx(),
                json!({
                    "title": "New Campaign Brief",
                    "fields": [
                        { "name": "campaign_name", "label": "Campaign Name", "type": "text", "required": true },
                        {
                            "name": "objective",
                            "label": "Objective",
                            "type": "select",
                            "options": [
                                { "label": "Awareness", "value": "awareness" },
                                { "label": "Conversions", "value": "conversions" }
                            ]
                        },
                        { "name": "launch_date", "label": "Launch Date", "type": "date" },
                        { "name": "auto_bid", "label": "Enable Auto-Bidding", "type": "switch" }
                    ]
                }),
            )
            .await
            .expect("form output");
        let form_widget =
            project_ui_tool_to_widget("render_form", Some("call-3"), &form_resp).expect("widget");
        assert_valid_ui_widget(&form_widget);
        let form_jsonl = form_widget["payload"]["jsonl"].as_str().unwrap();
        assert!(form_jsonl.contains("TextField"));
        assert!(form_jsonl.contains("ChoicePicker"));
        assert!(form_jsonl.contains("DateTimeInput"));
        assert!(form_jsonl.contains("CheckBox"));
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
        assert_valid_ui_widget(&widgets[0]);
        let jsonl = widgets[0]["payload"]["jsonl"].as_str().unwrap();
        assert!(jsonl.contains("OUT OF RANGE"));
        assert!(jsonl.contains("Black Friday spike"));
    }
}

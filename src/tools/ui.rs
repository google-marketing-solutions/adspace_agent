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
use serde_json::{Map, Value, json};

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

/// Parameters `adk-ui` advertises for a surface data model and a per-response
/// theme. The studio has neither: it draws the literal `data` it is given, in
/// its own theme, so a model that binds a chart to `data_source` gets an empty
/// chart. Each entry names the array-valued parameters to descend through,
/// then the parameter to drop there.
const UNSUPPORTED_FIELDS: &[(&[&str], &str)] = &[
    (&[], "data_source"),
    (&[], "data_path_prefix"),
    (&[], "theme"),
    (&["fields"], "path"),
    (&["sections"], "data_path_prefix"),
    (&["sections", "fields"], "path"),
];

/// Where form fields are declared: `render_form`, and the form sections of
/// `render_layout`.
const FORM_FIELD_PATHS: &[&[&str]] = &[&["fields"], &["sections", "fields"]];

/// Field types a form takes. `adk-ui` only knows `number`, `select` and
/// `textarea`, and turns every other type into a text input tagged with the
/// type. The studio draws the tagged ones it has a native input for (`date`
/// among them); [`UPGRADED_FIELD_TYPES`] are rebuilt here.
const FORM_FIELD_TYPES: &str =
    "text, email, password, number, date, select, multiselect, switch, textarea";

/// Field types `adk-ui` flattens into a text input although it has a component
/// for them, with the component each becomes. A multiselect also loses its
/// options on the way, so they are put back from the call's arguments.
const UPGRADED_FIELD_TYPES: &[(&str, &str)] =
    &[("multiselect", "multi_select"), ("switch", "switch")];

/// Wording in the library's tool descriptions that leaves out what the studio
/// draws: tool, the text to replace, and its replacement.
const DESCRIPTION_FIXES: &[(&str, &str, &str)] = &[
    (
        "render_chart",
        "bar, line, area, and pie charts",
        "bar, line, area, pie, scatter and sparkline charts",
    ),
    (
        "render_form",
        "text, email, password, number, select, textarea",
        FORM_FIELD_TYPES,
    ),
];

/// Parameters the library declares without a description, which leaves the
/// model guessing what they do: tool, parameter, description. Only used where
/// the library still gives none.
const PARAMETER_NOTES: &[(&str, &str, &str)] = &[
    ("render_chart", "x_label", "Label under the x-axis."),
    ("render_chart", "y_label", "Label beside the y-axis."),
    (
        "render_chart",
        "x_type",
        "How x values are read: category (default), numeric, or time for dates and timestamps. Numeric and time values are spaced by value.",
    ),
    (
        "render_chart",
        "time_format",
        "With x_type time, how x values are written: date, time or iso. Defaults to fit the data.",
    ),
    ("render_chart", "window", "Show only the last N rows."),
    (
        "render_chart",
        "show_legend",
        "Show the legend (default true).",
    ),
    ("render_chart", "colors", "Series colors, as hex values."),
];

/// The `properties` of the objects reached from an object schema through the
/// array-valued parameters named by `path`.
fn schema_properties_at<'a>(
    schema: &'a mut Value,
    path: &[&str],
) -> Option<&'a mut Map<String, Value>> {
    let mut node = schema;
    for segment in path {
        node = node
            .get_mut("properties")?
            .get_mut(*segment)?
            .get_mut("items")?;
    }
    node.get_mut("properties")?.as_object_mut()
}

/// Calls `edit` on every object reached from `value` through the array-valued
/// parameters named by `path`.
fn for_each_object_at(
    value: &mut Value,
    path: &[&str],
    edit: &mut dyn FnMut(&mut Map<String, Value>),
) {
    match path.split_first() {
        None => {
            if let Some(object) = value.as_object_mut() {
                edit(object);
            }
        }
        Some((segment, rest)) => {
            if let Some(items) = value.get_mut(*segment).and_then(Value::as_array_mut) {
                for item in items {
                    for_each_object_at(item, rest, edit);
                }
            }
        }
    }
}

/// A form field whose type is in [`UPGRADED_FIELD_TYPES`], as the model
/// declared it.
struct DeclaredField {
    name: String,
    component: &'static str,
    input_type: &'static str,
    options: Value,
}

fn declared_fields(args: &mut Value) -> Vec<DeclaredField> {
    let mut declared = Vec::new();
    for path in FORM_FIELD_PATHS {
        for_each_object_at(args, path, &mut |field| {
            let field_type = field.get("type").and_then(Value::as_str);
            let upgrade = UPGRADED_FIELD_TYPES
                .iter()
                .find(|(input_type, _)| Some(*input_type) == field_type);
            if let (Some((input_type, component)), Some(name)) =
                (upgrade, field.get("name").and_then(Value::as_str))
            {
                declared.push(DeclaredField {
                    name: name.to_string(),
                    component,
                    input_type,
                    options: field.get("options").cloned().unwrap_or_else(|| json!([])),
                });
            }
        });
    }
    declared
}

/// Turns the text inputs `adk-ui` made of the `declared` fields into the
/// components they were asked to be. Each declaration is used once, so two
/// forms in one layout can reuse a field name.
fn upgrade_fields(node: &mut Value, declared: &mut Vec<DeclaredField>) {
    match node {
        Value::Array(items) => {
            for item in items {
                upgrade_fields(item, declared);
            }
        }
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) != Some("text_input") {
                for value in object.values_mut() {
                    upgrade_fields(value, declared);
                }
                return;
            }
            let name = object.get("name").and_then(Value::as_str);
            let input_type = object.get("input_type").and_then(Value::as_str);
            let Some(position) = declared
                .iter()
                .position(|f| Some(f.name.as_str()) == name && Some(f.input_type) == input_type)
            else {
                return;
            };
            let field = declared.remove(position);
            // What is left is what the target component has: id, name, label
            // and, for a multiselect, `required`.
            for key in [
                "input_type",
                "placeholder",
                "default_value",
                "min_length",
                "max_length",
                "error",
            ] {
                object.remove(key);
            }
            object.insert("type".into(), json!(field.component));
            if field.component == "switch" {
                // A required checkbox would have to be ticked to submit.
                object.remove("required");
                object.insert("default_checked".into(), json!(false));
            } else {
                object.insert("options".into(), field.options);
            }
        }
        _ => {}
    }
}

struct CleanUiTool {
    inner: Arc<dyn Tool>,
    description: String,
    schema: Option<Value>,
}

impl CleanUiTool {
    fn new(inner: Arc<dyn Tool>) -> Self {
        let name = inner.name().to_string();
        let mut description = inner.description().to_string();
        for (_, from, to) in DESCRIPTION_FIXES.iter().filter(|(tool, ..)| *tool == name) {
            description = description.replace(from, to);
        }

        let mut schema = inner.parameters_schema();
        if let Some(schema) = schema.as_mut() {
            if let Some(properties) = schema_properties_at(schema, &[]) {
                for key in PROTOCOL_FIELDS {
                    properties.remove(*key);
                }
                for (_, parameter, note) in
                    PARAMETER_NOTES.iter().filter(|(tool, ..)| *tool == name)
                {
                    if let Some(parameter) = properties
                        .get_mut(*parameter)
                        .and_then(Value::as_object_mut)
                        && !parameter.contains_key("description")
                    {
                        parameter.insert("description".into(), json!(note));
                    }
                }
            }
            for (path, key) in UNSUPPORTED_FIELDS {
                if let Some(properties) = schema_properties_at(schema, path) {
                    properties.remove(*key);
                }
            }
            for path in FORM_FIELD_PATHS {
                if let Some(field_type) = schema_properties_at(schema, path)
                    .and_then(|properties| properties.get_mut("type"))
                    .and_then(Value::as_object_mut)
                {
                    field_type.insert(
                        "description".into(),
                        json!(format!(
                            "Field type: {FORM_FIELD_TYPES}. Give options for select and multiselect."
                        )),
                    );
                }
            }
        }
        Self {
            inner,
            description,
            schema,
        }
    }
}

#[async_trait]
impl Tool for CleanUiTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Option<Value> {
        self.schema.clone()
    }

    fn is_read_only(&self) -> bool {
        true
    }

    // Deliberately not `is_concurrency_safe`: ADK's `Auto` strategy runs the
    // concurrency-safe calls of a turn before the others, so a
    // `render_alert("done")` emitted after a write would be shown before the
    // write had run. Rendering is local and gains nothing from overlapping.

    async fn execute(&self, ctx: Arc<dyn ToolContext>, mut args: Value) -> Result<Value> {
        // Strip every field the schema no longer advertises, so a model that
        // sends one anyway cannot steer the surface/thread plumbing or bind a
        // component to a data model the studio does not have.
        if let Some(obj) = args.as_object_mut() {
            for key in PROTOCOL_FIELDS {
                obj.remove(*key);
            }
        }
        for (path, key) in UNSUPPORTED_FIELDS {
            for_each_object_at(&mut args, path, &mut |object| {
                object.remove(*key);
            });
        }
        let mut declared = declared_fields(&mut args);
        let mut out = self.inner.execute(ctx, args).await?;
        if !declared.is_empty() {
            upgrade_fields(&mut out, &mut declared);
        }
        Ok(out)
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

    async fn tool(name: &str) -> Arc<dyn Tool> {
        let ctx = Arc::new(SimpleToolContext::new("ui-test"));
        let tools = ui_toolset().tools(ctx).await.unwrap();
        tools.into_iter().find(|t| t.name() == name).unwrap()
    }

    /// Every component of `kind` in a rendered response, in document order.
    fn components_of<'a>(node: &'a Value, kind: &str, found: &mut Vec<&'a Value>) {
        match node {
            Value::Array(items) => items.iter().for_each(|i| components_of(i, kind, found)),
            Value::Object(object) => {
                if object.get("type").and_then(Value::as_str) == Some(kind) {
                    found.push(node);
                }
                object.values().for_each(|v| components_of(v, kind, found));
            }
            _ => {}
        }
    }

    #[tokio::test]
    async fn parameters_the_studio_cannot_honor_are_not_advertised_or_passed_on() {
        let ctx = Arc::new(SimpleToolContext::new("ui-test"));
        for tool in ui_toolset().tools(ctx.clone()).await.unwrap() {
            let schema = tool.parameters_schema().unwrap().to_string();
            for gone in [
                "data_source",
                "binding_path",
                "data_path_prefix",
                "\"theme\"",
                "\"path\"",
            ] {
                assert!(
                    !schema.contains(gone),
                    "{} still advertises {gone}",
                    tool.name()
                );
            }
        }

        let out = tool("render_chart")
            .await
            .execute(
                ctx.clone(),
                json!({
                    "type": "line",
                    "x_key": "day",
                    "y_keys": ["clicks"],
                    "data": [{ "day": "2026-11-01", "clicks": 3 }],
                    "data_source": { "binding_path": "/metrics" },
                    "theme": "dark"
                }),
            )
            .await
            .unwrap();
        assert!(out["components"][0].get("data_source").is_none(), "{out}");
        assert_eq!(out["components"][0]["data"][0]["clicks"], 3);

        // Only the parameters are dropped: a row may have a column by the same name.
        let out = tool("render_table")
            .await
            .execute(
                ctx,
                json!({
                    "columns": [{ "header": "Path", "accessor_key": "path" }],
                    "data": [{ "path": "/landing", "theme": "dark" }]
                }),
            )
            .await
            .unwrap();
        assert_eq!(out["components"][0]["data"][0]["path"], "/landing");
        assert_eq!(out["components"][0]["data"][0]["theme"], "dark");
    }

    #[tokio::test]
    async fn descriptions_cover_what_the_studio_draws() {
        let chart = tool("render_chart").await;
        assert!(chart.description().contains("scatter and sparkline"));
        let schema = chart.parameters_schema().unwrap();
        for (_, parameter, _) in PARAMETER_NOTES {
            let description = schema["properties"][parameter]["description"].as_str();
            assert!(
                description.is_some_and(|d| !d.is_empty()),
                "render_chart.{parameter} has no description"
            );
        }

        let form = tool("render_form").await;
        assert!(form.description().contains(FORM_FIELD_TYPES));
        let form_schema = form.parameters_schema().unwrap();
        let layout_schema = tool("render_layout").await.parameters_schema().unwrap();
        for field_type in [
            &form_schema["properties"]["fields"]["items"]["properties"]["type"],
            &layout_schema["properties"]["sections"]["items"]["properties"]["fields"]["items"]["properties"]
                ["type"],
        ] {
            let description = field_type["description"].as_str().unwrap_or_default();
            assert!(description.contains(FORM_FIELD_TYPES), "{field_type}");
        }
    }

    #[tokio::test]
    async fn form_fields_become_the_components_they_were_declared_as() {
        let ctx = Arc::new(SimpleToolContext::new("ui-test"));
        let channels = json!([
            { "label": "Search", "value": "search" },
            { "label": "YouTube", "value": "youtube" }
        ]);
        let fields = json!([
            { "name": "start", "label": "Start date", "type": "date", "required": true },
            { "name": "channels", "label": "Channels", "type": "multiselect",
              "required": true, "options": channels },
            { "name": "notify", "label": "Email me", "type": "switch", "required": true },
            { "name": "email", "label": "Email", "type": "email", "path": "/user/email" }
        ]);

        let form = tool("render_form")
            .await
            .execute(ctx.clone(), json!({ "title": "Brief", "fields": fields }))
            .await
            .unwrap();
        let layout = tool("render_layout")
            .await
            .execute(
                ctx,
                json!({
                    "title": "Plan",
                    "sections": [
                        { "title": "Filters", "type": "form", "fields": fields },
                        { "title": "Again", "type": "form", "fields": fields }
                    ]
                }),
            )
            .await
            .unwrap();

        for (out, forms) in [(&form, 1), (&layout, 2)] {
            let mut multi = Vec::new();
            components_of(out, "multi_select", &mut multi);
            assert_eq!(multi.len(), forms, "{out}");
            for component in multi {
                assert_eq!(component["name"], "channels");
                assert_eq!(component["label"], "Channels");
                assert_eq!(component["required"], true);
                assert_eq!(component["options"], channels);
                assert!(component.get("input_type").is_none());
            }

            let mut switches = Vec::new();
            components_of(out, "switch", &mut switches);
            assert_eq!(switches.len(), forms, "{out}");
            for component in switches {
                assert_eq!(component["name"], "notify");
                assert_eq!(component["default_checked"], false);
                assert!(component.get("required").is_none());
            }

            // Types the studio draws from the tag stay text inputs, under the
            // field's own name.
            let mut inputs = Vec::new();
            components_of(out, "text_input", &mut inputs);
            let tagged: Vec<(&str, &str)> = inputs
                .iter()
                .map(|c| {
                    (
                        c["name"].as_str().unwrap(),
                        c["input_type"].as_str().unwrap(),
                    )
                })
                .collect();
            let expected = [("start", "date"), ("email", "email")].repeat(forms);
            assert_eq!(tagged, expected, "{out}");
        }
    }
}

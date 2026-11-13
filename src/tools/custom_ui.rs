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

//! UI components defined in this repo, beyond what the `adk-ui` crate's
//! render tools can produce.
//!
//! A tool here returns the same envelope the library's render tools return,
//! `{"components": [{"type": …}]}`, and the shared renderer in
//! `src/app/adk-ui/` draws it. Nothing between the two parses components (the
//! server passes tool results through as JSON), so a new component or chart
//! kind needs a tool here and support in the renderer, and no change to
//! `adk-ui`.
//!
//! Tool names start with `render_` so the response is compacted before it is
//! sent back to the model, like the library's render tools (see
//! `compact_ui_function_response` in `agent.rs`).

use std::sync::Arc;

use adk_rust::tool::BasicToolset;
use adk_rust::{AdkError, Result, Tool, ToolContext};
use async_trait::async_trait;
use serde_json::{Map, Value, json};

/// Chart `kind` drawn by `renderChartInner` in `src/app/adk-ui/charts.js`.
pub const BAND_CHART_KIND: &str = "band";

/// Most rows a band chart accepts. The whole component is stored in the
/// session and streamed to the browser.
pub const MAX_BAND_CHART_ROWS: usize = 2_000;

/// How the renderer reads x values (`x_type` of a chart component).
const X_TYPES: &[&str] = &["category", "numeric", "time"];

/// Draws one series against a lower and an upper bound per row.
///
/// The chart is generic: it shades the range, draws the series, marks the
/// values outside their range and shows a note on each. What the range means
/// (an expected range, a forecast interval, a pacing corridor, a benchmark)
/// and where the bounds come from are the caller's business; this tool does
/// not compute them.
pub struct RenderBandChartTool;

fn invalid(message: impl std::fmt::Display) -> AdkError {
    AdkError::tool(format!("Invalid parameters: {message}"))
}

/// A number, or a string holding one (`"1,234.5"`, `"$12"`, `"8%"`), the way
/// the renderer reads cell values.
fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let cleaned: String = s
                .chars()
                .filter(|c| !matches!(c, '$' | ',' | '%') && !c.is_whitespace())
                .collect();
            cleaned.parse::<f64>().ok().filter(|n| n.is_finite())
        }
        _ => None,
    }
}

#[async_trait]
impl Tool for RenderBandChartTool {
    fn name(&self) -> &str {
        "render_band_chart"
    }

    fn description(&self) -> &str {
        "Draws a series against a lower and an upper bound: a shaded band for the range, a line \
         for the values, and a marker on every value outside its range, each with an optional \
         note that is also listed under the chart. Use it for anything with a range to compare \
         against: values against an expected range (anomalies), actuals against a forecast \
         interval, pacing against a corridor, a metric against a benchmark range. You supply the \
         bounds for each row; this tool does not calculate them. Rows are drawn in the order \
         given."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "Chart title." },
                "data": {
                    "type": "array",
                    "description": "Rows in display order. Each row holds the x value, the series value and, where known, the lower and upper bound and a note.",
                    "items": { "type": "object" }
                },
                "x_key": { "type": "string", "description": "Key of the x value in each row, for example a date." },
                "y_key": { "type": "string", "description": "Key of the series value in each row." },
                "lower_key": { "type": "string", "description": "Key of the lower bound. Rows without both bounds are drawn without a band." },
                "upper_key": { "type": "string", "description": "Key of the upper bound." },
                "note_key": { "type": "string", "description": "Optional key of a short note per row, shown for values outside their range, for example a likely cause." },
                "band_label": { "type": "string", "description": "Legend label for the range. Defaults to \"Expected range\"." },
                "x_type": { "type": "string", "enum": X_TYPES, "description": "How x values are read: category (default), numeric, or time for dates and timestamps. Numeric and time values are spaced by value." },
                "x_label": { "type": "string", "description": "Label under the x-axis." },
                "y_label": { "type": "string", "description": "Label beside the y-axis, for example the metric and its unit." }
            },
            "required": ["data", "x_key", "y_key", "lower_key", "upper_key"]
        }))
    }

    // Read-only but, like the library's render tools, not concurrency-safe:
    // a render must not run ahead of the calls that precede it in a turn.
    fn is_read_only(&self) -> bool {
        true
    }

    async fn execute(&self, _ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        let key = |name: &str| {
            args.get(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let required =
            |name: &str| key(name).ok_or_else(|| invalid(format!("`{name}` is required")));
        let x_key = required("x_key")?;
        let y_key = required("y_key")?;
        let lower_key = required("lower_key")?;
        let upper_key = required("upper_key")?;

        let rows = args
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("`data` must be an array of row objects"))?;
        if rows.is_empty() {
            return Err(invalid("`data` is empty"));
        }
        if rows.len() > MAX_BAND_CHART_ROWS {
            return Err(invalid(format!(
                "`data` has {} rows; the most a band chart takes is {MAX_BAND_CHART_ROWS}. Aggregate or shorten the range",
                rows.len()
            )));
        }

        let mut values = 0usize;
        let mut bounded = 0usize;
        for (index, row) in rows.iter().enumerate() {
            let Some(row) = row.as_object() else {
                return Err(invalid(format!("row {index} of `data` is not an object")));
            };
            if !row.contains_key(&x_key) {
                return Err(invalid(format!("row {index} has no `{x_key}`")));
            }
            if row.get(&y_key).and_then(as_number).is_some() {
                values += 1;
            }
            let lower = row.get(&lower_key).and_then(as_number);
            let upper = row.get(&upper_key).and_then(as_number);
            if let (Some(lower), Some(upper)) = (lower, upper) {
                if lower > upper {
                    return Err(invalid(format!(
                        "row {index}: `{lower_key}` ({lower}) is above `{upper_key}` ({upper})"
                    )));
                }
                bounded += 1;
            }
        }
        if values == 0 {
            return Err(invalid(format!("no row has a numeric `{y_key}`")));
        }
        if bounded == 0 {
            return Err(invalid(format!(
                "no row has both a numeric `{lower_key}` and `{upper_key}`; use render_chart for a plain line"
            )));
        }

        let mut chart = Map::new();
        chart.insert("type".into(), json!("chart"));
        chart.insert("kind".into(), json!(BAND_CHART_KIND));
        chart.insert("data".into(), Value::Array(rows.clone()));
        chart.insert("x_key".into(), json!(x_key));
        chart.insert("y_keys".into(), json!([y_key]));
        chart.insert("lower_key".into(), json!(lower_key));
        chart.insert("upper_key".into(), json!(upper_key));
        if let Some(x_type) = key("x_type") {
            if !X_TYPES.contains(&x_type.as_str()) {
                return Err(invalid(format!(
                    "`x_type` is `{x_type}`; it must be one of {}",
                    X_TYPES.join(", ")
                )));
            }
            chart.insert("x_type".into(), json!(x_type));
        }
        for optional in ["title", "note_key", "band_label", "x_label", "y_label"] {
            if let Some(value) = key(optional) {
                chart.insert(optional.into(), json!(value));
            }
        }
        Ok(json!({ "components": [Value::Object(chart)] }))
    }
}

/// Builds the toolset of UI components defined in this repo.
pub fn custom_ui_toolset() -> BasicToolset {
    let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(RenderBandChartTool)];
    BasicToolset::new("custom_ui", tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_rust::Toolset;
    use adk_rust::tool::SimpleToolContext;

    fn ctx() -> Arc<SimpleToolContext> {
        Arc::new(SimpleToolContext::new("custom-ui-test"))
    }

    async fn band_chart() -> Arc<dyn Tool> {
        let tools = custom_ui_toolset().tools(ctx()).await.unwrap();
        assert_eq!(tools.len(), 1);
        tools.into_iter().next().unwrap()
    }

    fn args() -> Value {
        json!({
            "title": "Daily spend",
            "x_key": "date",
            "y_key": "spend",
            "lower_key": "low",
            "upper_key": "high",
            "note_key": "why",
            "x_type": "time",
            "y_label": "Spend (USD)",
            "data": [
                { "date": "2026-11-01", "spend": 100, "low": 80, "high": 120 },
                { "date": "2026-11-02", "spend": "1,480", "low": 80, "high": 120, "why": "Budget raised" },
                { "date": "2026-11-03", "spend": 95 }
            ]
        })
    }

    #[tokio::test]
    async fn band_chart_is_a_chart_component_of_kind_band() {
        let tool = band_chart().await;
        assert_eq!(tool.name(), "render_band_chart");
        assert!(tool.is_read_only());
        assert!(!tool.is_concurrency_safe());

        let out = tool.execute(ctx(), args()).await.unwrap();
        let components = out["components"].as_array().unwrap();
        assert_eq!(components.len(), 1);
        let chart = &components[0];
        // A chart like the library's, so the studio titles and exports it the
        // same way; the kind and the extra keys are what this repo adds.
        assert_eq!(chart["type"], "chart");
        assert_eq!(chart["kind"], "band");
        assert_eq!(chart["title"], "Daily spend");
        assert_eq!(chart["x_key"], "date");
        assert_eq!(chart["y_keys"], json!(["spend"]));
        assert_eq!(chart["lower_key"], "low");
        assert_eq!(chart["upper_key"], "high");
        assert_eq!(chart["note_key"], "why");
        assert_eq!(chart["x_type"], "time");
        assert_eq!(chart["y_label"], "Spend (USD)");
        // Rows pass through untouched, including a row without bounds.
        assert_eq!(chart["data"], args()["data"]);
        // Optional keys that were not given are absent, not null.
        assert!(chart.get("band_label").is_none());
        assert!(chart.get("x_label").is_none());
    }

    #[tokio::test]
    async fn band_chart_rejects_input_it_cannot_draw() {
        let tool = band_chart().await;
        let with = |edit: fn(&mut Value)| {
            let mut a = args();
            edit(&mut a);
            a
        };
        let cases: Vec<(Value, &str)> = vec![
            (
                with(|a| {
                    a.as_object_mut().unwrap().remove("upper_key");
                }),
                "`upper_key` is required",
            ),
            (with(|a| a["data"] = json!([])), "`data` is empty"),
            (with(|a| a["data"] = json!("rows")), "must be an array"),
            (
                with(|a| a["data"][0] = json!(5)),
                "row 0 of `data` is not an object",
            ),
            (
                with(|a| a["data"][1] = json!({ "spend": 1 })),
                "row 1 has no `date`",
            ),
            (
                with(|a| a["y_key"] = json!("clicks")),
                "no row has a numeric `clicks`",
            ),
            (
                with(|a| a["lower_key"] = json!("floor")),
                "no row has both a numeric `floor` and `high`",
            ),
            (
                with(|a| a["data"][0]["low"] = json!(500)),
                "row 0: `low` (500) is above `high` (120)",
            ),
            (
                with(|a| a["x_type"] = json!("dates")),
                "`x_type` is `dates`; it must be one of category, numeric, time",
            ),
        ];
        for (bad, expected) in cases {
            let err = tool.execute(ctx(), bad).await.unwrap_err().to_string();
            assert!(err.contains(expected), "expected {expected:?} in {err:?}");
        }

        let too_many: Vec<Value> = (0..=MAX_BAND_CHART_ROWS)
            .map(|i| json!({ "date": i, "spend": 1, "low": 0, "high": 2 }))
            .collect();
        let mut a = args();
        a["data"] = Value::Array(too_many);
        let err = tool.execute(ctx(), a).await.unwrap_err().to_string();
        assert!(err.contains("the most a band chart takes is 2000"), "{err}");
    }

    #[test]
    fn numbers_are_read_the_way_the_renderer_reads_them() {
        assert_eq!(as_number(&json!(12.5)), Some(12.5));
        assert_eq!(as_number(&json!("1,234.5")), Some(1234.5));
        assert_eq!(as_number(&json!("$ 12")), Some(12.0));
        assert_eq!(as_number(&json!("8%")), Some(8.0));
        assert_eq!(as_number(&json!("n/a")), None);
        assert_eq!(as_number(&json!("")), None);
        assert_eq!(as_number(&json!(null)), None);
    }
}

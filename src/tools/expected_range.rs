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

//! `expected_range`: for each day of a daily series, the range its value
//! was expected to fall in, from the days before it.
//!
//! The model can do this arithmetic with code, and did: with a different
//! standard deviation from one run to the next, with rows standing in for
//! days when a day had no data, with the account's unfinished current day
//! left in, and with a time-zone offset typed from memory because the code
//! sandbox has no time-zone database. This tool does it the same way every
//! time. It knows nothing about what the series is; its rows go straight to
//! `render_band_chart`.

use std::collections::BTreeMap;
use std::sync::Arc;

use adk_rust::tool::BasicToolset;
use adk_rust::{AdkError, Result, Tool, ToolContext};
use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::{Map, Value, json};

use crate::tools::custom_ui::as_number;

/// Days before a day that make up its baseline, unless `window` says otherwise.
pub const DEFAULT_WINDOW: usize = 28;
/// Half-width of the range in standard deviations, unless `deviations` says otherwise.
pub const DEFAULT_DEVIATIONS: f64 = 2.0;
/// Most days a series may span, after any filling.
pub const MAX_DAYS: usize = 5_000;

/// Computes each day's expected range. See the module documentation.
pub struct ExpectedRangeTool;

fn invalid(message: impl std::fmt::Display) -> String {
    format!("Invalid parameters: {message}")
}

/// The member of `row` named by `key`: the key itself, or a dotted path into
/// nested objects (`metrics.clicks`), the way API results arrive.
fn lookup<'a>(row: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    if let Some(value) = row.get(key) {
        return Some(value);
    }
    let mut parts = key.split('.');
    let mut value = row.get(parts.next()?)?;
    for part in parts {
        value = value.get(part)?;
    }
    Some(value)
}

/// A calendar day from `YYYY-MM-DD`, or from a timestamp that starts with one.
fn parse_day(value: &Value) -> Option<NaiveDate> {
    let text = value.as_str()?.trim();
    let day = match text.char_indices().nth(10) {
        Some((at, 'T' | ' ')) => &text[..at],
        Some(_) => return None,
        None => text,
    };
    NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()
}

/// Rounds to four decimal places, which keeps a rate such as 0.0213 and
/// drops the noise from a sum.
fn rounded(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// A positive whole number argument, or `default` when it is absent.
fn count(
    args: &Value,
    name: &str,
    default: Option<usize>,
) -> std::result::Result<Option<usize>, String> {
    match args.get(name).filter(|v| !v.is_null()) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .filter(|n| *n > 0)
            .map(|n| Some(n as usize))
            .ok_or_else(|| invalid(format!("`{name}` must be a whole number above zero"))),
    }
}

/// The tool's result for `args`, with `now` as the current time.
fn compute(args: &Value, now: DateTime<Utc>) -> std::result::Result<Value, String> {
    let key = |name: &str| {
        args.get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid(format!("`{name}` is required")))
    };
    let date_key = key("date_key")?;
    let value_key = key("value_key")?;
    let window = count(args, "window", Some(DEFAULT_WINDOW))?.unwrap_or(DEFAULT_WINDOW);
    if window < 2 {
        return Err(invalid(
            "`window` must be at least 2: one day has no spread",
        ));
    }
    let last = count(args, "last", None)?;
    let number = |name: &str| match args.get(name).filter(|v| !v.is_null()) {
        None => Ok(None),
        Some(value) => as_number(value)
            .map(Some)
            .ok_or_else(|| invalid(format!("`{name}` must be a number"))),
    };
    let deviations = number("deviations")?.unwrap_or(DEFAULT_DEVIATIONS);
    if deviations <= 0.0 {
        return Err(invalid("`deviations` must be above zero"));
    }
    let floor = number("floor")?;
    let divide_by = number("divide_by")?.unwrap_or(1.0);
    if divide_by == 0.0 {
        return Err(invalid("`divide_by` cannot be zero"));
    }
    let fill = args
        .get("fill_missing_days")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let time_zone = match args.get("time_zone").and_then(Value::as_str).map(str::trim) {
        None | Some("") => None,
        Some(name) => Some(name.parse::<chrono_tz::Tz>().map_err(|_| {
            invalid(format!(
                "`time_zone` is `{name}`, which is not an IANA time zone name such as America/Chicago"
            ))
        })?),
    };

    let rows = args
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("`data` must be an array of row objects"))?;
    if rows.is_empty() {
        return Err(invalid("`data` is empty"));
    }

    let mut days: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        let row = row
            .as_object()
            .ok_or_else(|| invalid(format!("row {index} of `data` is not an object")))?;
        let day = lookup(row, date_key)
            .ok_or_else(|| invalid(format!("row {index} has no `{date_key}`")))
            .and_then(|value| {
                parse_day(value).ok_or_else(|| {
                    invalid(format!(
                        "row {index}: `{date_key}` is {value}, not a day written as YYYY-MM-DD"
                    ))
                })
            })?;
        let value = lookup(row, value_key)
            .and_then(as_number)
            .ok_or_else(|| invalid(format!("row {index} has no numeric `{value_key}`")))?;
        if days.insert(day, value / divide_by).is_some() {
            return Err(invalid(format!(
                "more than one row for {day}; add the rows up to one per day first"
            )));
        }
    }

    // The current day in the data's time zone is still running, and its
    // total so far would read as a drop.
    let mut dropped: Vec<String> = Vec::new();
    if let Some(zone) = time_zone {
        let today = now.with_timezone(&zone).date_naive();
        let incomplete = days.split_off(&today);
        dropped = incomplete.keys().map(NaiveDate::to_string).collect();
    }
    let (Some(first), Some(end)) = (
        days.keys().next().copied(),
        days.keys().next_back().copied(),
    ) else {
        return Err(invalid(
            "no complete day is left: every row is the current day or later in `time_zone`",
        ));
    };

    let span = (end - first).num_days() as usize + 1;
    if span.max(days.len()) > MAX_DAYS {
        return Err(invalid(format!(
            "the series spans {span} days; the most this takes is {MAX_DAYS}"
        )));
    }
    let mut filled: Vec<String> = Vec::new();
    if fill {
        let mut day = first;
        while day <= end {
            days.entry(day).or_insert_with(|| {
                filled.push(day.to_string());
                0.0
            });
            day += Duration::days(1);
        }
    }

    let series: Vec<(NaiveDate, f64)> = days.into_iter().collect();
    let mut out_rows: Vec<Value> = Vec::with_capacity(series.len());
    let mut outside: Vec<Value> = Vec::new();
    let mut without_range = 0usize;
    for (index, (day, value)) in series.iter().enumerate() {
        let mut row = Map::new();
        row.insert("date".into(), json!(day.to_string()));
        row.insert("value".into(), json!(rounded(*value)));
        if index >= window {
            let baseline = &series[index - window..index];
            let mean = baseline.iter().map(|(_, v)| v).sum::<f64>() / window as f64;
            // The sample standard deviation: the baseline is a sample of how
            // the series behaves, not the whole of it.
            let variance = baseline
                .iter()
                .map(|(_, v)| (v - mean).powi(2))
                .sum::<f64>()
                / (window - 1) as f64;
            let spread = deviations * variance.sqrt();
            let lower = floor.map_or(mean - spread, |f| (mean - spread).max(f));
            let upper = mean + spread;
            row.insert("lower".into(), json!(rounded(lower)));
            row.insert("upper".into(), json!(rounded(upper)));
            let side = if *value > upper {
                Some("above")
            } else if *value < lower {
                Some("below")
            } else {
                None
            };
            if let Some(side) = side {
                row.insert("outside".into(), json!(side));
            }
        } else {
            without_range += 1;
        }
        out_rows.push(Value::Object(row));
    }
    if without_range == series.len() {
        return Err(invalid(format!(
            "{} days of data cannot give a range: each day needs the {window} days before it. \
             Fetch a longer period or lower `window`",
            series.len()
        )));
    }

    if let Some(last) = last
        && out_rows.len() > last
    {
        out_rows.drain(..out_rows.len() - last);
    }
    for row in &out_rows {
        if row.get("outside").is_some() {
            outside.push(row.clone());
        }
    }
    let returned_without_range = out_rows.iter().filter(|r| r.get("lower").is_none()).count();

    let mut result = Map::new();
    result.insert("rows".into(), Value::Array(out_rows));
    result.insert("outside".into(), Value::Array(outside));
    result.insert("window".into(), json!(window));
    result.insert("deviations".into(), json!(deviations));
    if returned_without_range > 0 {
        result.insert(
            "rows_without_range".into(),
            json!(format!(
                "{returned_without_range} rows have no lower and upper: fewer than {window} days precede them"
            )),
        );
    }
    if !dropped.is_empty() {
        result.insert("dropped_incomplete_days".into(), json!(dropped));
    }
    if !filled.is_empty() {
        result.insert("days_filled_with_zero".into(), json!(filled));
    }
    Ok(Value::Object(result))
}

#[async_trait]
impl Tool for ExpectedRangeTool {
    fn name(&self) -> &str {
        "expected_range"
    }

    fn description(&self) -> &str {
        "For each day of a daily series, works out the range its value was expected to fall in: \
         the mean of the days before it, plus and minus a number of standard deviations. Returns \
         one row per day with `date`, `value`, `lower` and `upper`, marks the days outside their \
         range, and lists them. Use it instead of code to find unusual days, spikes or drops in \
         any daily metric. It can leave out the current, unfinished day of the data's time zone \
         and count days that have no row as zero. The rows it returns are what render_band_chart \
         draws."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "data": {
                    "type": "array",
                    "description": "One row per day, in any order. Rows can be passed as the API returned them.",
                    "items": { "type": "object" }
                },
                "date_key": { "type": "string", "description": "Key of the day in each row, written YYYY-MM-DD. A dotted path reaches into nested objects, for example segments.date." },
                "value_key": { "type": "string", "description": "Key of the value in each row; a dotted path works here too, for example metrics.clicks. Numbers sent as strings are read as numbers." },
                "window": { "type": "integer", "description": format!("How many days before a day make up its baseline. Default {DEFAULT_WINDOW}. Send at least this many days more than you want ranges for.") },
                "deviations": { "type": "number", "description": format!("Half-width of the range in standard deviations. Default {DEFAULT_DEVIATIONS}.") },
                "last": { "type": "integer", "description": "Return only the last N days; the earlier ones still serve as baseline." },
                "time_zone": { "type": "string", "description": "IANA time zone the days are in, for example America/Chicago. When given, the current day in that zone, which is not over, is left out along with any later day." },
                "fill_missing_days": { "type": "boolean", "description": "Count a day with no row as 0. Right for totals such as spend, clicks or conversions, where no row means nothing happened; wrong for rates and averages such as CTR or CPC. Default false." },
                "divide_by": { "type": "number", "description": "Divide every value by this first, for example 1000000 for amounts in micros." },
                "floor": { "type": "number", "description": "Lowest value the range may take, for example 0 for a metric that cannot be negative." }
            },
            "required": ["data", "date_key", "value_key"]
        }))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, _ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        compute(&args, Utc::now()).map_err(AdkError::tool)
    }
}

/// Builds the toolset of analysis tools that need no code execution.
pub fn analysis_toolset() -> BasicToolset {
    let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(ExpectedRangeTool)];
    BasicToolset::new("analysis", tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn day(offset: i64) -> String {
        (NaiveDate::from_ymd_opt(2026, 10, 1).unwrap() + Duration::days(offset)).to_string()
    }

    /// 2026-11-13 03:05 UTC: still 12 November in Chicago.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 13, 3, 5, 0).unwrap()
    }

    fn run(args: Value) -> Value {
        compute(&args, now()).unwrap()
    }

    fn error(args: Value) -> String {
        compute(&args, now()).unwrap_err()
    }

    #[test]
    fn range_is_the_mean_of_the_window_plus_and_minus_sample_deviations() {
        // Baseline 2, 4, 4, 4, 5, 5, 7, 9: mean 5, sample standard deviation
        // sqrt(32 / 7) = 2.1381.
        let values = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0, 5.0, 20.0, -4.0];
        let data: Vec<Value> = values
            .iter()
            .enumerate()
            .map(|(i, v)| json!({ "d": day(i as i64), "v": v }))
            .collect();
        let out = run(json!({ "data": data, "date_key": "d", "value_key": "v", "window": 8 }));

        let rows = out["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 11);
        // The first eight days have nothing before them to compare with.
        assert!(rows[..8].iter().all(|r| r.get("lower").is_none()));
        assert!(
            out["rows_without_range"]
                .as_str()
                .unwrap()
                .starts_with("8 rows")
        );

        assert_eq!(rows[8]["date"], day(8));
        assert_eq!(rows[8]["value"], 5.0);
        assert_eq!(rows[8]["lower"], 0.7238);
        assert_eq!(rows[8]["upper"], 9.2762);
        assert!(rows[8].get("outside").is_none());

        // Each day's baseline is the eight days before it, so it moves along.
        assert_eq!(rows[9]["outside"], "above");
        assert_eq!(rows[10]["outside"], "below");
        let listed: Vec<&str> = out["outside"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["date"].as_str().unwrap())
            .collect();
        assert_eq!(listed, [day(9), day(10)]);
        assert_eq!(out["window"], 8);
        assert_eq!(out["deviations"], 2.0);

        // A wider range takes the spike back in; a floor lifts the lower bound.
        let wide = run(json!({
            "data": data, "date_key": "d", "value_key": "v", "window": 8,
            "deviations": 9, "floor": 0, "last": 2
        }));
        let rows = wide["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["date"], day(9));
        assert!(rows[0].get("outside").is_none());
        assert_eq!(rows[0]["lower"], 0.0);
        assert_eq!(rows[1]["outside"], "below");
        assert!(wide.get("rows_without_range").is_none());
    }

    /// Rows shaped like a Google Ads search result, for an account in Chicago.
    fn ads_rows(days: i64, skip: &[i64]) -> Vec<Value> {
        (0..days)
            .filter(|i| !skip.contains(i))
            .map(|i| {
                let date = (NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()
                    - Duration::days(days - 1 - i))
                .to_string();
                json!({
                    "segments": { "date": date },
                    "metrics": { "costMicros": ((100 + i % 3) * 1_000_000).to_string() }
                })
            })
            .collect()
    }

    #[test]
    fn the_unfinished_day_of_the_time_zone_is_left_out() {
        let data = ads_rows(40, &[]);
        let args = |zone: Option<&str>| {
            json!({
                "data": data, "date_key": "segments.date", "value_key": "metrics.costMicros",
                "divide_by": 1_000_000, "last": 5, "time_zone": zone
            })
        };

        // In Chicago it is still the 12th, so the 12th is not over.
        let chicago = run(args(Some("America/Chicago")));
        assert_eq!(chicago["dropped_incomplete_days"], json!(["2026-11-12"]));
        let rows = chicago["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[4]["date"], "2026-11-11");
        // Micros arrive as strings and leave as currency units.
        assert_eq!(rows[4]["value"], 102.0);

        // In Sydney the 12th ended hours ago.
        let sydney = run(args(Some("Australia/Sydney")));
        assert!(sydney.get("dropped_incomplete_days").is_none());
        assert_eq!(sydney["rows"][4]["date"], "2026-11-12");

        // No zone, no opinion about which day is over.
        assert_eq!(run(args(None))["rows"][4]["date"], "2026-11-12");

        assert!(error(args(Some("Chicago"))).contains("not an IANA time zone name"));
        let only_today = json!({
            "data": [data[39]], "date_key": "segments.date",
            "value_key": "metrics.costMicros", "time_zone": "America/Chicago"
        });
        assert!(error(only_today).contains("no complete day is left"));
    }

    #[test]
    fn days_with_no_row_count_as_zero_only_when_asked() {
        // Day 35 of 40 has no row.
        let data = ads_rows(40, &[35]);
        let args = |fill: bool| {
            json!({
                "data": data, "date_key": "segments.date", "value_key": "metrics.costMicros",
                "divide_by": 1_000_000, "window": 28, "last": 6, "fill_missing_days": fill
            })
        };

        let filled = run(args(true));
        assert_eq!(filled["days_filled_with_zero"], json!(["2026-11-08"]));
        let rows = filled["rows"].as_array().unwrap();
        assert_eq!(rows[1]["date"], "2026-11-08");
        assert_eq!(rows[1]["value"], 0.0);
        // A day on which nothing was spent is far below the range.
        assert_eq!(rows[1]["outside"], "below");

        let skipped = run(args(false));
        assert!(skipped.get("days_filled_with_zero").is_none());
        let dates: Vec<&str> = skipped["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["date"].as_str().unwrap())
            .collect();
        assert!(!dates.contains(&"2026-11-08"));
        assert!(skipped["outside"].as_array().unwrap().is_empty());
    }

    #[test]
    fn input_it_cannot_use_is_rejected_with_the_reason() {
        let data: Vec<Value> = (0..5).map(|i| json!({ "d": day(i), "v": i })).collect();
        let base = json!({ "data": data, "date_key": "d", "value_key": "v", "window": 3 });
        let with = |edit: fn(&mut Value)| {
            let mut args = base.clone();
            edit(&mut args);
            args
        };
        let cases: Vec<(Value, &str)> = vec![
            (
                with(|a| a["date_key"] = json!("")),
                "`date_key` is required",
            ),
            (with(|a| a["data"] = json!([])), "`data` is empty"),
            (
                with(|a| a["data"][1] = json!(7)),
                "row 1 of `data` is not an object",
            ),
            (
                with(|a| a["data"][1] = json!({ "v": 1 })),
                "row 1 has no `d`",
            ),
            (
                with(|a| a["data"][1]["d"] = json!("11/02/2026")),
                "not a day written as YYYY-MM-DD",
            ),
            (
                with(|a| a["data"][2]["v"] = json!("n/a")),
                "row 2 has no numeric `v`",
            ),
            (
                with(|a| a["data"][2]["d"] = a["data"][1]["d"].clone()),
                "more than one row for 2026-10-02",
            ),
            (
                with(|a| a["window"] = json!(1)),
                "`window` must be at least 2",
            ),
            (with(|a| a["window"] = json!(0)), "whole number above zero"),
            (
                with(|a| a["window"] = json!(5)),
                "each day needs the 5 days before it",
            ),
            (
                with(|a| a["deviations"] = json!(0)),
                "`deviations` must be above zero",
            ),
            (
                with(|a| a["divide_by"] = json!(0)),
                "`divide_by` cannot be zero",
            ),
            (
                with(|a| a["data"][0]["d"] = json!("1990-01-01")),
                "the most this takes is 5000",
            ),
        ];
        for (bad, expected) in cases {
            let message = error(bad);
            assert!(
                message.contains(expected),
                "expected {expected:?} in {message:?}"
            );
        }
    }

    #[test]
    fn days_are_read_from_dates_and_from_timestamps() {
        assert_eq!(
            parse_day(&json!("2026-11-03")),
            NaiveDate::from_ymd_opt(2026, 11, 3)
        );
        assert_eq!(
            parse_day(&json!("2026-11-03T08:30:00Z")),
            NaiveDate::from_ymd_opt(2026, 11, 3)
        );
        assert_eq!(
            parse_day(&json!(" 2026-11-03 00:00:00 ")),
            NaiveDate::from_ymd_opt(2026, 11, 3)
        );
        assert_eq!(parse_day(&json!("2026-11-031")), None);
        assert_eq!(parse_day(&json!("2026-13-01")), None);
        assert_eq!(parse_day(&json!(20261103)), None);
    }

    #[tokio::test]
    async fn the_tool_is_registered_and_runs() {
        use adk_rust::Toolset;
        use adk_rust::tool::SimpleToolContext;
        let ctx = Arc::new(SimpleToolContext::new("analysis-test"));
        let tools = analysis_toolset().tools(ctx.clone()).await.unwrap();
        assert_eq!(tools.len(), 1);
        let tool = &tools[0];
        assert_eq!(tool.name(), "expected_range");
        assert!(tool.is_read_only());
        // Not a render tool: the model has to see which days are outside.
        assert!(!tool.name().starts_with("render_"));

        let data: Vec<Value> = (0..6)
            .map(|i| json!({ "d": day(i), "v": 10 + i % 2 }))
            .collect();
        let out = tool
            .execute(
                ctx.clone(),
                json!({ "data": data, "date_key": "d", "value_key": "v", "window": 4 }),
            )
            .await
            .unwrap();
        assert_eq!(out["rows"].as_array().unwrap().len(), 6);
        let failure = tool
            .execute(
                ctx,
                json!({ "data": [], "date_key": "d", "value_key": "v" }),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(failure.contains("`data` is empty"), "{failure}");
    }
}

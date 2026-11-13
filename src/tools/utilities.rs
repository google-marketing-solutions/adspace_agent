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

//! Utility tools: artifact listing, general-purpose artifact saving (`save_artifact`),
//! and adk-rust's built-in `load_artifacts`.

use std::sync::Arc;

use adk_rust::tool::{BasicToolset, LoadArtifactsTool};
use adk_rust::{Part, Result, Tool, ToolContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Maximum size of a text/data artifact saved by [`SaveArtifactTool`] (32 MiB).
pub const MAX_ARTIFACT_BYTES: usize = 32 * 1024 * 1024;

/// Maximum number of preview characters returned to the model / UI.
const ARTIFACT_PREVIEW_CHARS: usize = 2000;

/// Maximum sanitized filename length.
const MAX_FILENAME_LEN: usize = 128;

/// Internal session artifact names that user/model tool calls must not overwrite.
const RESERVED_ARTIFACT_NAMES: &[&str] = &["cm360_campaign_payload.json"];

/// Lists the artifacts available to the user in the current session.
pub struct ListUserFilesTool;

#[async_trait]
impl Tool for ListUserFilesTool {
    fn name(&self) -> &str {
        "list_user_files"
    }

    fn description(&self) -> &str {
        "Tool to list available artifacts (uploaded or generated files) for the user in this session."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, _args: Value) -> Result<Value> {
        let Some(artifacts) = ctx.artifacts() else {
            tracing::warn!("Error listing artifacts: artifact service is not configured.");
            return Ok(Value::String(
                "Error: Could not list artifacts.".to_string(),
            ));
        };
        match artifacts.list().await {
            Ok(files) if files.is_empty() => {
                Ok(Value::String("You have no saved artifacts.".to_string()))
            }
            Ok(files) => {
                let listing = files
                    .iter()
                    .map(|f| format!("- {f}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(Value::String(format!(
                    "Here are your available artifacts:\n{listing}"
                )))
            }
            Err(e) => {
                tracing::warn!("An unexpected error occurred during artifact list: {e}");
                Ok(Value::String(
                    "Error: An unexpected error occurred while listing artifacts.".to_string(),
                ))
            }
        }
    }
}

/// Infers the canonical `(extension, mime_type)` for a filename and optional MIME hint.
pub fn infer_format(
    raw_filename: Option<&str>,
    raw_mime: Option<&str>,
    has_rows: bool,
    content_is_structured_json: bool,
) -> (&'static str, &'static str) {
    let ext = raw_filename
        .and_then(|name| name.rsplit(['/', '\\']).next())
        .and_then(|base| base.rsplit_once('.'))
        .map(|(_, ext)| ext.trim().to_ascii_lowercase());

    match ext.as_deref() {
        Some("csv") => return ("csv", "text/csv"),
        Some("tsv") => return ("tsv", "text/tab-separated-values"),
        Some("json") => return ("json", "application/json"),
        Some("md" | "markdown") => return ("md", "text/markdown"),
        Some("txt" | "log") => return ("txt", "text/plain"),
        Some("html" | "htm") => return ("html", "text/html"),
        Some("xml") => return ("xml", "application/xml"),
        Some("yaml" | "yml") => return ("yaml", "application/x-yaml"),
        Some("sql") => return ("sql", "application/sql"),
        _ => {}
    }

    let mime = raw_mime
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match mime.as_str() {
        "text/csv" | "application/csv" => ("csv", "text/csv"),
        "text/tab-separated-values" | "text/tsv" => ("tsv", "text/tab-separated-values"),
        "application/json" => ("json", "application/json"),
        "text/markdown" => ("md", "text/markdown"),
        "text/html" => ("html", "text/html"),
        "application/xml" | "text/xml" => ("xml", "application/xml"),
        "application/x-yaml" | "text/yaml" => ("yaml", "application/x-yaml"),
        "application/sql" | "text/x-sql" => ("sql", "application/sql"),
        "text/plain" => ("txt", "text/plain"),
        _ if has_rows => ("csv", "text/csv"),
        _ if content_is_structured_json => ("json", "application/json"),
        _ => ("txt", "text/plain"),
    }
}

/// Sanitizes a user/model-supplied filename into a safe basename with `default_ext`.
pub fn sanitize_artifact_filename(raw: Option<&str>, default_ext: &str) -> String {
    let base = raw
        .unwrap_or_default()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();

    let known_exts = [
        "csv", "tsv", "json", "md", "markdown", "txt", "log", "html", "htm", "xml", "yaml", "yml",
        "sql",
    ];
    let (raw_stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) if known_exts.contains(&e.trim().to_ascii_lowercase().as_str()) => {
            let normalized_ext = match e.trim().to_ascii_lowercase().as_str() {
                "markdown" => "md".to_string(),
                "htm" => "html".to_string(),
                "yml" => "yaml".to_string(),
                other => other.to_string(),
            };
            (s, normalized_ext)
        }
        _ => (base, default_ext.to_string()),
    };

    let mut clean = String::with_capacity(raw_stem.len());
    let mut prev_underscore = false;
    for ch in raw_stem.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '.' {
            clean.push(ch);
            prev_underscore = false;
        } else if !prev_underscore {
            clean.push('_');
            prev_underscore = true;
        }
    }
    let clean = clean.trim_matches(['.', '_', '-']);
    let candidate = if clean.is_empty() {
        let id = uuid::Uuid::new_v4().simple().to_string();
        format!("artifact_{}.{ext}", &id[..8])
    } else {
        let max_stem = MAX_FILENAME_LEN.saturating_sub(ext.len() + 1);
        let truncated: String = clean.chars().take(max_stem).collect();
        format!("{truncated}.{ext}")
    };

    if RESERVED_ARTIFACT_NAMES
        .iter()
        .any(|r| r.eq_ignore_ascii_case(&candidate))
    {
        let id = uuid::Uuid::new_v4().simple().to_string();
        format!("user_{}_{candidate}", &id[..6])
    } else {
        candidate
    }
}

/// Formats a single JSON value as an unescaped cell string.
fn cell_to_string(val: &Value) -> String {
    match val {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Escapes a delimited field (RFC 4180 for CSV `,`, or quoting for TSV `\t`).
fn escape_delimited_cell(cell: &str, delimiter: char) -> String {
    if cell.contains([delimiter, '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

/// Builds a delimited line from an iterator of cell strings.
fn join_delimited_row<'a>(cells: impl IntoIterator<Item = &'a str>, delimiter: char) -> String {
    let sep = delimiter.to_string();
    cells
        .into_iter()
        .map(|c| escape_delimited_cell(c, delimiter))
        .collect::<Vec<_>>()
        .join(&sep)
}

/// Serializes `rows` (and optional `headers`) as CSV or TSV.
fn build_delimited_from_rows(
    args: &Value,
    delimiter: char,
) -> std::result::Result<(String, usize), String> {
    let explicit_headers: Option<Vec<String>> = args
        .get("headers")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(cell_to_string).collect())
        .filter(|v: &Vec<String>| !v.is_empty());

    let rows = args.get("rows").and_then(Value::as_array);
    let Some(rows) = rows else {
        if let Some(headers) = explicit_headers {
            let mut out = join_delimited_row(headers.iter().map(String::as_str), delimiter);
            out.push('\n');
            return Ok((out, 0));
        }
        return Err("Provide either 'content' (text/JSON) or 'rows' (for CSV/TSV).".to_string());
    };

    if rows.is_empty() {
        if let Some(headers) = explicit_headers {
            let mut out = join_delimited_row(headers.iter().map(String::as_str), delimiter);
            out.push('\n');
            return Ok((out, 0));
        }
        return Err(
            "'rows' must not be empty when 'headers' and 'content' are omitted.".to_string(),
        );
    }

    let mut lines: Vec<String> = Vec::with_capacity(rows.len() + 1);
    if rows.iter().all(Value::is_object) {
        let headers = explicit_headers.unwrap_or_else(|| {
            let mut cols: Vec<String> = Vec::new();
            for row in rows {
                if let Some(obj) = row.as_object() {
                    for key in obj.keys() {
                        if !cols.contains(key) {
                            cols.push(key.clone());
                        }
                    }
                }
            }
            cols
        });
        lines.push(join_delimited_row(
            headers.iter().map(String::as_str),
            delimiter,
        ));
        for row in rows {
            let obj = row.as_object().expect("checked is_object");
            let rendered: Vec<String> = headers
                .iter()
                .map(|col| obj.get(col).map_or_else(String::new, cell_to_string))
                .collect();
            lines.push(join_delimited_row(
                rendered.iter().map(String::as_str),
                delimiter,
            ));
        }
    } else {
        if let Some(ref headers) = explicit_headers {
            lines.push(join_delimited_row(
                headers.iter().map(String::as_str),
                delimiter,
            ));
        }
        for row in rows {
            match row {
                Value::Array(arr) => {
                    let rendered: Vec<String> = arr.iter().map(cell_to_string).collect();
                    lines.push(join_delimited_row(
                        rendered.iter().map(String::as_str),
                        delimiter,
                    ));
                }
                Value::Object(obj) => {
                    let rendered: Vec<String> = obj.values().map(cell_to_string).collect();
                    lines.push(join_delimited_row(
                        rendered.iter().map(String::as_str),
                        delimiter,
                    ));
                }
                primitive => {
                    let s = cell_to_string(primitive);
                    lines.push(escape_delimited_cell(&s, delimiter));
                }
            }
        }
    }

    let mut out = lines.join("\n");
    out.push('\n');
    Ok((out, rows.len()))
}

/// Builds the artifact text, filename, MIME type, and optional row count from [`SaveArtifactTool`] arguments.
pub fn build_artifact_from_args(
    args: &Value,
) -> std::result::Result<(String, &'static str, String, Option<usize>), String> {
    let raw_filename = args.get("filename").and_then(Value::as_str);
    let raw_mime = args.get("mime_type").and_then(Value::as_str);
    let content_val = args.get("content").or_else(|| args.get("csv_content"));
    let has_rows = args
        .get("rows")
        .and_then(Value::as_array)
        .is_some_and(|r| !r.is_empty())
        || args
            .get("headers")
            .and_then(Value::as_array)
            .is_some_and(|h| !h.is_empty());
    let content_is_structured_json = content_val.is_some_and(|v| v.is_object() || v.is_array());

    let (ext, mime_type) =
        infer_format(raw_filename, raw_mime, has_rows, content_is_structured_json);
    let filename = sanitize_artifact_filename(raw_filename, ext);

    // 1. If `content` (or `csv_content`) is a non-empty string, use it directly
    //    (pretty-printing if target is JSON and the string parses as JSON).
    if let Some(raw_str) = content_val.and_then(Value::as_str)
        && !raw_str.trim().is_empty()
    {
        let normalized = raw_str.replace("\r\n", "\n");
        if ext == "json"
            && let Ok(parsed) = serde_json::from_str::<Value>(&normalized)
            && let Ok(pretty) = serde_json::to_string_pretty(&parsed)
        {
            return Ok((filename, mime_type, format!("{pretty}\n"), None));
        }
        let mut text = normalized;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        let row_count =
            (ext == "csv" || ext == "tsv").then(|| text.lines().count().saturating_sub(1));
        return Ok((filename, mime_type, text, row_count));
    }

    // 2. If `content` is a structured JSON object/array:
    if let Some(structured) = content_val.filter(|v| v.is_object() || v.is_array()) {
        if (ext == "csv" || ext == "tsv")
            && let Some(arr) = structured.as_array()
        {
            let synthetic = json!({
                "headers": args.get("headers").cloned().unwrap_or(Value::Null),
                "rows": arr,
            });
            let delimiter = if ext == "tsv" { '\t' } else { ',' };
            let (text, rows) = build_delimited_from_rows(&synthetic, delimiter)?;
            return Ok((filename, mime_type, text, Some(rows)));
        }
        let pretty = serde_json::to_string_pretty(structured)
            .map_err(|e| format!("Failed to serialize JSON content: {e}"))?;
        return Ok((filename, mime_type, format!("{pretty}\n"), None));
    }

    // 3. If `rows` (or `headers`) was passed:
    if has_rows {
        if ext == "json" {
            let rows_val = args.get("rows").cloned().unwrap_or_else(|| json!([]));
            let pretty = serde_json::to_string_pretty(&rows_val)
                .map_err(|e| format!("Failed to serialize rows as JSON: {e}"))?;
            let count = rows_val.as_array().map(Vec::len);
            return Ok((filename, mime_type, format!("{pretty}\n"), count));
        }
        let delimiter = if ext == "tsv" { '\t' } else { ',' };
        let (text, rows) = build_delimited_from_rows(args, delimiter)?;
        return Ok((filename, mime_type, text, Some(rows)));
    }

    Err(
        "Provide either non-empty 'content' (text, Markdown, CSV, or JSON) or 'rows' (array of objects or arrays for CSV/TSV)."
            .to_string(),
    )
}

/// Saves a text or data file (CSV, TSV, JSON, Markdown, TXT, SQL, YAML, HTML, XML)
/// as a downloadable session artifact.
pub struct SaveArtifactTool;

#[async_trait]
impl Tool for SaveArtifactTool {
    fn name(&self) -> &str {
        "save_artifact"
    }

    fn description(&self) -> &str {
        "Saves a file (CSV, TSV, JSON, Markdown .md, plain text .txt, SQL, YAML, HTML, or XML) \
         as a downloadable session artifact so the user can view or download it from the Canvas \
         and chat. Pass 'filename' (e.g. 'report.csv', 'summary.md', 'data.json', 'notes.txt') \
         and either 'content' (raw text/Markdown/CSV/JSON string, or a JSON object/array) or \
         structured 'rows' (with optional 'headers') when generating a CSV/TSV."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "filename": {
                    "type": "string",
                    "description": "Filename with extension, e.g. 'campaign_performance.csv', 'executive_summary.md', 'payload.json', or 'notes.txt'."
                },
                "content": {
                    "description": "File content to save: a string (for CSV, Markdown, TXT, JSON, SQL, etc.) or a structured JSON object/array (for .json files). Provide either 'content' or 'rows'."
                },
                "rows": {
                    "type": "array",
                    "items": {},
                    "description": "For CSV/TSV (or JSON array) artifacts: an array of row objects or row cell arrays to serialize."
                },
                "headers": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional column header names when building a CSV/TSV from 'rows'. Inferred from object keys when 'rows' are objects."
                },
                "mime_type": {
                    "type": "string",
                    "description": "Optional MIME type override (e.g. 'text/csv', 'application/json', 'text/markdown', 'text/plain'). Automatically inferred from 'filename' when omitted."
                }
            },
            "required": ["filename"]
        }))
    }

    /// Writes a session artifact (a side effect), so it is not marked
    /// `is_read_only`, but each invocation writes its own artifact and action
    /// delta so concurrent calls are safe.
    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        let Some(artifacts) = ctx.artifacts() else {
            return Ok(json!({
                "status": "ERROR",
                "error_details": "Artifact service is not configured for this session."
            }));
        };

        let (filename, mime_type, text, row_count) = match build_artifact_from_args(&args) {
            Ok(built) => built,
            Err(msg) => {
                return Ok(json!({
                    "status": "ERROR",
                    "error_details": msg
                }));
            }
        };

        let bytes = text.len();
        if bytes > MAX_ARTIFACT_BYTES {
            return Ok(json!({
                "status": "ERROR",
                "error_details": format!(
                    "Artifact content is {bytes} bytes, exceeding the {MAX_ARTIFACT_BYTES}-byte limit."
                )
            }));
        }

        let preview: String = text.chars().take(ARTIFACT_PREVIEW_CHARS).collect();
        let part = Part::Text { text };

        match artifacts.save(&filename, &part).await {
            Ok(version) => {
                let mut actions = ctx.actions();
                actions.artifact_delta.insert(filename.clone(), version);
                ctx.set_actions(actions);
                tracing::info!(
                    "Saved artifact '{filename}' ({mime_type}, {bytes} bytes, version {version})"
                );
                let mut resp = json!({
                    "status": "SUCCESS",
                    "message": format!("Saved artifact '{filename}'."),
                    "filename": filename,
                    "mime_type": mime_type,
                    "bytes": bytes,
                    "preview": preview
                });
                if let Some(rows) = row_count {
                    resp["rows"] = json!(rows);
                }
                Ok(resp)
            }
            Err(e) => {
                tracing::warn!("Failed to save artifact '{filename}': {e}");
                Ok(json!({
                    "status": "ERROR",
                    "error_details": format!("Failed to save artifact '{filename}': {e}")
                }))
            }
        }
    }
}

/// Builds the utilities toolset (an adk-rust [`BasicToolset`] named
/// `utilities`): `list_user_files`, `save_artifact`, plus the built-in `load_artifacts`.
pub fn utilities_toolset() -> BasicToolset {
    BasicToolset::new(
        "utilities",
        vec![
            Arc::new(ListUserFilesTool),
            Arc::new(SaveArtifactTool),
            Arc::new(LoadArtifactsTool::new()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_artifact_filenames_across_formats() {
        assert_eq!(
            sanitize_artifact_filename(Some("campaign_report.csv"), "csv"),
            "campaign_report.csv"
        );
        assert_eq!(
            sanitize_artifact_filename(Some("brief.markdown"), "md"),
            "brief.md"
        );
        assert_eq!(
            sanitize_artifact_filename(Some("config.json"), "json"),
            "config.json"
        );
        assert_eq!(
            sanitize_artifact_filename(Some("notes.txt"), "txt"),
            "notes.txt"
        );
        assert_eq!(
            sanitize_artifact_filename(Some("q4_spend"), "csv"),
            "q4_spend.csv"
        );
        assert_eq!(
            sanitize_artifact_filename(Some("../../etc/passwd"), "txt"),
            "passwd.txt"
        );
        // Reserved internal artifact name cannot be overwritten directly.
        let escaped = sanitize_artifact_filename(Some("cm360_campaign_payload.json"), "json");
        assert_ne!(escaped, "cm360_campaign_payload.json");
        assert!(escaped.ends_with("_cm360_campaign_payload.json"));
    }

    #[test]
    fn builds_csv_json_markdown_and_txt_artifacts() {
        // 1. Structured rows -> CSV
        let (fname, mime, csv, rows) = build_artifact_from_args(&json!({
            "filename": "spend.csv",
            "rows": [
                { "campaign": "Summer, \"Sale\"", "clicks": 120, "cost": 45.5 },
                { "campaign": "Brand\nSearch", "clicks": 80, "cost": 19.25 }
            ]
        }))
        .unwrap();
        assert_eq!(fname, "spend.csv");
        assert_eq!(mime, "text/csv");
        assert_eq!(rows, Some(2));
        assert_eq!(
            csv,
            "campaign,clicks,cost\n\"Summer, \"\"Sale\"\"\",120,45.5\n\"Brand\nSearch\",80,19.25\n"
        );

        // 2. Structured object -> pretty JSON
        let (fname, mime, json_text, rows) = build_artifact_from_args(&json!({
            "filename": "payload.json",
            "content": { "account_id": "123-456-7890", "active": true }
        }))
        .unwrap();
        assert_eq!(fname, "payload.json");
        assert_eq!(mime, "application/json");
        assert_eq!(rows, None);
        assert!(json_text.contains("\"account_id\": \"123-456-7890\""));

        // 3. Markdown -> text/markdown
        let (fname, mime, md_text, _) = build_artifact_from_args(&json!({
            "filename": "summary.md",
            "content": "# Q4 Plan\n\n- Search\n- YouTube"
        }))
        .unwrap();
        assert_eq!(fname, "summary.md");
        assert_eq!(mime, "text/markdown");
        assert_eq!(md_text, "# Q4 Plan\n\n- Search\n- YouTube\n");

        // 4. Plain text -> text/plain
        let (fname, mime, txt_text, _) = build_artifact_from_args(&json!({
            "filename": "notes.txt",
            "content": "Action items for next week"
        }))
        .unwrap();
        assert_eq!(fname, "notes.txt");
        assert_eq!(mime, "text/plain");
        assert_eq!(txt_text, "Action items for next week\n");

        assert!(build_artifact_from_args(&json!({ "filename": "empty.txt" })).is_err());
    }
}

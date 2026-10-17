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

//! BigQuery toolset acting as the **signed-in user**.
//!
//! The four tools (`bigquery_list_datasets`, `bigquery_list_tables`,
//! `bigquery_get_table_schema`, `bigquery_execute_sql`) keep the names,
//! argument schemas and result shapes of adk-rust's `BigQueryToolset`, but
//! call the [BigQuery REST API v2](https://cloud.google.com/bigquery/docs/reference/rest)
//! with the chat session's OAuth token from [`SessionCredentialManager`] —
//! exactly like the Discovery toolsets and the CM360 trafficking tools — so a
//! user only ever sees the datasets their own Google account may read, and a
//! missing grant surfaces as `AUTH_REQUIRED` (opening the consent popup)
//! rather than falling back to the service account.
//!
//! Read-only mode (the default, `BIGQUERY_READ_ONLY=true`) asks for the
//! `bigquery.readonly` scope and lets `bigquery_execute_sql` run a single
//! `SELECT` / `WITH … SELECT` statement only: a lexical check rejects anything
//! else before any request is sent, and a dry run must classify the statement
//! as `SELECT` before it executes.

use std::sync::Arc;
use std::time::Duration;

use adk_rust::{ReadonlyContext, Result, Tool, ToolContext, Toolset};
use async_trait::async_trait;
use serde_json::{Map, Value, json};

use crate::auth::{SessionCredentialManager, auth_required};

/// Scope requested when writes are allowed.
pub const BIGQUERY_SCOPE: &str = "https://www.googleapis.com/auth/bigquery";
/// Scope requested in read-only mode.
pub const BIGQUERY_READ_ONLY_SCOPE: &str = "https://www.googleapis.com/auth/bigquery.readonly";
/// Base URL of the BigQuery REST API v2.
pub const DEFAULT_BASE_URL: &str = "https://bigquery.googleapis.com/bigquery/v2";
/// Default maximum number of result rows returned by `bigquery_execute_sql`.
pub const DEFAULT_MAX_RESULTS: u64 = 1000;
/// Hard cap on `max_results`, whatever the caller asks for.
pub const MAX_RESULT_ROWS: u64 = 10_000;
/// Largest single API response body handed to the model (per page).
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
/// Most datasets / tables returned by the listing tools.
const MAX_LISTED_ITEMS: usize = 1000;
/// How long one `jobs.query` / `jobs.getQueryResults` call waits for the
/// job server-side before returning `jobComplete: false`.
const QUERY_TIMEOUT_MS: u64 = 30_000;
/// How long a query may run in total before the tool gives up on it.
const QUERY_DEADLINE: Duration = Duration::from_secs(5 * 60);
/// Overall request timeout (connect + send + receive).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// TCP/TLS connect timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The statement type a BigQuery dry run reports for a query that only reads.
const SELECT_STATEMENT_TYPE: &str = "SELECT";

/// What the toolset's tools share: credentials, defaults and the HTTP client.
#[derive(Clone)]
struct BigQueryShared {
    credentials: Arc<SessionCredentialManager>,
    project_id: Option<String>,
    read_only: bool,
    base_url: String,
    http: reqwest::Client,
}

/// A failed BigQuery API call (HTTP error or oversized body).
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
struct ApiFailure {
    /// HTTP status, when the server answered.
    status: Option<u16>,
    /// Human-readable message (BigQuery's own `error.message` when present).
    message: String,
    /// The error body BigQuery returned, when it was JSON.
    details: Option<Value>,
}

/// Builds the shared `reqwest` client with sane timeouts.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .unwrap_or_else(|e| {
            tracing::warn!("Falling back to a default reqwest client without timeouts: {e}");
            reqwest::Client::new()
        })
}

impl BigQueryShared {
    /// OAuth scopes the tools ask the session for.
    fn scopes(&self) -> Vec<String> {
        vec![
            if self.read_only {
                BIGQUERY_READ_ONLY_SCOPE
            } else {
                BIGQUERY_SCOPE
            }
            .to_string(),
        ]
    }

    /// The signed-in user's access token (or an `AuthRequired` error).
    async fn token(&self, ctx: &Arc<dyn ToolContext>) -> anyhow::Result<String> {
        self.credentials
            .access_token(ctx.as_ref(), &self.scopes())
            .await
    }

    /// The project a call targets: the `project_id` argument, else the default.
    fn project_id(&self, args: &Value) -> anyhow::Result<String> {
        if let Some(project) = args["project_id"].as_str().filter(|p| !p.is_empty()) {
            return Ok(validated_id("project_id", project)?.to_string());
        }
        self.project_id.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Missing required parameter 'project_id'. Either provide it in the tool \
                 arguments or set GOOGLE_CLOUD_PROJECT."
            )
        })
    }

    async fn request(
        &self,
        token: &str,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> anyhow::Result<Value> {
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), path);
        let mut req = self
            .http
            .request(method.clone(), &url)
            .bearer_auth(token)
            .header(
                reqwest::header::USER_AGENT,
                format!(
                    "adspace-agent/{} adk-rust (toolset: bigquery)",
                    env!("CARGO_PKG_VERSION")
                ),
            )
            .header(reqwest::header::ACCEPT, "application/json");
        if !query.is_empty() {
            req = req.query(query);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        tracing::info!("Calling {method} {url}");
        let resp = req.send().await?;
        let status = resp.status();
        let bytes = read_body_limited(resp, MAX_RESPONSE_BYTES)
            .await?
            .ok_or_else(|| ApiFailure {
                status: Some(status.as_u16()),
                message: format!(
                    "{method} {url} returned more than {MAX_RESPONSE_BYTES} bytes, which is too much \
                     to hand to the model. Ask for fewer rows (max_results) or narrow the query."
                ),
                details: None,
            })?;
        let text = String::from_utf8_lossy(&bytes);
        let parsed: Option<Value> = serde_json::from_str(&text).ok();
        if status.is_success() {
            return parsed.ok_or_else(|| {
                anyhow::anyhow!("{method} {url} returned a non-JSON body: {}", text.trim())
            });
        }
        let message = parsed
            .as_ref()
            .and_then(|v| v["error"]["message"].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| text.trim().to_string());
        Err(ApiFailure {
            status: Some(status.as_u16()),
            message: format!("{method} {url} failed with HTTP {status}: {message}"),
            details: parsed,
        }
        .into())
    }

    async fn get(
        &self,
        token: &str,
        path: &str,
        query: &[(&str, String)],
    ) -> anyhow::Result<Value> {
        self.request(token, reqwest::Method::GET, path, query, None)
            .await
    }

    async fn post(&self, token: &str, path: &str, body: &Value) -> anyhow::Result<Value> {
        self.request(token, reqwest::Method::POST, path, &[], Some(body))
            .await
    }
}

/// Reads a response body, stopping (with `None`) as soon as it exceeds
/// `limit` bytes. Honours `Content-Length` up front to avoid buffering at all.
async fn read_body_limited(
    resp: reqwest::Response,
    limit: usize,
) -> anyhow::Result<Option<Vec<u8>>> {
    use futures::StreamExt as _;
    if resp.content_length().is_some_and(|len| len > limit as u64) {
        return Ok(None);
    }
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if buf.len() + chunk.len() > limit {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(Some(buf))
}

/// Rejects identifiers that could escape their URL path segment.
fn validated_id<'a>(name: &str, value: &'a str) -> anyhow::Result<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("Missing required parameter '{name}'");
    }
    if value
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '/' | '?' | '#' | '%' | '\\'))
    {
        anyhow::bail!("Invalid {name} {value:?}: it must be a plain BigQuery identifier");
    }
    Ok(value)
}

/// A required string argument.
fn required_str<'a>(args: &'a Value, name: &str) -> anyhow::Result<&'a str> {
    let value = args[name]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter '{name}'"))?;
    validated_id(name, value)
}

/// The JSON a tool returns when a call fails (the model reads it).
fn failure_json(e: &anyhow::Error) -> Value {
    match e.downcast_ref::<ApiFailure>() {
        Some(failure) => {
            let mut error = Map::new();
            if let Some(status) = failure.status {
                error.insert("status".into(), json!(status));
                if status == 401 || status == 403 {
                    error.insert(
                        "hint".into(),
                        json!(
                            "The signed-in Google account lacks access. Ask the user to grant \
                             BigQuery access to the project/dataset, or to authorize again."
                        ),
                    );
                }
            }
            error.insert("message".into(), json!(failure.message));
            if let Some(details) = &failure.details {
                error.insert("details".into(), details.clone());
            }
            json!({ "error": error })
        }
        None => json!({ "error": { "message": format!("{e:#}") } }),
    }
}

/// Runs `call`, turning a missing grant into `AUTH_REQUIRED` and any other
/// failure into an `error` object for the model.
async fn surface(call: impl Future<Output = anyhow::Result<Value>>) -> Result<Value> {
    match call.await {
        Ok(value) => Ok(value),
        Err(e) => {
            if let Some(required) = auth_required(&e) {
                return Ok(required.to_tool_response());
            }
            Ok(failure_json(&e))
        }
    }
}

const PROJECT_ID_DESCRIPTION: &str =
    "The Google Cloud project ID. Uses the toolset default if not provided.";

// ---------------------------------------------------------------------------
// Read-only SQL guard
// ---------------------------------------------------------------------------

/// Rejects SQL that is not a single read-only query, before any request is sent.
///
/// The check is lexical and conservative: comments are skipped, the contents of
/// string literals and quoted identifiers are ignored, the first keyword must be
/// `SELECT` or `WITH` (optionally after opening parentheses), and only comments may
/// follow a `;`. The dry run in [`BigQueryExecuteSql`] is the authoritative check;
/// this one keeps a statement that is plainly not a query from reaching BigQuery.
///
/// # Errors
///
/// Returns the reason the statement was refused.
pub fn check_read_only_sql(sql: &str) -> std::result::Result<(), String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    let mut started = false;
    let mut terminated = false;
    while let Some(&c) = chars.get(i) {
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
        } else if c == '#' || (c == '-' && next == Some('-')) {
            while chars.get(i).is_some_and(|&c| c != '\n') {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let close = (i + 2..chars.len().saturating_sub(1))
                .find(|&j| chars[j] == '*' && chars[j + 1] == '/')
                .ok_or_else(|| "the query has an unterminated comment".to_string())?;
            i = close + 2;
        } else if terminated {
            return Err("multiple statements are not allowed".to_string());
        } else if c == ';' {
            terminated = true;
            i += 1;
        } else if !started && c == '(' {
            i += 1;
        } else if !started {
            let end = (i..chars.len())
                .find(|&j| !(chars[j].is_alphanumeric() || chars[j] == '_'))
                .unwrap_or(chars.len());
            let keyword: String = chars[i..end].iter().collect();
            if !(keyword.eq_ignore_ascii_case("SELECT") || keyword.eq_ignore_ascii_case("WITH")) {
                let found = if keyword.is_empty() {
                    c.to_string()
                } else {
                    keyword.to_uppercase()
                };
                return Err(format!("it starts with {found}, not SELECT or WITH"));
            }
            started = true;
            i = end;
        } else if matches!(c, '\'' | '"' | '`') {
            // A string literal or quoted identifier; its contents are not SQL.
            let triple = c != '`' && next == Some(c) && chars.get(i + 2) == Some(&c);
            let width = if triple { 3 } else { 1 };
            let mut j = i + width;
            loop {
                match chars.get(j) {
                    None => {
                        return Err(
                            "the query has an unterminated string or quoted identifier".to_string()
                        );
                    }
                    Some('\\') => j += 2,
                    Some(&q) if q == c && (!triple || chars.get(j + 1..j + 3) == Some(&[c, c])) => {
                        break;
                    }
                    Some(_) => j += 1,
                }
            }
            i = j + width;
        } else {
            i += 1;
        }
    }
    if started {
        Ok(())
    } else {
        Err("the query is empty".to_string())
    }
}

/// The error returned when read-only mode refuses a statement.
fn read_only_violation(reason: &str) -> Value {
    json!({
        "error": {
            "code": "READ_ONLY_VIOLATION",
            "message": format!(
                "bigquery_execute_sql is read-only and refused the query: {reason}. Send a single \
                 SELECT or WITH query (set BIGQUERY_READ_ONLY=false on the server to allow writes)."
            ),
        }
    })
}

// ---------------------------------------------------------------------------
// Result conversion
// ---------------------------------------------------------------------------

/// Converts one BigQuery cell (`{"v": …}` already unwrapped) to plain JSON
/// using its schema field: scalars arrive as strings, records as `{"f": […]}`
/// and repeated fields as `[{"v": …}, …]`.
fn cell_to_json(field: &Value, value: &Value) -> Value {
    if field["mode"].as_str() == Some("REPEATED")
        && let Some(items) = value.as_array()
    {
        let scalar = json!({ "name": field["name"], "type": field["type"] });
        return Value::Array(
            items
                .iter()
                .map(|item| cell_to_json(&scalar, &item["v"]))
                .collect(),
        );
    }
    if value.is_null() {
        return Value::Null;
    }
    match field["type"].as_str().unwrap_or("STRING") {
        "RECORD" | "STRUCT" => {
            let fields = field["fields"].as_array().cloned().unwrap_or_default();
            let cells = value["f"].as_array().cloned().unwrap_or_default();
            let mut object = Map::new();
            for (sub_field, cell) in fields.iter().zip(cells.iter()) {
                let name = sub_field["name"].as_str().unwrap_or("").to_string();
                object.insert(name, cell_to_json(sub_field, &cell["v"]));
            }
            Value::Object(object)
        }
        "INTEGER" | "INT64" => value
            .as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .map_or_else(|| value.clone(), Value::from),
        "FLOAT" | "FLOAT64" => value
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .and_then(|f| serde_json::Number::from_f64(f).map(Value::Number))
            .unwrap_or_else(|| value.clone()),
        "BOOLEAN" | "BOOL" => match value.as_str() {
            Some(s) if s.eq_ignore_ascii_case("true") => Value::Bool(true),
            Some(s) if s.eq_ignore_ascii_case("false") => Value::Bool(false),
            _ => value.clone(),
        },
        _ => value.clone(),
    }
}

/// Converts a page of `rows` (`[{"f": [{"v": …}, …]}, …]`) to row objects.
fn rows_to_json(schema_fields: &[Value], rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            let cells = row["f"].as_array().cloned().unwrap_or_default();
            let mut object = Map::new();
            for (field, cell) in schema_fields.iter().zip(cells.iter()) {
                let name = field["name"].as_str().unwrap_or("").to_string();
                object.insert(name, cell_to_json(field, &cell["v"]));
            }
            Value::Object(object)
        })
        .collect()
}

/// Flattens a table schema into `{name, type, mode, description[, fields]}`.
fn schema_columns(fields: &[Value]) -> Vec<Value> {
    fields
        .iter()
        .map(|f| {
            let mut column = json!({
                "name": f["name"],
                "type": f["type"],
                "mode": f["mode"].as_str().unwrap_or("NULLABLE"),
                "description": f["description"],
            });
            if let Some(nested) = f["fields"].as_array()
                && !nested.is_empty()
            {
                column["fields"] = Value::Array(schema_columns(nested));
            }
            column
        })
        .collect()
}

// ---------------------------------------------------------------------------
// bigquery_list_datasets
// ---------------------------------------------------------------------------

/// List available datasets in a BigQuery project.
struct BigQueryListDatasets {
    shared: Arc<BigQueryShared>,
}

impl BigQueryListDatasets {
    async fn run(&self, ctx: &Arc<dyn ToolContext>, args: Value) -> anyhow::Result<Value> {
        let token = self.shared.token(ctx).await?;
        let project_id = self.shared.project_id(&args)?;
        let path = format!("projects/{project_id}/datasets");
        let mut datasets = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![("maxResults", MAX_LISTED_ITEMS.to_string())];
            if let Some(token) = &page_token {
                query.push(("pageToken", token.clone()));
            }
            let page = self.shared.get(&token, &path, &query).await?;
            for ds in page["datasets"].as_array().into_iter().flatten() {
                datasets.push(json!({
                    "dataset_id": ds["datasetReference"]["datasetId"],
                    "friendly_name": ds["friendlyName"].as_str().unwrap_or(""),
                    "location": ds["location"],
                }));
            }
            page_token = page["nextPageToken"].as_str().map(str::to_string);
            if page_token.is_none() || datasets.len() >= MAX_LISTED_ITEMS {
                break;
            }
        }
        Ok(json!({
            "project_id": project_id,
            "datasets": datasets,
            "total": datasets.len(),
            "truncated": page_token.is_some(),
        }))
    }
}

#[async_trait]
impl Tool for BigQueryListDatasets {
    fn name(&self) -> &str {
        "bigquery_list_datasets"
    }

    fn description(&self) -> &str {
        "List available datasets in a Google BigQuery project."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "project_id": { "type": "string", "description": PROJECT_ID_DESCRIPTION }
            }
        }))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        surface(self.run(&ctx, args)).await
    }
}

// ---------------------------------------------------------------------------
// bigquery_list_tables
// ---------------------------------------------------------------------------

/// List tables in a BigQuery dataset.
struct BigQueryListTables {
    shared: Arc<BigQueryShared>,
}

impl BigQueryListTables {
    async fn run(&self, ctx: &Arc<dyn ToolContext>, args: Value) -> anyhow::Result<Value> {
        let dataset_id = required_str(&args, "dataset_id")?.to_string();
        let token = self.shared.token(ctx).await?;
        let project_id = self.shared.project_id(&args)?;
        let path = format!("projects/{project_id}/datasets/{dataset_id}/tables");
        let mut tables = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![("maxResults", MAX_LISTED_ITEMS.to_string())];
            if let Some(token) = &page_token {
                query.push(("pageToken", token.clone()));
            }
            let page = self.shared.get(&token, &path, &query).await?;
            for t in page["tables"].as_array().into_iter().flatten() {
                tables.push(json!({
                    "table_id": t["tableReference"]["tableId"],
                    "type": t["type"].as_str().unwrap_or("TABLE"),
                    "friendly_name": t["friendlyName"].as_str().unwrap_or(""),
                }));
            }
            page_token = page["nextPageToken"].as_str().map(str::to_string);
            if page_token.is_none() || tables.len() >= MAX_LISTED_ITEMS {
                break;
            }
        }
        Ok(json!({
            "project_id": project_id,
            "dataset_id": dataset_id,
            "tables": tables,
            "total": tables.len(),
            "truncated": page_token.is_some(),
        }))
    }
}

#[async_trait]
impl Tool for BigQueryListTables {
    fn name(&self) -> &str {
        "bigquery_list_tables"
    }

    fn description(&self) -> &str {
        "List tables in a Google BigQuery dataset."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "project_id": { "type": "string", "description": PROJECT_ID_DESCRIPTION },
                "dataset_id": {
                    "type": "string",
                    "description": "The BigQuery dataset ID to list tables from."
                }
            },
            "required": ["dataset_id"]
        }))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        surface(self.run(&ctx, args)).await
    }
}

// ---------------------------------------------------------------------------
// bigquery_get_table_schema
// ---------------------------------------------------------------------------

/// Retrieve column definitions for a BigQuery table.
struct BigQueryGetTableSchema {
    shared: Arc<BigQueryShared>,
}

impl BigQueryGetTableSchema {
    async fn run(&self, ctx: &Arc<dyn ToolContext>, args: Value) -> anyhow::Result<Value> {
        let dataset_id = required_str(&args, "dataset_id")?.to_string();
        let table_id = required_str(&args, "table_id")?.to_string();
        let token = self.shared.token(ctx).await?;
        let project_id = self.shared.project_id(&args)?;
        let path = format!("projects/{project_id}/datasets/{dataset_id}/tables/{table_id}");
        let table = self.shared.get(&token, &path, &[]).await?;
        let fields = table["schema"]["fields"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(json!({
            "table": format!("{project_id}.{dataset_id}.{table_id}"),
            "type": table["type"],
            "description": table["description"],
            "num_rows": table["numRows"],
            "columns": schema_columns(&fields),
        }))
    }
}

#[async_trait]
impl Tool for BigQueryGetTableSchema {
    fn name(&self) -> &str {
        "bigquery_get_table_schema"
    }

    fn description(&self) -> &str {
        "Retrieve the schema (column definitions) for a BigQuery table."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "project_id": { "type": "string", "description": PROJECT_ID_DESCRIPTION },
                "dataset_id": {
                    "type": "string",
                    "description": "The BigQuery dataset ID containing the table."
                },
                "table_id": { "type": "string", "description": "The BigQuery table ID." }
            },
            "required": ["dataset_id", "table_id"]
        }))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        surface(self.run(&ctx, args)).await
    }
}

// ---------------------------------------------------------------------------
// bigquery_execute_sql
// ---------------------------------------------------------------------------

/// Execute a SQL query against BigQuery and return results as a JSON array.
///
/// Calls `jobs.query` and pages through `jobs.getQueryResults` up to
/// `max_results` rows. In read-only mode, the default, the query must pass
/// [`check_read_only_sql`], and a dry run (`jobs.insert` with `dryRun`) must
/// report a `SELECT` statement before it runs.
struct BigQueryExecuteSql {
    shared: Arc<BigQueryShared>,
}

impl BigQueryExecuteSql {
    async fn run(&self, ctx: &Arc<dyn ToolContext>, args: Value) -> anyhow::Result<Value> {
        let sql = args["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter 'query'"))?;
        if self.shared.read_only
            && let Err(reason) = check_read_only_sql(sql)
        {
            return Ok(read_only_violation(&reason));
        }
        let max_results = args["max_results"]
            .as_u64()
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .clamp(1, MAX_RESULT_ROWS);
        let token = self.shared.token(ctx).await?;
        let project_id = self.shared.project_id(&args)?;

        if self.shared.read_only {
            // A dry run validates the query and reports its statement type
            // without running it, which settles what a lexical check cannot.
            let job = self
                .shared
                .post(
                    &token,
                    &format!("projects/{project_id}/jobs"),
                    &json!({
                        "configuration": {
                            "dryRun": true,
                            "query": { "query": sql, "useLegacySql": false }
                        }
                    }),
                )
                .await?;
            let statement_type = job["statistics"]["query"]["statementType"].as_str();
            if statement_type != Some(SELECT_STATEMENT_TYPE) {
                return Ok(read_only_violation(&format!(
                    "BigQuery classifies it as {}",
                    statement_type.unwrap_or("no statement type")
                )));
            }
        }

        let deadline = tokio::time::Instant::now() + QUERY_DEADLINE;
        let mut response = self
            .shared
            .post(
                &token,
                &format!("projects/{project_id}/queries"),
                &json!({
                    "query": sql,
                    "useLegacySql": false,
                    "maxResults": max_results,
                    "timeoutMs": QUERY_TIMEOUT_MS,
                }),
            )
            .await?;
        let job_id = response["jobReference"]["jobId"]
            .as_str()
            .map(str::to_string);
        let location = response["jobReference"]["location"]
            .as_str()
            .map(str::to_string);
        let results_path = job_id
            .as_ref()
            .map(|id| format!("projects/{project_id}/queries/{id}"));

        // Wait for the job, then page through its rows.
        let mut rows: Vec<Value> = Vec::new();
        let mut schema_fields: Vec<Value> = Vec::new();
        let mut truncated = false;
        loop {
            if response["jobComplete"].as_bool() != Some(true) {
                let Some(path) = &results_path else {
                    anyhow::bail!("BigQuery did not complete the query and returned no job id");
                };
                if tokio::time::Instant::now() >= deadline {
                    anyhow::bail!(
                        "BigQuery query did not finish within {} s (job {}); narrow it or try again later",
                        QUERY_DEADLINE.as_secs(),
                        job_id.as_deref().unwrap_or("?")
                    );
                }
                response = self
                    .shared
                    .get(&token, path, &results_query(&location, max_results, None))
                    .await?;
                continue;
            }
            if schema_fields.is_empty()
                && let Some(fields) = response["schema"]["fields"].as_array()
            {
                schema_fields = fields.clone();
            }
            if let Some(page) = response["rows"].as_array() {
                let remaining = (max_results as usize).saturating_sub(rows.len());
                if page.len() > remaining {
                    truncated = true;
                }
                rows.extend(rows_to_json(
                    &schema_fields,
                    &page[..page.len().min(remaining)],
                ));
            }
            let next = response["pageToken"].as_str().map(str::to_string);
            if rows.len() >= max_results as usize {
                truncated |= next.is_some();
                break;
            }
            let (Some(next), Some(path)) = (next, &results_path) else {
                break;
            };
            response = self
                .shared
                .get(
                    &token,
                    path,
                    &results_query(&location, max_results - rows.len() as u64, Some(&next)),
                )
                .await?;
        }

        let mut result = json!({
            "rows": rows,
            "total_rows": rows.len(),
            "truncated": truncated,
        });
        for (key, source) in [
            ("rows_available", "totalRows"),
            ("total_bytes_processed", "totalBytesProcessed"),
            ("cache_hit", "cacheHit"),
            ("num_dml_affected_rows", "numDmlAffectedRows"),
        ] {
            if !response[source].is_null() {
                result[key] = response[source].clone();
            }
        }
        if let Some(id) = job_id {
            result["job_id"] = json!(id);
        }
        Ok(result)
    }
}

/// Query parameters of a `jobs.getQueryResults` call.
fn results_query(
    location: &Option<String>,
    max_results: u64,
    page_token: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut query = vec![
        ("maxResults", max_results.to_string()),
        ("timeoutMs", QUERY_TIMEOUT_MS.to_string()),
    ];
    if let Some(location) = location {
        query.push(("location", location.clone()));
    }
    if let Some(token) = page_token {
        query.push(("pageToken", token.to_string()));
    }
    query
}

#[async_trait]
impl Tool for BigQueryExecuteSql {
    fn name(&self) -> &str {
        "bigquery_execute_sql"
    }

    fn description(&self) -> &str {
        if self.shared.read_only {
            "Run a read-only SQL query (a single SELECT or WITH statement) against Google \
             BigQuery and return results as a JSON array of row objects."
        } else {
            "Execute a SQL statement against Google BigQuery and return results as a JSON \
             array of row objects."
        }
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The SQL query to execute." },
                "project_id": { "type": "string", "description": PROJECT_ID_DESCRIPTION },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of rows to return (default 1000)."
                }
            },
            "required": ["query"]
        }))
    }

    fn is_read_only(&self) -> bool {
        self.shared.read_only
    }

    fn is_concurrency_safe(&self) -> bool {
        self.shared.read_only
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        surface(self.run(&ctx, args)).await
    }
}

// ---------------------------------------------------------------------------
// Toolset
// ---------------------------------------------------------------------------

/// BigQuery tools acting with the signed-in user's credentials.
pub struct BigQueryToolset {
    shared: Arc<BigQueryShared>,
}

impl BigQueryToolset {
    /// Creates the toolset (read-only, no default project).
    pub fn new(credentials: Arc<SessionCredentialManager>) -> Self {
        Self {
            shared: Arc::new(BigQueryShared {
                credentials,
                project_id: None,
                read_only: true,
                base_url: DEFAULT_BASE_URL.to_string(),
                http: http_client(),
            }),
        }
    }

    fn update(mut self, f: impl FnOnce(&mut BigQueryShared)) -> Self {
        f(Arc::make_mut(&mut self.shared));
        self
    }

    /// Sets the project used when a call does not name one.
    pub fn with_project(self, project_id: Option<String>) -> Self {
        self.update(|s| s.project_id = project_id.filter(|p| !p.is_empty()))
    }

    /// Allows (`false`) or forbids (`true`, the default) statements other
    /// than a single `SELECT`; also picks the OAuth scope requested.
    pub fn with_read_only(self, read_only: bool) -> Self {
        self.update(|s| s.read_only = read_only)
    }

    /// Overrides the BigQuery API base URL (tests).
    pub fn with_base_url(self, base_url: impl Into<String>) -> Self {
        self.update(|s| s.base_url = base_url.into())
    }

    /// Whether writes are refused.
    pub fn is_read_only(&self) -> bool {
        self.shared.read_only
    }

    /// OAuth scopes the tools request from the session.
    pub fn scopes(&self) -> Vec<String> {
        self.shared.scopes()
    }

    /// Names of the tools.
    pub fn tool_names(&self) -> Vec<String> {
        self.build_tools()
            .iter()
            .map(|t| t.name().to_string())
            .collect()
    }

    fn build_tools(&self) -> Vec<Arc<dyn Tool>> {
        vec![
            Arc::new(BigQueryListDatasets {
                shared: self.shared.clone(),
            }),
            Arc::new(BigQueryListTables {
                shared: self.shared.clone(),
            }),
            Arc::new(BigQueryGetTableSchema {
                shared: self.shared.clone(),
            }),
            Arc::new(BigQueryExecuteSql {
                shared: self.shared.clone(),
            }),
        ]
    }
}

#[async_trait]
impl Toolset for BigQueryToolset {
    fn name(&self) -> &str {
        "bigquery"
    }

    async fn tools(&self, _ctx: Arc<dyn ReadonlyContext>) -> Result<Vec<Arc<dyn Tool>>> {
        Ok(self.build_tools())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{GoogleOAuthFlow, OAuthClientConfig};

    fn toolset() -> BigQueryToolset {
        BigQueryToolset::new(Arc::new(SessionCredentialManager::new(
            GoogleOAuthFlow::new(OAuthClientConfig {
                client_id: "cid".into(),
                client_secret: "secret".into(),
                redirect_uri: "http://localhost:8000/oauth2/callback".into(),
            }),
        )))
    }

    #[test]
    fn single_queries_are_accepted() {
        for sql in [
            "SELECT 1",
            "select 1;",
            "WITH t AS (SELECT 1 AS n) SELECT n FROM t",
            "(SELECT 1) UNION ALL (SELECT 2)",
            "  -- leading comment\n  SELECT 1",
            "#standardSQL\nSELECT 1",
            "/* block */ SELECT 1; -- trailing comment",
            "SELECT ';DROP TABLE x' AS s",
            "SELECT \"a;b\", 'it\\'s; fine', `col;name` FROM `p.d.t`",
            "SELECT '''multi;\nline''' AS s, r\"raw;\" AS r",
            "SELECT * FROM t WHERE note = '--not a comment'; ",
        ] {
            assert!(
                check_read_only_sql(sql).is_ok(),
                "rejected a read-only query: {sql}"
            );
        }
    }

    #[test]
    fn statements_that_are_not_queries_are_rejected() {
        for sql in [
            "DROP TABLE x",
            "INSERT INTO t VALUES (1)",
            "UPDATE t SET a = 1 WHERE true",
            "DELETE FROM t WHERE true",
            "MERGE t USING s ON t.id = s.id WHEN MATCHED THEN DELETE",
            "TRUNCATE TABLE t",
            "CREATE TABLE t (a INT64)",
            "ALTER TABLE t ADD COLUMN b INT64",
            "EXPORT DATA OPTIONS(uri='gs://b/*') AS SELECT 1",
            "CALL proc()",
            "EXECUTE IMMEDIATE 'DROP TABLE x'",
            "DECLARE x INT64",
            "BEGIN SELECT 1; END",
            "/* SELECT */ DELETE FROM t WHERE true",
            "-- SELECT\nDROP TABLE x",
            "'SELECT 1'",
        ] {
            assert!(
                check_read_only_sql(sql).is_err(),
                "accepted a non-query: {sql}"
            );
        }
    }

    #[test]
    fn multiple_statements_are_rejected() {
        for sql in [
            "SELECT 1; DROP TABLE x",
            "SELECT 1;DROP TABLE x",
            "SELECT 1; SELECT 2",
            "SELECT 1; /* c */ DROP TABLE x",
            "SELECT 'a'; DROP TABLE x",
            "SELECT 1;;",
        ] {
            assert!(
                check_read_only_sql(sql).is_err(),
                "accepted multiple statements: {sql}"
            );
        }
    }

    #[test]
    fn unterminated_or_empty_input_is_rejected() {
        for sql in [
            "",
            "   ",
            "-- only a comment",
            "SELECT 'open",
            "SELECT 1 /* open",
            "SELECT `x",
        ] {
            assert!(
                check_read_only_sql(sql).is_err(),
                "accepted malformed input: {sql:?}"
            );
        }
        let reason = check_read_only_sql("DROP TABLE x").unwrap_err();
        assert_eq!(reason, "it starts with DROP, not SELECT or WITH");
        let refusal = read_only_violation(&reason);
        assert_eq!(refusal["error"]["code"], "READ_ONLY_VIOLATION");
        assert!(
            refusal["error"]["message"]
                .as_str()
                .unwrap()
                .contains("BIGQUERY_READ_ONLY=false")
        );
    }

    #[test]
    fn toolset_exposes_the_four_adk_tools_and_scopes_follow_the_mode() {
        let read_only = toolset();
        assert_eq!(read_only.name(), "bigquery");
        assert_eq!(
            read_only.tool_names(),
            vec![
                "bigquery_list_datasets",
                "bigquery_list_tables",
                "bigquery_get_table_schema",
                "bigquery_execute_sql",
            ]
        );
        assert_eq!(read_only.scopes(), vec![BIGQUERY_READ_ONLY_SCOPE]);
        assert!(read_only.is_read_only());
        for tool in read_only.build_tools() {
            assert!(tool.is_read_only(), "{} is read-only", tool.name());
            assert!(tool.is_concurrency_safe());
            assert!(tool.parameters_schema().is_some());
        }

        let writable = toolset()
            .with_read_only(false)
            .with_project(Some("proj".into()));
        assert_eq!(writable.scopes(), vec![BIGQUERY_SCOPE]);
        assert!(!writable.is_read_only());
        let execute = writable
            .build_tools()
            .into_iter()
            .find(|t| t.name() == "bigquery_execute_sql")
            .unwrap();
        assert!(!execute.is_read_only());
        assert!(!execute.is_concurrency_safe());
        assert!(execute.description().starts_with("Execute a SQL statement"));
        assert_eq!(writable.shared.project_id.as_deref(), Some("proj"));
        assert!(
            toolset()
                .with_project(Some(String::new()))
                .shared
                .project_id
                .is_none()
        );
    }

    #[test]
    fn project_id_argument_overrides_the_default_and_is_validated() {
        let shared = toolset().with_project(Some("default".into())).shared;
        assert_eq!(shared.project_id(&json!({})).unwrap(), "default");
        assert_eq!(
            shared
                .project_id(&json!({ "project_id": "other-proj" }))
                .unwrap(),
            "other-proj"
        );
        assert!(
            shared
                .project_id(&json!({ "project_id": "a/../b" }))
                .is_err()
        );
        let no_default = toolset().shared;
        let err = no_default.project_id(&json!({})).unwrap_err();
        assert!(err.to_string().contains("GOOGLE_CLOUD_PROJECT"));
        assert!(required_str(&json!({ "dataset_id": "ds 1" }), "dataset_id").is_err());
        assert!(required_str(&json!({}), "dataset_id").is_err());
        assert_eq!(
            required_str(&json!({ "dataset_id": "ds_1" }), "dataset_id").unwrap(),
            "ds_1"
        );
    }

    #[test]
    fn rows_are_converted_using_the_schema() {
        let fields = vec![
            json!({ "name": "n", "type": "INTEGER", "mode": "NULLABLE" }),
            json!({ "name": "f", "type": "FLOAT", "mode": "NULLABLE" }),
            json!({ "name": "b", "type": "BOOLEAN", "mode": "NULLABLE" }),
            json!({ "name": "s", "type": "STRING", "mode": "NULLABLE" }),
            json!({ "name": "tags", "type": "STRING", "mode": "REPEATED" }),
            json!({ "name": "rec", "type": "RECORD", "mode": "NULLABLE",
                    "fields": [{ "name": "x", "type": "INT64" }] }),
        ];
        let rows = vec![json!({ "f": [
            { "v": "42" }, { "v": "1.5" }, { "v": "true" }, { "v": "hi" },
            { "v": [{ "v": "a" }, { "v": "b" }] },
            { "v": { "f": [{ "v": "7" }] } },
        ] })];
        assert_eq!(
            rows_to_json(&fields, &rows),
            vec![json!({
                "n": 42, "f": 1.5, "b": true, "s": "hi", "tags": ["a", "b"], "rec": { "x": 7 }
            })]
        );
        let nulls = vec![json!({ "f": [
            { "v": null }, { "v": null }, { "v": null }, { "v": null }, { "v": [] }, { "v": null }
        ] })];
        assert_eq!(
            rows_to_json(&fields, &nulls),
            vec![json!({
                "n": null, "f": null, "b": null, "s": null, "tags": [], "rec": null
            })]
        );
        let columns = schema_columns(&fields);
        assert_eq!(columns[0]["mode"], "NULLABLE");
        assert_eq!(columns[5]["fields"][0]["name"], "x");
        assert!(columns[0].get("fields").is_none());
    }

    #[test]
    fn api_failures_become_error_objects_with_hints() {
        let forbidden: anyhow::Error = ApiFailure {
            status: Some(403),
            message: "GET x failed with HTTP 403: denied".into(),
            details: Some(json!({ "error": { "code": 403 } })),
        }
        .into();
        let value = failure_json(&forbidden);
        assert_eq!(value["error"]["status"], 403);
        assert!(value["error"]["hint"].is_string());
        assert_eq!(value["error"]["details"]["error"]["code"], 403);
        let other = failure_json(&anyhow::anyhow!("boom"));
        assert_eq!(other["error"]["message"], "boom");
        assert!(other["error"].get("status").is_none());
    }
}

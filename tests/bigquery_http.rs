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

//! HTTP-level tests for the user-credentialed BigQuery toolset against a
//! minimal axum stand-in for the BigQuery REST API v2: listing, schema,
//! the read-only dry-run gate, query paging, 401/403 handling and the
//! `AUTH_REQUIRED` flow when the session holds no token.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use adk_rust::{
    Artifacts, CallbackContext, Content, EventActions, MemoryEntry, ReadonlyContext, Session,
    State, Tool, ToolContext, Toolset,
};
use adspace_agent::auth::{
    GoogleOAuthFlow, OAuthClientConfig, OAuthToken, SessionCredentialManager,
    USER_CREDENTIALS_STATE_KEY,
};
use adspace_agent::tools::bigquery::{
    BIGQUERY_READ_ONLY_SCOPE, BIGQUERY_SCOPE, BigQueryToolset, MAX_RESULT_ROWS,
};
use async_trait::async_trait;
use axum::Router;
use axum::extract::{Path as AxumPath, Query, State as AxumState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};

/// Tool context shaped like the runner's: a session whose state holds the
/// given OAuth token (or none).
struct SessionToolContext {
    state: HashMap<String, Value>,
    actions: Mutex<EventActions>,
    content: Content,
}

impl SessionToolContext {
    fn with_scopes(scopes: &[&str]) -> Arc<Self> {
        let token = OAuthToken {
            access_token: "ya29.user-token".into(),
            refresh_token: None,
            expires_at: None,
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
        };
        Arc::new(Self {
            state: HashMap::from([(
                USER_CREDENTIALS_STATE_KEY.to_string(),
                serde_json::to_value(token).unwrap(),
            )]),
            actions: Mutex::new(EventActions::default()),
            content: Content::new("user"),
        })
    }

    fn without_token() -> Arc<Self> {
        Arc::new(Self {
            state: HashMap::new(),
            actions: Mutex::new(EventActions::default()),
            content: Content::new("user"),
        })
    }
}

impl State for SessionToolContext {
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

impl ReadonlyContext for SessionToolContext {
    fn invocation_id(&self) -> &str {
        "inv"
    }
    fn agent_name(&self) -> &str {
        "adspace_agent"
    }
    fn user_id(&self) -> &str {
        "user"
    }
    fn app_name(&self) -> &str {
        "adspace_agent"
    }
    fn session_id(&self) -> &str {
        "s-1"
    }
    fn branch(&self) -> &str {
        ""
    }
    fn user_content(&self) -> &Content {
        &self.content
    }
}

impl Session for SessionToolContext {
    fn id(&self) -> &str {
        "s-1"
    }
    fn app_name(&self) -> &str {
        "adspace_agent"
    }
    fn user_id(&self) -> &str {
        "user"
    }
    fn state(&self) -> &dyn State {
        self
    }
    fn conversation_history(&self) -> Vec<Content> {
        Vec::new()
    }
}

#[async_trait]
impl CallbackContext for SessionToolContext {
    fn artifacts(&self) -> Option<Arc<dyn Artifacts>> {
        None
    }
}

#[async_trait]
impl ToolContext for SessionToolContext {
    fn function_call_id(&self) -> &str {
        "call"
    }
    fn actions(&self) -> EventActions {
        self.actions.lock().unwrap().clone()
    }
    fn set_actions(&self, actions: EventActions) {
        *self.actions.lock().unwrap() = actions;
    }
    async fn search_memory(&self, _query: &str) -> adk_rust::Result<Vec<MemoryEntry>> {
        Ok(vec![])
    }
    fn session(&self) -> Option<&dyn Session> {
        Some(self)
    }
}

/// One request as the fake API saw it: method, path, query, bearer token, body.
type SeenRequest = (
    String,
    String,
    HashMap<String, String>,
    String,
    Option<Value>,
);

#[derive(Default)]
struct Fake {
    requests: Mutex<Vec<SeenRequest>>,
    /// Status + body the fake answers every request with, when set.
    forced: Mutex<Option<(StatusCode, Value)>>,
    /// How many `jobs.getQueryResults` polls report `jobComplete: false`.
    pending_polls: Mutex<usize>,
}

fn bearer(headers: &HeaderMap) -> String {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string()
}

fn record(
    fake: &Fake,
    method: &str,
    path: &str,
    query: &HashMap<String, String>,
    headers: &HeaderMap,
    body: Option<Value>,
) -> Option<Response> {
    fake.requests.lock().unwrap().push((
        method.into(),
        path.into(),
        query.clone(),
        bearer(headers),
        body,
    ));
    if bearer(headers) != "ya29.user-token" {
        return Some(
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({ "error": { "code": 401, "message": "Invalid Credentials" } })),
            )
                .into_response(),
        );
    }
    fake.forced
        .lock()
        .unwrap()
        .clone()
        .map(|(status, body)| (status, axum::Json(body)).into_response())
}

async fn list_datasets(
    AxumState(fake): AxumState<Arc<Fake>>,
    AxumPath(project): AxumPath<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if let Some(forced) = record(
        &fake,
        "GET",
        &format!("/projects/{project}/datasets"),
        &query,
        &headers,
        None,
    ) {
        return forced;
    }
    // Two pages of datasets.
    match query.get("pageToken").map(String::as_str) {
        None => axum::Json(json!({
            "datasets": [
                { "datasetReference": { "datasetId": "sales", "projectId": project }, "friendlyName": "Sales", "location": "US" }
            ],
            "nextPageToken": "p2"
        }))
        .into_response(),
        Some("p2") => axum::Json(json!({
            "datasets": [{ "datasetReference": { "datasetId": "marketing" }, "location": "EU" }]
        }))
        .into_response(),
        Some(other) => (StatusCode::BAD_REQUEST, format!("bad page token {other}")).into_response(),
    }
}

async fn list_tables(
    AxumState(fake): AxumState<Arc<Fake>>,
    AxumPath((project, dataset)): AxumPath<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if let Some(forced) = record(
        &fake,
        "GET",
        &format!("/projects/{project}/datasets/{dataset}/tables"),
        &query,
        &headers,
        None,
    ) {
        return forced;
    }
    axum::Json(json!({
        "tables": [
            { "tableReference": { "tableId": "orders" }, "type": "TABLE", "friendlyName": "Orders" },
            { "tableReference": { "tableId": "daily" }, "type": "VIEW" }
        ]
    }))
    .into_response()
}

async fn get_table(
    AxumState(fake): AxumState<Arc<Fake>>,
    AxumPath((project, dataset, table)): AxumPath<(String, String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if let Some(forced) = record(
        &fake,
        "GET",
        &format!("/projects/{project}/datasets/{dataset}/tables/{table}"),
        &query,
        &headers,
        None,
    ) {
        return forced;
    }
    if table == "missing" {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": { "code": 404, "message": "Not found: Table missing" } })),
        )
            .into_response();
    }
    axum::Json(json!({
        "type": "TABLE",
        "numRows": "12",
        "schema": { "fields": [
            { "name": "id", "type": "INTEGER", "mode": "REQUIRED" },
            { "name": "amount", "type": "FLOAT", "description": "USD" },
            { "name": "items", "type": "RECORD", "mode": "REPEATED",
              "fields": [{ "name": "sku", "type": "STRING" }] }
        ] }
    }))
    .into_response()
}

async fn insert_job(
    AxumState(fake): AxumState<Arc<Fake>>,
    AxumPath(project): AxumPath<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if let Some(forced) = record(
        &fake,
        "POST",
        &format!("/projects/{project}/jobs"),
        &query,
        &headers,
        Some(body.clone()),
    ) {
        return forced;
    }
    assert_eq!(body["configuration"]["dryRun"], true, "{body}");
    let sql = body["configuration"]["query"]["query"]
        .as_str()
        .unwrap()
        .to_uppercase();
    // What BigQuery would say: a SELECT wrapped in a procedural script or a
    // DML statement is not a SELECT.
    let statement_type = if sql.contains("DELETE") {
        "DELETE"
    } else if sql.starts_with("WITH") && sql.contains("INSERT") {
        "INSERT"
    } else {
        "SELECT"
    };
    axum::Json(json!({
        "statistics": { "query": { "statementType": statement_type, "totalBytesProcessed": "10" } }
    }))
    .into_response()
}

fn page(rows: &[u64], next: Option<&str>, job_complete: bool) -> Value {
    let mut body = json!({
        "jobReference": { "jobId": "job-1", "location": "US" },
        "jobComplete": job_complete,
    });
    if job_complete {
        body["schema"] = json!({ "fields": [
            { "name": "n", "type": "INTEGER", "mode": "NULLABLE" },
            { "name": "label", "type": "STRING", "mode": "NULLABLE" }
        ] });
        body["totalRows"] = json!("5");
        body["cacheHit"] = json!(false);
        body["totalBytesProcessed"] = json!("1024");
        body["rows"] = Value::Array(
            rows.iter()
                .map(|n| json!({ "f": [{ "v": n.to_string() }, { "v": format!("row {n}") }] }))
                .collect(),
        );
        if let Some(next) = next {
            body["pageToken"] = json!(next);
        }
    }
    body
}

async fn query(
    AxumState(fake): AxumState<Arc<Fake>>,
    AxumPath(project): AxumPath<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if let Some(forced) = record(
        &fake,
        "POST",
        &format!("/projects/{project}/queries"),
        &query,
        &headers,
        Some(body.clone()),
    ) {
        return forced;
    }
    assert_eq!(body["useLegacySql"], false);
    let pending = *fake.pending_polls.lock().unwrap();
    if pending > 0 {
        return axum::Json(page(&[], None, false)).into_response();
    }
    axum::Json(page(&[1, 2], Some("next-1"), true)).into_response()
}

async fn query_results(
    AxumState(fake): AxumState<Arc<Fake>>,
    AxumPath((project, job)): AxumPath<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if let Some(forced) = record(
        &fake,
        "GET",
        &format!("/projects/{project}/queries/{job}"),
        &query,
        &headers,
        None,
    ) {
        return forced;
    }
    assert_eq!(query.get("location").map(String::as_str), Some("US"));
    {
        let mut pending = fake.pending_polls.lock().unwrap();
        if *pending > 0 {
            *pending -= 1;
            return axum::Json(page(&[], None, false)).into_response();
        }
    }
    match query.get("pageToken").map(String::as_str) {
        None => axum::Json(page(&[1, 2], Some("next-1"), true)).into_response(),
        Some("next-1") => axum::Json(page(&[3, 4], Some("next-2"), true)).into_response(),
        Some("next-2") => axum::Json(page(&[5], None, true)).into_response(),
        Some(other) => (StatusCode::BAD_REQUEST, format!("bad page token {other}")).into_response(),
    }
}

async fn spawn(fake: Arc<Fake>) -> String {
    let router = Router::new()
        .route(
            "/bigquery/v2/projects/{project}/datasets",
            get(list_datasets),
        )
        .route(
            "/bigquery/v2/projects/{project}/datasets/{dataset}/tables",
            get(list_tables),
        )
        .route(
            "/bigquery/v2/projects/{project}/datasets/{dataset}/tables/{table}",
            get(get_table),
        )
        .route("/bigquery/v2/projects/{project}/jobs", post(insert_job))
        .route("/bigquery/v2/projects/{project}/queries", post(query))
        .route(
            "/bigquery/v2/projects/{project}/queries/{job}",
            get(query_results),
        )
        .with_state(fake);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, router).into_future());
    format!("http://{addr}/bigquery/v2")
}

fn credentials() -> Arc<SessionCredentialManager> {
    Arc::new(SessionCredentialManager::new(GoogleOAuthFlow::new(
        OAuthClientConfig {
            client_id: "id".into(),
            client_secret: "secret".into(),
            redirect_uri: "http://localhost/oauth2/callback".into(),
        },
    )))
}

struct Harness {
    fake: Arc<Fake>,
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl Harness {
    async fn new(read_only: bool) -> Self {
        Self::build(read_only, false).await
    }

    async fn with_dry_run(read_only: bool, dry_run: bool) -> Self {
        Self::build(read_only, dry_run).await
    }

    async fn build(read_only: bool, dry_run: bool) -> Self {
        Self::build_capped(read_only, dry_run, None).await
    }

    async fn build_capped(read_only: bool, dry_run: bool, max_bytes_billed: Option<u64>) -> Self {
        let fake = Arc::new(Fake::default());
        let base = spawn(fake.clone()).await;
        let toolset = BigQueryToolset::new(credentials())
            .with_project(Some("proj".into()))
            .with_read_only(read_only)
            .with_dry_run(dry_run)
            .with_max_bytes_billed(max_bytes_billed)
            .with_base_url(base);
        let ctx: Arc<dyn ReadonlyContext> = SessionToolContext::with_scopes(&[]);
        let tools = toolset
            .tools(ctx)
            .await
            .unwrap()
            .into_iter()
            .map(|t| (t.name().to_string(), t))
            .collect();
        Self { fake, tools }
    }

    async fn call(&self, tool: &str, ctx: Arc<SessionToolContext>, args: Value) -> Value {
        self.tools[tool].execute(ctx, args).await.unwrap()
    }

    fn paths(&self) -> Vec<String> {
        self.fake
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|(m, p, ..)| format!("{m} {p}"))
            .collect()
    }

    fn clear(&self) {
        self.fake.requests.lock().unwrap().clear();
    }
}

#[tokio::test]
async fn listing_and_schema_tools_use_the_users_token() {
    let h = Harness::new(true).await;
    let ctx = SessionToolContext::with_scopes(&[BIGQUERY_READ_ONLY_SCOPE]);

    let datasets = h
        .call("bigquery_list_datasets", ctx.clone(), json!({}))
        .await;
    assert_eq!(datasets["project_id"], "proj");
    assert_eq!(datasets["total"], 2, "{datasets}");
    assert_eq!(datasets["datasets"][0]["dataset_id"], "sales");
    assert_eq!(datasets["datasets"][0]["friendly_name"], "Sales");
    assert_eq!(datasets["datasets"][1]["dataset_id"], "marketing");
    assert_eq!(datasets["datasets"][1]["friendly_name"], "");
    assert_eq!(datasets["truncated"], false);
    assert_eq!(
        h.paths(),
        vec!["GET /projects/proj/datasets", "GET /projects/proj/datasets"],
        "two pages"
    );
    {
        let seen = h.fake.requests.lock().unwrap();
        assert_eq!(seen[0].3, "ya29.user-token", "bearer token is the user's");
        assert_eq!(seen[1].2["pageToken"], "p2");
    }
    h.clear();

    // `project_id` in the arguments wins over the toolset default.
    let tables = h
        .call(
            "bigquery_list_tables",
            ctx.clone(),
            json!({ "project_id": "other", "dataset_id": "sales" }),
        )
        .await;
    assert_eq!(tables["project_id"], "other");
    assert_eq!(tables["dataset_id"], "sales");
    assert_eq!(tables["total"], 2);
    assert_eq!(tables["tables"][0]["table_id"], "orders");
    assert_eq!(tables["tables"][1]["type"], "VIEW");
    assert_eq!(h.paths(), vec!["GET /projects/other/datasets/sales/tables"]);
    h.clear();

    let schema = h
        .call(
            "bigquery_get_table_schema",
            ctx.clone(),
            json!({ "dataset_id": "sales", "table_id": "orders" }),
        )
        .await;
    assert_eq!(schema["table"], "proj.sales.orders");
    assert_eq!(schema["num_rows"], "12");
    assert_eq!(schema["columns"][0]["name"], "id");
    assert_eq!(schema["columns"][0]["mode"], "REQUIRED");
    assert_eq!(schema["columns"][1]["description"], "USD");
    assert_eq!(schema["columns"][1]["mode"], "NULLABLE");
    assert_eq!(schema["columns"][2]["fields"][0]["name"], "sku");

    // Missing arguments and unknown tables are reported, not panicked on.
    let missing = h.call("bigquery_list_tables", ctx.clone(), json!({})).await;
    assert!(
        missing["error"]["message"]
            .as_str()
            .unwrap()
            .contains("dataset_id"),
        "{missing}"
    );
    let not_found = h
        .call(
            "bigquery_get_table_schema",
            ctx,
            json!({ "dataset_id": "sales", "table_id": "missing" }),
        )
        .await;
    assert_eq!(not_found["error"]["status"], 404);
    assert!(
        not_found["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Not found: Table missing")
    );
}

#[tokio::test]
async fn read_only_mode_gates_queries_lexically_in_single_round_trip_by_default() {
    let h = Harness::new(true).await;
    let ctx = SessionToolContext::with_scopes(&[BIGQUERY_READ_ONLY_SCOPE]);

    // Plainly not a query: refused before any request.
    let refused = h
        .call(
            "bigquery_execute_sql",
            ctx.clone(),
            json!({ "query": "DELETE FROM sales.orders WHERE true" }),
        )
        .await;
    assert_eq!(refused["error"]["code"], "READ_ONLY_VIOLATION");
    assert!(h.paths().is_empty(), "{:?}", h.paths());

    // CTE followed by DML (`WITH ... INSERT`) is caught lexically in 0 HTTP round-trips!
    let refused = h
        .call(
            "bigquery_execute_sql",
            ctx.clone(),
            json!({ "query": "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x" }),
        )
        .await;
    assert_eq!(refused["error"]["code"], "READ_ONLY_VIOLATION");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("INSERT"),
        "{refused}"
    );
    assert!(
        h.paths().is_empty(),
        "CTE DML refused without any HTTP call"
    );

    // A real SELECT executes directly without a separate jobs.insert dryRun preflight,
    // then pages up to max_results.
    let result = h
        .call(
            "bigquery_execute_sql",
            ctx.clone(),
            json!({ "query": "SELECT n, label FROM sales.orders", "max_results": 4 }),
        )
        .await;
    assert_eq!(
        result["rows"],
        json!([
            { "n": 1, "label": "row 1" }, { "n": 2, "label": "row 2" },
            { "n": 3, "label": "row 3" }, { "n": 4, "label": "row 4" }
        ]),
        "{result}"
    );
    assert_eq!(result["total_rows"], 4);
    assert_eq!(result["truncated"], true, "a fifth row exists");
    assert_eq!(result["rows_available"], "5");
    assert_eq!(result["job_id"], "job-1");
    assert_eq!(result["total_bytes_processed"], "1024");
    assert_eq!(
        h.paths(),
        vec![
            "POST /projects/proj/queries",
            "GET /projects/proj/queries/job-1",
        ]
    );
    {
        let seen = h.fake.requests.lock().unwrap();
        assert_eq!(seen[0].4.as_ref().unwrap()["maxResults"], 4);
        assert_eq!(seen[1].2["pageToken"], "next-1");
        assert_eq!(seen[1].2["maxResults"], "2", "only the rows still needed");
    }
    h.clear();

    // All rows when max_results allows; out-of-range values are clamped.
    let result = h
        .call(
            "bigquery_execute_sql",
            ctx,
            json!({ "query": "select * from sales.orders", "max_results": MAX_RESULT_ROWS * 10 }),
        )
        .await;
    assert_eq!(result["total_rows"], 5);
    assert_eq!(result["truncated"], false);
    let seen = h.fake.requests.lock().unwrap();
    assert_eq!(seen[0].4.as_ref().unwrap()["maxResults"], MAX_RESULT_ROWS);
}

#[tokio::test]
async fn optional_dry_run_mode_classifies_queries_via_jobs_endpoint() {
    let h = Harness::with_dry_run(true, true).await;
    let ctx = SessionToolContext::with_scopes(&[BIGQUERY_READ_ONLY_SCOPE]);

    // Passes lexical check (literal contains 'DELETE'), but fake dry-run classifies as DELETE.
    let refused = h
        .call(
            "bigquery_execute_sql",
            ctx.clone(),
            json!({ "query": "SELECT 'DELETE' AS kind" }),
        )
        .await;
    assert_eq!(refused["error"]["code"], "READ_ONLY_VIOLATION");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("classifies it as DELETE")
    );
    assert_eq!(h.paths(), vec!["POST /projects/proj/jobs"]);
    h.clear();

    let result = h
        .call(
            "bigquery_execute_sql",
            ctx,
            json!({ "query": "SELECT n, label FROM sales.orders", "max_results": 2 }),
        )
        .await;
    assert_eq!(result["total_rows"], 2);
    assert_eq!(
        h.paths(),
        vec!["POST /projects/proj/jobs", "POST /projects/proj/queries",]
    );
}

/// The body of the last request to `path`, as the fake saw it.
fn last_body(h: &Harness, path: &str) -> Value {
    h.fake
        .requests
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(_, seen, ..)| seen == path)
        .and_then(|(.., body)| body.clone())
        .expect("a request with a body")
}

#[tokio::test]
async fn a_cap_on_bytes_billed_is_sent_and_checked_against_the_dry_run() {
    let ctx = SessionToolContext::with_scopes(&[BIGQUERY_READ_ONLY_SCOPE]);
    let query = json!({ "query": "SELECT n, label FROM sales.orders", "max_results": 2 });

    // The fake's dry run estimates 10 bytes: over a cap of 5, the query is
    // refused without being sent.
    let h = Harness::build_capped(true, true, Some(5)).await;
    let refused = h
        .call("bigquery_execute_sql", ctx.clone(), query.clone())
        .await;
    assert_eq!(refused["error"]["code"], "BYTES_BILLED_LIMIT", "{refused}");
    assert_eq!(refused["error"]["estimated_bytes"], 10);
    assert_eq!(refused["error"]["limit_bytes"], 5);
    let message = refused["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("about 10 B") && message.contains("is 5 B"),
        "{message}"
    );
    assert_eq!(h.paths(), vec!["POST /projects/proj/jobs"]);

    // Under the cap it runs, and BigQuery is told the cap so it enforces it.
    let h = Harness::build_capped(true, true, Some(100)).await;
    let result = h
        .call("bigquery_execute_sql", ctx.clone(), query.clone())
        .await;
    assert_eq!(result["total_rows"], 2);
    assert_eq!(
        last_body(&h, "/projects/proj/queries")["maximumBytesBilled"],
        "100"
    );

    // Without a dry run there is no estimate; the cap still goes to BigQuery.
    let h = Harness::build_capped(true, false, Some(5)).await;
    let result = h
        .call("bigquery_execute_sql", ctx.clone(), query.clone())
        .await;
    assert_eq!(result["total_rows"], 2);
    assert_eq!(
        last_body(&h, "/projects/proj/queries")["maximumBytesBilled"],
        "5"
    );

    // No cap, or a cap of zero: the field is not sent.
    for cap in [None, Some(0)] {
        let h = Harness::build_capped(true, true, cap).await;
        h.call("bigquery_execute_sql", ctx.clone(), query.clone())
            .await;
        assert!(
            last_body(&h, "/projects/proj/queries")
                .get("maximumBytesBilled")
                .is_none()
        );
    }
}

#[tokio::test]
async fn writable_mode_skips_the_dry_run_and_waits_for_slow_jobs() {
    let h = Harness::new(false).await;
    *h.fake.pending_polls.lock().unwrap() = 2;
    let ctx = SessionToolContext::with_scopes(&[BIGQUERY_SCOPE]);
    let result = h
        .call(
            "bigquery_execute_sql",
            ctx,
            json!({ "query": "DELETE FROM sales.orders WHERE n = 1", "max_results": 2 }),
        )
        .await;
    assert_eq!(result["total_rows"], 2, "{result}");
    assert_eq!(
        h.paths(),
        vec![
            "POST /projects/proj/queries",
            "GET /projects/proj/queries/job-1",
            "GET /projects/proj/queries/job-1",
            "GET /projects/proj/queries/job-1",
        ],
        "no dry run; two incomplete polls, then the first page"
    );
}

#[tokio::test]
async fn auth_failures_are_surfaced_to_the_model() {
    let h = Harness::new(true).await;

    // No token in the session: AUTH_REQUIRED with a consent link for the
    // read-only scope, and nothing is sent to BigQuery.
    let required = h
        .call(
            "bigquery_list_datasets",
            SessionToolContext::without_token(),
            json!({}),
        )
        .await;
    assert_eq!(required["status"], "AUTH_REQUIRED", "{required}");
    assert_eq!(required["scopes"], json!([BIGQUERY_READ_ONLY_SCOPE]));
    assert!(
        required["auth_url"]
            .as_str()
            .unwrap()
            .contains("bigquery.readonly")
    );
    assert!(h.paths().is_empty());

    // A token that lacks the scope is not sent either.
    let required = h
        .call(
            "bigquery_execute_sql",
            SessionToolContext::with_scopes(&["https://www.googleapis.com/auth/drive"]),
            json!({ "query": "SELECT 1" }),
        )
        .await;
    assert_eq!(required["status"], "AUTH_REQUIRED");
    assert!(h.paths().is_empty());

    // A broader `bigquery` grant covers the read-only scope.
    let ok = h
        .call(
            "bigquery_list_datasets",
            SessionToolContext::with_scopes(&[BIGQUERY_SCOPE]),
            json!({}),
        )
        .await;
    assert_eq!(ok["total"], 2, "{ok}");
    h.clear();

    // 401 / 403 from BigQuery become error objects with a hint.
    let ctx = SessionToolContext::with_scopes(&[BIGQUERY_READ_ONLY_SCOPE]);
    *h.fake.forced.lock().unwrap() = Some((
        StatusCode::FORBIDDEN,
        json!({ "error": { "code": 403, "message": "Access Denied: Dataset proj:sales", "status": "PERMISSION_DENIED" } }),
    ));
    let denied = h
        .call(
            "bigquery_list_tables",
            ctx.clone(),
            json!({ "dataset_id": "sales" }),
        )
        .await;
    assert_eq!(denied["error"]["status"], 403, "{denied}");
    assert!(
        denied["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Access Denied: Dataset proj:sales")
    );
    assert!(denied["error"]["hint"].is_string());
    assert_eq!(
        denied["error"]["details"]["error"]["status"],
        "PERMISSION_DENIED"
    );

    h.clear();
    *h.fake.forced.lock().unwrap() = Some((
        StatusCode::UNAUTHORIZED,
        json!({ "error": { "code": 401, "message": "Invalid Credentials" } }),
    ));
    let unauthorized = h
        .call("bigquery_execute_sql", ctx, json!({ "query": "SELECT 1" }))
        .await;
    assert_eq!(unauthorized["error"]["status"], 401, "{unauthorized}");
    assert_eq!(
        h.paths(),
        vec!["POST /projects/proj/queries"],
        "fails directly at jobs.query without extra dryRun round-trip"
    );
}

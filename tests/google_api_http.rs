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

//! HTTP-level tests for the Discovery fetcher and `GoogleApiTool::call`,
//! against a minimal axum stand-in for Google's endpoints.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use adk_rust::{
    Artifacts, CallbackContext, Content, EventActions, MemoryEntry, ReadonlyContext, Session,
    State, Tool, ToolContext,
};
use adspace_agent::auth::{
    CREDENTIALS_STATE_KEY, GoogleOAuthFlow, OAuthClientConfig, OAuthToken, SessionCredentialManager,
};
use adspace_agent::google_api::toolset::MAX_RESPONSE_BYTES;
use adspace_agent::google_api::{
    DiscoveryDocument, DiscoveryFetcher, GoogleApiToolset, GoogleApiToolsetConfig,
};
use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path as AxumPath, Query, State as AxumState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};

const DOC: &str = r#"{
  "name": "demo", "version": "v1",
  "rootUrl": "https://demo.googleapis.com/", "servicePath": "demo/v1/",
  "parameters": {"fields": {"type": "string", "location": "query", "description": "Partial response selector."}},
  "auth": {"oauth2": {"scopes": {"https://www.googleapis.com/auth/demo": {"description": "Demo"}}}},
  "schemas": {"Thing": {"id": "Thing", "type": "object", "properties": {"name": {"type": "string"}, "size": {"type": "integer", "format": "int32"}}}},
  "resources": {"things": {"methods": {
    "list": {"id": "demo.things.list", "path": "things", "httpMethod": "GET",
             "parameters": {"pageSize": {"type": "integer", "format": "int32", "location": "query"},
                            "X-Demo-Trace": {"type": "string", "location": "header"}},
             "scopes": ["https://www.googleapis.com/auth/demo"]},
    "get": {"id": "demo.things.get", "path": "things/{thingId}", "httpMethod": "GET",
            "parameters": {"thingId": {"type": "string", "location": "path", "required": true}},
            "scopes": ["https://www.googleapis.com/auth/demo"]},
    "create": {"id": "demo.things.create", "path": "things", "httpMethod": "POST",
               "request": {"$ref": "Thing"}, "scopes": ["https://www.googleapis.com/auth/demo"]}
  }}}
}"#;

/// Tool context shaped like the runner's: a session whose state already
/// holds a valid, scoped OAuth token.
struct SessionToolContext {
    state: HashMap<String, Value>,
    actions: Mutex<EventActions>,
    content: Content,
}

impl SessionToolContext {
    fn with_token() -> Arc<Self> {
        let token = OAuthToken {
            access_token: "ya29.test-token".into(),
            refresh_token: None,
            expires_at: None,
            scopes: vec!["https://www.googleapis.com/auth/demo".into()],
        };
        Arc::new(Self {
            state: HashMap::from([(
                CREDENTIALS_STATE_KEY.to_string(),
                serde_json::to_value(token).unwrap(),
            )]),
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

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, router).into_future());
    format!("http://{addr}")
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

/// One request as the fake API saw it: method, path, query + headers, body.
type SeenRequest = (String, String, HashMap<String, String>, Option<Value>);

/// What the fake API saw for each request.
#[derive(Default)]
struct Seen {
    requests: Mutex<Vec<SeenRequest>>,
}

async fn record(
    seen: &Seen,
    method: &str,
    path: &str,
    query: &HashMap<String, String>,
    headers: &HeaderMap,
    body: Option<Value>,
) {
    let mut h: HashMap<String, String> = query.clone();
    for (k, v) in headers {
        h.insert(k.as_str().to_string(), v.to_str().unwrap_or("").to_string());
    }
    seen.requests
        .lock()
        .unwrap()
        .push((method.to_string(), path.to_string(), h, body));
}

fn api_router(seen: Arc<Seen>) -> Router {
    Router::new()
        .route(
            "/demo/v1/things",
            get(
                |AxumState(seen): AxumState<Arc<Seen>>,
                 Query(q): Query<HashMap<String, String>>,
                 headers: HeaderMap| async move {
                    record(&seen, "GET", "/demo/v1/things", &q, &headers, None).await;
                    if headers.get("authorization").and_then(|v| v.to_str().ok())
                        != Some("Bearer ya29.test-token")
                    {
                        return (
                            StatusCode::UNAUTHORIZED,
                            axum::Json(json!({"error": {"message": "bad token"}})),
                        )
                            .into_response();
                    }
                    if q.get("pageSize").map(String::as_str) == Some("huge") {
                        // A body just over the cap, declared up front.
                        let body = "x".repeat(MAX_RESPONSE_BYTES + 1);
                        return Response::builder()
                            .status(StatusCode::OK)
                            .header("content-type", "application/json")
                            .body(Body::from(body))
                            .unwrap();
                    }
                    if q.get("pageSize").map(String::as_str) == Some("chunked") {
                        // Same size, streamed without a Content-Length.
                        let chunks: Vec<Result<Vec<u8>, std::io::Error>> = (0..3)
                            .map(|_| Ok(vec![b'y'; MAX_RESPONSE_BYTES / 2]))
                            .collect();
                        return Response::builder()
                            .status(StatusCode::OK)
                            .body(Body::from_stream(futures::stream::iter(chunks)))
                            .unwrap();
                    }
                    axum::Json(json!({"things": [{"name": "a"}], "echoFields": q.get("fields")}))
                        .into_response()
                },
            )
            .post(
                |AxumState(seen): AxumState<Arc<Seen>>,
                 headers: HeaderMap,
                 axum::Json(body): axum::Json<Value>| async move {
                    record(
                        &seen,
                        "POST",
                        "/demo/v1/things",
                        &HashMap::new(),
                        &headers,
                        Some(body.clone()),
                    )
                    .await;
                    (StatusCode::CREATED, axum::Json(json!({"created": body})))
                },
            ),
        )
        .route(
            "/demo/v1/things/{id}",
            get(
                |AxumState(seen): AxumState<Arc<Seen>>,
                 AxumPath(id): AxumPath<String>,
                 headers: HeaderMap| async move {
                    record(
                        &seen,
                        "GET",
                        &format!("/demo/v1/things/{id}"),
                        &HashMap::new(),
                        &headers,
                        None,
                    )
                    .await;
                    if id == "missing" {
                        return (StatusCode::NOT_FOUND, "plain text not found").into_response();
                    }
                    axum::Json(json!({"name": id})).into_response()
                },
            ),
        )
        .with_state(seen)
}

async fn toolset(base: &str) -> GoogleApiToolset {
    let doc = DiscoveryDocument::parse(DOC).unwrap();
    GoogleApiToolset::from_document(
        &doc,
        GoogleApiToolsetConfig::new("demo", "demo", "v1")
            .with_header("x-extra", "yes")
            .with_base_url_override(format!("{base}/demo/v1")),
        credentials(),
    )
}

fn tool<'a>(
    ts: &'a GoogleApiToolset,
    name: &str,
) -> &'a Arc<adspace_agent::google_api::GoogleApiTool> {
    ts.api_tools().iter().find(|t| t.name() == name).unwrap()
}

#[tokio::test]
async fn tool_call_sends_bearer_query_headers_and_body() {
    let seen = Arc::new(Seen::default());
    let base = spawn(api_router(seen.clone())).await;
    let ts = toolset(&base).await;
    let ctx = SessionToolContext::with_token() as Arc<dyn ToolContext>;

    // GET with query, header param, global `fields` param and extra header.
    let out = tool(&ts, "demo_things_list")
        .execute(
            ctx.clone(),
            json!({"page_size": 5, "x_demo_trace": "t-1", "fields": "things(name)"}),
        )
        .await
        .unwrap();
    assert_eq!(out["things"][0]["name"], "a");
    assert_eq!(out["echoFields"], "things(name)");
    let (method, path, h, body) = seen.requests.lock().unwrap().pop().unwrap();
    assert_eq!((method.as_str(), path.as_str()), ("GET", "/demo/v1/things"));
    assert_eq!(h["pageSize"], "5");
    assert_eq!(h["fields"], "things(name)");
    assert_eq!(h["x-demo-trace"], "t-1", "declared header params are sent");
    assert_eq!(h["x-extra"], "yes", "additional headers are sent");
    assert_eq!(h["authorization"], "Bearer ya29.test-token");
    assert!(h["user-agent"].contains("demo_things_list"));
    assert!(body.is_none());

    // POST with a flattened body.
    let out = tool(&ts, "demo_things_create")
        .execute(ctx.clone(), json!({"name": "n", "size": 3}))
        .await
        .unwrap();
    assert_eq!(out["created"], json!({"name": "n", "size": 3}));

    // Path parameter encoding.
    let out = tool(&ts, "demo_things_get")
        .execute(ctx.clone(), json!({"thing_id": "a b/c"}))
        .await
        .unwrap();
    assert_eq!(out["name"], "a b/c");
    let (_, path, _, _) = seen.requests.lock().unwrap().pop().unwrap();
    assert_eq!(path, "/demo/v1/things/a b/c", "axum decodes %20 and %2F");
}

#[tokio::test]
async fn tool_call_reports_http_errors_and_non_json_bodies_structurally() {
    let seen = Arc::new(Seen::default());
    let base = spawn(api_router(seen)).await;
    let ts = toolset(&base).await;
    let ctx = SessionToolContext::with_token() as Arc<dyn ToolContext>;
    let out = tool(&ts, "demo_things_get")
        .execute(ctx, json!({"thing_id": "missing"}))
        .await
        .unwrap();
    assert_eq!(out["error"]["status"], 404);
    assert_eq!(out["error"]["details"]["content"], "plain text not found");
    assert!(
        out["error"]["message"]
            .as_str()
            .unwrap()
            .contains("HTTP 404")
    );
}

#[tokio::test]
async fn oversized_responses_become_a_narrowing_hint() {
    let seen = Arc::new(Seen::default());
    let base = spawn(api_router(seen)).await;
    let ts = toolset(&base).await;
    let ctx = SessionToolContext::with_token() as Arc<dyn ToolContext>;
    for size in ["huge", "chunked"] {
        let out = tool(&ts, "demo_things_list")
            .execute(ctx.clone(), json!({"page_size": size}))
            .await
            .unwrap();
        assert_eq!(out["error"]["code"], "RESPONSE_TOO_LARGE", "{size}: {out}");
        assert!(out["error"]["hint"].as_str().unwrap().contains("fields"));
        assert!(out["error"]["hint"].as_str().unwrap().contains("pageSize"));
        assert!(
            out.to_string().len() < 2_000,
            "the body itself is not echoed"
        );
    }
}

#[tokio::test]
async fn missing_credentials_surface_as_auth_required() {
    let seen = Arc::new(Seen::default());
    let base = spawn(api_router(seen.clone())).await;
    let ts = toolset(&base).await;
    let ctx = Arc::new(adk_rust::tool::SimpleToolContext::new("t")) as Arc<dyn ToolContext>;
    let out = tool(&ts, "demo_things_list")
        .execute(ctx, json!({}))
        .await
        .unwrap();
    assert_eq!(out["status"], "AUTH_REQUIRED", "{out}");
    assert!(
        seen.requests.lock().unwrap().is_empty(),
        "no upstream call without a token"
    );
}

// ---------------------------------------------------------------------------
// DiscoveryFetcher
// ---------------------------------------------------------------------------

#[derive(Default)]
struct DiscoveryServer {
    hits: AtomicUsize,
    /// Body served by the primary URL (`None` = HTTP 500).
    primary: Mutex<Option<String>>,
    /// Body served by the per-API URL (`None` = HTTP 500).
    secondary: Mutex<Option<String>>,
}

fn discovery_router(server: Arc<DiscoveryServer>) -> Router {
    fn serve(body: Option<String>) -> Response {
        match body {
            Some(body) => (StatusCode::OK, body).into_response(),
            None => (StatusCode::INTERNAL_SERVER_ERROR, "boom").into_response(),
        }
    }
    Router::new()
        .route(
            "/discovery/v1/apis/{api}/{version}/rest",
            get(|AxumState(s): AxumState<Arc<DiscoveryServer>>| async move {
                s.hits.fetch_add(1, Ordering::SeqCst);
                let body = s.primary.lock().unwrap().clone();
                serve(body)
            }),
        )
        .route(
            "/$discovery/rest",
            get(|AxumState(s): AxumState<Arc<DiscoveryServer>>| async move {
                s.hits.fetch_add(1, Ordering::SeqCst);
                let body = s.secondary.lock().unwrap().clone();
                serve(body)
            }),
        )
        .with_state(server)
}

#[tokio::test]
async fn discovery_fetch_caches_falls_back_and_survives_corrupt_cache() {
    let server = Arc::new(DiscoveryServer::default());
    *server.primary.lock().unwrap() = Some(DOC.to_string());
    let base = spawn(discovery_router(server.clone())).await;
    let cache = tempfile::tempdir().unwrap();
    let url = format!("{base}/discovery/v1/apis/demo/v1/rest");
    let fetcher = DiscoveryFetcher::new().with_cache_dir(cache.path());

    // 1. First fetch goes to the network and populates the cache.
    let doc = fetcher.fetch("demo", "v1", Some(&url)).await.unwrap();
    assert_eq!(doc.base_url(), "https://demo.googleapis.com/demo/v1");
    assert_eq!(server.hits.load(Ordering::SeqCst), 1);
    let cached: Vec<_> = std::fs::read_dir(cache.path()).unwrap().collect();
    assert_eq!(cached.len(), 1);

    // 2. Second fetch is served from the cache.
    fetcher.fetch("demo", "v1", Some(&url)).await.unwrap();
    assert_eq!(server.hits.load(Ordering::SeqCst), 1);

    // 3. A corrupt/malicious cache entry is ignored and refetched, never used.
    let path = cached[0].as_ref().unwrap().path();
    std::fs::write(
        &path,
        r#"{"name":"demo","version":"v1","rootUrl":"https://attacker.example/"}"#,
    )
    .unwrap();
    let doc = fetcher.fetch("demo", "v1", Some(&url)).await.unwrap();
    assert_eq!(doc.root_url, "https://demo.googleapis.com/");
    assert_eq!(server.hits.load(Ordering::SeqCst), 2);
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("demo.googleapis.com")
    );

    // 4. A poisoned server document is rejected outright.
    *server.primary.lock().unwrap() =
        Some(r#"{"name":"demo","version":"v1","rootUrl":"https://attacker.example/"}"#.to_string());
    let err = DiscoveryFetcher::new()
        .without_cache()
        .fetch("demo", "v1", Some(&url))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("googleapis.com"), "{err:#}");

    // 5. When every fetch fails, a stale cache entry is used as a fallback.
    *server.primary.lock().unwrap() = None;
    let stale = DiscoveryFetcher::new()
        .with_cache_dir(cache.path())
        .with_cache_ttl(std::time::Duration::ZERO);
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let hits_before = server.hits.load(Ordering::SeqCst);
    let doc = stale.fetch("demo", "v1", Some(&url)).await.unwrap();
    assert_eq!(doc.root_url, "https://demo.googleapis.com/");
    assert!(
        server.hits.load(Ordering::SeqCst) > hits_before,
        "it did try the network first"
    );

    // 6. ... and without any cache the failure is reported.
    let err = DiscoveryFetcher::new()
        .without_cache()
        .fetch("demo", "v1", Some(&url))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("HTTP 500"), "{err:#}");
}

#[tokio::test]
async fn concurrent_fetches_share_the_cache_without_tearing() {
    let server = Arc::new(DiscoveryServer::default());
    *server.primary.lock().unwrap() = Some(DOC.to_string());
    let base = spawn(discovery_router(server.clone())).await;
    let cache = tempfile::tempdir().unwrap();
    let url = format!("{base}/discovery/v1/apis/demo/v1/rest");
    let fetcher = DiscoveryFetcher::new().with_cache_dir(cache.path());

    let results = futures::future::join_all((0..8).map(|_| {
        let fetcher = fetcher.clone();
        let url = url.clone();
        async move { fetcher.fetch("demo", "v1", Some(&url)).await }
    }))
    .await;
    for r in results {
        assert_eq!(r.unwrap().name, "demo");
    }
    let entries: Vec<String> = std::fs::read_dir(cache.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries.len(), 1, "no temp files left behind: {entries:?}");
    assert!(entries[0].ends_with(".json") && !entries[0].starts_with('.'));
    let body = std::fs::read_to_string(cache.path().join(&entries[0])).unwrap();
    assert!(
        DiscoveryDocument::parse(&body).is_ok(),
        "cache file is whole"
    );

    // Afterwards everyone reads the cache.
    let hits = server.hits.load(Ordering::SeqCst);
    fetcher.fetch("demo", "v1", Some(&url)).await.unwrap();
    assert_eq!(server.hits.load(Ordering::SeqCst), hits);
}

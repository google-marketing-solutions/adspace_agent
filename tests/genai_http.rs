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

//! HTTP-level tests for the REST paths of [`GenAiClient`] (Imagen `:predict`,
//! Veo `:predictLongRunning` + polling) against a minimal axum stand-in for
//! Vertex AI / the Gemini Developer API, using the `with_base_url` and
//! `with_polling` hooks.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use adspace_agent::auth::{SharedTokenProvider, StaticTokenProvider};
use adspace_agent::genai::GenAiClient;
use axum::Router;
use axum::extract::{Path as AxumPath, State as AxumState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine as _;
use serde_json::{Value, json};

#[derive(Default)]
struct Fake {
    polls: AtomicUsize,
    /// Polls that fail with HTTP 503 before the operation reports progress.
    transient_failures: usize,
    /// Polls (counting successes only) before `done: true`.
    polls_until_done: usize,
    /// Whether Studio-style responses are expected (operation GET).
    studio: bool,
    /// Base URL of the fake server (for absolute download URIs).
    base: std::sync::Mutex<String>,
    last_predict_body: std::sync::Mutex<Option<Value>>,
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake";
const MP4: &[u8] = b"\x00\x00\x00\x18ftypmp42fake";

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn authorised(fake: &Fake, headers: &HeaderMap) -> bool {
    if fake.studio {
        headers.get("x-goog-api-key").and_then(|v| v.to_str().ok()) == Some("studio-key")
    } else {
        headers.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer vertex-token")
    }
}

fn operation(fake: &Fake) -> Response {
    let n = fake.polls.fetch_add(1, Ordering::SeqCst);
    if n < fake.transient_failures {
        return (StatusCode::SERVICE_UNAVAILABLE, "try again").into_response();
    }
    let done = n - fake.transient_failures >= fake.polls_until_done;
    let mut op = json!({"name": "operations/op-1", "done": done});
    if done {
        op["response"] = if fake.studio {
            let base = fake.base.lock().unwrap().clone();
            json!({"generateVideoResponse": {"generatedSamples": [{"video": {"uri": format!("{base}/files/v1:download")}}]}})
        } else {
            json!({"videos": [{"mimeType": "video/mp4", "bytesBase64Encoded": b64(MP4)}]})
        };
    }
    axum::Json(op).into_response()
}

async fn model_verb(
    AxumState(fake): AxumState<Arc<Fake>>,
    headers: HeaderMap,
    AxumPath(params): AxumPath<std::collections::HashMap<String, String>>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if !authorised(&fake, &headers) {
        return (StatusCode::UNAUTHORIZED, "no").into_response();
    }
    let model_verb = params.get("model_verb").cloned().unwrap_or_default();
    let Some((model, verb)) = model_verb.split_once(':') else {
        return (StatusCode::NOT_FOUND, "no verb").into_response();
    };
    *fake.last_predict_body.lock().unwrap() = Some(body);
    match verb {
        "predict" if model.starts_with("imagen") => axum::Json(json!({
            "predictions": [{"mimeType": "image/png", "bytesBase64Encoded": b64(PNG)}]
        }))
        .into_response(),
        "predict" => axum::Json(json!({"predictions": []})).into_response(),
        "predictLongRunning" => axum::Json(json!({"name": "operations/op-1"})).into_response(),
        "fetchPredictOperation" => operation(&fake),
        _ => (StatusCode::NOT_FOUND, "unknown verb").into_response(),
    }
}

fn router(fake: Arc<Fake>) -> Router {
    Router::new()
        // Vertex: /v1/projects/{p}/locations/{l}/publishers/google/models/{model}:{verb}
        .route(
            "/v1/projects/{p}/locations/{l}/publishers/google/models/{model_verb}",
            post(model_verb),
        )
        // Studio: /v1beta/models/{model}:{verb} and /v1beta/{operation}
        .route("/v1beta/models/{model_verb}", post(model_verb))
        .route(
            "/v1beta/operations/{id}",
            get(
                |AxumState(fake): AxumState<Arc<Fake>>, headers: HeaderMap| async move {
                    if !authorised(&fake, &headers) {
                        return (StatusCode::UNAUTHORIZED, "no").into_response();
                    }
                    operation(&fake)
                },
            ),
        )
        .route(
            "/files/{file}",
            get(
                |AxumState(fake): AxumState<Arc<Fake>>, headers: HeaderMap| async move {
                    if !authorised(&fake, &headers) {
                        return (StatusCode::UNAUTHORIZED, "no").into_response();
                    }
                    ([("content-type", "video/mp4")], MP4).into_response()
                },
            ),
        )
        .with_state(fake)
}

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, router).into_future());
    format!("http://{addr}")
}

fn vertex(base: &str) -> GenAiClient {
    let provider: SharedTokenProvider = Arc::new(StaticTokenProvider::new("vertex-token"));
    GenAiClient::vertex("proj", "us-central1", provider)
        .with_base_url(base)
        .with_polling(Duration::from_millis(5), Duration::from_secs(5))
}

#[tokio::test]
async fn imagen_predict_returns_decoded_bytes() {
    let fake = Arc::new(Fake::default());
    let base = spawn(router(fake.clone())).await;
    let media = vertex(&base)
        .generate_image("imagen-4.0-generate-001", "a cat")
        .await
        .unwrap();
    assert_eq!(media.mime_type, "image/png");
    assert_eq!(media.data, PNG);
    assert_eq!(
        fake.last_predict_body.lock().unwrap().clone().unwrap(),
        json!({"instances": [{"prompt": "a cat"}], "parameters": {"sampleCount": 1}})
    );

    let err = vertex(&base)
        .generate_image("other-model", "a cat")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("No images generated"), "{err:#}");
}

#[tokio::test]
async fn veo_polling_tolerates_transient_errors_on_vertex() {
    let fake = Arc::new(Fake {
        transient_failures: 2,
        polls_until_done: 2,
        ..Fake::default()
    });
    let base = spawn(router(fake.clone())).await;
    let media = vertex(&base)
        .generate_video("veo-3.1-fast-generate-001", "a dog")
        .await
        .unwrap();
    assert_eq!(media.mime_type, "video/mp4");
    assert_eq!(media.data, MP4);
    // 2 failed + 2 pending + 1 done
    assert_eq!(fake.polls.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn veo_polling_gives_up_after_persistent_errors() {
    let fake = Arc::new(Fake {
        transient_failures: 1_000,
        ..Fake::default()
    });
    let base = spawn(router(fake.clone())).await;
    let err = vertex(&base)
        .generate_video("veo-3.1-fast-generate-001", "a dog")
        .await
        .unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("failed 5 times in a row"), "{text}");
    assert!(text.contains("HTTP 503"), "{text}");
    assert_eq!(fake.polls.load(Ordering::SeqCst), 5, "bounded retries");
}

#[tokio::test]
async fn veo_polling_times_out() {
    let fake = Arc::new(Fake {
        polls_until_done: usize::MAX,
        ..Fake::default()
    });
    let base = spawn(router(fake)).await;
    let provider: SharedTokenProvider = Arc::new(StaticTokenProvider::new("vertex-token"));
    let client = GenAiClient::vertex("proj", "us-central1", provider)
        .with_base_url(&base)
        .with_polling(Duration::from_millis(5), Duration::from_millis(50));
    let err = client
        .generate_video("veo-3.1-fast-generate-001", "a dog")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("timed out"), "{err:#}");
}

#[tokio::test]
async fn veo_on_studio_polls_the_operation_and_downloads_the_file() {
    let fake = Arc::new(Fake {
        studio: true,
        transient_failures: 1,
        polls_until_done: 1,
        ..Fake::default()
    });
    let base = spawn(router(fake.clone())).await;
    *fake.base.lock().unwrap() = base.clone();
    let client = GenAiClient::studio("studio-key")
        .with_base_url(&base)
        .with_polling(Duration::from_millis(5), Duration::from_secs(5));
    let media = client
        .generate_video("veo-3.1-fast-generate-001", "a bird")
        .await
        .unwrap();
    assert_eq!(media.mime_type, "video/mp4");
    assert_eq!(media.data, MP4);
    // 1 failed + 1 pending + 1 done, all with the API key header.
    assert_eq!(fake.polls.load(Ordering::SeqCst), 3);

    // A wrong key is an immediate, reported failure.
    let err = GenAiClient::studio("nope")
        .with_base_url(&base)
        .generate_image("imagen-4.0-generate-001", "x")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("HTTP 401"), "{err:#}");
}

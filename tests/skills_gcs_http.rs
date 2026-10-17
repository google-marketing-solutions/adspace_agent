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

//! HTTP-level tests for [`GcsClient`] and for GCS-backed skill staging
//! (`SkillsToolset`) against a minimal axum stand-in for the Cloud Storage
//! JSON API: object names chosen by whoever can write to the bucket must
//! never produce a file outside the skill's staging directory.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use adspace_agent::auth::{SharedTokenProvider, StaticTokenProvider};
use adspace_agent::gcs::GcsClient;
use adspace_agent::tools::skills::{SkillSources, SkillsToolset};
use axum::Router;
use axum::extract::{Path as AxumPath, Query, State as AxumState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::json;

/// In-memory bucket: object name -> bytes.
#[derive(Default)]
struct Bucket {
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
    /// Raw (still percent-encoded) object path segments seen on downloads.
    download_paths: Mutex<Vec<String>>,
    /// Objects served in pages of this many entries.
    page_size: usize,
}

fn authorised(headers: &HeaderMap) -> bool {
    headers.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer gcs-token")
}

async fn list(
    AxumState(b): AxumState<Arc<Bucket>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !authorised(&headers) {
        return (StatusCode::UNAUTHORIZED, "no").into_response();
    }
    let prefix = q.get("prefix").cloned().unwrap_or_default();
    let delimiter = q.get("delimiter").cloned();
    let objects = b.objects.lock().unwrap();
    let mut items = Vec::new();
    let mut prefixes: Vec<String> = Vec::new();
    for name in objects.keys().filter(|n| n.starts_with(&prefix)) {
        let rest = &name[prefix.len()..];
        match &delimiter {
            Some(d) if rest.contains(d.as_str()) => {
                let p = format!("{prefix}{}{d}", rest.split(d.as_str()).next().unwrap());
                if !prefixes.contains(&p) {
                    prefixes.push(p);
                }
            }
            _ => items.push(name.clone()),
        }
    }
    // Pagination: `pageToken` is the index of the first item of the page.
    let start: usize = q.get("pageToken").and_then(|t| t.parse().ok()).unwrap_or(0);
    let page_size = if b.page_size == 0 {
        usize::MAX
    } else {
        b.page_size
    };
    let end = (start + page_size).min(items.len());
    let next = (end < items.len()).then(|| end.to_string());
    let page: Vec<_> = items[start..end]
        .iter()
        .map(|n| json!({"name": n}))
        .collect();
    let mut body = json!({"items": page, "prefixes": if start == 0 { prefixes } else { vec![] }});
    if let Some(next) = next {
        body["nextPageToken"] = json!(next);
    }
    axum::Json(body).into_response()
}

async fn download(
    AxumState(b): AxumState<Arc<Bucket>>,
    headers: HeaderMap,
    AxumPath((_bucket, object)): AxumPath<(String, String)>,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
) -> Response {
    if !authorised(&headers) {
        return (StatusCode::UNAUTHORIZED, "no").into_response();
    }
    b.download_paths
        .lock()
        .unwrap()
        .push(uri.path().to_string());
    match b.objects.lock().unwrap().get(&object) {
        Some(bytes) => bytes.clone().into_response(),
        None => (StatusCode::NOT_FOUND, format!("no such object {object}")).into_response(),
    }
}

async fn upload(
    AxumState(b): AxumState<Arc<Bucket>>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
    body: String,
) -> Response {
    if !authorised(&headers) {
        return (StatusCode::UNAUTHORIZED, "no").into_response();
    }
    let name = q.get("name").cloned().unwrap_or_default();
    b.objects
        .lock()
        .unwrap()
        .insert(name.clone(), body.into_bytes());
    axum::Json(json!({"name": name})).into_response()
}

fn gcs_router(bucket: Arc<Bucket>) -> Router {
    Router::new()
        .route("/storage/v1/b/{bucket}/o", get(list))
        .route("/storage/v1/b/{bucket}/o/{object}", get(download))
        .route("/upload/storage/v1/b/{bucket}/o", post(upload))
        .with_state(bucket)
}

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, router).into_future());
    format!("http://{addr}")
}

async fn client(bucket: Arc<Bucket>) -> GcsClient {
    let base = spawn(gcs_router(bucket)).await;
    let provider: SharedTokenProvider = Arc::new(StaticTokenProvider::new("gcs-token"));
    GcsClient::new(provider).with_endpoints(
        format!("{base}/storage/v1"),
        format!("{base}/upload/storage/v1"),
    )
}

#[tokio::test]
async fn gcs_client_round_trips_objects_with_awkward_names() {
    let bucket = Arc::new(Bucket::default());
    let gcs = client(bucket.clone()).await;

    let url = gcs
        .upload_string(
            "b",
            "cm360_trafficking/s 1/My Doc+1.json",
            "{\"a\":1}",
            "application/json",
        )
        .await
        .unwrap();
    assert_eq!(url, "gs://b/cm360_trafficking/s 1/My Doc+1.json");
    assert!(
        bucket
            .objects
            .lock()
            .unwrap()
            .contains_key("cm360_trafficking/s 1/My Doc+1.json"),
        "{:?}",
        bucket.objects.lock().unwrap().keys().collect::<Vec<_>>()
    );

    // Download: the object name is one percent-encoded path segment, with
    // `%20` for the space (a `+` would have been a literal plus -> 404).
    let text = gcs
        .download_string("b", "cm360_trafficking/s 1/My Doc+1.json")
        .await
        .unwrap();
    assert_eq!(text, "{\"a\":1}");
    let paths = bucket.download_paths.lock().unwrap().clone();
    assert_eq!(
        paths,
        vec!["/storage/v1/b/b/o/cm360_trafficking%2Fs%201%2FMy%20Doc%2B1.json"]
    );

    let err = gcs.download_bytes("b", "nope").await.unwrap_err();
    assert!(err.to_string().contains("404"), "{err:#}");
}

#[tokio::test]
async fn gcs_client_lists_with_delimiter_and_pages() {
    let bucket = Arc::new(Bucket {
        page_size: 2,
        ..Bucket::default()
    });
    for name in [
        "skills/a/SKILL.md",
        "skills/a/notes.md",
        "skills/b/SKILL.md",
        "skills/b/res/x.md",
        "skills/b/res/y.md",
        "other/z",
    ] {
        bucket
            .objects
            .lock()
            .unwrap()
            .insert(name.to_string(), b"x".to_vec());
    }
    let gcs = client(bucket).await;
    let (names, prefixes) = gcs.list_objects("b", "skills/", Some("/")).await.unwrap();
    assert!(names.is_empty());
    assert_eq!(prefixes, vec!["skills/a/", "skills/b/"]);
    let (names, prefixes) = gcs.list_objects("b", "skills/b/", None).await.unwrap();
    assert_eq!(
        names,
        vec![
            "skills/b/SKILL.md",
            "skills/b/res/x.md",
            "skills/b/res/y.md"
        ],
        "all pages are followed"
    );
    assert!(prefixes.is_empty());
}

#[tokio::test]
async fn gcs_client_rejects_bad_tokens() {
    let bucket = Arc::new(Bucket::default());
    let base = spawn(gcs_router(bucket)).await;
    let provider: SharedTokenProvider = Arc::new(StaticTokenProvider::new("wrong"));
    let gcs = GcsClient::new(provider).with_endpoints(
        format!("{base}/storage/v1"),
        format!("{base}/upload/storage/v1"),
    );
    let err = gcs.list_objects("b", "", None).await.unwrap_err();
    assert!(err.to_string().contains("401"), "{err:#}");
}

fn skill_md(name: &str) -> Vec<u8> {
    format!("---\nname: {name}\ndescription: {name} skill.\n---\n\n# {name}\n\nBody.\n")
        .into_bytes()
}

/// Every file under the staging root, relative to it.
fn files_under(root: &std::path::Path) -> Vec<String> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[tokio::test]
async fn gcs_skills_are_staged_and_hostile_object_names_cannot_escape() {
    let bucket = Arc::new(Bucket::default());
    {
        let mut objects = bucket.objects.lock().unwrap();
        // A well-formed skill with a nested resource.
        objects.insert("skills/good/SKILL.md".into(), skill_md("good"));
        objects.insert("skills/good/res/notes.md".into(), b"notes".to_vec());
        objects.insert("skills/good/res/".into(), Vec::new()); // "directory" placeholder
        // A skill whose extra objects try to escape: the whole skill is refused.
        objects.insert("skills/evil/SKILL.md".into(), skill_md("evil"));
        objects.insert(
            "skills/evil//etc/cron.d/x".into(),
            b"* * * * * root evil".to_vec(),
        );
        objects.insert(
            "skills/evil/../../../escaped.txt".into(),
            b"escaped".to_vec(),
        );
        objects.insert(
            "skills/evil/skills/evil/again.md".into(),
            b"repeated prefix".to_vec(),
        );
        // Prefixes that are not plain skill ids.
        objects.insert("skills/../SKILL.md".into(), skill_md("dotdot"));
        objects.insert("skills/./SKILL.md".into(), skill_md("dot"));
        // Missing SKILL.md.
        objects.insert("skills/empty/readme.md".into(), b"x".to_vec());
    }
    let gcs = client(bucket).await;

    let sandbox = tempfile::tempdir().unwrap();
    let staging = sandbox.path().join("stage").join("inner");
    let toolset = SkillsToolset::load(SkillSources {
        local_dir: None,
        gcs: Some((gcs, "b".to_string())),
        staging_dir: staging.clone(),
    })
    .await;

    assert_eq!(toolset.skill_names().await, vec!["good"]);
    let errors = toolset.load_errors().await;
    assert!(
        errors
            .iter()
            .any(|e| e.contains("'evil'") && e.contains("unsafe")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("'empty'") && e.contains("SKILL.md not found")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("skills/../") && e.contains("unsafe skill id")),
        "{errors:?}"
    );

    // Nothing landed outside the staging generation, and nothing hostile
    // landed inside it either.
    let everything = files_under(sandbox.path());
    assert!(
        everything.iter().all(|f| f.starts_with("stage/inner/")),
        "{everything:?}"
    );
    assert!(
        everything
            .iter()
            .all(|f| !f.contains("cron") && !f.contains("escaped") && !f.contains("again")),
        "{everything:?}"
    );
    let staged: Vec<&String> = everything
        .iter()
        .filter(|f| f.contains("/.skills/good/"))
        .collect();
    assert_eq!(staged.len(), 2, "{staged:?}");
    assert!(staged.iter().any(|f| f.ends_with("/.skills/good/SKILL.md")));
    assert!(
        staged
            .iter()
            .any(|f| f.ends_with("/.skills/good/res/notes.md"))
    );
    assert!(!std::path::Path::new("/etc/cron.d/x").exists());
}

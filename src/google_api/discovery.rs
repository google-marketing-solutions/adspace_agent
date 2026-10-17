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

//! Google API Discovery Service document model and fetching.
//!
//! A Discovery document (`discovery#restDescription`) describes every REST
//! method of a Google API together with its parameters and JSON schemas. The
//! agent turns each method into an LLM tool (see [`super::spec`]).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, bail};
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

/// Default time-to-live for on-disk cached discovery documents.
pub const DEFAULT_DISCOVERY_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// A parameter of a Discovery method (or a global parameter).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryParameter {
    /// JSON type: `string`, `integer`, `number`, `boolean`, `object`, `array`, `any`.
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// Where the parameter goes: `query` or `path`.
    pub location: Option<String>,
    /// Human description.
    #[serde(default)]
    pub description: String,
    /// Whether the parameter is required.
    #[serde(default)]
    pub required: bool,
    /// Whether the parameter may be repeated.
    #[serde(default)]
    pub repeated: bool,
    /// Allowed enum values.
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<String>>,
    /// Descriptions for the enum values.
    pub enum_descriptions: Option<Vec<String>>,
    /// Value format (`int32`, `int64`, `date-time`, ...).
    pub format: Option<String>,
    /// Default value.
    pub default: Option<Value>,
    /// Regex pattern for the value.
    pub pattern: Option<String>,
}

/// A `$ref` wrapper used by `request`/`response`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SchemaRef {
    /// Name of the referenced schema in `schemas`.
    #[serde(rename = "$ref")]
    pub reference: Option<String>,
}

/// A REST method.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryMethod {
    /// Fully-qualified id, e.g. `drive.files.list`.
    #[serde(default)]
    pub id: String,
    /// URI template relative to the service path (may contain `{+name}`).
    #[serde(default)]
    pub path: String,
    /// Flattened URI template (preferred when present).
    pub flat_path: Option<String>,
    /// HTTP verb.
    #[serde(default = "default_http_method")]
    pub http_method: String,
    /// Human description.
    #[serde(default)]
    pub description: String,
    /// Declared parameters (path + query).
    #[serde(default)]
    pub parameters: IndexMap<String, DiscoveryParameter>,
    /// Required-parameter order.
    #[serde(default)]
    pub parameter_order: Vec<String>,
    /// Request body schema reference.
    pub request: Option<SchemaRef>,
    /// Response body schema reference.
    pub response: Option<SchemaRef>,
    /// OAuth scopes accepted by the method.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Whether the method supports media upload.
    #[serde(default)]
    pub supports_media_upload: bool,
    /// Whether the method supports media download.
    #[serde(default)]
    pub supports_media_download: bool,
}

fn default_http_method() -> String {
    "GET".to_string()
}

/// A resource grouping methods and nested resources.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryResource {
    /// Methods on this resource.
    #[serde(default)]
    pub methods: IndexMap<String, DiscoveryMethod>,
    /// Nested resources.
    #[serde(default)]
    pub resources: IndexMap<String, DiscoveryResource>,
}

/// OAuth 2.0 scope descriptions.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DiscoveryOAuth2 {
    /// Scope URL → `{ "description": ... }`.
    #[serde(default)]
    pub scopes: IndexMap<String, Value>,
}

/// Authentication block.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DiscoveryAuth {
    /// OAuth 2.0 configuration.
    pub oauth2: Option<DiscoveryOAuth2>,
}

/// A parsed Discovery document.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryDocument {
    /// API name, e.g. `drive`.
    #[serde(default)]
    pub name: String,
    /// API version, e.g. `v3`.
    #[serde(default)]
    pub version: String,
    /// Human title.
    #[serde(default)]
    pub title: String,
    /// Human description.
    #[serde(default)]
    pub description: String,
    /// Link to the API documentation.
    pub documentation_link: Option<String>,
    /// Root URL, e.g. `https://www.googleapis.com/`.
    #[serde(default)]
    pub root_url: String,
    /// Service path, e.g. `drive/v3/`.
    #[serde(default)]
    pub service_path: String,
    /// Global parameters (`alt`, `fields`, ...). Not exposed as tool params.
    #[serde(default)]
    pub parameters: IndexMap<String, DiscoveryParameter>,
    /// Authentication information.
    pub auth: Option<DiscoveryAuth>,
    /// Named JSON schemas.
    #[serde(default)]
    pub schemas: IndexMap<String, Value>,
    /// Top-level resources.
    #[serde(default)]
    pub resources: IndexMap<String, DiscoveryResource>,
    /// Top-level methods (rare).
    #[serde(default)]
    pub methods: IndexMap<String, DiscoveryMethod>,
}

impl DiscoveryDocument {
    /// Parses a Discovery document from JSON text.
    ///
    /// # Errors
    ///
    /// Returns an error when the JSON is not a valid Discovery document or
    /// its `rootUrl` is not an `https://*.googleapis.com/` origin (the root
    /// URL decides where bearer tokens are sent, so it is never trusted
    /// blindly — least of all from an on-disk cache).
    pub fn parse(json: &str) -> anyhow::Result<Self> {
        let doc: Self = serde_json::from_str(json).context("invalid Discovery document")?;
        if doc.root_url.is_empty() {
            bail!("Discovery document is missing rootUrl");
        }
        validate_root_url(&doc.root_url)?;
        Ok(doc)
    }

    /// Base URL for REST calls: `rootUrl + servicePath` without trailing slash.
    pub fn base_url(&self) -> String {
        let mut base = format!("{}{}", self.root_url, self.service_path);
        while base.ends_with('/') {
            base.pop();
        }
        base
    }

    /// All OAuth 2.0 scopes declared by the API, in document order.
    pub fn oauth_scopes(&self) -> Vec<String> {
        self.auth
            .as_ref()
            .and_then(|a| a.oauth2.as_ref())
            .map(|o| o.scopes.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Every method in the document, in the same order the Python ADK
    /// converter visits them: resources depth-first (a resource's methods,
    /// then its nested resources), followed by top-level methods.
    pub fn all_methods(&self) -> Vec<&DiscoveryMethod> {
        fn walk<'a>(
            resources: &'a IndexMap<String, DiscoveryResource>,
            out: &mut Vec<&'a DiscoveryMethod>,
        ) {
            for resource in resources.values() {
                out.extend(resource.methods.values());
                walk(&resource.resources, out);
            }
        }
        let mut out = Vec::new();
        walk(&self.resources, &mut out);
        out.extend(self.methods.values());
        out
    }
}

/// Candidate URLs for a Discovery document, in the order the Google API client
/// libraries try them.
pub fn default_discovery_urls(api_name: &str, api_version: &str) -> [String; 2] {
    [
        format!("https://www.googleapis.com/discovery/v1/apis/{api_name}/{api_version}/rest"),
        format!("https://{api_name}.googleapis.com/$discovery/rest?version={api_version}"),
    ]
}

/// Checks that a `rootUrl` is an `https://` origin on `googleapis.com` (or a
/// sub-domain of it). Every tool of the API sends the user's bearer token to
/// this host, so a document naming any other host is rejected.
///
/// # Errors
///
/// Returns an error describing why the URL is not acceptable.
pub fn validate_root_url(root_url: &str) -> anyhow::Result<()> {
    let url = url::Url::parse(root_url).with_context(|| format!("invalid rootUrl '{root_url}'"))?;
    if url.scheme() != "https" {
        bail!("rootUrl '{root_url}' must use https");
    }
    let host = url
        .host_str()
        .with_context(|| format!("rootUrl '{root_url}' has no host"))?;
    let host = host.to_ascii_lowercase();
    if host != "googleapis.com" && !host.ends_with(".googleapis.com") {
        bail!("rootUrl '{root_url}' is not a googleapis.com host");
    }
    if url.username() != "" || url.password().is_some() {
        bail!("rootUrl '{root_url}' must not carry credentials");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("rootUrl '{root_url}' must not carry a query or fragment");
    }
    Ok(())
}

/// Upper bound for a Discovery document body (the largest live documents are
/// a few megabytes).
const MAX_DISCOVERY_BODY_BYTES: usize = 32 * 1024 * 1024;

/// Fetches and caches Discovery documents.
#[derive(Debug, Clone)]
pub struct DiscoveryFetcher {
    http: reqwest::Client,
    cache_dir: Option<PathBuf>,
    cache_ttl: Duration,
}

impl Default for DiscoveryFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscoveryFetcher {
    /// Creates a fetcher with an on-disk cache in a per-user directory
    /// (override with `DISCOVERY_CACHE_DIR`; set it to `off` to disable).
    ///
    /// The default location is `$XDG_CACHE_HOME/adspace-agent/discovery`,
    /// then `$HOME/.cache/adspace-agent/discovery`, and only when neither is
    /// available a per-user directory under the OS temp dir. Cached documents
    /// are validated like fetched ones (see [`DiscoveryDocument::parse`]).
    pub fn new() -> Self {
        let cache_dir = match std::env::var("DISCOVERY_CACHE_DIR") {
            Ok(dir) if dir.eq_ignore_ascii_case("off") || dir.is_empty() => None,
            Ok(dir) => Some(PathBuf::from(dir)),
            Err(_) => Some(default_cache_dir()),
        };
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .connect_timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            cache_dir,
            cache_ttl: DEFAULT_DISCOVERY_CACHE_TTL,
        }
    }

    /// Disables the on-disk cache.
    pub fn without_cache(mut self) -> Self {
        self.cache_dir = None;
        self
    }

    /// Uses a specific cache directory.
    pub fn with_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(dir.into());
        self
    }

    /// Uses a specific time-to-live for cached documents.
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// The on-disk cache directory, if caching is enabled.
    pub fn cache_dir(&self) -> Option<&Path> {
        self.cache_dir.as_deref()
    }

    fn cache_path(
        &self,
        api_name: &str,
        api_version: &str,
        discovery_url: Option<&str>,
    ) -> Option<PathBuf> {
        let safe = |s: &str| {
            s.replace(
                |c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-',
                "_",
            )
        };
        // An explicit discovery URL yields a different document than the
        // standard ones, so it gets its own cache entry.
        let suffix = discovery_url.map_or_else(String::new, |url| {
            use sha2::Digest as _;
            let digest = sha2::Sha256::digest(url.as_bytes());
            let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
            format!("_{hex}")
        });
        self.cache_dir.as_ref().map(|d| {
            d.join(format!(
                "{}_{}{suffix}.json",
                safe(api_name),
                safe(api_version)
            ))
        })
    }

    /// Reads a cached document body. With `allow_stale`, entries older than
    /// the TTL are returned too (used as a fallback when every fetch fails).
    fn read_cache(&self, path: &Path, allow_stale: bool) -> Option<String> {
        let meta = std::fs::metadata(path).ok()?;
        if !meta.is_file() {
            return None;
        }
        let age = SystemTime::now()
            .duration_since(meta.modified().ok()?)
            .ok()?;
        if !allow_stale && age > self.cache_ttl {
            return None;
        }
        std::fs::read_to_string(path).ok()
    }

    /// Writes a document body to the cache atomically: the body goes to a
    /// sibling temp file first and is renamed into place, so concurrent
    /// loaders never observe a torn file and an existing symlink is replaced
    /// rather than followed.
    fn write_cache(&self, path: &Path, body: &str) {
        let Some(parent) = path.parent() else {
            return;
        };
        if let Err(e) = create_private_dir(parent) {
            tracing::debug!(
                "could not create discovery cache dir {}: {e}",
                parent.display()
            );
            return;
        }
        let tmp = parent.join(format!(
            ".{}.{}.tmp",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("discovery"),
            uuid::Uuid::new_v4().simple()
        ));
        let result = std::fs::write(&tmp, body).and_then(|()| std::fs::rename(&tmp, path));
        if let Err(e) = result {
            let _ = std::fs::remove_file(&tmp);
            tracing::debug!("could not write discovery cache {}: {e}", path.display());
        }
    }

    /// Fetches the Discovery document for `api_name`/`api_version`.
    ///
    /// When `discovery_url` is given it is used verbatim; otherwise the
    /// standard discovery URLs are tried in order. A fresh cache entry is
    /// used first; when every URL fails, a stale cache entry (if any) is used
    /// rather than failing the load.
    ///
    /// # Errors
    ///
    /// Returns an error when no candidate URL yields a valid document and no
    /// cached copy exists.
    pub async fn fetch(
        &self,
        api_name: &str,
        api_version: &str,
        discovery_url: Option<&str>,
    ) -> anyhow::Result<DiscoveryDocument> {
        let cache_path = self.cache_path(api_name, api_version, discovery_url);
        if let Some(path) = &cache_path
            && let Some(body) = self.read_cache(path, false)
        {
            match DiscoveryDocument::parse(&body) {
                Ok(doc) => {
                    tracing::debug!(
                        "Loaded cached Discovery document for {api_name} {api_version}"
                    );
                    return Ok(doc);
                }
                Err(e) => tracing::warn!(
                    "Ignoring invalid cached Discovery document {}: {e}",
                    path.display()
                ),
            }
        }

        let candidates: Vec<String> = match discovery_url {
            Some(url) => vec![url.to_string()],
            None => default_discovery_urls(api_name, api_version).to_vec(),
        };

        let mut errors = Vec::new();
        for url in &candidates {
            tracing::info!("Fetching Discovery document for {api_name} {api_version} from {url}");
            match self.http.get(url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let body = match read_body_limited(resp, MAX_DISCOVERY_BODY_BYTES).await {
                        Ok(body) => body,
                        Err(e) => {
                            errors.push(format!("{url}: {e}"));
                            continue;
                        }
                    };
                    match DiscoveryDocument::parse(&body) {
                        Ok(doc) => {
                            if let Some(path) = &cache_path {
                                self.write_cache(path, &body);
                            }
                            return Ok(doc);
                        }
                        Err(e) => errors.push(format!("{url}: {e}")),
                    }
                }
                Ok(resp) => errors.push(format!("{url}: HTTP {}", resp.status())),
                Err(e) => errors.push(format!("{url}: {e}")),
            }
        }

        // Every fetch failed: fall back to a stale cache entry if there is one.
        if let Some(path) = &cache_path
            && let Some(body) = self.read_cache(path, true)
            && let Ok(doc) = DiscoveryDocument::parse(&body)
        {
            tracing::warn!(
                "Using stale cached Discovery document for {api_name} {api_version} after fetch failures: {}",
                errors.join("; ")
            );
            return Ok(doc);
        }
        bail!(
            "Failed to fetch Discovery document for {api_name} {api_version}: {}",
            errors.join("; ")
        )
    }
}

/// Reads a response body, failing once it exceeds `limit` bytes.
async fn read_body_limited(resp: reqwest::Response, limit: usize) -> anyhow::Result<String> {
    use futures::StreamExt as _;
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if buf.len() + chunk.len() > limit {
            bail!("response body exceeds {limit} bytes");
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf).context("response body is not valid UTF-8")
}

/// Per-user default cache directory for Discovery documents.
fn default_cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            // Last resort: a per-user directory under the shared temp dir.
            let user = std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| std::process::id().to_string());
            std::env::temp_dir().join(format!("adspace-agent-{user}"))
        });
    base.join("adspace-agent").join("discovery")
}

/// Creates `dir` (and parents) readable and writable by the owner only.
/// Existing directories keep their mode (the operator may have chosen them).
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{
      "kind": "discovery#restDescription",
      "name": "demo", "version": "v1", "title": "Demo API",
      "rootUrl": "https://demo.googleapis.com/", "servicePath": "demo/v1/",
      "auth": {"oauth2": {"scopes": {"https://www.googleapis.com/auth/demo": {"description": "Demo"}}}},
      "schemas": {"Thing": {"id": "Thing", "type": "object", "properties": {"name": {"type": "string"}}}},
      "resources": {
        "things": {
          "methods": {"list": {"id": "demo.things.list", "path": "things", "flatPath": "things", "httpMethod": "GET"}},
          "resources": {"parts": {"methods": {"get": {"id": "demo.things.parts.get", "path": "things/{+name}", "flatPath": "things/{thingsId}/parts/{partsId}", "httpMethod": "GET"}}}}
        }
      },
      "methods": {"ping": {"id": "demo.ping", "path": "ping", "httpMethod": "POST"}}
    }"#;

    #[test]
    fn parses_and_walks_methods_in_python_order() {
        let doc = DiscoveryDocument::parse(DOC).unwrap();
        assert_eq!(doc.base_url(), "https://demo.googleapis.com/demo/v1");
        assert_eq!(
            doc.oauth_scopes(),
            vec!["https://www.googleapis.com/auth/demo"]
        );
        let ids: Vec<&str> = doc.all_methods().iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["demo.things.list", "demo.things.parts.get", "demo.ping"]
        );
    }

    #[test]
    fn rejects_documents_without_root_url() {
        assert!(DiscoveryDocument::parse(r#"{"name":"x"}"#).is_err());
    }

    #[test]
    fn rejects_documents_whose_root_url_is_not_a_googleapis_origin() {
        for bad in [
            "https://attacker.example/",
            "http://www.googleapis.com/",
            "https://googleapis.com.evil.net/",
            "https://evilgoogleapis.com/",
            "https://user:pw@www.googleapis.com/",
            "https://www.googleapis.com/?x=1",
            "not a url",
        ] {
            let json = format!(r#"{{"name":"x","version":"v1","rootUrl":"{bad}"}}"#);
            assert!(
                DiscoveryDocument::parse(&json).is_err(),
                "{bad} must be rejected"
            );
            assert!(validate_root_url(bad).is_err(), "{bad} must be rejected");
        }
        for ok in [
            "https://www.googleapis.com/",
            "https://googleads.googleapis.com/",
            "https://GOOGLEAPIS.com/",
            "https://displayvideo.googleapis.com/v4/",
        ] {
            assert!(validate_root_url(ok).is_ok(), "{ok} must be accepted");
        }
    }

    #[test]
    fn default_urls_follow_client_library_order() {
        let urls = default_discovery_urls("drive", "v3");
        assert_eq!(
            urls[0],
            "https://www.googleapis.com/discovery/v1/apis/drive/v3/rest"
        );
        assert_eq!(
            urls[1],
            "https://drive.googleapis.com/$discovery/rest?version=v3"
        );
    }

    #[test]
    fn cache_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let fetcher = DiscoveryFetcher::new().with_cache_dir(dir.path().join("cache"));
        let path = fetcher.cache_path("demo", "v1", None).unwrap();
        fetcher.write_cache(&path, DOC);
        assert_eq!(fetcher.read_cache(&path, false).as_deref(), Some(DOC));
        let expired = DiscoveryFetcher {
            cache_ttl: Duration::ZERO,
            ..fetcher.clone()
        };
        std::thread::sleep(Duration::from_millis(5));
        assert!(expired.read_cache(&path, false).is_none());
        assert_eq!(
            expired.read_cache(&path, true).as_deref(),
            Some(DOC),
            "stale entries are still readable for the fallback"
        );

        // The write is atomic: no temp files are left behind, and the cache
        // directory is private to the user.
        let entries: Vec<String> = std::fs::read_dir(dir.path().join("cache"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["demo_v1.json"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(dir.path().join("cache"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }

    #[test]
    fn cache_key_includes_the_discovery_url() {
        let fetcher = DiscoveryFetcher::new().with_cache_dir("/cache");
        let plain = fetcher.cache_path("demo", "v1", None).unwrap();
        let custom = fetcher
            .cache_path(
                "demo",
                "v1",
                Some("https://demo.googleapis.com/$discovery/rest"),
            )
            .unwrap();
        let other = fetcher
            .cache_path(
                "demo",
                "v1",
                Some("https://other.googleapis.com/$discovery/rest"),
            )
            .unwrap();
        assert_eq!(plain, Path::new("/cache/demo_v1.json"));
        assert_ne!(plain, custom);
        assert_ne!(custom, other);
        assert!(custom.to_string_lossy().starts_with("/cache/demo_v1_"));
        // Unsafe characters in api/version never escape the cache dir.
        let weird = fetcher.cache_path("../x", "v/1", None).unwrap();
        assert_eq!(weird, Path::new("/cache/___x_v_1.json"));
    }
}

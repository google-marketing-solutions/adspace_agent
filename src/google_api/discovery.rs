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
    /// Returns an error when the JSON is not a valid Discovery document.
    pub fn parse(json: &str) -> anyhow::Result<Self> {
        let doc: Self = serde_json::from_str(json).context("invalid Discovery document")?;
        if doc.root_url.is_empty() {
            bail!("Discovery document is missing rootUrl");
        }
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
    /// Creates a fetcher with an on-disk cache under the OS cache directory
    /// (override with `DISCOVERY_CACHE_DIR`; set it to `off` to disable).
    pub fn new() -> Self {
        let cache_dir = match std::env::var("DISCOVERY_CACHE_DIR") {
            Ok(dir) if dir.eq_ignore_ascii_case("off") || dir.is_empty() => None,
            Ok(dir) => Some(PathBuf::from(dir)),
            Err(_) => std::env::temp_dir()
                .join("adspace-agent")
                .join("discovery")
                .into(),
        };
        Self {
            http: reqwest::Client::new(),
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

    fn cache_path(&self, api_name: &str, api_version: &str) -> Option<PathBuf> {
        let safe = |s: &str| {
            s.replace(
                |c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-',
                "_",
            )
        };
        self.cache_dir
            .as_ref()
            .map(|d| d.join(format!("{}_{}.json", safe(api_name), safe(api_version))))
    }

    fn read_cache(&self, path: &Path) -> Option<String> {
        let meta = std::fs::metadata(path).ok()?;
        let age = SystemTime::now()
            .duration_since(meta.modified().ok()?)
            .ok()?;
        if age > self.cache_ttl {
            return None;
        }
        std::fs::read_to_string(path).ok()
    }

    fn write_cache(&self, path: &Path, body: &str) {
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::debug!(
                "could not create discovery cache dir {}: {e}",
                parent.display()
            );
            return;
        }
        if let Err(e) = std::fs::write(path, body) {
            tracing::debug!("could not write discovery cache {}: {e}", path.display());
        }
    }

    /// Fetches the Discovery document for `api_name`/`api_version`.
    ///
    /// When `discovery_url` is given it is used verbatim; otherwise the
    /// standard discovery URLs are tried in order.
    ///
    /// # Errors
    ///
    /// Returns an error when no candidate URL yields a valid document.
    pub async fn fetch(
        &self,
        api_name: &str,
        api_version: &str,
        discovery_url: Option<&str>,
    ) -> anyhow::Result<DiscoveryDocument> {
        let cache_path = self.cache_path(api_name, api_version);
        if let Some(path) = &cache_path
            && let Some(body) = self.read_cache(path)
            && let Ok(doc) = DiscoveryDocument::parse(&body)
        {
            tracing::debug!("Loaded cached Discovery document for {api_name} {api_version}");
            return Ok(doc);
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
                    let body = resp.text().await?;
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
        bail!(
            "Failed to fetch Discovery document for {api_name} {api_version}: {}",
            errors.join("; ")
        )
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
        let fetcher = DiscoveryFetcher::new().with_cache_dir(dir.path());
        let path = fetcher.cache_path("demo", "v1").unwrap();
        fetcher.write_cache(&path, DOC);
        assert_eq!(fetcher.read_cache(&path).as_deref(), Some(DOC));
        let expired = DiscoveryFetcher {
            cache_ttl: Duration::ZERO,
            ..fetcher.clone()
        };
        std::thread::sleep(Duration::from_millis(5));
        assert!(expired.read_cache(&path).is_none());
    }
}

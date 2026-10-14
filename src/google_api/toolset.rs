//! `GoogleApiToolset`: one LLM tool per REST method of a Google API.

use std::sync::Arc;

use adk_rust::{ReadonlyContext, Result, Tool, ToolContext, Toolset};
use async_trait::async_trait;
use serde_json::{Value, json};

use super::discovery::{DiscoveryDocument, DiscoveryFetcher};
use super::spec::{MethodSpec, SpecOptions, build_method_specs};
use crate::auth::{SessionCredentialManager, auth_required};

/// Configuration of a [`GoogleApiToolset`].
#[derive(Debug, Clone)]
pub struct GoogleApiToolsetConfig {
    /// Toolset name shown to the framework (e.g. `display_video_360`).
    pub toolset_name: String,
    /// Discovery API name (e.g. `displayvideo`).
    pub api_name: String,
    /// Discovery API version (e.g. `v4`).
    pub api_version: String,
    /// Explicit Discovery document URL (otherwise the standard URLs are tried).
    pub discovery_url: Option<String>,
    /// Allow-list of tool names; `None` exposes every method.
    pub tool_filter: Option<Vec<String>>,
    /// Extra HTTP headers sent with every request (e.g. `developer-token`).
    pub additional_headers: Vec<(String, String)>,
    /// OAuth scopes to request for every tool of the toolset. When `None`,
    /// each tool requests the least-privileged scope its own method accepts
    /// (see [`least_privilege_scope`]).
    pub scopes: Option<Vec<String>>,
    /// Optional prefix prepended to every tool name.
    pub tool_name_prefix: Option<String>,
    /// Schema generation options.
    pub spec_options: SpecOptions,
}

impl GoogleApiToolsetConfig {
    /// Creates a config for `api_name`/`api_version` named `toolset_name`.
    pub fn new(
        toolset_name: impl Into<String>,
        api_name: impl Into<String>,
        api_version: impl Into<String>,
    ) -> Self {
        Self {
            toolset_name: toolset_name.into(),
            api_name: api_name.into(),
            api_version: api_version.into(),
            discovery_url: None,
            tool_filter: None,
            additional_headers: Vec::new(),
            scopes: None,
            tool_name_prefix: None,
            spec_options: SpecOptions::default(),
        }
    }

    /// Restricts the toolset to the given tool names.
    pub fn with_tool_filter<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tool_filter = Some(names.into_iter().map(Into::into).collect());
        self
    }

    /// Restricts the toolset when a filter is provided.
    pub fn with_optional_tool_filter(mut self, filter: Option<Vec<String>>) -> Self {
        self.tool_filter = filter;
        self
    }

    /// Uses an explicit Discovery document URL.
    pub fn with_discovery_url(mut self, url: impl Into<String>) -> Self {
        self.discovery_url = Some(url.into());
        self
    }

    /// Adds a header to every request.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.additional_headers.push((name.into(), value.into()));
        self
    }

    /// Overrides the OAuth scopes requested for this API.
    pub fn with_scopes<I, S>(mut self, scopes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.scopes = Some(scopes.into_iter().map(Into::into).collect());
        self
    }

    /// Sets the schema generation options.
    pub fn with_spec_options(mut self, options: SpecOptions) -> Self {
        self.spec_options = options;
        self
    }
}

/// Shared pieces every tool of a toolset needs.
struct ToolShared {
    http: reqwest::Client,
    credentials: Arc<SessionCredentialManager>,
    additional_headers: Vec<(String, String)>,
}

/// Markers that identify a read-only scope (`drive.readonly`,
/// `devstorage.read_only`, `cloud-platform.read-only`).
const READ_ONLY_MARKERS: [&str; 3] = [".readonly", ".read_only", ".read-only"];

/// Scopes that are far broader than any single API (or reserved for content
/// partners) and are only chosen when a method accepts nothing else.
fn is_umbrella_scope(scope: &str) -> bool {
    let name = scope.rsplit('/').next().unwrap_or(scope);
    name.starts_with("cloud-platform")
        || name.ends_with("full_control")
        || name.starts_with("youtubepartner")
}

/// The scope with the read-only marker removed (`drive.readonly` -> `drive`).
fn scope_stem(scope: &str) -> &str {
    READ_ONLY_MARKERS
        .iter()
        .find_map(|m| scope.strip_suffix(m))
        .unwrap_or(scope)
}

/// Picks the least-privileged scope that authorizes a method.
///
/// Discovery lists, per method, the scopes any one of which is sufficient.
/// Preference: a read-only scope whenever the method accepts one; then
/// anything that is not an umbrella scope; within a group the most general
/// stem (`drive.readonly` over `drive.metadata.readonly`), Discovery order
/// breaking ties. Methods that declare no scopes fall back to the API's list.
pub fn least_privilege_scope(method_scopes: &[String], api_scopes: &[String]) -> Option<String> {
    let candidates: &[String] = if method_scopes.is_empty() {
        api_scopes
    } else {
        method_scopes
    };
    let pick = |group: Vec<&String>| -> Option<String> {
        let specific: Vec<&String> = group
            .iter()
            .copied()
            .filter(|s| !is_umbrella_scope(s))
            .collect();
        let pool = if specific.is_empty() { group } else { specific };
        pool.into_iter()
            .min_by_key(|s| scope_stem(s).len())
            .cloned()
    };
    let read_only: Vec<&String> = candidates
        .iter()
        .filter(|s| READ_ONLY_MARKERS.iter().any(|m| s.ends_with(m)))
        .collect();
    if !read_only.is_empty() {
        return pick(read_only);
    }
    pick(candidates.iter().collect())
}

/// A single Google API method exposed as a tool.
pub struct GoogleApiTool {
    name: String,
    description: String,
    spec: Arc<MethodSpec>,
    /// Scopes this tool asks the session for (least privilege by default).
    scopes: Vec<String>,
    shared: Arc<ToolShared>,
}

impl GoogleApiTool {
    /// The underlying method spec.
    pub fn spec(&self) -> &MethodSpec {
        &self.spec
    }

    /// OAuth scopes this tool requests.
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    async fn call(&self, ctx: &Arc<dyn ToolContext>, args: Value) -> anyhow::Result<Value> {
        let token = self
            .shared
            .credentials
            .access_token(ctx.as_ref(), &self.scopes)
            .await?;
        let prepared = self.spec.prepare_request(&args)?;
        let method = reqwest::Method::from_bytes(prepared.method.as_bytes())?;
        let mut req = self
            .shared
            .http
            .request(method, &prepared.url)
            .bearer_auth(token)
            .header(
                reqwest::header::USER_AGENT,
                format!(
                    "adspace-agent/{} adk-rust (tool: {})",
                    env!("CARGO_PKG_VERSION"),
                    self.name
                ),
            )
            .header(reqwest::header::ACCEPT, "application/json");
        if !prepared.query.is_empty() {
            req = req.query(&prepared.query);
        }
        for (name, value) in &self.shared.additional_headers {
            req = req.header(name.as_str(), value.as_str());
        }
        if let Some(body) = &prepared.body {
            req = req.json(body);
        }
        tracing::info!("Calling {} {}", prepared.method, prepared.url);
        let resp = req.send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        let body: Value =
            serde_json::from_str(&text).unwrap_or_else(|_| json!({ "content": text }));
        if status.is_success() {
            Ok(body)
        } else {
            Ok(json!({
                "error": {
                    "status": status.as_u16(),
                    "message": format!("{} {} failed with HTTP {status}", prepared.method, prepared.url),
                    "details": body,
                }
            }))
        }
    }
}

#[async_trait]
impl Tool for GoogleApiTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(self.spec.parameters_schema())
    }

    fn is_read_only(&self) -> bool {
        self.spec.http_method == "GET"
    }

    fn is_concurrency_safe(&self) -> bool {
        self.spec.http_method == "GET"
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        match self.call(&ctx, args).await {
            Ok(value) => Ok(value),
            Err(e) => {
                if let Some(required) = auth_required(&e) {
                    return Ok(required.to_tool_response());
                }
                Ok(json!({ "error": { "message": format!("{e:#}") } }))
            }
        }
    }
}

/// A toolset exposing a Google API described by a Discovery document.
pub struct GoogleApiToolset {
    name: String,
    api_name: String,
    api_version: String,
    scopes: Vec<String>,
    scopes_by_tool: Vec<(String, Vec<String>)>,
    tools: Vec<Arc<dyn Tool>>,
}

impl GoogleApiToolset {
    /// Fetches the Discovery document and builds the toolset.
    ///
    /// # Errors
    ///
    /// Returns an error when the Discovery document cannot be fetched.
    pub async fn load(
        config: GoogleApiToolsetConfig,
        fetcher: &DiscoveryFetcher,
        credentials: Arc<SessionCredentialManager>,
    ) -> anyhow::Result<Self> {
        let doc = fetcher
            .fetch(
                &config.api_name,
                &config.api_version,
                config.discovery_url.as_deref(),
            )
            .await?;
        Ok(Self::from_document(&doc, config, credentials))
    }

    /// Builds the toolset from an already-parsed Discovery document.
    pub fn from_document(
        doc: &DiscoveryDocument,
        config: GoogleApiToolsetConfig,
        credentials: Arc<SessionCredentialManager>,
    ) -> Self {
        let api_scopes = doc.oauth_scopes();
        let shared = Arc::new(ToolShared {
            http: reqwest::Client::new(),
            credentials,
            additional_headers: config.additional_headers.clone(),
        });
        let prefix = config.tool_name_prefix.clone().unwrap_or_default();
        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();
        let mut scopes: Vec<String> = Vec::new();
        let mut scopes_by_tool: Vec<(String, Vec<String>)> = Vec::new();
        for spec in build_method_specs(doc, &config.spec_options) {
            let name = format!("{prefix}{}", spec.tool_name);
            if let Some(filter) = &config.tool_filter
                && !filter.iter().any(|f| f == &name || f == &spec.tool_name)
            {
                continue;
            }
            let description = if spec.description.trim().is_empty() {
                format!(
                    "Calls the {} method of the {} {} API.",
                    spec.method_id, config.api_name, config.api_version
                )
            } else {
                spec.description.clone()
            };
            let tool_scopes = match &config.scopes {
                Some(explicit) => explicit.clone(),
                None => least_privilege_scope(&spec.scopes, &api_scopes)
                    .into_iter()
                    .collect(),
            };
            for scope in &tool_scopes {
                if !scopes.contains(scope) {
                    scopes.push(scope.clone());
                }
            }
            scopes_by_tool.push((name.clone(), tool_scopes.clone()));
            tools.push(Arc::new(GoogleApiTool {
                name,
                description,
                spec: Arc::new(spec),
                scopes: tool_scopes,
                shared: shared.clone(),
            }));
        }
        if let Some(filter) = &config.tool_filter {
            let available: Vec<&str> = tools.iter().map(|t| t.name()).collect();
            for wanted in filter {
                if !available.contains(&wanted.as_str()) {
                    tracing::warn!(
                        "Tool filter entry '{wanted}' does not match any method of {} {}",
                        config.api_name,
                        config.api_version
                    );
                }
            }
        }
        tracing::info!(
            "Loaded {} tools for {} {} ({})",
            tools.len(),
            config.api_name,
            config.api_version,
            config.toolset_name
        );
        Self {
            name: config.toolset_name,
            api_name: config.api_name,
            api_version: config.api_version,
            scopes,
            scopes_by_tool,
            tools,
        }
    }

    /// The scopes each tool requests, in tool order.
    pub fn tool_scopes(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.scopes_by_tool
            .iter()
            .map(|(name, scopes)| (name.as_str(), scopes.as_slice()))
    }

    /// Distinct OAuth scopes requested across the toolset's tools (each tool
    /// only ever asks for its own).
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    /// Names of the exposed tools, in declaration order.
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name().to_string()).collect()
    }

    /// Discovery API name.
    pub fn api_name(&self) -> &str {
        &self.api_name
    }

    /// Discovery API version.
    pub fn api_version(&self) -> &str {
        &self.api_version
    }
}

#[async_trait]
impl Toolset for GoogleApiToolset {
    fn name(&self) -> &str {
        &self.name
    }

    async fn tools(&self, _ctx: Arc<dyn ReadonlyContext>) -> Result<Vec<Arc<dyn Tool>>> {
        Ok(self.tools.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{GoogleOAuthFlow, OAuthClientConfig};

    fn credentials() -> Arc<SessionCredentialManager> {
        Arc::new(SessionCredentialManager::new(GoogleOAuthFlow::new(
            OAuthClientConfig {
                client_id: "id".into(),
                client_secret: "secret".into(),
                redirect_uri: "http://localhost/oauth2/callback".into(),
            },
        )))
    }

    fn doc() -> DiscoveryDocument {
        DiscoveryDocument::parse(
            r#"{"name":"drive","version":"v3","rootUrl":"https://www.googleapis.com/","servicePath":"drive/v3/",
            "auth":{"oauth2":{"scopes":{"https://www.googleapis.com/auth/drive":{"description":"full"},"https://www.googleapis.com/auth/drive.readonly":{"description":"ro"}}}},
            "resources":{"files":{"methods":{
              "list":{"id":"drive.files.list","path":"files","flatPath":"files","httpMethod":"GET","description":"Lists files.","parameters":{"q":{"type":"string","location":"query"}},"scopes":["https://www.googleapis.com/auth/drive","https://www.googleapis.com/auth/drive.readonly"]},
              "get":{"id":"drive.files.get","path":"files/{fileId}","flatPath":"files/{fileId}","httpMethod":"GET","parameters":{"fileId":{"type":"string","location":"path","required":true}},"scopes":["https://www.googleapis.com/auth/drive","https://www.googleapis.com/auth/drive.readonly"]},
              "delete":{"id":"drive.files.delete","path":"files/{fileId}","flatPath":"files/{fileId}","httpMethod":"DELETE","parameters":{"fileId":{"type":"string","location":"path","required":true}},"scopes":["https://www.googleapis.com/auth/drive"]}
            }}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn builds_filtered_tools_with_least_privilege_scopes() {
        let config = GoogleApiToolsetConfig::new("drive", "drive", "v3").with_tool_filter([
            "drive_files_list",
            "drive_files_get",
            "missing_tool",
        ]);
        let toolset = GoogleApiToolset::from_document(&doc(), config, credentials());
        assert_eq!(
            toolset.tool_names(),
            vec!["drive_files_list", "drive_files_get"]
        );
        // Reads ask for the read-only scope only.
        assert_eq!(
            toolset.scopes(),
            &["https://www.googleapis.com/auth/drive.readonly".to_string()]
        );
    }

    #[test]
    fn each_tool_requests_only_what_its_method_needs() {
        let toolset = GoogleApiToolset::from_document(
            &doc(),
            GoogleApiToolsetConfig::new("drive", "drive", "v3"),
            credentials(),
        );
        let scopes: Vec<(&str, &[String])> = toolset.tool_scopes().collect();
        assert_eq!(
            scopes,
            vec![
                (
                    "drive_files_list",
                    &["https://www.googleapis.com/auth/drive.readonly".to_string()][..]
                ),
                (
                    "drive_files_get",
                    &["https://www.googleapis.com/auth/drive.readonly".to_string()][..]
                ),
                (
                    "drive_files_delete",
                    &["https://www.googleapis.com/auth/drive".to_string()][..]
                ),
            ]
        );
        assert_eq!(toolset.scopes().len(), 2, "distinct union across tools");
    }

    fn scopes(names: &[&str]) -> Vec<String> {
        names
            .iter()
            .map(|n| format!("https://www.googleapis.com/auth/{n}"))
            .collect()
    }

    /// Pinned against the live Discovery documents (October 2026).
    #[test]
    fn least_privilege_scope_follows_the_discovery_data() {
        let pick = |method: &[&str], api: &[&str]| {
            least_privilege_scope(&scopes(method), &scopes(api))
                .map(|s| s.rsplit('/').next().unwrap().to_string())
        };
        // youtube.videos.list: the read-only scope wins over youtube/force-ssl/partner.
        assert_eq!(
            pick(
                &[
                    "youtube",
                    "youtube.force-ssl",
                    "youtube.readonly",
                    "youtubepartner"
                ],
                &[]
            )
            .as_deref(),
            Some("youtube.readonly")
        );
        // youtube.commentThreads.list only accepts force-ssl.
        assert_eq!(
            pick(&["youtube.force-ssl"], &[]).as_deref(),
            Some("youtube.force-ssl")
        );
        // youtube.captions.list: partner scopes are a last resort.
        assert_eq!(
            pick(&["youtube.force-ssl", "youtubepartner"], &[]).as_deref(),
            Some("youtube.force-ssl")
        );
        // drive.files.get: the general read-only scope, not meet/metadata/photos.
        assert_eq!(
            pick(
                &[
                    "drive",
                    "drive.appdata",
                    "drive.file",
                    "drive.meet.readonly",
                    "drive.metadata",
                    "drive.metadata.readonly",
                    "drive.photos.readonly",
                    "drive.readonly"
                ],
                &[]
            )
            .as_deref(),
            Some("drive.readonly")
        );
        // storage.objects.get: devstorage.read_only, never cloud-platform.
        assert_eq!(
            pick(
                &[
                    "cloud-platform",
                    "cloud-platform.read-only",
                    "devstorage.full_control",
                    "devstorage.read_only",
                    "devstorage.read_write"
                ],
                &[]
            )
            .as_deref(),
            Some("devstorage.read_only")
        );
        // storage.objects.copy (write): read_write over full_control/cloud-platform.
        assert_eq!(
            pick(
                &[
                    "cloud-platform",
                    "devstorage.full_control",
                    "devstorage.read_write"
                ],
                &[]
            )
            .as_deref(),
            Some("devstorage.read_write")
        );
        // Single-scope APIs and the API-level fallback.
        assert_eq!(pick(&["adwords"], &[]).as_deref(), Some("adwords"));
        assert_eq!(
            pick(&[], &["doubleclickbidmanager", "display-video"]).as_deref(),
            Some("display-video"),
            "methods without scopes fall back to the API list"
        );
        assert_eq!(
            pick(&["cloud-platform"], &[]).as_deref(),
            Some("cloud-platform")
        );
        assert_eq!(pick(&[], &[]), None);
    }

    #[test]
    fn exposes_all_tools_without_filter_and_marks_reads() {
        let toolset = GoogleApiToolset::from_document(
            &doc(),
            GoogleApiToolsetConfig::new("drive", "drive", "v3"),
            credentials(),
        );
        assert_eq!(
            toolset.tool_names(),
            vec!["drive_files_list", "drive_files_get", "drive_files_delete"]
        );
        let tools = &toolset.tools;
        assert!(tools[0].is_read_only());
        assert!(!tools[2].is_read_only());
        assert_eq!(tools[0].description(), "Lists files.");
        assert_eq!(
            tools[1].description(),
            "Calls the drive.files.get method of the drive v3 API."
        );
        let schema = tools[1].parameters_schema().unwrap();
        assert_eq!(schema["required"], json!(["file_id"]));
    }

    #[test]
    fn scope_override_and_prefix() {
        let config = GoogleApiToolsetConfig::new("cm360", "drive", "v3").with_scopes(["a", "b"]);
        let config = GoogleApiToolsetConfig {
            tool_name_prefix: Some("x_".into()),
            ..config
        };
        let toolset = GoogleApiToolset::from_document(&doc(), config, credentials());
        assert_eq!(toolset.scopes(), &["a".to_string(), "b".to_string()]);
        assert!(
            toolset
                .tool_names()
                .iter()
                .all(|n| n.starts_with("x_drive_"))
        );
    }
}

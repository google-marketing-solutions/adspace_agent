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
    /// OAuth scopes to request. Defaults to the first scope declared by the
    /// API (the Python ADK behaviour) when `None`.
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
    scopes: Vec<String>,
    additional_headers: Vec<(String, String)>,
}

/// A single Google API method exposed as a tool.
pub struct GoogleApiTool {
    name: String,
    description: String,
    spec: Arc<MethodSpec>,
    shared: Arc<ToolShared>,
}

impl GoogleApiTool {
    /// The underlying method spec.
    pub fn spec(&self) -> &MethodSpec {
        &self.spec
    }

    async fn call(&self, ctx: &Arc<dyn ToolContext>, args: Value) -> anyhow::Result<Value> {
        let token = self
            .shared
            .credentials
            .access_token(ctx.as_ref(), &self.shared.scopes)
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
        let scopes = config
            .scopes
            .clone()
            .unwrap_or_else(|| doc.oauth_scopes().into_iter().take(1).collect());
        let shared = Arc::new(ToolShared {
            http: reqwest::Client::new(),
            credentials,
            scopes: scopes.clone(),
            additional_headers: config.additional_headers.clone(),
        });
        let prefix = config.tool_name_prefix.clone().unwrap_or_default();
        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();
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
            tools.push(Arc::new(GoogleApiTool {
                name,
                description,
                spec: Arc::new(spec),
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
            tools,
        }
    }

    /// OAuth scopes this toolset needs.
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
              "list":{"id":"drive.files.list","path":"files","flatPath":"files","httpMethod":"GET","description":"Lists files.","parameters":{"q":{"type":"string","location":"query"}}},
              "get":{"id":"drive.files.get","path":"files/{fileId}","flatPath":"files/{fileId}","httpMethod":"GET","parameters":{"fileId":{"type":"string","location":"path","required":true}}},
              "delete":{"id":"drive.files.delete","path":"files/{fileId}","flatPath":"files/{fileId}","httpMethod":"DELETE","parameters":{"fileId":{"type":"string","location":"path","required":true}}}
            }}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn builds_filtered_tools_with_default_scope() {
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
        assert_eq!(
            toolset.scopes(),
            &["https://www.googleapis.com/auth/drive".to_string()]
        );
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

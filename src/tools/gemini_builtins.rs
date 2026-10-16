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

//! Gemini server-side ("built-in") tools: Google Search grounding, URL
//! context, Google Maps grounding and code execution.
//!
//! These are executed by Gemini itself, not by the agent process, and are
//! declared through adk-rust's built-in tool types. Selection is controlled
//! by `GEMINI_BUILTIN_TOOLS` (comma-separated; default
//! `google_search,url_context,google_maps`; `none` disables all; `all` also
//! adds `code_execution`). Gemini code execution can alternatively be chosen
//! as the data-analysis backend with `PYTHON_EXECUTOR=gemini`.

use std::str::FromStr;
use std::sync::Arc;

use adk_rust::Tool;
use adk_rust::tool::{
    BasicToolset, GeminiCodeExecutionTool, GoogleMapsContext, GoogleMapsTool, GoogleSearchTool,
    UrlContextTool,
};

/// Built-in tools enabled when `GEMINI_BUILTIN_TOOLS` is unset.
pub const DEFAULT_GEMINI_BUILTIN_TOOLS: &[GeminiBuiltinTool] = &[
    GeminiBuiltinTool::GoogleSearch,
    GeminiBuiltinTool::UrlContext,
    GeminiBuiltinTool::GoogleMaps,
];

/// A Gemini built-in tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeminiBuiltinTool {
    /// `google_search`: grounding with Google Search.
    GoogleSearch,
    /// `url_context`: lets the model read URLs mentioned in the prompt.
    UrlContext,
    /// `google_maps`: grounding with Google Maps places and routes.
    GoogleMaps,
    /// `gemini_code_execution`: server-side Python execution.
    CodeExecution,
}

impl GeminiBuiltinTool {
    /// Every variant, in display order.
    pub const ALL: &'static [Self] = &[
        Self::GoogleSearch,
        Self::UrlContext,
        Self::GoogleMaps,
        Self::CodeExecution,
    ];

    /// Name used in `GEMINI_BUILTIN_TOOLS`.
    pub fn key(self) -> &'static str {
        match self {
            Self::GoogleSearch => "google_search",
            Self::UrlContext => "url_context",
            Self::GoogleMaps => "google_maps",
            Self::CodeExecution => "code_execution",
        }
    }

    /// Builds the adk-rust tool.
    pub fn build(self) -> Arc<dyn Tool> {
        match self {
            Self::GoogleSearch => Arc::new(GoogleSearchTool::new()),
            Self::UrlContext => Arc::new(UrlContextTool::new()),
            Self::GoogleMaps => {
                let mut tool = GoogleMapsTool::new();
                if let Some(context) = maps_context_from_env() {
                    tool = tool.with_context(context);
                }
                Arc::new(tool)
            }
            Self::CodeExecution => Arc::new(GeminiCodeExecutionTool::new()),
        }
    }
}

impl FromStr for GeminiBuiltinTool {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "google_search" | "search" => Ok(Self::GoogleSearch),
            "url_context" | "url" => Ok(Self::UrlContext),
            "google_maps" | "maps" => Ok(Self::GoogleMaps),
            "code_execution" | "gemini_code_execution" | "code" => Ok(Self::CodeExecution),
            other => anyhow::bail!(
                "Unknown GEMINI_BUILTIN_TOOLS entry '{other}'. Valid values are: {}",
                Self::ALL
                    .iter()
                    .map(|t| t.key())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

/// Reads `GOOGLE_MAPS_CONTEXT_LAT` / `GOOGLE_MAPS_CONTEXT_LNG` for a default
/// map location.
fn maps_context_from_env() -> Option<GoogleMapsContext> {
    let lat: f64 = std::env::var("GOOGLE_MAPS_CONTEXT_LAT")
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let lng: f64 = std::env::var("GOOGLE_MAPS_CONTEXT_LNG")
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(GoogleMapsContext::new(lat, lng))
}

/// Parses a raw `GEMINI_BUILTIN_TOOLS` value.
///
/// # Errors
///
/// Returns an error for unknown entries.
pub fn parse_gemini_builtin_tools(raw: Option<&str>) -> anyhow::Result<Vec<GeminiBuiltinTool>> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_GEMINI_BUILTIN_TOOLS.to_vec());
    };
    let parts: Vec<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return Ok(DEFAULT_GEMINI_BUILTIN_TOOLS.to_vec());
    }
    if parts.len() == 1 && parts[0].eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    if parts.len() == 1 && matches!(parts[0].to_lowercase().as_str(), "all" | "*") {
        return Ok(GeminiBuiltinTool::ALL.to_vec());
    }
    let mut tools = Vec::new();
    for part in parts {
        let tool: GeminiBuiltinTool = part.parse()?;
        if !tools.contains(&tool) {
            tools.push(tool);
        }
    }
    Ok(tools)
}

/// Builds the `gemini_builtins` toolset from `GEMINI_BUILTIN_TOOLS`.
///
/// `exclude` lists built-ins already provided elsewhere (e.g. code execution
/// when `PYTHON_EXECUTOR=gemini`) so they are not declared twice. Returns
/// `None` when nothing is enabled.
///
/// # Errors
///
/// Returns an error for unknown `GEMINI_BUILTIN_TOOLS` entries.
pub fn gemini_builtin_toolset(
    exclude: &[GeminiBuiltinTool],
) -> anyhow::Result<Option<BasicToolset>> {
    let selected: Vec<GeminiBuiltinTool> =
        parse_gemini_builtin_tools(std::env::var("GEMINI_BUILTIN_TOOLS").ok().as_deref())?
            .into_iter()
            .filter(|t| !exclude.contains(t))
            .collect();
    if selected.is_empty() {
        return Ok(None);
    }
    tracing::info!(
        "Gemini built-in tools: {}",
        selected
            .iter()
            .map(|t| t.key())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(Some(BasicToolset::new(
        "gemini_builtins",
        selected.into_iter().map(GeminiBuiltinTool::build).collect(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builtin_selection() {
        assert_eq!(
            parse_gemini_builtin_tools(None).unwrap(),
            DEFAULT_GEMINI_BUILTIN_TOOLS.to_vec()
        );
        assert_eq!(parse_gemini_builtin_tools(Some("none")).unwrap(), vec![]);
        assert_eq!(
            parse_gemini_builtin_tools(Some("all")).unwrap(),
            GeminiBuiltinTool::ALL.to_vec()
        );
        assert_eq!(
            parse_gemini_builtin_tools(Some("search, code_execution, search")).unwrap(),
            vec![
                GeminiBuiltinTool::GoogleSearch,
                GeminiBuiltinTool::CodeExecution
            ]
        );
        assert!(parse_gemini_builtin_tools(Some("bing")).is_err());
    }

    #[test]
    fn builds_builtin_tools_with_expected_names() {
        let names: Vec<String> = GeminiBuiltinTool::ALL
            .iter()
            .map(|t| t.build().name().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "google_search",
                "url_context",
                "google_maps",
                "gemini_code_execution"
            ]
        );
        assert!(
            GeminiBuiltinTool::ALL
                .iter()
                .all(|t| t.build().is_builtin())
        );
    }
}

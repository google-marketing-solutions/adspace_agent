//! Data-analysis (Python execution) backend selection.
//!
//! The Python agent exposed LangChain's in-process `PythonAstREPLTool`. The
//! Rust port relies exclusively on adk-rust built-ins, selected with
//! `PYTHON_EXECUTOR`:
//!
//! | `PYTHON_EXECUTOR` | Tool | Notes |
//! |---|---|---|
//! | `gemini` (default) | `gemini_code_execution` | Gemini writes and runs Python server-side (numpy, pandas, matplotlib, scikit-learn, ... available). No infrastructure; stateless per turn. |
//! | `docker` | `python_code` | Container-backed CPython (`PYTHON_DOCKER_IMAGE`, default `python:3.12-slim`). Requires a Docker daemon. |
//! | `vertex` | `vertex_sandbox_code_execution` | Vertex AI Agent Engine managed sandbox (`VERTEX_SANDBOX_NAME` or `VERTEX_AGENT_ENGINE`). Stateful per session. |
//! | `monty` | `monty_python_code` | In-process sandboxed Monty interpreter (Python subset). Needs `--features monty`. |

use std::str::FromStr;
use std::sync::Arc;

use adk_rust::Tool;
use adk_rust::code::{
    ContainerConfig, SandboxCodeExecutor, VertexSandboxClient, VertexSandboxConfig,
    VertexSandboxTool,
};
#[cfg(feature = "monty")]
use adk_rust::tool::MontyPythonCodeTool;
use adk_rust::tool::{BasicToolset, GeminiCodeExecutionTool, PythonCodeTool};

/// Which Python execution backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PythonExecutorKind {
    /// adk-rust `GeminiCodeExecutionTool` (server-side, run by Gemini).
    #[default]
    Gemini,
    /// adk-rust `PythonCodeTool` (Docker container).
    Docker,
    /// adk-rust `VertexSandboxTool` (Agent Engine sandbox).
    Vertex,
    /// adk-rust `MontyPythonCodeTool` (in-process sandbox).
    Monty,
}

impl FromStr for PythonExecutorKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "" | "gemini" | "builtin" | "code_execution" => Ok(Self::Gemini),
            "docker" | "container" => Ok(Self::Docker),
            "vertex" | "vertex_sandbox" | "agent_engine" => Ok(Self::Vertex),
            "monty" => Ok(Self::Monty),
            other => anyhow::bail!(
                "Unknown PYTHON_EXECUTOR '{other}'. Valid values are: gemini, docker, vertex, monty"
            ),
        }
    }
}

impl PythonExecutorKind {
    /// Reads `PYTHON_EXECUTOR` from the environment.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown values.
    pub fn from_env() -> anyhow::Result<Self> {
        std::env::var("PYTHON_EXECUTOR")
            .ok()
            .as_deref()
            .unwrap_or("gemini")
            .parse()
    }
}

/// Builds the Python execution tool for the configured backend.
///
/// # Errors
///
/// Returns an error when the backend cannot be initialised (e.g. missing
/// Vertex configuration).
pub fn build_python_tool(kind: PythonExecutorKind) -> anyhow::Result<Arc<dyn Tool>> {
    Ok(match kind {
        PythonExecutorKind::Gemini => Arc::new(GeminiCodeExecutionTool::new()),
        PythonExecutorKind::Docker => Arc::new(PythonCodeTool::with_config(ContainerConfig {
            runtime: std::env::var("PYTHON_DOCKER_RUNTIME")
                .unwrap_or_else(|_| "docker".to_string()),
            default_image: std::env::var("PYTHON_DOCKER_IMAGE")
                .unwrap_or_else(|_| "python:3.12-slim".to_string()),
            extra_flags: vec![],
            auto_remove: true,
        })),
        PythonExecutorKind::Vertex => {
            let config = VertexSandboxConfig::from_env().map_err(|e| anyhow::anyhow!("{e}"))?;
            let client = Arc::new(
                VertexSandboxClient::new_with_adc(config).map_err(|e| anyhow::anyhow!("{e}"))?,
            );
            let executor = match std::env::var("VERTEX_SANDBOX_NAME")
                .ok()
                .filter(|s| !s.is_empty())
            {
                Some(sandbox) => SandboxCodeExecutor::for_sandbox(client, sandbox),
                None => {
                    let engine = std::env::var("VERTEX_AGENT_ENGINE").ok().filter(|s| !s.is_empty()).ok_or_else(|| {
                        anyhow::anyhow!("PYTHON_EXECUTOR=vertex requires VERTEX_SANDBOX_NAME or VERTEX_AGENT_ENGINE (reasoning engine ID)")
                    })?;
                    SandboxCodeExecutor::for_engine(client, engine)
                }
            };
            Arc::new(VertexSandboxTool::new(Arc::new(executor)))
        }
        #[cfg(feature = "monty")]
        PythonExecutorKind::Monty => Arc::new(MontyPythonCodeTool::repl()),
        #[cfg(not(feature = "monty"))]
        PythonExecutorKind::Monty => {
            anyhow::bail!(
                "PYTHON_EXECUTOR=monty requires building adspace-agent with `--features monty`"
            )
        }
    })
}

/// Builds the `data_analysis` toolset for the backend selected by
/// `PYTHON_EXECUTOR`.
///
/// # Errors
///
/// Returns an error when the backend cannot be initialised.
pub fn data_analysis_toolset() -> anyhow::Result<BasicToolset> {
    let kind = PythonExecutorKind::from_env()?;
    tracing::info!("Python execution backend: {kind:?}");
    Ok(BasicToolset::new(
        "data_analysis",
        vec![build_python_tool(kind)?],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_executor_kinds() {
        assert_eq!(
            "".parse::<PythonExecutorKind>().unwrap(),
            PythonExecutorKind::Gemini
        );
        assert_eq!(
            "gemini".parse::<PythonExecutorKind>().unwrap(),
            PythonExecutorKind::Gemini
        );
        assert_eq!(
            "docker".parse::<PythonExecutorKind>().unwrap(),
            PythonExecutorKind::Docker
        );
        assert_eq!(
            "vertex".parse::<PythonExecutorKind>().unwrap(),
            PythonExecutorKind::Vertex
        );
        assert_eq!(
            "MONTY".parse::<PythonExecutorKind>().unwrap(),
            PythonExecutorKind::Monty
        );
        assert!(
            "host".parse::<PythonExecutorKind>().is_err(),
            "the host REPL was removed"
        );
    }

    #[test]
    fn builtin_tools_have_expected_names() {
        assert_eq!(
            build_python_tool(PythonExecutorKind::Gemini)
                .unwrap()
                .name(),
            "gemini_code_execution"
        );
        assert_eq!(
            build_python_tool(PythonExecutorKind::Docker)
                .unwrap()
                .name(),
            "python_code"
        );
        #[cfg(feature = "monty")]
        assert_eq!(
            build_python_tool(PythonExecutorKind::Monty).unwrap().name(),
            "monty_python_code"
        );
        #[cfg(not(feature = "monty"))]
        assert!(build_python_tool(PythonExecutorKind::Monty).is_err());
    }
}

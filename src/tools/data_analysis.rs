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

//! Data-analysis (Python execution) backend selection.
//!
//! The Python agent exposed LangChain's in-process `PythonAstREPLTool`. The
//! Rust port relies exclusively on adk-rust built-ins, selected with
//! `PYTHON_EXECUTOR`:
//!
//! | `PYTHON_EXECUTOR` | Tool | Notes |
//! |---|---|---|
//! | `gemini` (default) | `gemini_code_execution` | Gemini writes and runs Python server-side (numpy, pandas, matplotlib, scikit-learn, ... available). No infrastructure; stateless per turn. |
//! | `docker` | `python_code` | Container-backed CPython (`PYTHON_DOCKER_IMAGE`, default `python:3.15-slim`). Requires a Docker daemon. Runs with no network, all capabilities dropped, a read-only root filesystem and memory/PID limits (see [`docker_hardening_flags`]). |
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

/// Default `--memory` limit for the Docker executor (`PYTHON_DOCKER_MEMORY`).
pub const DEFAULT_DOCKER_MEMORY: &str = "512m";
/// Default `--pids-limit` for the Docker executor (`PYTHON_DOCKER_PIDS_LIMIT`).
pub const DEFAULT_DOCKER_PIDS_LIMIT: &str = "256";
/// Default size of the writable `/tmp` tmpfs mounted into the otherwise
/// read-only container (`PYTHON_DOCKER_TMPFS_SIZE`).
pub const DEFAULT_DOCKER_TMPFS_SIZE: &str = "256m";

/// Extra `docker run` flags that harden the ephemeral Python container on top
/// of adk's strict sandbox policy (`--network=none`, no mounts, no env).
///
/// adk-code's `ContainerCommandExecutor` appends `extra_flags` verbatim before
/// the image name, so these compose with the policy-derived flags:
///
/// * `--memory` / `--pids-limit` bound memory and fork bombs
///   (`PYTHON_DOCKER_MEMORY`, `PYTHON_DOCKER_PIDS_LIMIT`);
/// * `--cap-drop=ALL` and `--security-opt=no-new-privileges` remove every
///   Linux capability and block setuid escalation;
/// * `--read-only` makes the image filesystem immutable, with a small
///   `noexec,nosuid` tmpfs on `/tmp` so CPython, matplotlib caches and
///   `tempfile` keep working (`PYTHON_DOCKER_TMPFS_SIZE`).
///
/// Values are read with a helper so the pure function can be unit-tested.
pub fn docker_hardening_flags(env: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let setting = |name: &str, default: &str| {
        env(name)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| default.to_string())
    };
    vec![
        format!(
            "--memory={}",
            setting("PYTHON_DOCKER_MEMORY", DEFAULT_DOCKER_MEMORY)
        ),
        format!(
            "--pids-limit={}",
            setting("PYTHON_DOCKER_PIDS_LIMIT", DEFAULT_DOCKER_PIDS_LIMIT)
        ),
        "--cap-drop=ALL".to_string(),
        "--security-opt=no-new-privileges".to_string(),
        "--read-only".to_string(),
        "--tmpfs".to_string(),
        format!(
            "/tmp:rw,noexec,nosuid,size={}",
            setting("PYTHON_DOCKER_TMPFS_SIZE", DEFAULT_DOCKER_TMPFS_SIZE)
        ),
    ]
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
                .unwrap_or_else(|_| "python:3.15-slim".to_string()),
            extra_flags: docker_hardening_flags(|name| std::env::var(name).ok()),
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
    fn docker_hardening_flags_have_safe_defaults_and_env_overrides() {
        let defaults = docker_hardening_flags(|_| None);
        assert_eq!(
            defaults,
            vec![
                "--memory=512m",
                "--pids-limit=256",
                "--cap-drop=ALL",
                "--security-opt=no-new-privileges",
                "--read-only",
                "--tmpfs",
                "/tmp:rw,noexec,nosuid,size=256m",
            ]
        );

        let overridden = docker_hardening_flags(|name| match name {
            "PYTHON_DOCKER_MEMORY" => Some("2g".into()),
            "PYTHON_DOCKER_PIDS_LIMIT" => Some(" 64 ".into()),
            "PYTHON_DOCKER_TMPFS_SIZE" => Some(String::new()),
            _ => None,
        });
        assert!(overridden.contains(&"--memory=2g".to_string()));
        assert!(overridden.contains(&"--pids-limit=64".to_string()));
        assert!(
            overridden.contains(&"/tmp:rw,noexec,nosuid,size=256m".to_string()),
            "blank override falls back to the default"
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

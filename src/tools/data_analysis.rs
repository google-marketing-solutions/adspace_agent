//! Data-analysis tools: Python code execution.
//!
//! The Python agent exposed LangChain's `PythonAstREPLTool` as
//! `python_repl_ast`. Four backends are available here, selected with
//! `PYTHON_EXECUTOR`:
//!
//! | `PYTHON_EXECUTOR` | Tool | Notes |
//! |---|---|---|
//! | `host` (default) | `python_repl_ast` | Persistent `python3` REPL per session using the host interpreter and whatever packages are installed (pandas, scikit-learn, ...). Closest to the original tool. |
//! | `monty` | `monty_python_code` | adk-rust's in-process, sandboxed [Monty](https://github.com/pydantic/monty) interpreter (REPL mode). No third-party packages. |
//! | `docker` | `python_code` | adk-rust's container-backed CPython tool (`PYTHON_DOCKER_IMAGE`, default `python:3.12-slim`). Requires a Docker daemon. |
//! | `vertex` | `vertex_sandbox_code_execution` | adk-rust's Vertex AI Agent Engine managed sandbox (`VERTEX_SANDBOX_NAME` or `VERTEX_AGENT_ENGINE`). |
//! | `gemini` | `gemini_code_execution` | adk-rust's Gemini built-in code execution: the model writes and runs Python server-side (no infrastructure; stateless per turn). |

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use adk_rust::code::{
    ContainerConfig, SandboxCodeExecutor, VertexSandboxClient, VertexSandboxConfig,
    VertexSandboxTool,
};
#[cfg(feature = "monty")]
use adk_rust::tool::MontyPythonCodeTool;
use adk_rust::tool::{BasicToolset, GeminiCodeExecutionTool, PythonCodeTool};
use adk_rust::{Result, Tool, ToolContext};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

/// Kernel script executed by `python3`.
pub const PYTHON_KERNEL_SOURCE: &str = include_str!("python_kernel.py");

/// Default per-call execution timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
/// Idle kernels older than this are reaped when a new call arrives.
const KERNEL_IDLE_TTL: Duration = Duration::from_secs(60 * 60);

/// Which Python execution backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PythonExecutorKind {
    /// Persistent host `python3` REPL (default).
    #[default]
    Host,
    /// adk-rust `MontyPythonCodeTool` (in-process sandbox).
    Monty,
    /// adk-rust `PythonCodeTool` (Docker container).
    Docker,
    /// adk-rust `VertexSandboxTool` (Agent Engine sandbox).
    Vertex,
    /// adk-rust `GeminiCodeExecutionTool` (server-side, run by Gemini).
    Gemini,
}

impl FromStr for PythonExecutorKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "" | "host" | "repl" | "python_repl_ast" => Ok(Self::Host),
            "monty" => Ok(Self::Monty),
            "docker" | "container" => Ok(Self::Docker),
            "vertex" | "vertex_sandbox" | "agent_engine" => Ok(Self::Vertex),
            other => anyhow::bail!(
                "Unknown PYTHON_EXECUTOR '{other}'. Valid values are: host, monty, docker, vertex"
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
            .unwrap_or("host")
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
        PythonExecutorKind::Host => Arc::new(PythonReplTool::from_env()),
        #[cfg(feature = "monty")]
        PythonExecutorKind::Monty => Arc::new(MontyPythonCodeTool::repl()),
        #[cfg(not(feature = "monty"))]
        PythonExecutorKind::Monty => {
            anyhow::bail!(
                "PYTHON_EXECUTOR=monty requires building adspace-agent with `--features monty`"
            )
        }
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
        PythonExecutorKind::Gemini => Arc::new(GeminiCodeExecutionTool::new()),
    })
}

#[derive(Debug, Deserialize)]
struct KernelResponse {
    ok: bool,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

struct Kernel {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    last_used: Instant,
}

impl Kernel {
    async fn spawn(python: &str, workdir: &PathBuf, script_path: &PathBuf) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(workdir).await?;
        let mut child = Command::new(python)
            .arg("-u")
            .arg(script_path)
            .current_dir(workdir)
            .env("MPLBACKEND", "Agg")
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to start `{python}`: {e}. Install Python 3 or set PYTHON_EXECUTABLE."
                )
            })?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(Self {
            child,
            stdin,
            stdout,
            last_used: Instant::now(),
        })
    }

    async fn run(&mut self, code: &str, timeout: Duration) -> anyhow::Result<KernelResponse> {
        let mut request = serde_json::to_string(&json!({ "code": code }))?;
        request.push('\n');
        self.stdin.write_all(request.as_bytes()).await?;
        self.stdin.flush().await?;
        let mut line = String::new();
        let read = tokio::time::timeout(timeout, self.stdout.read_line(&mut line)).await;
        self.last_used = Instant::now();
        match read {
            Ok(Ok(0)) => anyhow::bail!("the Python kernel exited unexpectedly"),
            Ok(Ok(_)) => Ok(serde_json::from_str(&line)?),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => {
                let _ = self.child.kill().await;
                anyhow::bail!(
                    "execution timed out after {} seconds; the Python kernel was restarted",
                    timeout.as_secs()
                )
            }
        }
    }
}

/// A Python REPL tool backed by one persistent `python3` process per session.
pub struct PythonReplTool {
    python: String,
    timeout: Duration,
    root_dir: PathBuf,
    kernels: Mutex<HashMap<String, Arc<Mutex<Kernel>>>>,
}

impl PythonReplTool {
    /// Creates the tool using `PYTHON_EXECUTABLE` (default `python3`),
    /// `PYTHON_REPL_TIMEOUT_SECS` (default 120) and `PYTHON_REPL_WORKDIR`
    /// (default a temp directory).
    pub fn from_env() -> Self {
        let python = std::env::var("PYTHON_EXECUTABLE")
            .ok()
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "python3".to_string());
        let timeout = std::env::var("PYTHON_REPL_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TIMEOUT);
        let root_dir = std::env::var("PYTHON_REPL_WORKDIR")
            .ok()
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("adspace-agent").join("python"));
        Self::new(python, timeout, root_dir)
    }

    /// Creates the tool with explicit settings.
    pub fn new(python: impl Into<String>, timeout: Duration, root_dir: impl Into<PathBuf>) -> Self {
        Self {
            python: python.into(),
            timeout,
            root_dir: root_dir.into(),
            kernels: Mutex::new(HashMap::new()),
        }
    }

    async fn kernel_for(&self, session_key: &str) -> anyhow::Result<Arc<Mutex<Kernel>>> {
        let mut kernels = self.kernels.lock().await;
        let mut stale = Vec::new();
        for (id, kernel) in kernels.iter() {
            if let Ok(k) = kernel.try_lock()
                && k.last_used.elapsed() > KERNEL_IDLE_TTL
            {
                stale.push(id.clone());
            }
        }
        for id in stale {
            kernels.remove(&id);
        }
        if let Some(kernel) = kernels.get(session_key) {
            return Ok(kernel.clone());
        }
        let safe_id: String = session_key
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        let workdir = self.root_dir.join(if safe_id.is_empty() {
            "default".to_string()
        } else {
            safe_id
        });
        let script_path = self.root_dir.join("python_kernel.py");
        tokio::fs::create_dir_all(&self.root_dir).await?;
        tokio::fs::write(&script_path, PYTHON_KERNEL_SOURCE).await?;
        let kernel = Arc::new(Mutex::new(
            Kernel::spawn(&self.python, &workdir, &script_path).await?,
        ));
        kernels.insert(session_key.to_string(), kernel.clone());
        Ok(kernel)
    }

    /// Executes `code` in the kernel for `session_key` and renders the output.
    pub async fn run_code(&self, session_key: &str, code: &str) -> String {
        let kernel = match self.kernel_for(session_key).await {
            Ok(k) => k,
            Err(e) => return format!("Error: {e:#}"),
        };
        let mut guard = kernel.lock().await;
        match guard.run(code, self.timeout).await {
            Ok(resp) => {
                let mut output = resp.stdout;
                if let Some(result) = resp.result {
                    output.push_str(&result);
                }
                if !resp.ok
                    && let Some(error) = resp.error
                {
                    output.push_str(&error);
                }
                output
            }
            Err(e) => {
                drop(guard);
                self.kernels.lock().await.remove(session_key);
                format!("Error: {e:#}")
            }
        }
    }
}

#[async_trait]
impl Tool for PythonReplTool {
    fn name(&self) -> &str {
        "python_repl_ast"
    }

    fn description(&self) -> &str {
        "A Python shell. Use this to execute python commands. Input should be a valid python command. \
         When using this tool, sometimes output is abbreviated - make sure it does not look abbreviated \
         before using it in your answer."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "A valid Python command or script to execute." }
            },
            "required": ["query"]
        }))
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        let code = args
            .get("query")
            .or_else(|| args.get("code"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if code.trim().is_empty() {
            return Ok(Value::String(
                "Error: `query` must contain Python code.".to_string(),
            ));
        }
        // Key kernels by the full ADK identity so state never leaks across users.
        let key = format!(
            "{}__{}__{}",
            ctx.app_name(),
            ctx.user_id(),
            ctx.session_id()
        );
        Ok(Value::String(self.run_code(&key, &code).await))
    }
}

/// Builds the data-analysis toolset for the backend selected by
/// `PYTHON_EXECUTOR` (an adk-rust [`BasicToolset`] named `data_analysis`).
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

    fn python_available() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok()
    }

    #[test]
    fn parses_executor_kinds() {
        assert_eq!(
            "host".parse::<PythonExecutorKind>().unwrap(),
            PythonExecutorKind::Host
        );
        assert_eq!(
            "MONTY".parse::<PythonExecutorKind>().unwrap(),
            PythonExecutorKind::Monty
        );
        assert_eq!(
            "docker".parse::<PythonExecutorKind>().unwrap(),
            PythonExecutorKind::Docker
        );
        assert_eq!(
            "vertex".parse::<PythonExecutorKind>().unwrap(),
            PythonExecutorKind::Vertex
        );
        assert!("nope".parse::<PythonExecutorKind>().is_err());
    }

    #[test]
    fn builtin_tools_have_expected_names() {
        assert_eq!(
            build_python_tool(PythonExecutorKind::Host).unwrap().name(),
            "python_repl_ast"
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

    #[tokio::test]
    async fn repl_persists_state_and_returns_last_expression() {
        if !python_available() {
            eprintln!("python3 not available; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let tool = PythonReplTool::new("python3", Duration::from_secs(30), dir.path());
        assert_eq!(tool.run_code("s1", "x = 21").await, "");
        assert_eq!(tool.run_code("s1", "print('hi'); x * 2").await, "hi\n42");
        assert!(
            tool.run_code("s1", "1/0")
                .await
                .contains("ZeroDivisionError")
        );
        assert!(tool.run_code("s2", "x").await.contains("NameError"));
    }
}

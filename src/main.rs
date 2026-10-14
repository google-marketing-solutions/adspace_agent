//! AdSpace Agent command-line entry point.
//!
//! ```text
//! adspace-agent                      # serve the web UI + API (default)
//! adspace-agent serve --port 8000
//! adspace-agent console              # interactive terminal chat
//! adspace-agent list-tools google_ads
//! adspace-agent list-tools youtube
//! adspace-agent list-tools google --api drive --version v3
//! adspace-agent list-tools all       # every tool the configured agent exposes
//! ```

use adk_rust::Toolset as _;
use adspace_agent::agent::{
    self, build_credential_manager, build_runtime, google_api_toolset_configs,
};
use adspace_agent::config::{GOOGLE_ADS_API_VERSION, Settings, YOUTUBE_TOOL_FILTER};
use adspace_agent::google_api::{DiscoveryFetcher, GoogleApiToolset, GoogleApiToolsetConfig};
use adspace_agent::server;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "adspace-agent",
    version,
    about = "AdSpace Agent — Google marketing agent built on adk-rust"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Serve the ADK web UI and REST API (default).
    Serve {
        /// Interface to bind.
        #[arg(long, env = "HOST", default_value = "0.0.0.0")]
        host: String,
        /// Port to bind (Cloud Run sets `PORT`).
        #[arg(long, env = "PORT", default_value_t = 8000)]
        port: u16,
    },
    /// Chat with the agent in the terminal (adk-cli console; user id `user`).
    /// The OAuth callback is served on `host:port` so authorizations complete.
    Console {
        /// Interface to bind the OAuth callback to.
        #[arg(long, env = "HOST", default_value = "0.0.0.0")]
        host: String,
        /// Port of the OAuth callback (must match `PUBLIC_BASE_URL`).
        #[arg(long, env = "PORT", default_value_t = 8000)]
        port: u16,
    },
    /// List the tools of a Google API toolset.
    ListTools {
        /// Also print, per toolset, the size of the tool declarations sent to
        /// the model on every request (bytes and a rough token estimate).
        #[arg(long, global = true)]
        sizes: bool,
        #[command(subcommand)]
        target: ListTarget,
    },
}

#[derive(Subcommand, Debug)]
enum ListTarget {
    /// Google Ads API tools (unfiltered).
    GoogleAds,
    /// YouTube Data API tools (the agent's filter).
    Youtube,
    /// Any Google API by Discovery name and version.
    Google {
        /// API name (e.g. drive, storage).
        #[arg(long)]
        api: String,
        /// API version (e.g. v3, v1).
        #[arg(long)]
        version: String,
    },
    /// Every Discovery-based toolset the current configuration enables.
    All,
}

/// Default `RUST_LOG` filter when the operator did not set one.
const DEFAULT_LOG_FILTER: &str = "info,sqlx=warn,hyper=warn,h2=warn,reqwest=warn";

/// Installs adk-rust's telemetry: a `tracing` console layer plus the
/// in-process span exporter that backs the web UI's Trace tab.
///
/// The Launcher calls the same initializer again when building the server;
/// that call is idempotent and reuses this exporter. Must run before the
/// Tokio runtime starts because it may set `RUST_LOG`.
fn init_tracing() {
    if std::env::var_os("RUST_LOG").is_none() {
        // SAFETY: called from `main` before any other thread exists.
        unsafe { std::env::set_var("RUST_LOG", DEFAULT_LOG_FILTER) };
    }
    if let Err(e) = adk_rust::telemetry::init_with_adk_exporter("adspace-agent") {
        eprintln!("warning: telemetry initialization failed: {e}");
    }
}

async fn list_tools(target: ListTarget, sizes: bool) -> anyhow::Result<()> {
    let settings = Settings::from_env()?;
    let credentials = build_credential_manager(&settings);
    let fetcher = DiscoveryFetcher::new();
    let configs: Vec<GoogleApiToolsetConfig> = match target {
        ListTarget::GoogleAds => vec![
            GoogleApiToolsetConfig::new("google_ads", "googleads", GOOGLE_ADS_API_VERSION)
                .with_discovery_url(format!("https://googleads.googleapis.com/$discovery/rest?version={GOOGLE_ADS_API_VERSION}")),
        ],
        ListTarget::Youtube => vec![GoogleApiToolsetConfig::new("youtube", "youtube", "v3").with_tool_filter(YOUTUBE_TOOL_FILTER.iter().copied())],
        ListTarget::Google { api, version } => vec![GoogleApiToolsetConfig::new(format!("google_{api}"), api, version)],
        ListTarget::All => google_api_toolset_configs(&settings),
    };
    let mut names = Vec::new();
    let mut total_bytes = 0usize;
    for config in configs {
        let toolset = GoogleApiToolset::load(config, &fetcher, credentials.clone()).await?;
        if sizes {
            let (count, bytes) = agent::declaration_size(&toolset).await?;
            total_bytes += bytes;
            eprintln!(
                "{:<32} {:>5} tools  {:>9} bytes  (~{} tokens per model request)",
                toolset.name(),
                count,
                bytes,
                bytes / 4
            );
        }
        names.extend(agent::list_tool_names(&toolset).await?);
    }
    if sizes {
        eprintln!(
            "{:<32} {:>5} tools  {:>9} bytes  (~{} tokens per model request)",
            "TOTAL",
            names.len(),
            total_bytes,
            total_bytes / 4
        );
    }
    names.sort();
    for name in names {
        println!("{name}");
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();
    adk_rust::ensure_crypto_provider();
    let cli = Cli::parse();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(cli))
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let command = cli.command.unwrap_or(Command::Serve {
        host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into()),
        port: std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8000),
    });
    match command {
        Command::Serve { host, port } => {
            let runtime = build_runtime().await?;
            server::serve(runtime, &host, port).await
        }
        Command::Console { host, port } => {
            let runtime = build_runtime().await?;
            server::console(runtime, &host, port).await
        }
        Command::ListTools { target, sizes } => list_tools(target, sizes).await,
    }
}

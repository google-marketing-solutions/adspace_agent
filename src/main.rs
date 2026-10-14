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
    /// Chat with the agent in the terminal.
    Console {
        /// User id for the session.
        #[arg(long, default_value = "user")]
        user: String,
    },
    /// List the tools of a Google API toolset.
    ListTools {
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

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("info,sqlx=warn,hyper=warn,h2=warn,reqwest=warn")
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

async fn list_tools(target: ListTarget) -> anyhow::Result<()> {
    let settings = Settings::from_env()?;
    let credentials = build_credential_manager(&settings).await;
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
    for config in configs {
        let toolset = GoogleApiToolset::load(config, &fetcher, credentials.clone()).await?;
        names.extend(agent::list_tool_names(&toolset).await?);
    }
    names.sort();
    for name in names {
        println!("{name}");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();
    adk_rust::ensure_crypto_provider();
    let cli = Cli::parse();
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
        Command::Console { user } => {
            let runtime = build_runtime().await?;
            server::console(runtime, &user).await
        }
        Command::ListTools { target } => list_tools(target).await,
    }
}

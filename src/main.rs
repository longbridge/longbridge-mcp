mod auth;
mod counter;
mod error;
mod metrics;
mod registry;
mod serialize;
mod tools;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use serde::Deserialize;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::registry::UserRegistry;

fn default_bind() -> SocketAddr {
    "127.0.0.1:8000".parse().unwrap()
}

fn default_idle_timeout() -> u64 {
    300
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    bind: Option<SocketAddr>,
    base_url: Option<String>,
    idle_timeout: Option<u64>,
    log_dir: Option<PathBuf>,
}

#[derive(Debug, Parser)]
#[command(name = "longbridge-mcp", about = "Longbridge MCP Server")]
struct Cli {
    /// HTTP server bind address
    #[arg(long)]
    bind: Option<SocketAddr>,

    /// Public base URL for OAuth callbacks (e.g. https://mcp.example.com)
    #[arg(long)]
    base_url: Option<String>,

    /// Session idle timeout in seconds
    #[arg(long)]
    idle_timeout: Option<u64>,

    /// Log directory (stderr if not specified)
    #[arg(long)]
    log_dir: Option<PathBuf>,
}

/// Resolved configuration (CLI > config file > defaults)
pub struct AppConfig {
    pub bind: SocketAddr,
    pub base_url: String,
    pub idle_timeout: u64,
    pub log_dir: Option<PathBuf>,
}

fn mcp_dir() -> PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".longbridge")
        .join("mcp")
}

fn load_config() -> AppConfig {
    let cli = Cli::parse();

    let config_path = mcp_dir().join("config.json");
    let file_config: FileConfig = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).expect("failed to read config.json");
        serde_json::from_str(&content).expect("failed to parse config.json")
    } else {
        FileConfig::default()
    };

    let bind = cli.bind.or(file_config.bind).unwrap_or_else(default_bind);

    let base_url = cli
        .base_url
        .or(file_config.base_url)
        .unwrap_or_else(|| format!("http://localhost:{}", bind.port()));

    AppConfig {
        bind,
        base_url,
        idle_timeout: cli
            .idle_timeout
            .or(file_config.idle_timeout)
            .unwrap_or_else(default_idle_timeout),
        log_dir: cli.log_dir.or(file_config.log_dir),
    }
}

fn init_logging(log_dir: Option<&PathBuf>) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,longbridge_mcp=debug"));

    if let Some(dir) = log_dir {
        let file_appender = tracing_appender::rolling::daily(dir, "longbridge-mcp.log");
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(file_appender)
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = load_config();
    init_logging(config.log_dir.as_ref());

    // Ensure mcp directory exists
    let mcp_dir = mcp_dir();
    std::fs::create_dir_all(&mcp_dir)?;

    let registry = Arc::new(
        UserRegistry::new(
            std::time::Duration::from_secs(config.idle_timeout),
            &mcp_dir,
        )
        .await?,
    );
    registry.spawn_cleanup_task();

    let jwt_secret = auth::token::load_or_create_jwt_secret(&mcp_dir)?;

    let app_state = Arc::new(crate::auth::AppState {
        registry: registry.clone(),
        jwt_secret,
        base_url: config.base_url.clone(),
    });

    let app =
        auth::create_router(app_state.clone()).layer(tower_http::cors::CorsLayer::permissive());

    let listener = TcpListener::bind(config.bind).await?;
    tracing::info!("listening on {}", config.bind);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("shutting down");
        })
        .await?;

    Ok(())
}

//! TUI (Text User Interface) binary for stopwatch control using Ratatui

use std::path::PathBuf;

use clap::Parser;
use color_eyre::Result;
use tracing::info;
use tracing_subscriber::EnvFilter;

use speedskating_backend::tui;

/// TUI binary for Speed Skating Timer
#[derive(Parser, Debug)]
#[command(name = "speedskating-tui")]
#[command(about = "Text User Interface for Speed Skating Timer")]
#[command(version)]
struct Args {
    /// NATS server URL
    #[arg(long, default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Path to NATS credentials file for authentication
    #[arg(long)]
    nats_creds: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize TUI tracing subscriber before any other logging
    tui::init_tui_tracing()?;

    info!("Speed Skating Timer TUI starting...");
    info!("NATS URL: {}", args.nats_url);

    // Connect to NATS with credentials if provided
    let nats = if let Some(creds_path) = &args.nats_creds {
        // Resolve relative paths to absolute paths
        let creds_file = if creds_path.starts_with('/') {
            PathBuf::from(creds_path)
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(creds_path)
        };
        
        if creds_file.exists() {
            info!("Connecting to NATS with credentials from {}", creds_file.display());
            async_nats::ConnectOptions::with_credentials_file(&creds_file)
                .await?
                .connect(&args.nats_url)
                .await?
        } else {
            info!("Credentials file not found at {}, connecting without authentication", creds_file.display());
            async_nats::connect(&args.nats_url).await?
        }
    } else {
        async_nats::connect(&args.nats_url).await?
    };
    info!("Connected to NATS at {}", args.nats_url);

    // Run TUI (blocks until exit)
    tokio::select! {
        result = tui::run_tui(nats.clone()) => {
            result?;
        }
        _ = tokio::signal::ctrl_c() => {
            // Re-enable fmt for shutdown message
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::new("info"))
                .init();
            info!("Received shutdown signal");
        }
    }

    // Re-enable fmt for shutdown message
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("info"))
        .init();
    info!("Shutting down...");

    Ok(())
}


//! Speed Skating Timer Backend Server
//!
//! A NATS-based stopwatch timing service with trigger abstraction.

use std::sync::Arc;

use axum::{
    Router,
    extract::ws::{WebSocket, WebSocketUpgrade},
    routing::get,
};
use clap::{Parser, Subcommand};
use color_eyre::Result;
use futures::{SinkExt, StreamExt, future};
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use nats_jwt::KeyPair;
use speedskating_backend::auth::{load_auth_config, run_auth_service};
use speedskating_backend::hardware_setup::{
    new_hardware_setup_shared, run_hardware_setup_publisher, run_hardware_setup_service,
};
use speedskating_backend::simulator::run_simulator;
use speedskating_backend::stopwatch::{
    SharedStopwatch, Stopwatch, run_event_logger, run_stopwatch_service,
};
use speedskating_backend::system::run_ping_service;
use speedskating_backend::trigger::{TriggerConfig, run_trigger_source};
use speedskating_backend::types::GpioConfig;
use std::fs;
use std::path::PathBuf;

/// Speed Skating Timer Backend Server
#[derive(Parser, Debug)]
#[command(name = "speedskating-server")]
#[command(about = "NATS-based stopwatch timing service for speed skating")]
#[command(version)]
struct Args {
    /// NATS server URL
    #[arg(long, default_value = "nats://localhost:4222")]
    nats_url: String,

    /// NATS WebSocket server host for proxying
    #[arg(long, default_value = "localhost")]
    nats_ws_host: String,

    /// NATS WebSocket server port for proxying
    #[arg(long, default_value = "4223")]
    nats_ws_port: u16,

    /// HTTP server port for health endpoint and frontend
    #[arg(long, default_value = "8080")]
    http_port: u16,

    /// Frontend static files directory
    #[arg(long, default_value = "frontend")]
    frontend_dir: String,

    /// Enable event logger to dump all state and lap events
    #[arg(long, default_value = "true")]
    event_logger: bool,

    /// Path to authentication config file
    #[arg(long)]
    auth_config: String,

    /// Path to NATS account seed file (.seed format)
    #[arg(long)]
    account_seed: String,

    /// Path to frontend user seed file (to extract public key for generated JWTs)
    #[arg(long)]
    frontend_user_seed: String,

    /// Path to NATS credentials file for server authentication
    #[arg(long)]
    nats_creds: Option<String>,

    /// Trigger source configuration
    #[command(subcommand)]
    trigger: Option<TriggerSource>,

    /// Simulator service configuration (uses default sequence: "arm 1.4s trigger 8.4s trigger 10.2s trigger reset 5s")
    #[arg(long)]
    simulator: bool,

    /// Enable dev mode - proxy frontend requests to Vite dev server instead of serving static files
    #[arg(long)]
    dev: bool,

    /// Vite dev server URL (used when --dev is enabled)
    #[arg(long, default_value = "http://localhost:5173")]
    vite_url: String,
}

#[derive(Subcommand, Debug, Clone)]
enum TriggerSource {
    /// Use keyboard as trigger source
    Keyboard {
        /// Key to use for triggering (single character, use '\n' for Enter)
        #[arg(short, long, default_value = "\n")]
        key: char,
    },
    /// Use GPIO pins as trigger source (configured via gpio.yaml)
    Gpio {
        /// Path to GPIO configuration file
        #[arg(short, long, default_value = "gpio.yaml")]
        config: String,
    },
    /// Use mock trigger for testing
    Mock {
        /// Trigger interval in milliseconds
        #[arg(short, long, default_value = "5000")]
        interval_ms: u64,
    },
}

/// Load GPIO configuration from file, creating default if not found
fn load_gpio_config(config_path: &str) -> Result<GpioConfig> {
    let path = PathBuf::from(config_path);
    if !path.exists() {
        tracing::warn!("GPIO config file not found at {:?}, creating default", path);
        let default_config = GpioConfig::default();
        let config_str = serde_yaml::to_string(&default_config)?;
        fs::write(&path, config_str)?;
        info!("Created default GPIO config at {:?}", path);
        return Ok(default_config);
    }

    let config_str = fs::read_to_string(&path)?;
    let config: GpioConfig = serde_yaml::from_str(&config_str)?;
    info!(
        "Loaded GPIO config from {:?}: chip={}, {} pins",
        path,
        config.chip,
        config.pins.len()
    );
    for pin in &config.pins {
        info!("  Pin {}: {}", pin.pin, pin.name);
    }
    Ok(config)
}

fn trigger_source_to_config(source: TriggerSource) -> Result<TriggerConfig> {
    match source {
        TriggerSource::Keyboard { key } => Ok(TriggerConfig::Keyboard { key }),
        TriggerSource::Gpio {
            config: config_path,
        } => {
            let gpio_config = load_gpio_config(&config_path)?;
            Ok(TriggerConfig::Gpio {
                config: gpio_config,
            })
        }
        TriggerSource::Mock { interval_ms } => Ok(TriggerConfig::Mock { interval_ms }),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize error handling and logging
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Speed Skating Timer Backend starting...");
    info!("NATS URL: {}", args.nats_url);
    info!("HTTP port: {}", args.http_port);
    info!(
        "NATS WebSocket: {}:{}",
        args.nats_ws_host, args.nats_ws_port
    );
    if args.dev {
        info!(
            "Dev mode: enabled - proxying frontend requests to {}",
            args.vite_url
        );
    } else {
        info!("Frontend dir: {}", args.frontend_dir);
    }
    info!(
        "Trigger source: {:?}",
        args.trigger
            .as_ref()
            .map(|t| format!("{:?}", t))
            .unwrap_or_else(|| "None".to_string())
    );

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
            info!(
                "Connecting to NATS with credentials from {}",
                creds_file.display()
            );
            async_nats::ConnectOptions::with_credentials_file(&creds_file)
                .await?
                .connect(&args.nats_url)
                .await?
        } else {
            info!(
                "Credentials file not found at {}, connecting without authentication",
                creds_file.display()
            );
            async_nats::connect(&args.nats_url).await?
        }
    } else {
        async_nats::connect(&args.nats_url).await?
    };
    info!("Connected to NATS at {}", args.nats_url);

    // Load authentication configuration
    let auth_config_path = PathBuf::from(&args.auth_config);
    let auth_config = load_auth_config(&auth_config_path)?;
    info!("Loaded auth config from {:?}", auth_config_path);

    // Load account key for JWT signing (NATS seed format)
    // Resolve relative paths to absolute paths
    let account_seed_path = if args.account_seed.starts_with('/') {
        PathBuf::from(&args.account_seed)
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(&args.account_seed)
    };
    let account_seed = fs::read_to_string(&account_seed_path).map_err(|e| {
        color_eyre::eyre::eyre!(
            "Failed to read account seed from {:?}: {}",
            account_seed_path,
            e
        )
    })?;

    let account_key = KeyPair::from_seed(account_seed.trim())
        .map_err(|e| color_eyre::eyre::eyre!("Failed to parse account seed: {}. Make sure the seed is in NATS format (starts with SA...).", e))?;
    info!("Loaded account seed from {:?}", account_seed_path);

    // Load frontend user seed and extract public key
    let frontend_seed_path = if args.frontend_user_seed.starts_with('/') {
        PathBuf::from(&args.frontend_user_seed)
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(&args.frontend_user_seed)
    };
    let frontend_seed = fs::read_to_string(&frontend_seed_path).map_err(|e| {
        color_eyre::eyre::eyre!(
            "Failed to read frontend seed from {:?}: {}",
            frontend_seed_path,
            e
        )
    })?;

    let frontend_user_keypair = KeyPair::from_seed(frontend_seed.trim())
        .map_err(|e| color_eyre::eyre::eyre!("Failed to parse frontend seed: {}. Make sure the seed is in NATS format (starts with SU...).", e))?;
    let frontend_user_public_key = frontend_user_keypair.public_key();
    info!(
        "Loaded frontend user public key from {:?}",
        frontend_seed_path
    );

    // Create shared stopwatch state
    let stopwatch: SharedStopwatch = Arc::new(RwLock::new(Stopwatch::new()));

    // Create shared hardware setup state
    let hardware_setup = new_hardware_setup_shared();

    // Spawn all services
    let ping_handle = tokio::spawn(run_ping_service(nats.clone()));
    let stopwatch_handle = tokio::spawn(run_stopwatch_service(nats.clone(), stopwatch.clone()));
    let auth_handle = tokio::spawn(run_auth_service(
        nats.clone(),
        auth_config,
        account_key,
        frontend_user_public_key.clone(),
    ));
    let hardware_setup_handle = tokio::spawn(run_hardware_setup_service(
        nats.clone(),
        hardware_setup.clone(),
    ));
    tokio::spawn(run_hardware_setup_publisher(
        nats.clone(),
        hardware_setup.clone(),
    ));
    let http_handle = tokio::spawn(run_http_server(
        args.http_port,
        args.frontend_dir,
        args.nats_ws_host,
        args.nats_ws_port,
        args.dev,
        args.vite_url,
    ));
    if args.event_logger {
        tokio::spawn(run_event_logger(nats.clone()));
    }

    // Handle trigger source
    let trigger_handle = if let Some(trigger) = args.trigger {
        let trigger_config = trigger_source_to_config(trigger)?;
        Some(tokio::spawn(run_trigger_source(
            trigger_config,
            nats.clone(),
            hardware_setup.clone(),
        )))
    } else {
        None
    };

    // Handle simulator service
    let simulator_handle = if args.simulator {
        let default_sequence =
            "arm 1s trigger 3.3s trigger 4.4s trigger 5.2s trigger 0.5s reset 5s unarm 1s";
        info!(
            "Starting simulator service with sequence: {}",
            default_sequence
        );
        Some(tokio::spawn(run_simulator(
            default_sequence.to_string(),
            nats.clone(),
        )))
    } else {
        None
    };

    info!(
        "All services started.{} Press Ctrl+C to exit.",
        if trigger_handle.is_some() || simulator_handle.is_some() {
            ""
        } else {
            " No trigger source or simulator configured."
        }
    );

    // Wait for shutdown signal or any service to exit
    let trigger_future = trigger_handle.map_or_else(
        || tokio::spawn(async { future::pending::<Result<()>>().await }),
        |handle| handle,
    );
    let simulator_future = simulator_handle.map_or_else(
        || tokio::spawn(async { future::pending::<Result<()>>().await }),
        |handle| handle,
    );

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        r = ping_handle => {
            panic!("Ping service exited: {:?}", r);
        }
        r = stopwatch_handle => {
            panic!("Stopwatch service exited: {:?}", r);
        }
        r = http_handle => {
            panic!("HTTP server exited: {:?}", r);
        }
        r = auth_handle => {
            panic!("Auth service exited: {:?}", r);
        }
        r = hardware_setup_handle => {
            panic!("Hardware setup service exited: {:?}", r);
        }
        r = trigger_future => {
            panic!("Trigger service exited: {:?}", r);
        }
        r = simulator_future => {
            panic!("Simulator service exited: {:?}", r);
        }
    }

    info!("Shutting down...");
    Ok(())
}

/// Health check endpoint
async fn health_handler() -> &'static str {
    "OK"
}

/// WebSocket proxy handler
async fn ws_proxy_handler(ws: WebSocket, ws_host: String, ws_port: u16) {
    let nats_ws_url = format!("ws://{}:{}", ws_host, ws_port);
    info!("Connecting to NATS WebSocket at {}", nats_ws_url);
    // Connect to NATS WebSocket server
    let (nats_stream, _) = match connect_async(&nats_ws_url).await {
        Ok(stream) => stream,
        Err(e) => {
            error!(
                "Failed to connect to NATS WebSocket at {}: {}",
                nats_ws_url, e
            );
            return;
        }
    };

    let (mut client_sender, mut client_receiver) = ws.split();
    let (mut nats_sender, mut nats_receiver) = nats_stream.split();

    // Forward messages from client to NATS
    let client_to_nats = tokio::spawn(async move {
        while let Some(Ok(msg)) = client_receiver.next().await {
            let nats_msg = match msg {
                axum::extract::ws::Message::Text(text) => Message::Text(text.to_string()),
                axum::extract::ws::Message::Binary(data) => Message::Binary(data.to_vec()),
                axum::extract::ws::Message::Ping(data) => Message::Ping(data.to_vec()),
                axum::extract::ws::Message::Pong(data) => Message::Pong(data.to_vec()),
                axum::extract::ws::Message::Close(frame) => Message::Close(frame.map(|f| {
                    tokio_tungstenite::tungstenite::protocol::CloseFrame {
                        code: f.code.into(),
                        reason: f.reason.to_string().into(),
                    }
                })),
            };

            if nats_sender.send(nats_msg).await.is_err() {
                break;
            }
        }
    });

    // Forward messages from NATS to client
    let nats_to_client = tokio::spawn(async move {
        while let Some(Ok(msg)) = nats_receiver.next().await {
            let client_msg = match msg {
                Message::Text(text) => axum::extract::ws::Message::Text(text.into()),
                Message::Binary(data) => axum::extract::ws::Message::Binary(data.into()),
                Message::Ping(data) => axum::extract::ws::Message::Ping(data.into()),
                Message::Pong(data) => axum::extract::ws::Message::Pong(data.into()),
                Message::Close(frame) => axum::extract::ws::Message::Close(frame.map(|f| {
                    axum::extract::ws::CloseFrame {
                        code: f.code.into(),
                        reason: f.reason.to_string().into(),
                    }
                })),
                Message::Frame(_) => continue,
            };

            if client_sender.send(client_msg).await.is_err() {
                break;
            }
        }
    });

    // Wait for either direction to finish
    tokio::select! {
        _ = client_to_nats => {}
        _ = nats_to_client => {}
    }
}

/// HTTP server state for NATS WebSocket proxy
#[derive(Clone)]
struct AppState {
    /// NATS WebSocket host
    nats_ws_host: String,
    /// NATS WebSocket port
    nats_ws_port: u16,
}

/// WebSocket upgrade handler for NATS proxy
async fn nats_ws_upgrade_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    info!("NATS WebSocket upgrade handler called");
    let host = state.nats_ws_host;
    let port = state.nats_ws_port;
    ws.on_upgrade(move |socket| ws_proxy_handler(socket, host, port))
}

/// Run the HTTP server for health checks and frontend static files
async fn run_http_server(
    port: u16,
    frontend_dir: String,
    nats_ws_host: String,
    nats_ws_port: u16,
    dev_mode: bool,
    vite_url: String,
) -> Result<()> {
    // log dev_mode
    info!("Dev mode: {}", dev_mode);
    info!("Vite URL: {}", vite_url);
    info!("Frontend dir: {}", frontend_dir);
    info!("NATS WebSocket host: {}", nats_ws_host);
    info!("NATS WebSocket port: {}", nats_ws_port);
    info!("HTTP port: {}", port);
    let app_state = AppState {
        nats_ws_host,
        nats_ws_port,
    };


    let app = if dev_mode {
        // Dev mode: proxy all non-matched routes to Vite dev server
        let vite_addr = vite_url
            .strip_prefix("http://")
            .or_else(|| vite_url.strip_prefix("https://"))
            .unwrap_or(&vite_url);
        Router::new()
            .route("/health", get(health_handler))
            .route("/nats", get(nats_ws_upgrade_handler))
            .fallback_service(
                reverse_proxy_service::builder_http(vite_addr)?
                    .build(reverse_proxy_service::Identity),
            )
            .layer(CorsLayer::permissive())
            .with_state(app_state)
    } else {
        // Production mode: serve static files from frontend_dir
        Router::new()
            .route("/health", get(health_handler))
            .route("/nats", get(nats_ws_upgrade_handler))
            .fallback_service(ServeDir::new(&frontend_dir))
            .layer(CorsLayer::permissive())
            .with_state(app_state)
    };

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    info!("HTTP server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

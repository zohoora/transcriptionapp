use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".fabricscribe")
}

#[tokio::main]
async fn main() {
    // Logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // CLI args: --port PORT --data-dir PATH --api-key KEY
    let args: Vec<String> = std::env::args().collect();
    let port = find_arg(&args, "--port")
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8090);
    let data_dir = find_arg(&args, "--data-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_data_dir);

    // API key: CLI arg takes precedence, then env var
    let api_key = find_arg(&args, "--api-key")
        .or_else(|| std::env::var("PROFILE_API_KEY").ok());

    if api_key.is_some() {
        info!("API key authentication enabled");
    } else {
        info!("API key authentication disabled (no key configured)");
    }

    // Build state and app via library functions
    let state = profile_service::create_app_state(&data_dir);
    let app = profile_service::build_app(state, api_key);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Profile service starting on {addr}");
    info!("Data directory: {}", data_dir.display());

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");

    // Graceful shutdown on Ctrl+C
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    info!("Shutting down...");
}

fn find_arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

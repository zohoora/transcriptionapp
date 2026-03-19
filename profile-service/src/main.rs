mod error;
mod routes;
mod store;
mod types;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

use store::AppState;

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

    // CLI args: --port PORT --data-dir PATH
    let args: Vec<String> = std::env::args().collect();
    let port = find_arg(&args, "--port")
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8090);
    let data_dir = find_arg(&args, "--data-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_data_dir);

    // Ensure data directory exists
    std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");

    // Load stores
    let physicians = store::physicians::PhysicianManager::load(data_dir.join("physicians.json"))
        .expect("Failed to load physicians");
    let rooms = store::rooms::RoomManager::load(data_dir.join("rooms.json"))
        .expect("Failed to load rooms");
    let speakers = store::speakers::SpeakerManager::load(data_dir.join("speakers.json"))
        .expect("Failed to load speakers");
    let sessions = store::sessions::SessionStore::new(data_dir.join("sessions"));

    let state = Arc::new(AppState {
        physicians: RwLock::new(physicians),
        rooms: RwLock::new(rooms),
        speakers: RwLock::new(speakers),
        sessions,
        data_dir: data_dir.clone(),
    });

    // Build router
    let app = routes::build_router(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(RequestBodyLimitLayer::new(500 * 1024 * 1024)); // 500 MB for audio

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

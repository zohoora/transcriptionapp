pub mod auth;
pub mod error;
pub mod routes;
pub mod store;
pub mod types;

use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;

use store::AppState;

/// Create an `AppState` from a data directory path.
/// The directory (and all store files) will be created if they don't exist.
pub fn create_app_state(data_dir: &Path) -> Arc<AppState> {
    std::fs::create_dir_all(data_dir).expect("Failed to create data directory");

    let physicians =
        store::physicians::PhysicianManager::load(data_dir.join("physicians.json"))
            .expect("Failed to load physicians");
    let rooms = store::rooms::RoomManager::load(data_dir.join("rooms.json"))
        .expect("Failed to load rooms");
    let speakers =
        store::speakers::SpeakerManager::load(data_dir.join("speakers.json"))
            .expect("Failed to load speakers");
    let infrastructure =
        store::infrastructure::InfrastructureStore::load(data_dir.join("infrastructure.json"))
            .expect("Failed to load infrastructure settings");
    let sessions = store::sessions::SessionStore::new(data_dir.join("sessions"));
    let mobile_jobs = store::mobile_jobs::MobileJobStore::load(
        data_dir.join("mobile_jobs.json"),
        data_dir.join("mobile_uploads"),
    )
    .expect("Failed to load mobile jobs");
    let config_data = store::config_data::ConfigDataStore::load(data_dir)
        .expect("Failed to load config data");

    Arc::new(AppState {
        physicians: RwLock::new(physicians),
        rooms: RwLock::new(rooms),
        speakers: RwLock::new(speakers),
        infrastructure: RwLock::new(infrastructure),
        sessions,
        mobile_jobs: RwLock::new(mobile_jobs),
        config_data: RwLock::new(config_data),
        data_dir: data_dir.to_path_buf(),
    })
}

/// Build the full application router with middleware layers.
///
/// Layer order (outermost → innermost): CORS → body limit → auth.
/// Requests flow: CORS check → body limit → auth → route handler.
pub fn build_app(
    state: Arc<AppState>,
    api_key: Option<String>,
) -> axum::Router {
    let router = routes::build_router(state);

    // Auth middleware (innermost — runs closest to handlers)
    let key = api_key;
    let router = router.layer(axum::middleware::from_fn(move |req, next| {
        auth::check_api_key(key.clone(), req, next)
    }));

    // Body limit layer
    let router = router.layer(RequestBodyLimitLayer::new(500 * 1024 * 1024)); // 500 MB for audio

    // CORS layer (outermost)
    router.layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    )
}

pub mod infrastructure;
pub mod physicians;
pub mod rooms;
pub mod sessions;
pub mod speakers;

use std::path::PathBuf;
use tokio::sync::RwLock;

pub struct AppState {
    pub physicians: RwLock<physicians::PhysicianManager>,
    pub rooms: RwLock<rooms::RoomManager>,
    pub speakers: RwLock<speakers::SpeakerManager>,
    pub infrastructure: RwLock<infrastructure::InfrastructureStore>,
    pub sessions: sessions::SessionStore,
    pub data_dir: PathBuf,
}

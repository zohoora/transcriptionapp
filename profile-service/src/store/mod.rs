pub mod config_data;
pub mod infrastructure;
pub mod mobile_jobs;
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
    pub mobile_jobs: RwLock<mobile_jobs::MobileJobStore>,
    pub config_data: RwLock<config_data::ConfigDataStore>,
    pub data_dir: PathBuf,
}

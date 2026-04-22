pub mod config_data;
pub mod infrastructure;
pub mod medplum_auth;
pub mod mobile_jobs;
pub mod openai_image;
pub mod patients;
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
    pub patients: RwLock<patients::PatientManager>,
    pub medplum_auth: medplum_auth::MedplumAuthProxy,
    pub openai_image: openai_image::OpenAIImageProxy,
    pub data_dir: PathBuf,
}

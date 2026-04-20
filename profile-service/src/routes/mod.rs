pub mod config_data;
pub mod health;
pub mod infrastructure;
pub mod mobile;
pub mod patients;
pub mod physicians;
pub mod rooms;
pub mod sessions;
pub mod speakers;

use crate::store::AppState;
use axum::routing::{get, post, put};
use axum::Router;
use std::sync::Arc;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Health
        .route("/health", get(health::health))
        // Mobile — upload & job tracking
        .route("/mobile/upload", post(mobile::upload))
        .route(
            "/mobile/jobs",
            get(mobile::list_jobs),
        )
        .route(
            "/mobile/jobs/:job_id",
            get(mobile::get_job)
                .put(mobile::update_job)
                .delete(mobile::delete_job),
        )
        .route(
            "/mobile/uploads/:job_id",
            get(mobile::download_audio),
        )
        // Infrastructure settings (singleton)
        .route("/infrastructure", get(infrastructure::get).put(infrastructure::update))
        // Server-configurable data (prompts, billing rules, detection thresholds)
        .route("/config/version", get(config_data::get_version))
        .route("/config/prompts", get(config_data::get_prompts).put(config_data::update_prompts))
        .route("/config/billing", get(config_data::get_billing).put(config_data::update_billing))
        .route("/config/thresholds", get(config_data::get_thresholds).put(config_data::update_thresholds))
        .route("/config/defaults", get(config_data::get_defaults).put(config_data::update_defaults))
        // Physicians
        .route("/physicians", get(physicians::list).post(physicians::create))
        .route(
            "/physicians/:id",
            get(physicians::get)
                .put(physicians::update)
                .delete(physicians::delete),
        )
        // Rooms
        .route("/rooms", get(rooms::list).post(rooms::create))
        .route(
            "/rooms/:id",
            get(rooms::get)
                .put(rooms::update)
                .delete(rooms::delete),
        )
        // Speakers
        .route("/speakers", get(speakers::list).post(speakers::create))
        .route(
            "/speakers/:id",
            get(speakers::get)
                .put(speakers::update)
                .delete(speakers::delete),
        )
        // Sessions — dates & list
        .route(
            "/physicians/:id/sessions/dates",
            get(sessions::list_dates),
        )
        .route(
            "/physicians/:id/sessions",
            get(sessions::list_sessions),
        )
        // Sessions — merge & renumber (collection-level, must be before :sid routes)
        .route(
            "/physicians/:id/sessions/merge",
            post(sessions::merge_sessions),
        )
        .route(
            "/physicians/:id/sessions/renumber",
            post(sessions::renumber_encounters),
        )
        // Sessions — individual CRUD
        .route(
            "/physicians/:physician_id/sessions/:session_id",
            get(sessions::get_session)
                .post(sessions::upload_session)
                .delete(sessions::delete_session),
        )
        .route(
            "/physicians/:physician_id/sessions/:session_id/metadata",
            put(sessions::update_metadata),
        )
        .route(
            "/physicians/:physician_id/sessions/:session_id/soap",
            put(sessions::update_soap),
        )
        .route(
            "/physicians/:physician_id/sessions/:session_id/feedback",
            put(sessions::update_feedback),
        )
        .route(
            "/physicians/:physician_id/sessions/:session_id/patient-name",
            put(sessions::update_patient_name),
        )
        .route(
            "/physicians/:physician_id/sessions/:session_id/transcript-lines",
            get(sessions::get_transcript_lines),
        )
        .route(
            "/physicians/:physician_id/sessions/:session_id/split",
            post(sessions::split_session),
        )
        .route(
            "/physicians/:physician_id/sessions/:session_id/audio",
            post(sessions::upload_audio).get(sessions::download_audio),
        )
        // Session auxiliary files
        .route(
            "/physicians/:physician_id/sessions/:session_id/files/:filename",
            put(sessions::upload_session_file).get(sessions::download_session_file),
        )
        // Session screenshot files (subdirectory)
        .route(
            "/physicians/:physician_id/sessions/:session_id/files/screenshots/:screenshot",
            put(sessions::upload_screenshot).get(sessions::download_screenshot),
        )
        // Day log per date
        .route(
            "/physicians/:physician_id/day-log/:date",
            put(sessions::upload_day_log).get(sessions::download_day_log),
        )
        // Longitudinal patient memory (v0.10.46+) — /confirm must be registered
        // before the /:patient_id route or it will be shadowed by the wildcard.
        .route(
            "/physicians/:physician_id/patients/confirm",
            post(patients::confirm),
        )
        .route(
            "/physicians/:physician_id/patients",
            get(patients::list_or_search),
        )
        .route(
            "/physicians/:physician_id/patients/:patient_id",
            get(patients::get_by_id).delete(patients::delete),
        )
        .with_state(state)
}

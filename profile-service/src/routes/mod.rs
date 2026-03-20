pub mod health;
pub mod infrastructure;
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
        // Infrastructure settings (singleton)
        .route("/infrastructure", get(infrastructure::get).put(infrastructure::update))
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
        // Day log per date
        .route(
            "/physicians/:physician_id/day-log/:date",
            put(sessions::upload_day_log).get(sessions::download_day_log),
        )
        .with_state(state)
}

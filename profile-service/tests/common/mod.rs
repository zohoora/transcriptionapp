#![allow(dead_code)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// A test application backed by a temp directory.
pub struct TestApp {
    router: axum::Router,
    _temp_dir: tempfile::TempDir, // held for lifetime
}

impl TestApp {
    /// Create a test app with no API key (all routes open).
    pub fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let state = profile_service::create_app_state(temp_dir.path());
        let router = profile_service::build_app(state, None);
        Self {
            router,
            _temp_dir: temp_dir,
        }
    }

    /// Create a test app with API key authentication enabled.
    pub fn with_auth(key: &str) -> Self {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let state = profile_service::create_app_state(temp_dir.path());
        let router = profile_service::build_app(state, Some(key.to_string()));
        Self {
            router,
            _temp_dir: temp_dir,
        }
    }

    // ── Unauthenticated helpers ─────────────────────────────────────

    pub async fn get(&self, uri: &str) -> TestResponse {
        self.request(Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap()).await
    }

    pub async fn post_json(&self, uri: &str, body: &serde_json::Value) -> TestResponse {
        self.request(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(body).unwrap()))
                .unwrap(),
        )
        .await
    }

    pub async fn put_json(&self, uri: &str, body: &serde_json::Value) -> TestResponse {
        self.request(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(body).unwrap()))
                .unwrap(),
        )
        .await
    }

    pub async fn delete(&self, uri: &str) -> TestResponse {
        self.request(Request::builder().method("DELETE").uri(uri).body(Body::empty()).unwrap()).await
    }

    // ── Authenticated helpers ───────────────────────────────────────

    pub async fn get_authed(&self, uri: &str, key: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("X-API-Key", key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    pub async fn post_json_authed(&self, uri: &str, body: &serde_json::Value, key: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .header("X-API-Key", key)
                .body(Body::from(serde_json::to_vec(body).unwrap()))
                .unwrap(),
        )
        .await
    }

    pub async fn put_json_authed(&self, uri: &str, body: &serde_json::Value, key: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header("content-type", "application/json")
                .header("X-API-Key", key)
                .body(Body::from(serde_json::to_vec(body).unwrap()))
                .unwrap(),
        )
        .await
    }

    pub async fn delete_authed(&self, uri: &str, key: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("X-API-Key", key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    // ── Internal ────────────────────────────────────────────────────

    async fn request(&self, req: Request<Body>) -> TestResponse {
        let response = self
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("Request failed");

        let status = response.status();
        let body_bytes = response
            .into_body()
            .collect()
            .await
            .expect("Failed to read body")
            .to_bytes()
            .to_vec();

        TestResponse {
            status,
            body: body_bytes,
        }
    }
}

/// Response wrapper with convenience methods.
pub struct TestResponse {
    pub status: StatusCode,
    body: Vec<u8>,
}

impl TestResponse {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("Failed to parse JSON: {e}\nBody: {}", self.text()))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    pub fn assert_ok(&self) {
        assert!(
            self.status.is_success(),
            "Expected success, got {} — body: {}",
            self.status,
            self.text()
        );
    }

    pub fn assert_status(&self, expected: StatusCode) {
        assert_eq!(
            self.status, expected,
            "Expected {expected}, got {} — body: {}",
            self.status,
            self.text()
        );
    }
}

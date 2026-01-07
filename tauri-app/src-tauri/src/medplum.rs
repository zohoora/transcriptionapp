//! Medplum EMR Integration Module
//!
//! Provides OAuth 2.0 authentication with PKCE and FHIR R4 resource operations
//! for integrating with a local Medplum server.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Default Medplum server URL (local development)
pub const DEFAULT_MEDPLUM_URL: &str = "http://localhost:8103";

/// Custom URI scheme for OAuth callback
pub const OAUTH_REDIRECT_URI: &str = "fabricscribe://oauth/callback";

/// HTTP client timeout for Medplum requests
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Medplum-specific errors
#[derive(Debug, thiserror::Error)]
pub enum MedplumError {
    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Not authenticated")]
    NotAuthenticated,

    #[error("Access denied to resource: {0}")]
    AccessDenied(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid FHIR resource: {0}")]
    ValidationError(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Invalid URL: {0}")]
    UrlError(String),
}

/// OAuth token response from Medplum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
}

/// User info from OAuth userinfo endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub sub: String,
    pub profile: String, // e.g., "Practitioner/123"
    pub name: Option<String>,
    pub email: Option<String>,
}

/// Authentication state stored in the app
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthState {
    pub is_authenticated: bool,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub token_expiry: Option<i64>, // Unix timestamp
    pub practitioner_id: Option<String>,
    pub practitioner_name: Option<String>,
}

impl AuthState {
    /// Check if the current token is expired (with 5-minute buffer)
    pub fn is_token_expired(&self) -> bool {
        if let Some(expiry) = self.token_expiry {
            let now = Utc::now().timestamp();
            now >= (expiry - 300) // 5 minute buffer
        } else {
            true
        }
    }

    /// Get the path to the auth state file
    fn auth_file_path() -> Option<std::path::PathBuf> {
        dirs::home_dir().map(|h| h.join(".transcriptionapp").join("medplum_auth.json"))
    }

    /// Save auth state to disk for persistence across app restarts
    pub fn save_to_file(&self) -> Result<(), std::io::Error> {
        if let Some(path) = Self::auth_file_path() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let json = serde_json::to_string_pretty(self)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            std::fs::write(&path, json)?;
            tracing::debug!("Saved auth state to {:?}", path);
        }
        Ok(())
    }

    /// Load auth state from disk
    pub fn load_from_file() -> Option<Self> {
        let path = Self::auth_file_path()?;
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str::<AuthState>(&content) {
                        Ok(state) => {
                            tracing::info!("Loaded saved auth state for {:?}", state.practitioner_name);
                            Some(state)
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse saved auth state: {}", e);
                            None
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read auth file: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Delete saved auth state file
    pub fn delete_file() {
        if let Some(path) = Self::auth_file_path() {
            if path.exists() {
                let _ = std::fs::remove_file(&path);
                tracing::debug!("Deleted auth state file");
            }
        }
    }
}

/// PKCE (Proof Key for Code Exchange) data
#[derive(Debug, Clone)]
pub struct PkceData {
    pub code_verifier: String,
    pub code_challenge: String,
    pub state: String,
}

impl PkceData {
    /// Generate new PKCE codes using S256 method
    pub fn new() -> Self {
        // Generate a random 32-byte code verifier
        let mut rng = rand::thread_rng();
        let verifier_bytes: [u8; 32] = rng.gen();
        let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        // Create S256 challenge: base64url(sha256(verifier))
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        let code_challenge = URL_SAFE_NO_PAD.encode(hash);

        // Generate random state for CSRF protection
        let state_bytes: [u8; 16] = rng.gen();
        let state = URL_SAFE_NO_PAD.encode(state_bytes);

        Self {
            code_verifier,
            code_challenge,
            state,
        }
    }
}

impl Default for PkceData {
    fn default() -> Self {
        Self::new()
    }
}

/// Authorization URL response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUrl {
    pub url: String,
    pub state: String,
}

/// FHIR Patient resource (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patient {
    pub id: String,
    pub name: String,
    pub mrn: Option<String>,
    #[serde(rename = "birthDate")]
    pub birth_date: Option<String>,
}

/// FHIR Encounter resource (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Encounter {
    pub id: String,
    #[serde(rename = "patientId")]
    pub patient_id: String,
    #[serde(rename = "patientName")]
    pub patient_name: String,
    pub status: String,
    #[serde(rename = "startTime")]
    pub start_time: String,
    #[serde(rename = "endTime")]
    pub end_time: Option<String>,
}

/// Encounter summary for history view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterSummary {
    pub id: String,
    #[serde(rename = "fhirId")]
    pub fhir_id: String,
    #[serde(rename = "patientName")]
    pub patient_name: String,
    pub date: String,
    #[serde(rename = "durationMinutes")]
    pub duration_minutes: Option<i64>,
    #[serde(rename = "hasSoapNote")]
    pub has_soap_note: bool,
    #[serde(rename = "hasAudio")]
    pub has_audio: bool,
}

/// Detailed encounter data for viewing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterDetails {
    #[serde(flatten)]
    pub summary: EncounterSummary,
    pub transcript: Option<String>,
    #[serde(rename = "soapNote")]
    pub soap_note: Option<String>,
    #[serde(rename = "audioUrl")]
    pub audio_url: Option<String>,
    #[serde(rename = "sessionInfo")]
    pub session_info: Option<String>,
}

/// Sync status for an encounter
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncStatus {
    #[serde(rename = "encounterSynced")]
    pub encounter_synced: bool,
    #[serde(rename = "transcriptSynced")]
    pub transcript_synced: bool,
    #[serde(rename = "soapNoteSynced")]
    pub soap_note_synced: bool,
    #[serde(rename = "audioSynced")]
    pub audio_synced: bool,
    #[serde(rename = "lastSyncTime")]
    pub last_sync_time: Option<String>,
}

/// Sync result returned after syncing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub success: bool,
    pub status: SyncStatus,
    pub error: Option<String>,
}

/// Medplum client for API interactions
#[derive(Debug)]
pub struct MedplumClient {
    http_client: reqwest::Client,
    base_url: String,
    client_id: String,
    auth_state: Arc<RwLock<AuthState>>,
    pending_pkce: Arc<RwLock<Option<PkceData>>>,
}

impl MedplumClient {
    /// Create a new Medplum client
    pub fn new(base_url: &str, client_id: &str) -> Result<Self, MedplumError> {
        let cleaned_url = base_url.trim_end_matches('/');
        info!("Creating MedplumClient with base_url: {}", cleaned_url);

        // Validate URL
        let parsed = url::Url::parse(cleaned_url)
            .map_err(|e| MedplumError::UrlError(format!("Invalid URL '{}': {}", cleaned_url, e)))?;

        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(MedplumError::UrlError(format!(
                "URL must use http or https scheme, got: {}",
                parsed.scheme()
            )));
        }

        let http_client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| MedplumError::AuthError(format!("Failed to create HTTP client: {}", e)))?;

        // Try to load saved auth state from previous session
        let initial_state = AuthState::load_from_file().unwrap_or_default();

        Ok(Self {
            http_client,
            base_url: cleaned_url.to_string(),
            client_id: client_id.to_string(),
            auth_state: Arc::new(RwLock::new(initial_state)),
            pending_pkce: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the current authentication state
    pub async fn get_auth_state(&self) -> AuthState {
        self.auth_state.read().await.clone()
    }

    /// Check if authenticated and token is valid
    pub async fn is_authenticated(&self) -> bool {
        let state = self.auth_state.read().await;
        state.is_authenticated && !state.is_token_expired()
    }

    /// Try to restore a previous session by refreshing the token if needed
    /// Returns the auth state (authenticated or not)
    pub async fn try_restore_session(&self) -> AuthState {
        let state = self.auth_state.read().await.clone();

        // If not authenticated or no refresh token, return current state
        if !state.is_authenticated || state.refresh_token.is_none() {
            return state;
        }

        // If token is still valid, return current state
        if !state.is_token_expired() {
            tracing::info!("Session restored - token still valid");
            return state;
        }

        // Token expired but we have refresh token - try to refresh
        tracing::info!("Session token expired, attempting refresh...");
        drop(state); // Release read lock before refresh

        match self.refresh_token().await {
            Ok(new_state) => {
                tracing::info!("Session restored via token refresh");
                new_state
            }
            Err(e) => {
                tracing::warn!("Failed to refresh token: {}", e);
                // Clear the invalid session
                self.logout().await;
                AuthState::default()
            }
        }
    }

    /// Check if the server is reachable (doesn't require authentication)
    pub async fn check_server_connectivity(&self) -> bool {
        let url = format!("{}/.well-known/openid-configuration", self.base_url);
        match self.http_client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    // =====================
    // OAuth 2.0 Methods
    // =====================

    /// Start the OAuth authorization flow
    /// Returns the authorization URL to open in a browser
    pub async fn start_auth_flow(&self) -> Result<AuthUrl, MedplumError> {
        let pkce = PkceData::new();
        let state = pkce.state.clone();

        // Build authorization URL
        let auth_url = format!(
            "{}/oauth2/authorize?response_type=code&client_id={}&redirect_uri={}&scope=openid%20profile&code_challenge={}&code_challenge_method=S256&state={}",
            self.base_url,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(OAUTH_REDIRECT_URI),
            urlencoding::encode(&pkce.code_challenge),
            urlencoding::encode(&pkce.state)
        );

        info!("Generated OAuth URL: {}", auth_url);

        // Store PKCE data for later use
        *self.pending_pkce.write().await = Some(pkce);

        Ok(AuthUrl { url: auth_url, state })
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str, state: &str) -> Result<AuthState, MedplumError> {
        // Get and consume PKCE data
        let pkce = self
            .pending_pkce
            .write()
            .await
            .take()
            .ok_or_else(|| MedplumError::AuthError("No pending auth flow".to_string()))?;

        // Verify state matches
        if pkce.state != state {
            return Err(MedplumError::AuthError("State mismatch - possible CSRF attack".to_string()));
        }

        // Exchange code for tokens
        let token_response: TokenResponse = self
            .http_client
            .post(&format!("{}/oauth2/token", self.base_url))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", &self.client_id),
                ("redirect_uri", OAUTH_REDIRECT_URI),
                ("code_verifier", &pkce.code_verifier),
            ])
            .send()
            .await?
            .json()
            .await?;

        // Get user info
        let user_info = self.get_user_info(&token_response.access_token).await?;

        // Extract practitioner ID from profile reference
        let practitioner_id = user_info
            .profile
            .strip_prefix("Practitioner/")
            .map(|s| s.to_string());

        // Calculate token expiry
        let token_expiry = token_response.expires_in.map(|secs| {
            Utc::now().timestamp() + secs as i64
        });

        // Update auth state
        let new_state = AuthState {
            is_authenticated: true,
            access_token: Some(token_response.access_token),
            refresh_token: token_response.refresh_token,
            token_expiry,
            practitioner_id,
            practitioner_name: user_info.name,
        };

        *self.auth_state.write().await = new_state.clone();

        // Save auth state to disk for persistence
        if let Err(e) = new_state.save_to_file() {
            tracing::warn!("Failed to save auth state: {}", e);
        }

        Ok(new_state)
    }

    /// Get user info from the OAuth userinfo endpoint
    async fn get_user_info(&self, access_token: &str) -> Result<UserInfo, MedplumError> {
        let response = self
            .http_client
            .get(&format!("{}/oauth2/userinfo", self.base_url))
            .bearer_auth(access_token)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Refresh the access token using the refresh token
    pub async fn refresh_token(&self) -> Result<AuthState, MedplumError> {
        let current_state = self.auth_state.read().await.clone();

        let refresh_token = current_state
            .refresh_token
            .ok_or_else(|| MedplumError::AuthError("No refresh token available".to_string()))?;

        let token_response: TokenResponse = self
            .http_client
            .post(&format!("{}/oauth2/token", self.base_url))
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", &self.client_id),
                ("refresh_token", &refresh_token),
            ])
            .send()
            .await?
            .json()
            .await?;

        // Calculate new token expiry
        let token_expiry = token_response.expires_in.map(|secs| {
            Utc::now().timestamp() + secs as i64
        });

        // Update auth state
        let mut state = self.auth_state.write().await;
        state.access_token = Some(token_response.access_token);
        if let Some(new_refresh) = token_response.refresh_token {
            state.refresh_token = Some(new_refresh);
        }
        state.token_expiry = token_expiry;

        // Save updated auth state to disk
        if let Err(e) = state.save_to_file() {
            tracing::warn!("Failed to save refreshed auth state: {}", e);
        }

        Ok(state.clone())
    }

    /// Logout and clear tokens
    pub async fn logout(&self) {
        *self.auth_state.write().await = AuthState::default();
        // Delete saved auth state file
        AuthState::delete_file();
    }

    /// Get access token, refreshing if needed
    async fn get_valid_token(&self) -> Result<String, MedplumError> {
        {
            let state = self.auth_state.read().await;
            if !state.is_authenticated {
                return Err(MedplumError::NotAuthenticated);
            }

            if !state.is_token_expired() {
                if let Some(ref token) = state.access_token {
                    return Ok(token.clone());
                }
            }
        }

        // Token is expired, try to refresh
        let new_state = self.refresh_token().await?;
        new_state
            .access_token
            .ok_or_else(|| MedplumError::AuthError("Failed to get new token".to_string()))
    }

    // =====================
    // FHIR Resource Methods
    // =====================

    /// Search for patients by name or MRN
    pub async fn search_patients(&self, query: &str) -> Result<Vec<Patient>, MedplumError> {
        let token = self.get_valid_token().await?;

        let response = self
            .http_client
            .get(&format!(
                "{}/fhir/R4/Patient?name:contains={}&_count=20",
                self.base_url,
                urlencoding::encode(query)
            ))
            .bearer_auth(&token)
            .send()
            .await?;

        let bundle: serde_json::Value = self.handle_response(response).await?;

        let mut patients = Vec::new();
        if let Some(entries) = bundle["entry"].as_array() {
            for entry in entries {
                let resource = &entry["resource"];
                if let Some(id) = resource["id"].as_str() {
                    let name = self.extract_patient_name(resource);
                    let mrn = resource["identifier"]
                        .as_array()
                        .and_then(|ids| {
                            ids.iter().find(|id| {
                                id["system"].as_str() == Some("http://hospital.example.org/mrn")
                            })
                        })
                        .and_then(|id| id["value"].as_str())
                        .map(|s| s.to_string());

                    patients.push(Patient {
                        id: id.to_string(),
                        name,
                        mrn,
                        birth_date: resource["birthDate"].as_str().map(|s| s.to_string()),
                    });
                }
            }
        }

        Ok(patients)
    }

    /// Extract patient name from FHIR Patient resource
    fn extract_patient_name(&self, resource: &serde_json::Value) -> String {
        if let Some(names) = resource["name"].as_array() {
            if let Some(name) = names.first() {
                let family = name["family"].as_str().unwrap_or("");
                let given = name["given"]
                    .as_array()
                    .and_then(|g| g.first())
                    .and_then(|g| g.as_str())
                    .unwrap_or("");
                return format!("{} {}", given, family).trim().to_string();
            }
        }
        "Unknown Patient".to_string()
    }

    /// Create a placeholder patient for storing scribe sessions
    /// This creates a patient record that can be easily identified as app-created
    pub async fn create_placeholder_patient(&self) -> Result<Patient, MedplumError> {
        let token = self.get_valid_token().await?;
        let state = self.auth_state.read().await;

        let practitioner_id = state
            .practitioner_id
            .as_ref()
            .ok_or_else(|| MedplumError::NotAuthenticated)?;

        let practitioner_name = state
            .practitioner_name
            .clone()
            .unwrap_or_else(|| "Unknown Practitioner".to_string());

        // Generate unique identifier for this session
        let session_uuid = uuid::Uuid::new_v4().to_string();
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M").to_string();

        // Create a placeholder patient with identifiable metadata
        let patient = serde_json::json!({
            "resourceType": "Patient",
            "identifier": [{
                "system": "urn:fabricscribe:session",
                "value": session_uuid
            }],
            "name": [{
                "use": "official",
                "family": "Session",
                "given": ["Scribe"],
                "text": format!("Scribe Session - {}", timestamp)
            }],
            "meta": {
                "tag": [{
                    "system": "urn:fabricscribe",
                    "code": "placeholder-patient"
                }, {
                    "system": "urn:fabricscribe:practitioner",
                    "code": practitioner_id.clone()
                }]
            },
            "generalPractitioner": [{
                "reference": format!("Practitioner/{}", practitioner_id)
            }],
            "active": true
        });

        let response = self
            .http_client
            .post(&format!("{}/fhir/R4/Patient", self.base_url))
            .bearer_auth(&token)
            .json(&patient)
            .send()
            .await?;

        let created: serde_json::Value = self.handle_response(response).await?;

        let patient_id = created["id"]
            .as_str()
            .ok_or_else(|| MedplumError::ValidationError("No patient ID in response".to_string()))?
            .to_string();

        info!("Created placeholder patient: {} for practitioner: {}", patient_id, practitioner_name);

        Ok(Patient {
            id: patient_id,
            name: format!("Scribe Session - {}", timestamp),
            mrn: Some(session_uuid),
            birth_date: None,
        })
    }

    /// Create a new encounter for a patient
    pub async fn create_encounter(&self, patient_id: &str) -> Result<Encounter, MedplumError> {
        let token = self.get_valid_token().await?;
        let state = self.auth_state.read().await;

        let practitioner_id = state
            .practitioner_id
            .as_ref()
            .ok_or_else(|| MedplumError::NotAuthenticated)?;

        // Generate unique encounter identifier
        let encounter_uuid = uuid::Uuid::new_v4().to_string();
        let start_time = Utc::now().to_rfc3339();

        // First, get patient name
        let patient_response = self
            .http_client
            .get(&format!("{}/fhir/R4/Patient/{}", self.base_url, patient_id))
            .bearer_auth(&token)
            .send()
            .await?;

        let patient_resource: serde_json::Value = self.handle_response(patient_response).await?;
        let patient_name = self.extract_patient_name(&patient_resource);

        // Create encounter resource
        let encounter = serde_json::json!({
            "resourceType": "Encounter",
            "identifier": [{
                "system": "urn:fabricscribe:encounter",
                "value": encounter_uuid
            }],
            "status": "in-progress",
            "class": {
                "system": "http://terminology.hl7.org/CodeSystem/v3-ActCode",
                "code": "AMB",
                "display": "ambulatory"
            },
            "subject": {
                "reference": format!("Patient/{}", patient_id)
            },
            "participant": [{
                "type": [{
                    "coding": [{
                        "system": "http://terminology.hl7.org/CodeSystem/v3-ParticipationType",
                        "code": "PPRF",
                        "display": "primary performer"
                    }]
                }],
                "individual": {
                    "reference": format!("Practitioner/{}", practitioner_id)
                }
            }],
            "period": {
                "start": start_time
            },
            "meta": {
                "tag": [{
                    "system": "urn:fabricscribe",
                    "code": "scribe-session"
                }]
            }
        });

        let response = self
            .http_client
            .post(&format!("{}/fhir/R4/Encounter", self.base_url))
            .bearer_auth(&token)
            .json(&encounter)
            .send()
            .await?;

        let created: serde_json::Value = self.handle_response(response).await?;

        // Use the server-returned ID, not the pre-generated UUID
        let fhir_id = created["id"].as_str().unwrap_or(&encounter_uuid).to_string();
        tracing::info!("Created encounter with FHIR ID: {} (local UUID was: {})", fhir_id, encounter_uuid);

        Ok(Encounter {
            id: fhir_id,
            patient_id: patient_id.to_string(),
            patient_name,
            status: "in-progress".to_string(),
            start_time,
            end_time: None,
        })
    }

    /// Upload transcript to an encounter
    pub async fn upload_transcript(
        &self,
        encounter_id: &str,
        encounter_fhir_id: &str,
        patient_id: &str,
        transcript: &str,
    ) -> Result<String, MedplumError> {
        let token = self.get_valid_token().await?;

        let doc_ref = serde_json::json!({
            "resourceType": "DocumentReference",
            "identifier": [{
                "system": "urn:fabricscribe:encounter",
                "value": encounter_id
            }],
            "status": "current",
            "type": {
                "coding": [{
                    "system": "http://loinc.org",
                    "code": "75476-2",
                    "display": "Transcript"
                }]
            },
            "category": [{
                "coding": [{
                    "system": "urn:fabricscribe",
                    "code": "transcription"
                }]
            }],
            "subject": {
                "reference": format!("Patient/{}", patient_id)
            },
            "context": {
                "encounter": [{
                    "reference": format!("Encounter/{}", encounter_fhir_id)
                }]
            },
            "content": [{
                "attachment": {
                    "contentType": "text/plain",
                    "data": base64::engine::general_purpose::STANDARD.encode(transcript)
                }
            }],
            "date": Utc::now().to_rfc3339()
        });

        let response = self
            .http_client
            .post(&format!("{}/fhir/R4/DocumentReference", self.base_url))
            .bearer_auth(&token)
            .json(&doc_ref)
            .send()
            .await?;

        let created: serde_json::Value = self.handle_response(response).await?;
        Ok(created["id"].as_str().unwrap_or("").to_string())
    }

    /// Upload SOAP note to an encounter
    pub async fn upload_soap_note(
        &self,
        encounter_id: &str,
        encounter_fhir_id: &str,
        patient_id: &str,
        soap_note: &str,
    ) -> Result<String, MedplumError> {
        let token = self.get_valid_token().await?;

        let doc_ref = serde_json::json!({
            "resourceType": "DocumentReference",
            "identifier": [{
                "system": "urn:fabricscribe:encounter",
                "value": encounter_id
            }],
            "status": "current",
            "type": {
                "coding": [{
                    "system": "http://loinc.org",
                    "code": "11506-3",
                    "display": "Progress note"
                }]
            },
            "category": [{
                "coding": [{
                    "system": "urn:fabricscribe",
                    "code": "soap-note"
                }]
            }],
            "subject": {
                "reference": format!("Patient/{}", patient_id)
            },
            "context": {
                "encounter": [{
                    "reference": format!("Encounter/{}", encounter_fhir_id)
                }]
            },
            "content": [{
                "attachment": {
                    "contentType": "text/plain",
                    "data": base64::engine::general_purpose::STANDARD.encode(soap_note)
                }
            }],
            "date": Utc::now().to_rfc3339()
        });

        let response = self
            .http_client
            .post(&format!("{}/fhir/R4/DocumentReference", self.base_url))
            .bearer_auth(&token)
            .json(&doc_ref)
            .send()
            .await?;

        let created: serde_json::Value = self.handle_response(response).await?;
        Ok(created["id"].as_str().unwrap_or("").to_string())
    }

    /// Upload audio recording as Binary + Media resources
    pub async fn upload_audio(
        &self,
        encounter_id: &str,
        encounter_fhir_id: &str,
        patient_id: &str,
        audio_data: &[u8],
        content_type: &str,
        duration_seconds: Option<u64>,
    ) -> Result<String, MedplumError> {
        let token = self.get_valid_token().await?;

        // Step 1: Upload binary audio data
        let binary_response = self
            .http_client
            .post(&format!("{}/fhir/R4/Binary", self.base_url))
            .bearer_auth(&token)
            .header("Content-Type", content_type)
            .body(audio_data.to_vec())
            .send()
            .await?;

        let binary: serde_json::Value = self.handle_response(binary_response).await?;
        let binary_id = binary["id"].as_str().unwrap_or("");

        // Step 2: Create Media resource linking to Binary
        let mut media = serde_json::json!({
            "resourceType": "Media",
            "identifier": [{
                "system": "urn:fabricscribe:encounter",
                "value": encounter_id
            }],
            "status": "completed",
            "type": {
                "coding": [{
                    "system": "http://terminology.hl7.org/CodeSystem/media-type",
                    "code": "audio",
                    "display": "Audio"
                }]
            },
            "subject": {
                "reference": format!("Patient/{}", patient_id)
            },
            "encounter": {
                "reference": format!("Encounter/{}", encounter_fhir_id)
            },
            "content": {
                "contentType": content_type,
                "url": format!("Binary/{}", binary_id)
            },
            "meta": {
                "tag": [{
                    "system": "urn:fabricscribe",
                    "code": "scribe-session"
                }]
            }
        });

        if let Some(duration) = duration_seconds {
            media["duration"] = serde_json::json!(duration);
        }

        let media_response = self
            .http_client
            .post(&format!("{}/fhir/R4/Media", self.base_url))
            .bearer_auth(&token)
            .json(&media)
            .send()
            .await?;

        let created: serde_json::Value = self.handle_response(media_response).await?;
        Ok(created["id"].as_str().unwrap_or("").to_string())
    }

    /// Complete an encounter (set status to finished)
    pub async fn complete_encounter(&self, encounter_fhir_id: &str) -> Result<(), MedplumError> {
        let token = self.get_valid_token().await?;

        // Get current encounter
        let get_response = self
            .http_client
            .get(&format!("{}/fhir/R4/Encounter/{}", self.base_url, encounter_fhir_id))
            .bearer_auth(&token)
            .send()
            .await?;

        let mut encounter: serde_json::Value = self.handle_response(get_response).await?;

        // Update status and end time
        encounter["status"] = serde_json::json!("finished");
        encounter["period"]["end"] = serde_json::json!(Utc::now().to_rfc3339());

        // PUT the updated encounter
        let put_response = self
            .http_client
            .put(&format!("{}/fhir/R4/Encounter/{}", self.base_url, encounter_fhir_id))
            .bearer_auth(&token)
            .json(&encounter)
            .send()
            .await?;

        self.handle_response::<serde_json::Value>(put_response).await?;
        Ok(())
    }

    /// Get encounter history for the current practitioner
    pub async fn get_encounter_history(
        &self,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> Result<Vec<EncounterSummary>, MedplumError> {
        let token = self.get_valid_token().await?;
        let state = self.auth_state.read().await;

        let practitioner_id = state
            .practitioner_id
            .as_ref()
            .ok_or_else(|| MedplumError::NotAuthenticated)?;

        // Build query URL - search all encounters, filter by practitioner and documents in code
        // (Medplum has issues with complex combined searches)
        let mut url = format!(
            "{}/fhir/R4/Encounter?_sort=-date&_count=100",
            self.base_url
        );

        if let Some(start) = start_date {
            // Start date is inclusive - query for >= start day midnight UTC
            url.push_str(&format!("&date=ge{}T00:00:00Z", start));
        }
        if let Some(end) = end_date {
            // End date is inclusive - query for < next day midnight UTC
            if let Ok(date) = NaiveDate::parse_from_str(end, "%Y-%m-%d") {
                let next_day = date + Duration::days(1);
                url.push_str(&format!("&date=lt{}T00:00:00Z", next_day.format("%Y-%m-%d")));
            } else {
                // Fallback if parsing fails - use end of day
                url.push_str(&format!("&date=le{}T23:59:59Z", end));
            }
        }

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await?;

        let bundle: serde_json::Value = self.handle_response(response).await?;

        let mut encounters = Vec::new();
        let practitioner_ref = format!("Practitioner/{}", practitioner_id);

        if let Some(entries) = bundle["entry"].as_array() {
            for entry in entries {
                let resource = &entry["resource"];

                // Filter by practitioner (since we removed participant from URL query)
                let is_our_encounter = resource["participant"]
                    .as_array()
                    .map(|participants| {
                        participants.iter().any(|p| {
                            p["individual"]["reference"].as_str() == Some(&practitioner_ref)
                        })
                    })
                    .unwrap_or(false);

                if !is_our_encounter {
                    continue;
                }

                let fhir_id = resource["id"].as_str().unwrap_or("").to_string();
                let encounter_id = resource["identifier"]
                    .as_array()
                    .and_then(|ids| ids.first())
                    .and_then(|id| id["value"].as_str())
                    .unwrap_or("")
                    .to_string();

                let start_time = resource["period"]["start"].as_str().unwrap_or("");
                let end_time = resource["period"]["end"].as_str();

                // Calculate duration if both times exist
                let duration_minutes = if let (Ok(start), Some(end_str)) = (
                    DateTime::parse_from_rfc3339(start_time),
                    end_time,
                ) {
                    if let Ok(end) = DateTime::parse_from_rfc3339(end_str) {
                        Some((end - start).num_minutes())
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Get patient name from subject reference
                let patient_name = self
                    .get_patient_name_from_encounter(&token, resource)
                    .await
                    .unwrap_or_else(|_| "Unknown".to_string());

                encounters.push(EncounterSummary {
                    id: encounter_id,
                    fhir_id,
                    patient_name,
                    date: start_time.to_string(),
                    duration_minutes,
                    has_soap_note: false, // Updated below
                    has_audio: false,     // Updated below
                });
            }
        }

        // Batch query for SOAP notes and audio for all encounters
        if !encounters.is_empty() {
            let encounter_fhir_ids: Vec<&str> =
                encounters.iter().map(|e| e.fhir_id.as_str()).collect();

            // Query SOAP notes (DocumentReference with category=soap-note)
            let soap_encounter_ids = self
                .get_encounters_with_soap_notes(&token, &encounter_fhir_ids)
                .await
                .unwrap_or_default();

            // Query audio (Media resources)
            let audio_encounter_ids = self
                .get_encounters_with_audio(&token, &encounter_fhir_ids)
                .await
                .unwrap_or_default();

            // Update encounter summaries with document indicators
            for encounter in &mut encounters {
                encounter.has_soap_note = soap_encounter_ids.contains(&encounter.fhir_id);
                encounter.has_audio = audio_encounter_ids.contains(&encounter.fhir_id);
            }
        }

        Ok(encounters)
    }

    /// Get patient name from encounter subject reference
    async fn get_patient_name_from_encounter(
        &self,
        token: &str,
        encounter: &serde_json::Value,
    ) -> Result<String, MedplumError> {
        if let Some(reference) = encounter["subject"]["reference"].as_str() {
            let response = self
                .http_client
                .get(&format!("{}/fhir/R4/{}", self.base_url, reference))
                .bearer_auth(token)
                .send()
                .await?;

            let patient: serde_json::Value = self.handle_response(response).await?;
            return Ok(self.extract_patient_name(&patient));
        }
        Ok("Unknown".to_string())
    }

    /// Get encounter FHIR IDs that have SOAP notes
    async fn get_encounters_with_soap_notes(
        &self,
        token: &str,
        encounter_ids: &[&str],
    ) -> Result<std::collections::HashSet<String>, MedplumError> {
        use std::collections::HashSet;

        let mut result = HashSet::new();
        if encounter_ids.is_empty() {
            return Ok(result);
        }

        // Query DocumentReference resources with soap-note category
        // FHIR search: context:encounter references and category code
        let url = format!(
            "{}/fhir/R4/DocumentReference?category=soap-note&_count=200&_elements=context",
            self.base_url
        );

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await?;

        let bundle: serde_json::Value = self.handle_response(response).await?;

        if let Some(entries) = bundle["entry"].as_array() {
            for entry in entries {
                // Extract encounter reference from context.encounter
                if let Some(encounters) = entry["resource"]["context"]["encounter"].as_array() {
                    for enc in encounters {
                        if let Some(reference) = enc["reference"].as_str() {
                            // Reference format: "Encounter/{id}"
                            if let Some(id) = reference.strip_prefix("Encounter/") {
                                if encounter_ids.contains(&id) {
                                    result.insert(id.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Get encounter FHIR IDs that have audio recordings
    async fn get_encounters_with_audio(
        &self,
        token: &str,
        encounter_ids: &[&str],
    ) -> Result<std::collections::HashSet<String>, MedplumError> {
        use std::collections::HashSet;

        let mut result = HashSet::new();
        if encounter_ids.is_empty() {
            return Ok(result);
        }

        // Query Media resources
        let url = format!(
            "{}/fhir/R4/Media?_count=200&_elements=encounter",
            self.base_url
        );

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await?;

        let bundle: serde_json::Value = self.handle_response(response).await?;

        if let Some(entries) = bundle["entry"].as_array() {
            for entry in entries {
                // Extract encounter reference
                if let Some(reference) = entry["resource"]["encounter"]["reference"].as_str() {
                    // Reference format: "Encounter/{id}"
                    if let Some(id) = reference.strip_prefix("Encounter/") {
                        if encounter_ids.contains(&id) {
                            result.insert(id.to_string());
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Get detailed encounter data including documents
    pub async fn get_encounter_details(&self, encounter_id: &str) -> Result<EncounterDetails, MedplumError> {
        let token = self.get_valid_token().await?;

        // Fetch encounter directly by FHIR ID
        let encounter_response = self
            .http_client
            .get(&format!(
                "{}/fhir/R4/Encounter/{}",
                self.base_url, encounter_id
            ))
            .bearer_auth(&token)
            .send()
            .await?;

        let encounter: serde_json::Value = self.handle_response(encounter_response).await?;

        let fhir_id = encounter["id"].as_str().unwrap_or("").to_string();
        let start_time = encounter["period"]["start"].as_str().unwrap_or("");
        let end_time = encounter["period"]["end"].as_str();

        let duration_minutes = if let (Ok(start), Some(end_str)) = (
            DateTime::parse_from_rfc3339(start_time),
            end_time,
        ) {
            if let Ok(end) = DateTime::parse_from_rfc3339(end_str) {
                Some((end - start).num_minutes())
            } else {
                None
            }
        } else {
            None
        };

        let patient_name = self
            .get_patient_name_from_encounter(&token, &encounter)
            .await
            .unwrap_or_else(|_| "Unknown".to_string());

        // Fetch documents by encounter reference
        let docs_response = self
            .http_client
            .get(&format!(
                "{}/fhir/R4/DocumentReference?encounter=Encounter/{}",
                self.base_url, encounter_id
            ))
            .bearer_auth(&token)
            .send()
            .await?;

        let docs_bundle: serde_json::Value = self.handle_response(docs_response).await?;

        let mut transcript = None;
        let mut soap_note = None;
        let mut session_info = None;

        if let Some(entries) = docs_bundle["entry"].as_array() {
            for entry in entries {
                let resource = &entry["resource"];
                let category = resource["category"]
                    .as_array()
                    .and_then(|c| c.first())
                    .and_then(|c| c["coding"].as_array())
                    .and_then(|c| c.first())
                    .and_then(|c| c["code"].as_str())
                    .unwrap_or("");

                let content_data = resource["content"]
                    .as_array()
                    .and_then(|c| c.first())
                    .and_then(|c| c["attachment"]["data"].as_str())
                    .and_then(|d| {
                        base64::engine::general_purpose::STANDARD
                            .decode(d)
                            .ok()
                            .and_then(|bytes| String::from_utf8(bytes).ok())
                    });

                match category {
                    "transcription" => transcript = content_data,
                    "soap-note" => soap_note = content_data,
                    "session-info" => session_info = content_data,
                    _ => {}
                }
            }
        }

        // Fetch media for audio URL by encounter reference
        let media_response = self
            .http_client
            .get(&format!(
                "{}/fhir/R4/Media?encounter=Encounter/{}",
                self.base_url, encounter_id
            ))
            .bearer_auth(&token)
            .send()
            .await?;

        let media_bundle: serde_json::Value = self.handle_response(media_response).await?;
        let audio_url = media_bundle["entry"]
            .as_array()
            .and_then(|e| e.first())
            .and_then(|e| e["resource"]["content"]["url"].as_str())
            .map(|url| format!("{}/fhir/R4/{}", self.base_url, url));

        Ok(EncounterDetails {
            summary: EncounterSummary {
                id: encounter_id.to_string(),
                fhir_id,
                patient_name,
                date: start_time.to_string(),
                duration_minutes,
                has_soap_note: soap_note.is_some(),
                has_audio: audio_url.is_some(),
            },
            transcript,
            soap_note,
            audio_url,
            session_info,
        })
    }

    /// Fetch raw binary data (e.g., audio files) from Medplum
    pub async fn get_audio_data(&self, binary_id: &str) -> Result<Vec<u8>, MedplumError> {
        let token = self.get_valid_token().await?;

        let response = self
            .http_client
            .get(&format!("{}/fhir/R4/Binary/{}", self.base_url, binary_id))
            .bearer_auth(&token)
            .header("Accept", "application/octet-stream")
            .send()
            .await?;

        match response.status() {
            status if status.is_success() => {
                Ok(response.bytes().await?.to_vec())
            }
            reqwest::StatusCode::UNAUTHORIZED => {
                Err(MedplumError::TokenExpired)
            }
            reqwest::StatusCode::FORBIDDEN => {
                let body = response.text().await.unwrap_or_default();
                Err(MedplumError::AccessDenied(body))
            }
            reqwest::StatusCode::NOT_FOUND => {
                Err(MedplumError::NotFound(format!("Binary/{}", binary_id)))
            }
            _ => {
                let body = response.text().await.unwrap_or_default();
                Err(MedplumError::AuthError(format!(
                    "Failed to fetch audio: {}",
                    body
                )))
            }
        }
    }

    /// Handle HTTP response and convert to appropriate error
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, MedplumError> {
        match response.status() {
            status if status.is_success() => {
                Ok(response.json().await?)
            }
            reqwest::StatusCode::UNAUTHORIZED => {
                Err(MedplumError::TokenExpired)
            }
            reqwest::StatusCode::FORBIDDEN => {
                let body = response.text().await.unwrap_or_default();
                Err(MedplumError::AccessDenied(body))
            }
            reqwest::StatusCode::NOT_FOUND => {
                Err(MedplumError::NotFound("Resource not found".into()))
            }
            reqwest::StatusCode::UNPROCESSABLE_ENTITY => {
                let body = response.text().await.unwrap_or_default();
                Err(MedplumError::ValidationError(body))
            }
            _ => {
                let body = response.text().await.unwrap_or_default();
                Err(MedplumError::AuthError(format!(
                    "Unexpected response: {}",
                    body
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let pkce = PkceData::new();

        // Verify verifier is URL-safe base64 encoded
        assert!(!pkce.code_verifier.is_empty());
        assert!(!pkce.code_verifier.contains('+'));
        assert!(!pkce.code_verifier.contains('/'));

        // Verify challenge is derived from verifier
        assert!(!pkce.code_challenge.is_empty());
        assert!(!pkce.code_challenge.contains('+'));
        assert!(!pkce.code_challenge.contains('/'));

        // Verify state is generated
        assert!(!pkce.state.is_empty());

        // Verify different calls generate different values
        let pkce2 = PkceData::new();
        assert_ne!(pkce.code_verifier, pkce2.code_verifier);
    }

    #[test]
    fn test_auth_state_expiry() {
        let mut state = AuthState {
            is_authenticated: true,
            access_token: Some("test".to_string()),
            refresh_token: Some("refresh".to_string()),
            token_expiry: Some(Utc::now().timestamp() + 3600), // 1 hour from now
            practitioner_id: Some("123".to_string()),
            practitioner_name: Some("Dr. Test".to_string()),
        };

        // Token should not be expired
        assert!(!state.is_token_expired());

        // Set expiry to past
        state.token_expiry = Some(Utc::now().timestamp() - 100);
        assert!(state.is_token_expired());

        // Set expiry to within 5 minute buffer
        state.token_expiry = Some(Utc::now().timestamp() + 200);
        assert!(state.is_token_expired());
    }

    #[test]
    fn test_client_creation() {
        let client = MedplumClient::new("http://localhost:8103", "test-client-id");
        assert!(client.is_ok());

        // Invalid URL should fail
        let client = MedplumClient::new("not-a-url", "test-client-id");
        assert!(client.is_err());

        // Invalid scheme should fail
        let client = MedplumClient::new("ftp://localhost:8103", "test-client-id");
        assert!(client.is_err());
    }

    #[test]
    fn test_sync_status_default() {
        let status = SyncStatus::default();
        assert!(!status.encounter_synced);
        assert!(!status.transcript_synced);
        assert!(!status.soap_note_synced);
        assert!(!status.audio_synced);
        assert!(status.last_sync_time.is_none());
    }

    /// Integration test to verify Medplum server connection
    /// Run with: cargo test test_medplum_server_connection -- --ignored
    #[tokio::test]
    #[ignore]
    async fn test_medplum_server_connection() {
        let client = MedplumClient::new(
            "http://localhost:8103",
            "18abd78d-96be-4901-9351-59b597de6407",
        )
        .expect("Failed to create client");

        // Test generating auth URL
        let auth_url = client.start_auth_flow().await.expect("Failed to start auth flow");
        assert!(auth_url.url.contains("oauth2/authorize"));
        assert!(auth_url.url.contains("18abd78d-96be-4901-9351-59b597de6407"));
        // URL-encoded: fabricscribe%3A%2F%2Foauth%2Fcallback
        assert!(auth_url.url.contains("fabricscribe"));
        assert!(auth_url.url.contains("oauth") && auth_url.url.contains("callback"));
        println!("Auth URL: {}", auth_url.url);

        // Test that we can reach the server's well-known endpoint
        let response = reqwest::get("http://localhost:8103/.well-known/openid-configuration")
            .await
            .expect("Failed to reach Medplum server");
        assert!(response.status().is_success());

        let config: serde_json::Value = response.json().await.expect("Invalid JSON");
        assert_eq!(config["issuer"], "http://localhost:8103/");
        println!("Medplum server is reachable and configured correctly");
    }

    /// Integration test to verify FHIR metadata endpoint
    /// Run with: cargo test test_medplum_fhir_metadata -- --ignored
    #[tokio::test]
    #[ignore]
    async fn test_medplum_fhir_metadata() {
        let response = reqwest::get("http://localhost:8103/fhir/R4/metadata")
            .await
            .expect("Failed to reach FHIR metadata");
        assert!(response.status().is_success());

        let metadata: serde_json::Value = response.json().await.expect("Invalid JSON");
        assert_eq!(metadata["resourceType"], "CapabilityStatement");
        assert_eq!(metadata["fhirVersion"], "4.0.1");
        println!("FHIR R4 endpoint is available");
    }
}

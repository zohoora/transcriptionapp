use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{info, warn};

// Re-use types from the profile service (mirrored here for decoupling)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicianProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specialty: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_detail_level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_custom_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charting_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_require_enrolled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_required_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_end_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_end_silence_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_merge_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_check_interval_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_silence_trigger_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_auto_sync: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diarization_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_speakers: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_practitioner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_default_visit_setting: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_counselling_exhausted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_is_hospital: Option<bool>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerProfile {
    pub id: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub embedding: Vec<f32>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    // Room-tier settings
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_detection_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_sensor_port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_sensor_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_absence_threshold_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_debounce_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal_hot_pixel_threshold_c: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub co2_baseline_ppm: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hybrid_confirm_window_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hybrid_min_words_for_sensor_split: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_encounter_timeout_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_capture_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_capture_interval_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_active_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_csv_log_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_csv_log_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vad_threshold: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silence_to_flush_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_utterance_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greeting_sensitivity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_speech_duration_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_storage_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_device_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Room> for crate::config::RoomOverlay {
    fn from(r: &Room) -> Self {
        Self {
            encounter_detection_mode: r.encounter_detection_mode.clone(),
            presence_sensor_port: r.presence_sensor_port.clone(),
            presence_sensor_url: r.presence_sensor_url.clone(),
            presence_absence_threshold_secs: r.presence_absence_threshold_secs,
            presence_debounce_secs: r.presence_debounce_secs,
            thermal_hot_pixel_threshold_c: r.thermal_hot_pixel_threshold_c,
            co2_baseline_ppm: r.co2_baseline_ppm,
            hybrid_confirm_window_secs: r.hybrid_confirm_window_secs,
            hybrid_min_words_for_sensor_split: r.hybrid_min_words_for_sensor_split,
            idle_encounter_timeout_secs: r.idle_encounter_timeout_secs,
            screen_capture_enabled: r.screen_capture_enabled,
            screen_capture_interval_secs: r.screen_capture_interval_secs,
            shadow_active_method: r.shadow_active_method.clone(),
            shadow_csv_log_enabled: r.shadow_csv_log_enabled,
            presence_csv_log_enabled: r.presence_csv_log_enabled,
            vad_threshold: r.vad_threshold,
            silence_to_flush_ms: r.silence_to_flush_ms,
            max_utterance_ms: r.max_utterance_ms,
            greeting_sensitivity: r.greeting_sensitivity,
            min_speech_duration_ms: r.min_speech_duration_ms,
            whisper_model: r.whisper_model.clone(),
            debug_storage_enabled: r.debug_storage_enabled,
            input_device_id: r.input_device_id.clone(),
        }
    }
}

pub struct ProfileClient {
    base_urls: Vec<String>,
    active_index: AtomicUsize,
    client: reqwest::Client,
    api_key: Option<String>,
}

impl Clone for ProfileClient {
    fn clone(&self) -> Self {
        Self {
            base_urls: self.base_urls.clone(),
            active_index: AtomicUsize::new(self.active_index.load(Ordering::Relaxed)),
            client: self.client.clone(),
            api_key: self.api_key.clone(),
        }
    }
}

impl ProfileClient {
    pub fn new(urls: &[String], api_key: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            base_urls: urls
                .iter()
                .map(|u| u.trim_end_matches('/').to_string())
                .collect(),
            active_index: AtomicUsize::new(0),
            client,
            api_key,
        }
    }

    /// Probe all URLs and switch to the first that responds to /health.
    /// Uses a short 2s timeout per URL so the total probe is fast.
    pub async fn select_best_url(&self) {
        if self.base_urls.len() <= 1 {
            return;
        }
        let probe = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap_or_default();

        for (i, url) in self.base_urls.iter().enumerate() {
            match probe.get(format!("{}/health", url)).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let prev = self.active_index.swap(i, Ordering::Relaxed);
                    if prev != i {
                        info!(
                            "Selected profile server: {} (was: {})",
                            url, self.base_urls[prev]
                        );
                    }
                    return;
                }
                Ok(resp) => warn!("Profile server {} returned {}", url, resp.status()),
                Err(e) => warn!("Profile server {} unreachable: {}", url, e),
            }
        }
        warn!("No profile server URL responded to health check");
    }

    /// Add the API key header to a request builder, if configured.
    fn with_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => builder.header("X-API-Key", key),
            None => builder,
        }
    }

    /// Return a `HeaderMap` containing the API key header (if configured).
    /// Useful for call sites that build requests manually via `http_client()`.
    pub fn auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(ref key) = self.api_key {
            if let Ok(val) = reqwest::header::HeaderValue::from_str(key) {
                headers.insert("X-API-Key", val);
            }
        }
        headers
    }

    /// Get the currently active base URL.
    pub fn base_url(&self) -> &str {
        let idx = self.active_index.load(Ordering::Relaxed);
        &self.base_urls[idx]
    }

    /// Get a reference to the underlying HTTP client
    pub fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    pub async fn health(&self) -> Result<bool> {
        let resp = self
            .with_auth(self.client.get(format!("{}/health", self.base_url())))
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    // ── Infrastructure settings ─────────────────────────────────────

    pub async fn get_infrastructure(&self) -> Result<crate::config::InfrastructureOverlay> {
        let resp = self
            .with_auth(self.client.get(format!("{}/infrastructure", self.base_url())))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Get infrastructure failed: {} - {}", status, &text[..text.len().min(200)]);
        }
        let infra: crate::config::InfrastructureOverlay = resp.json().await?;
        Ok(infra)
    }

    pub async fn update_infrastructure(
        &self,
        settings: &crate::config::InfrastructureOverlay,
    ) -> Result<crate::config::InfrastructureOverlay> {
        let resp = self
            .with_auth(self.client.put(format!("{}/infrastructure", self.base_url())))
            .json(settings)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Update infrastructure failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        let infra: crate::config::InfrastructureOverlay = resp.json().await?;
        Ok(infra)
    }

    pub async fn get_room(&self, room_id: &str) -> Result<Room> {
        let resp = self
            .with_auth(self.client.get(format!("{}/rooms/{}", self.base_url(), room_id)))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Get room failed: {} - {}", status, &text[..text.len().min(200)]);
        }
        let room: Room = resp.json().await?;
        Ok(room)
    }

    /// Fetch infrastructure + room settings from server and merge into local config.
    /// Returns true if any settings were merged.
    pub async fn merge_server_settings(&self, room_id: Option<&str>) -> Result<bool> {
        let mut config = crate::config::Config::load_or_default();
        let mut merged = false;

        // Run both fetches concurrently
        let (infra_result, room_result) = tokio::join!(
            self.get_infrastructure(),
            async {
                match room_id {
                    Some(rid) => Some(self.get_room(rid).await),
                    None => None,
                }
            }
        );

        if let Ok(infra) = infra_result {
            config.settings.apply_infrastructure(&infra);
            merged = true;
        }

        if let Some(Ok(room)) = room_result {
            let room_overlay = crate::config::RoomOverlay::from(&room);
            config.settings.apply_room(&room_overlay);
            merged = true;
        }

        if merged {
            config.save().map_err(|e| anyhow::anyhow!("Failed to save merged config: {e}"))?;
        }

        Ok(merged)
    }

    // ── Physicians ───────────────────────────────────────────────────

    pub async fn list_physicians(&self) -> Result<Vec<PhysicianProfile>> {
        let resp = self
            .with_auth(self.client.get(format!("{}/physicians", self.base_url())))
            .send()
            .await?;
        let profiles: Vec<PhysicianProfile> = resp.json().await?;
        Ok(profiles)
    }

    pub async fn get_physician(&self, id: &str) -> Result<PhysicianProfile> {
        let resp = self
            .with_auth(self.client.get(format!("{}/physicians/{}", self.base_url(), id)))
            .send()
            .await?;
        let profile: PhysicianProfile = resp.json().await?;
        Ok(profile)
    }

    // Session upload methods (for server sync)
    pub async fn upload_session(
        &self,
        physician_id: &str,
        session_id: &str,
        body: &serde_json::Value,
    ) -> Result<()> {
        let url = format!(
            "{}/physicians/{}/sessions/{}",
            self.base_url(), physician_id, session_id
        );
        let resp = self.with_auth(self.client.post(&url)).json(body).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Upload session failed: {} - {}",
                status,
                &body[..body.len().min(200)]
            );
        }
        Ok(())
    }

    pub async fn update_soap(
        &self,
        physician_id: &str,
        session_id: &str,
        body: &serde_json::Value,
    ) -> Result<()> {
        let url = format!(
            "{}/physicians/{}/sessions/{}/soap",
            self.base_url(), physician_id, session_id
        );
        let resp = self.with_auth(self.client.put(&url)).json(body).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Update SOAP failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        Ok(())
    }

    pub async fn update_metadata(
        &self,
        physician_id: &str,
        session_id: &str,
        metadata: &serde_json::Value,
    ) -> Result<()> {
        let url = format!(
            "{}/physicians/{}/sessions/{}/metadata",
            self.base_url(), physician_id, session_id
        );
        let resp = self.with_auth(self.client.put(&url)).json(metadata).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Update metadata failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        Ok(())
    }

    pub async fn delete_session(&self, physician_id: &str, session_id: &str) -> Result<()> {
        let url = format!(
            "{}/physicians/{}/sessions/{}",
            self.base_url(), physician_id, session_id
        );
        let resp = self.with_auth(self.client.delete(&url)).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Delete session failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        Ok(())
    }

    pub async fn upload_audio(
        &self,
        physician_id: &str,
        session_id: &str,
        audio_path: &std::path::Path,
    ) -> Result<()> {
        let url = format!(
            "{}/physicians/{}/sessions/{}/audio",
            self.base_url(), physician_id, session_id
        );
        let file_bytes = tokio::fs::read(audio_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read audio file: {e}"))?;
        let part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;
        let form = reqwest::multipart::Form::new().part("audio", part);

        // Use a longer timeout for large audio files
        let resp = self
            .with_auth(self.client.post(&url))
            .multipart(form)
            .timeout(std::time::Duration::from_secs(300))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Upload audio failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        Ok(())
    }

    pub async fn get_session_dates(
        &self,
        physician_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut url = format!(
            "{}/physicians/{}/sessions/dates",
            self.base_url(), physician_id
        );
        let mut params = vec![];
        if let Some(f) = from {
            params.push(format!("from={f}"));
        }
        if let Some(t) = to {
            params.push(format!("to={t}"));
        }
        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }
        let resp = self.with_auth(self.client.get(&url)).send().await?;
        let dates: Vec<String> = resp.json().await?;
        Ok(dates)
    }

    pub async fn get_sessions_by_date(
        &self,
        physician_id: &str,
        date: &str,
    ) -> Result<Vec<crate::local_archive::ArchiveSummary>> {
        let url = format!(
            "{}/physicians/{}/sessions?date={}",
            self.base_url(), physician_id, date
        );
        let resp = self.with_auth(self.client.get(&url)).send().await?;
        let sessions: Vec<crate::local_archive::ArchiveSummary> = resp.json().await?;
        Ok(sessions)
    }

    pub async fn get_session(
        &self,
        physician_id: &str,
        session_id: &str,
    ) -> Result<crate::local_archive::ArchiveDetails> {
        let url = format!(
            "{}/physicians/{}/sessions/{}",
            self.base_url(), physician_id, session_id
        );
        let resp = self.with_auth(self.client.get(&url)).send().await?;
        let details: crate::local_archive::ArchiveDetails = resp.json().await?;
        Ok(details)
    }

    pub async fn list_speakers(&self) -> Result<Vec<SpeakerProfile>> {
        let resp = self
            .with_auth(self.client.get(format!("{}/speakers", self.base_url())))
            .send()
            .await?;
        let profiles: Vec<SpeakerProfile> = resp.json().await?;
        Ok(profiles)
    }

    pub async fn upload_speaker(&self, speaker: &serde_json::Value) -> Result<()> {
        let resp = self
            .with_auth(self.client.post(format!("{}/speakers", self.base_url())))
            .json(speaker)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Upload speaker failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        Ok(())
    }

    pub async fn delete_speaker(&self, speaker_id: &str) -> Result<()> {
        let resp = self
            .with_auth(self.client.delete(format!("{}/speakers/{}", self.base_url(), speaker_id)))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Delete speaker failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        Ok(())
    }

    pub async fn list_rooms(&self) -> Result<Vec<Room>> {
        let resp = self
            .with_auth(self.client.get(format!("{}/rooms", self.base_url())))
            .send()
            .await?;
        let rooms: Vec<Room> = resp.json().await?;
        Ok(rooms)
    }

    pub async fn create_room(&self, name: &str, description: Option<&str>) -> Result<Room> {
        let mut body = serde_json::json!({ "name": name });
        if let Some(desc) = description {
            body["description"] = serde_json::Value::String(desc.to_string());
        }
        let resp = self
            .with_auth(self.client.post(format!("{}/rooms", self.base_url())))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Create room failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        let room: Room = resp.json().await?;
        Ok(room)
    }

    pub async fn update_room(&self, room_id: &str, updates: &serde_json::Value) -> Result<Room> {
        let resp = self
            .with_auth(self.client.put(format!("{}/rooms/{}", self.base_url(), room_id)))
            .json(updates)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Update room failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        let room: Room = resp.json().await?;
        Ok(room)
    }

    pub async fn delete_room(&self, room_id: &str) -> Result<()> {
        let resp = self
            .with_auth(self.client.delete(format!("{}/rooms/{}", self.base_url(), room_id)))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Delete room failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        Ok(())
    }

    /// Upload an auxiliary session file (pipeline_log.jsonl, replay_bundle.json, segments.jsonl)
    pub async fn upload_session_file(
        &self,
        physician_id: &str,
        session_id: &str,
        filename: &str,
        data: Vec<u8>,
    ) -> Result<()> {
        let url = format!(
            "{}/physicians/{}/sessions/{}/files/{}",
            self.base_url(), physician_id, session_id, filename
        );
        let resp = self.with_auth(self.client.put(&url)).body(data).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Upload session file failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        Ok(())
    }

    /// Upload the day log for a date
    pub async fn upload_day_log(
        &self,
        physician_id: &str,
        date: &str,
        data: Vec<u8>,
    ) -> Result<()> {
        let url = format!(
            "{}/physicians/{}/day-log/{}",
            self.base_url(), physician_id, date
        );
        let resp = self.with_auth(self.client.put(&url)).body(data).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Upload day log failed: {} - {}",
                status,
                &text[..text.len().min(200)]
            );
        }
        Ok(())
    }
}

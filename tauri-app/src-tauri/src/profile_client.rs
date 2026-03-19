use anyhow::Result;
use serde::{Deserialize, Serialize};

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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct ProfileClient {
    base_url: String,
    client: reqwest::Client,
}

impl ProfileClient {
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    /// Get the base URL for constructing custom API calls
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get a reference to the underlying HTTP client
    pub fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    pub async fn health(&self) -> Result<bool> {
        let resp = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    pub async fn list_physicians(&self) -> Result<Vec<PhysicianProfile>> {
        let resp = self
            .client
            .get(format!("{}/physicians", self.base_url))
            .send()
            .await?;
        let profiles: Vec<PhysicianProfile> = resp.json().await?;
        Ok(profiles)
    }

    pub async fn get_physician(&self, id: &str) -> Result<PhysicianProfile> {
        let resp = self
            .client
            .get(format!("{}/physicians/{}", self.base_url, id))
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
            self.base_url, physician_id, session_id
        );
        let resp = self.client.post(&url).json(body).send().await?;
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
            self.base_url, physician_id, session_id
        );
        let resp = self.client.put(&url).json(body).send().await?;
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
            self.base_url, physician_id, session_id
        );
        let resp = self.client.put(&url).json(metadata).send().await?;
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
            self.base_url, physician_id, session_id
        );
        let resp = self.client.delete(&url).send().await?;
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
            self.base_url, physician_id, session_id
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
            .client
            .post(&url)
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
            self.base_url, physician_id
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
        let resp = self.client.get(&url).send().await?;
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
            self.base_url, physician_id, date
        );
        let resp = self.client.get(&url).send().await?;
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
            self.base_url, physician_id, session_id
        );
        let resp = self.client.get(&url).send().await?;
        let details: crate::local_archive::ArchiveDetails = resp.json().await?;
        Ok(details)
    }

    pub async fn list_speakers(&self) -> Result<Vec<SpeakerProfile>> {
        let resp = self
            .client
            .get(format!("{}/speakers", self.base_url))
            .send()
            .await?;
        let profiles: Vec<SpeakerProfile> = resp.json().await?;
        Ok(profiles)
    }

    pub async fn upload_speaker(&self, speaker: &serde_json::Value) -> Result<()> {
        let resp = self
            .client
            .post(format!("{}/speakers", self.base_url))
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
            .client
            .delete(format!("{}/speakers/{}", self.base_url, speaker_id))
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
            .client
            .get(format!("{}/rooms", self.base_url))
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
            .client
            .post(format!("{}/rooms", self.base_url))
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
            .client
            .put(format!("{}/rooms/{}", self.base_url, room_id))
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
            .client
            .delete(format!("{}/rooms/{}", self.base_url, room_id))
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
            self.base_url, physician_id, session_id, filename
        );
        let resp = self.client.put(&url).body(data).send().await?;
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
            self.base_url, physician_id, date
        );
        let resp = self.client.put(&url).body(data).send().await?;
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

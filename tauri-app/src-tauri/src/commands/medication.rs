//! Medication-related Tauri commands.
//!
//! Two surfaces:
//!  - `capture_screenshot_for_meds` — one-shot screenshot + vision LLM that
//!    extracts the patient's current medication list. Mirrors the
//!    `screenshot_task.rs` patient-name pipeline but parses arrays of
//!    `{name, dose, frequency}` instead of a single `{name, dob}`.
//!  - `analyze_medications` / `generate_medication_plan` — thin reqwest
//!    HTTP wrappers around the MacBook-hosted pharmacotherapy-refactorer
//!    service (`POST {pharm_service_url}/analyze` etc).
//!
//! Trust boundary is unchanged from the rest of the vision pipeline:
//! vision-derived output is clinician-reviewed before any action.

use super::{physicians::SharedServerConfig, CommandError};
use crate::config::Config;
use crate::llm_client::{truncate_error_body, ContentPart, ImageUrlContent, LLMClient};
use crate::medication_extraction::{
    build_medication_extraction_prompt, medications_to_text, parse_medication_vision_response,
    MedEntry, MedExtractionResult,
};
use crate::screenshot;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tauri::State;
use tracing::{error, info, warn};

const VISION_TIMEOUT_SECS: u64 = 30;
const PHARM_SERVICE_TIMEOUT_SECS: u64 = 30;
const SCREENSHOT_MAX_EDGE: u32 = 1280;

// ── capture_screenshot_for_meds ───────────────────────────────────────

/// Capture one screenshot and run it through the vision LLM to extract a
/// medication list. Fail-soft: vision errors / timeouts return an empty
/// list rather than propagating an error, so the UI can fall through to
/// manual entry without an error toast.
#[tauri::command]
pub async fn capture_screenshot_for_meds(
    server_config: State<'_, SharedServerConfig>,
) -> Result<MedExtractionResult, CommandError> {
    let config = Config::load_or_default();

    // capture_to_base64 is sync + CPU-bound; spawn_blocking keeps the tokio runtime free.
    let capture =
        tokio::task::spawn_blocking(move || screenshot::capture_to_base64(SCREENSHOT_MAX_EDGE))
            .await
            .map_err(|e| CommandError::Other(format!("screenshot task join failed: {}", e)))?
            .map_err(CommandError::Other)?;

    if capture.likely_blank {
        warn!("Medication screenshot likely blank — probably no Screen Recording permission");
        return Ok(MedExtractionResult {
            medications: Vec::new(),
            likely_blank: true,
        });
    }

    // Clone prompt templates out of the lock before drop so we can use them
    // for the vision call without holding the read guard across .await.
    let templates = {
        let sc = server_config.read().await;
        sc.prompts.clone()
    };

    let client = LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &config.fast_model,
    )
    .map_err(CommandError::Network)?;

    let (system_prompt, user_text) = build_medication_extraction_prompt(Some(&templates));
    let content_parts = vec![
        ContentPart::Text { text: user_text },
        ContentPart::ImageUrl {
            image_url: ImageUrlContent {
                url: format!("data:image/jpeg;base64,{}", capture.base64),
            },
        },
    ];

    let vision_future = client.generate_vision_timed(
        "vision-model",
        &system_prompt,
        content_parts,
        "med_list_extraction",
        Some(0.1),
        Some(500),
        None,
        None,
    );

    let response = match tokio::time::timeout(
        Duration::from_secs(VISION_TIMEOUT_SECS),
        vision_future,
    )
    .await
    {
        Ok((Ok(resp), _metrics)) => resp,
        Ok((Err(e), _)) => {
            error!("Vision call for med extraction failed: {}", e);
            return Ok(MedExtractionResult {
                medications: Vec::new(),
                likely_blank: false,
            });
        }
        Err(_) => {
            warn!(
                "Vision call for med extraction timed out after {}s",
                VISION_TIMEOUT_SECS
            );
            return Ok(MedExtractionResult {
                medications: Vec::new(),
                likely_blank: false,
            });
        }
    };

    let medications = parse_medication_vision_response(&response);
    info!(
        "Medication extraction returned {} medications",
        medications.len()
    );

    Ok(MedExtractionResult {
        medications,
        likely_blank: false,
    })
}

// ── analyze_medications ──────────────────────────────────────────────

/// Shape returned by pharm-refactor's `MedicationResponse` (api/server.py).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisMedication {
    #[serde(default)]
    pub raw_text: String,
    #[serde(default)]
    pub name_canonical: String,
    #[serde(default)]
    pub dose_value: Option<f64>,
    #[serde(default)]
    pub dose_unit: Option<String>,
    #[serde(default)]
    pub frequency: String,
    #[serde(default)]
    pub formulation: String,
    #[serde(default)]
    pub dose_band: String,
    #[serde(default)]
    pub is_combo: bool,
    #[serde(default)]
    pub combo_components: Vec<String>,
}

/// Shape returned by pharm-refactor's `CardResponse` (api/server.py).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisCard {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub rationale: Option<String>,
    pub category: String,
    pub severity: String,
    pub confidence: String,
    #[serde(default)]
    pub meds_involved: Vec<String>,
    #[serde(default)]
    pub verify_checklist: Vec<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BurdenScores {
    #[serde(default)]
    pub acb_total: f64,
    #[serde(default)]
    pub sedation_total: f64,
    #[serde(default)]
    pub constipation_total: f64,
    #[serde(default)]
    pub qt_risk_count: u32,
    #[serde(default)]
    pub serotonergic_count: u32,
    #[serde(default)]
    pub bleeding_risk_count: u32,
    #[serde(default)]
    pub falls_risk_count: u32,
    #[serde(default)]
    pub nephrotoxic_count: u32,
    #[serde(default)]
    pub hepatotoxic_count: u32,
    #[serde(default)]
    pub hyperkalemia_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub medications: Vec<AnalysisMedication>,
    pub cards: Vec<AnalysisCard>,
    pub burden_scores: BurdenScores,
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,
}

/// Request body matching pharm-refactor's `AnalyzeRequest`.
#[derive(Debug, Clone, Serialize)]
struct AnalyzeRequestBody<'a> {
    medications: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<&'a HashMap<String, bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    patient_age: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    patient_egfr: Option<f64>,
    strategy: &'a str,
}

/// Analyze a medication list via the MacBook pharm-refactor service.
///
/// Frontend passes the service URL explicitly (resolved through the same
/// fallback pattern as `profile_server_url`). Empty medication list is a
/// validation error; HTTP errors from the service are surfaced as
/// `CommandError::Network` with the response body truncated to 200 chars
/// (same PHI-safety pattern as `truncate_error_body` in llm_client.rs).
#[tauri::command]
pub async fn analyze_medications(
    pharm_service_url: String,
    medications: Vec<MedEntry>,
    patient_age: Option<i32>,
    patient_egfr: Option<f64>,
    context: Option<HashMap<String, bool>>,
) -> Result<AnalysisResult, CommandError> {
    if medications.is_empty() {
        return Err(CommandError::Validation("Empty medication list".into()));
    }
    if pharm_service_url.trim().is_empty() {
        return Err(CommandError::Config(
            "Pharmacotherapy service URL is not configured".into(),
        ));
    }

    let med_text = medications_to_text(&medications);
    let url = format!("{}/analyze", pharm_service_url.trim_end_matches('/'));

    let client = HttpClient::builder()
        .timeout(Duration::from_secs(PHARM_SERVICE_TIMEOUT_SECS))
        .build()
        .map_err(|e| CommandError::Network(format!("HTTP client init failed: {}", e)))?;

    let body = AnalyzeRequestBody {
        medications: &med_text,
        context: context.as_ref(),
        patient_age,
        patient_egfr,
        strategy: "safety_first",
    };

    info!(
        "POST {} — {} medications",
        url,
        medications.len()
    );

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| CommandError::Network(format!("Pharm service unreachable: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(CommandError::Network(format!(
            "Pharm service returned {}: {}",
            status,
            truncate_error_body(&body, 200)
        )));
    }

    let analysis: AnalysisResult = response
        .json()
        .await
        .map_err(|e| CommandError::Network(format!("Pharm service response parse failed: {}", e)))?;

    info!(
        "Analysis returned {} cards (burden ACB={:.1}, sedation={:.1})",
        analysis.cards.len(),
        analysis.burden_scores.acb_total,
        analysis.burden_scores.sedation_total
    );
    Ok(analysis)
}


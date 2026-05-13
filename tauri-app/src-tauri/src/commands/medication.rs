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

use super::ollama::load_effective_models_and_client;
use super::{physicians::SharedServerConfig, CommandError};
use crate::llm_client::{tasks, truncate_error_body, ContentPart, ImageUrlContent, LLMClient};
use crate::medication_extraction::{
    build_medication_extraction_prompt, build_medication_text_parse_prompt, medications_to_text,
    parse_medication_vision_response, MedEntry, MedExtractionResult,
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
const TEXT_PARSE_TIMEOUT_SECS: u64 = 15;
/// 2048 is the OCR threshold for the EMR's small meds-panel font in our
/// testing; 1280 silently returns []. 2560 used on retry — above that the
/// model starts returning [] on dense charts.
const SCREENSHOT_MAX_EDGE_PRIMARY: u32 = 2048;
const SCREENSHOT_MAX_EDGE_RETRY: u32 = 2560;
const MED_EXTRACTION_MAX_TOKENS: u32 = 2000;
/// Reject pathologically large inputs before paying for an LLM round-trip.
const MAX_PARSE_TEXT_LEN: usize = 8_000;
const MAX_CURRENT_MEDS: usize = 200;

// ── capture_screenshot_for_meds ───────────────────────────────────────

/// One vision-extraction attempt at `max_edge`. Returns `(meds, likely_blank)`.
/// `likely_blank` short-circuits the caller — no point retrying a screenshot
/// the OS blanked out due to missing Screen Recording permission.
///
/// Fail-soft: vision errors and timeouts return `(Vec::new(), false)` rather
/// than propagating, so the caller can decide whether to retry or surface
/// "no meds found" to the user.
async fn try_extract_meds(
    max_edge: u32,
    client: &LLMClient,
    templates: &crate::server_config::PromptTemplates,
) -> Result<(Vec<MedEntry>, bool), CommandError> {
    // Full-screen capture, NOT window-under-cursor. The clinician triggers
    // this from the AMI Assist sidebar — at click time the cursor is on the
    // "Clinical Assistant" button, so the window-under-cursor path captured
    // the sidebar (no meds visible) and returned empty regardless of
    // resolution. See activity.log on v0.10.83 — both 2048 and 2560
    // retries returned 0 medications because the source image was wrong.
    let capture =
        tokio::task::spawn_blocking(move || screenshot::capture_full_screen_to_base64(max_edge))
            .await
            .map_err(|e| CommandError::Other(format!("screenshot task join failed: {}", e)))?
            .map_err(CommandError::Other)?;

    if capture.likely_blank {
        return Ok((Vec::new(), true));
    }

    let (system_prompt, user_text) = build_medication_extraction_prompt(Some(templates));
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
        Some(MED_EXTRACTION_MAX_TOKENS),
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
            error!("Vision call for med extraction failed at {}px: {}", max_edge, e);
            return Ok((Vec::new(), false));
        }
        Err(_) => {
            warn!(
                "Vision call for med extraction timed out at {}px after {}s",
                max_edge, VISION_TIMEOUT_SECS
            );
            return Ok((Vec::new(), false));
        }
    };

    Ok((parse_medication_vision_response(&response), false))
}

/// Capture one screenshot and run it through the vision LLM to extract a
/// medication list. Tries `SCREENSHOT_MAX_EDGE_PRIMARY` first; on an empty
/// non-blank result, retakes the screenshot at `SCREENSHOT_MAX_EDGE_RETRY`
/// and tries again. Borderline charts intermittently return [] at the
/// primary resolution even when meds are clearly visible; the retry covers
/// that case at the cost of an extra vision call (~15-20s) only when the
/// first attempt found nothing.
#[tauri::command]
pub async fn capture_screenshot_for_meds(
    server_config: State<'_, SharedServerConfig>,
) -> Result<MedExtractionResult, CommandError> {
    let (_config, _models, client, templates) = load_effective_models_and_client(server_config.inner()).await?;

    let (meds, likely_blank) =
        try_extract_meds(SCREENSHOT_MAX_EDGE_PRIMARY, &client, &templates).await?;

    if likely_blank {
        warn!("Medication screenshot likely blank — probably no Screen Recording permission");
        return Ok(MedExtractionResult {
            medications: Vec::new(),
            likely_blank: true,
        });
    }

    if !meds.is_empty() {
        info!(
            "Medication extraction returned {} medications at {}px",
            meds.len(),
            SCREENSHOT_MAX_EDGE_PRIMARY
        );
        return Ok(MedExtractionResult {
            medications: meds,
            likely_blank: false,
        });
    }

    info!(
        "Medication extraction empty at {}px; retrying at {}px",
        SCREENSHOT_MAX_EDGE_PRIMARY, SCREENSHOT_MAX_EDGE_RETRY
    );
    let (retry_meds, retry_blank) =
        try_extract_meds(SCREENSHOT_MAX_EDGE_RETRY, &client, &templates).await?;

    if retry_blank {
        return Ok(MedExtractionResult {
            medications: Vec::new(),
            likely_blank: true,
        });
    }

    info!(
        "Medication extraction (retry) returned {} medications at {}px",
        retry_meds.len(),
        SCREENSHOT_MAX_EDGE_RETRY
    );
    Ok(MedExtractionResult {
        medications: retry_meds,
        likely_blank: false,
    })
}

// ── parse_medications_from_text ──────────────────────────────────────

/// Normalize a clinician's free-text medication input into structured
/// `MedEntry` JSON via an LLM call. The LLM is given the current list
/// plus the typed text so modifications ("stop X", "add Y") work
/// against state, not just fresh lists. Output shape matches the
/// vision path so `parse_medication_vision_response` parses both.
#[tauri::command]
pub async fn parse_medications_from_text(
    server_config: State<'_, SharedServerConfig>,
    text: String,
    current_medications: Vec<MedEntry>,
) -> Result<Vec<MedEntry>, CommandError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(CommandError::Validation("Medication text is empty".into()));
    }
    if trimmed.len() > MAX_PARSE_TEXT_LEN {
        return Err(CommandError::Validation(format!(
            "Medication text too long ({} > {} chars)",
            trimmed.len(),
            MAX_PARSE_TEXT_LEN
        )));
    }
    if current_medications.len() > MAX_CURRENT_MEDS {
        return Err(CommandError::Validation(format!(
            "Too many existing medications ({} > {})",
            current_medications.len(),
            MAX_CURRENT_MEDS
        )));
    }

    let (_config, models, client, templates) = load_effective_models_and_client(server_config.inner()).await?;

    let (system_prompt, user_prompt) =
        build_medication_text_parse_prompt(&current_medications, trimmed, Some(&templates));

    let response = tokio::time::timeout(
        Duration::from_secs(TEXT_PARSE_TIMEOUT_SECS),
        client.generate(
            &models.fast_model,
            &system_prompt,
            &user_prompt,
            tasks::MED_LIST_TEXT_PARSE,
        ),
    )
    .await
    .map_err(|_| {
        warn!(
            "Med-text LLM parse timed out after {}s",
            TEXT_PARSE_TIMEOUT_SECS
        );
        CommandError::Network(format!("Parsing timed out after {}s", TEXT_PARSE_TIMEOUT_SECS))
    })?
    .map_err(|e| {
        error!("Med-text LLM parse failed: {}", e);
        CommandError::Network(format!("Couldn't parse medication list: {}", e))
    })?;

    let parsed = parse_medication_vision_response(&response);
    info!(
        "Med-text parse: input {} chars, current={} → output={} medications",
        trimmed.len(),
        current_medications.len(),
        parsed.len()
    );

    Ok(parsed)
}

// ── analyze_medications ──────────────────────────────────────────────

/// Shape returned by pharm-refactor's `MedicationResponse` (api/server.py).
///
/// The pharm service serializes in snake_case (FastAPI/pydantic default) while
/// we send this struct through to the JS frontend as camelCase. `rename_all`
/// handles the JS direction; per-field `alias` handles the pharm direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisMedication {
    #[serde(default, alias = "raw_text")]
    pub raw_text: String,
    #[serde(default, alias = "name_canonical")]
    pub name_canonical: String,
    #[serde(default, alias = "dose_value")]
    pub dose_value: Option<f64>,
    #[serde(default, alias = "dose_unit")]
    pub dose_unit: Option<String>,
    #[serde(default)]
    pub frequency: String,
    #[serde(default)]
    pub formulation: String,
    #[serde(default, alias = "dose_band")]
    pub dose_band: String,
    #[serde(default, alias = "is_combo")]
    pub is_combo: bool,
    #[serde(default, alias = "combo_components")]
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
    #[serde(default, alias = "meds_involved")]
    pub meds_involved: Vec<String>,
    #[serde(default, alias = "verify_checklist")]
    pub verify_checklist: Vec<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BurdenScores {
    #[serde(default, alias = "acb_total")]
    pub acb_total: f64,
    #[serde(default, alias = "sedation_total")]
    pub sedation_total: f64,
    #[serde(default, alias = "constipation_total")]
    pub constipation_total: f64,
    #[serde(default, alias = "qt_risk_count")]
    pub qt_risk_count: u32,
    #[serde(default, alias = "serotonergic_count")]
    pub serotonergic_count: u32,
    #[serde(default, alias = "bleeding_risk_count")]
    pub bleeding_risk_count: u32,
    #[serde(default, alias = "falls_risk_count")]
    pub falls_risk_count: u32,
    #[serde(default, alias = "nephrotoxic_count")]
    pub nephrotoxic_count: u32,
    #[serde(default, alias = "hepatotoxic_count")]
    pub hepatotoxic_count: u32,
    #[serde(default, alias = "hyperkalemia_count")]
    pub hyperkalemia_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub medications: Vec<AnalysisMedication>,
    pub cards: Vec<AnalysisCard>,
    #[serde(alias = "burden_scores")]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip the exact wire format pharm-refactor's FastAPI returns
    /// (snake_case keys, including the required `burden_scores`). Catches
    /// the v0.10.82 regression where `rename_all = "camelCase"` made serde
    /// look for `burdenScores` and fail with "missing field".
    #[test]
    fn deserializes_pharm_service_snake_case_response() {
        let body = r#"{
            "medications": [{
                "raw_text": "lipitor 40 od",
                "name_canonical": "atorvastatin",
                "dose_value": 40.0,
                "dose_unit": "mg",
                "frequency": "UNKNOWN",
                "formulation": "UNKNOWN",
                "dose_band": "UNKNOWN",
                "is_combo": false,
                "combo_components": []
            }],
            "cards": [{
                "id": "guideline_avoid_d3d8a891",
                "title": "Guideline: Consider avoiding atorvastatin",
                "rationale": "...",
                "category": "GuidelineDeviation",
                "severity": "critical",
                "confidence": "HIGH",
                "meds_involved": ["atorvastatin"],
                "verify_checklist": ["Confirm patient has COVID-19"],
                "action": null,
                "notes": null
            }],
            "burden_scores": {
                "acb_total": 0.0,
                "sedation_total": 0.5,
                "constipation_total": 0.0,
                "qt_risk_count": 0,
                "serotonergic_count": 0,
                "bleeding_risk_count": 0,
                "falls_risk_count": 0,
                "nephrotoxic_count": 0,
                "hepatotoxic_count": 0,
                "hyperkalemia_count": 0
            },
            "context": {}
        }"#;
        let analysis: AnalysisResult =
            serde_json::from_str(body).expect("pharm-service snake_case response must deserialize");
        assert_eq!(analysis.medications.len(), 1);
        let med = &analysis.medications[0];
        assert_eq!(med.raw_text, "lipitor 40 od");
        assert_eq!(med.name_canonical, "atorvastatin");
        assert_eq!(med.dose_value, Some(40.0));
        assert_eq!(med.dose_unit.as_deref(), Some("mg"));
        assert_eq!(analysis.cards.len(), 1);
        assert_eq!(analysis.cards[0].meds_involved, vec!["atorvastatin"]);
        assert_eq!(
            analysis.cards[0].verify_checklist,
            vec!["Confirm patient has COVID-19"]
        );
        assert_eq!(analysis.burden_scores.sedation_total, 0.5);
    }

    /// Same struct must still serialize as camelCase when we send it on
    /// to the JS frontend via Tauri IPC. JS reads e.g. `burdenScores` not
    /// `burden_scores`.
    #[test]
    fn serializes_to_camel_case_for_js() {
        let result = AnalysisResult {
            medications: vec![AnalysisMedication {
                raw_text: "lipitor".into(),
                name_canonical: "atorvastatin".into(),
                dose_value: Some(40.0),
                dose_unit: Some("mg".into()),
                frequency: String::new(),
                formulation: String::new(),
                dose_band: String::new(),
                is_combo: false,
                combo_components: vec![],
            }],
            cards: vec![],
            burden_scores: BurdenScores {
                acb_total: 1.5,
                ..Default::default()
            },
            context: HashMap::new(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"rawText\":\"lipitor\""));
        assert!(json.contains("\"nameCanonical\":\"atorvastatin\""));
        assert!(json.contains("\"burdenScores\":{"));
        assert!(json.contains("\"acbTotal\":1.5"));
        assert!(!json.contains("raw_text"));
        assert!(!json.contains("burden_scores"));
    }
}

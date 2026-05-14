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
    parse_medication_vision_response, parse_medication_vision_response_with_patient, MedEntry,
    MedExtractionResult, PatientIdentity,
};
use crate::screenshot;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tauri::State;
use tracing::{error, info, warn};

/// Bumped 30s → 180s in v0.10.94 to match the v0.10.74 precedent (encounter
/// detection, tools-model, multi-patient detect all sit at 180s). The 30s
/// ceiling started firing consistently on 2026-05-13 — patient-name vision
/// calls completed in 2–5s using the same `vision-model` alias, but the
/// med-extraction workload (2048–2560px full-screen + structured multi-med
/// JSON output) is materially heavier and exceeded the budget. Fail-soft
/// path silently turned timeouts into "0 meds found", so clinicians saw
/// "Vision returned no medications" instead of a real error.
const VISION_TIMEOUT_SECS: u64 = 180;

/// LLM router alias for the medication-extraction vision call. Hard-coded
/// (matches the rest of the Clinical Assistant surface in `clinical_chat.rs`
/// and `generate_clinical_feedback`). Server-configurable aliases like
/// `soap_model` / `fast_model` flow through `OperationalDefaults`;
/// `clinical-assistant` does not, by design — the router pins this one to a
/// vision-capable model with the right tool registry.
const MED_EXTRACTION_MODEL: &str = "clinical-assistant";
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
///
/// `vision_model` is the LLM router alias used for the call. Callers pass
/// [`MED_EXTRACTION_MODEL`] (`clinical-assistant`) — the parameter stays
/// `&str` so tests can substitute a different alias if needed.
async fn try_extract_meds(
    max_edge: u32,
    client: &LLMClient,
    templates: &crate::server_config::PromptTemplates,
    vision_model: &str,
) -> Result<(Vec<MedEntry>, Option<PatientIdentity>, bool), CommandError> {
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
        return Ok((Vec::new(), None, true));
    }

    let image_kb = capture.base64.len() / 1024;
    info!(
        "Med extraction starting: model={}, max_edge={}px, image_base64={}KB, max_tokens={}, timeout={}s",
        vision_model, max_edge, image_kb, MED_EXTRACTION_MAX_TOKENS, VISION_TIMEOUT_SECS
    );

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
        vision_model,
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
        Ok((Ok(resp), metrics)) => {
            info!(
                "Med extraction vision OK at {}px: wall_ms={} scheduling_ms={} network_ms={} concurrent_at_start={} retry_count={} response_chars={}",
                max_edge,
                metrics.wall_ms,
                metrics.scheduling_ms,
                metrics.network_ms,
                metrics.concurrent_at_start,
                metrics.retry_count,
                resp.len()
            );
            resp
        }
        Ok((Err(e), metrics)) => {
            error!(
                "Med extraction vision FAILED at {}px after wall_ms={} (scheduling_ms={} network_ms={} concurrent_at_start={} retry_count={} image={}KB): {}",
                max_edge,
                metrics.wall_ms,
                metrics.scheduling_ms,
                metrics.network_ms,
                metrics.concurrent_at_start,
                metrics.retry_count,
                image_kb,
                e
            );
            return Ok((Vec::new(), None, false));
        }
        Err(_) => {
            warn!(
                "Med extraction vision TIMED OUT at {}px after {}s (image={}KB, max_tokens={}) — vision-model latency exceeds client timeout",
                max_edge, VISION_TIMEOUT_SECS, image_kb, MED_EXTRACTION_MAX_TOKENS
            );
            return Ok((Vec::new(), None, false));
        }
    };

    let (meds, patient) = parse_medication_vision_response_with_patient(&response);
    info!(
        "Med extraction parse at {}px: response_chars={} → {} meds, patient_name={} patient_dob={} (empty result kind: {})",
        max_edge,
        response.len(),
        meds.len(),
        patient
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("<none>"),
        patient
            .as_ref()
            .and_then(|p| p.dob.as_deref())
            .unwrap_or("<none>"),
        if response.trim().is_empty() {
            "empty_response"
        } else if response.trim() == "[]" {
            "empty_array_from_llm"
        } else if meds.is_empty() {
            "parse_returned_zero"
        } else {
            "ok"
        }
    );

    Ok((meds, patient, false))
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
    // `models` is unused here — med extraction uses the hardcoded
    // `clinical-assistant` alias (MED_EXTRACTION_MODEL) rather than a
    // server-configurable one. `client` and `templates` are still needed.
    let (_config, _models, client, templates) =
        load_effective_models_and_client(server_config.inner()).await?;

    let (meds, patient, likely_blank) =
        try_extract_meds(SCREENSHOT_MAX_EDGE_PRIMARY, &client, &templates, MED_EXTRACTION_MODEL).await?;

    if likely_blank {
        warn!("Medication screenshot likely blank — probably no Screen Recording permission");
        return Ok(MedExtractionResult {
            medications: Vec::new(),
            likely_blank: true,
            patient: None,
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
            patient,
        });
    }

    info!(
        "Medication extraction empty at {}px; retrying at {}px",
        SCREENSHOT_MAX_EDGE_PRIMARY, SCREENSHOT_MAX_EDGE_RETRY
    );
    let (retry_meds, retry_patient, retry_blank) =
        try_extract_meds(SCREENSHOT_MAX_EDGE_RETRY, &client, &templates, MED_EXTRACTION_MODEL).await?;

    if retry_blank {
        return Ok(MedExtractionResult {
            medications: Vec::new(),
            likely_blank: true,
            patient: None,
        });
    }

    info!(
        "Medication extraction (retry) returned {} medications at {}px",
        retry_meds.len(),
        SCREENSHOT_MAX_EDGE_RETRY
    );
    // Prefer the primary-pass identity if the retry returned None — meds may
    // have been empty on the first pass, but the chart header identity is
    // unrelated to dense med-list OCR.
    Ok(MedExtractionResult {
        medications: retry_meds,
        likely_blank: false,
        patient: retry_patient.or(patient),
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
    #[serde(default, alias = "cns_depressant_count")]
    pub cns_depressant_count: u32,
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

/// Shared body for `/analyze`, `/plan/questions`, and `/plan`. `answers` is
/// dropped from the wire when `None` so the questions/analyze endpoints —
/// which don't define `answers` in their schema — never see the extra field.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    answers: Option<&'a HashMap<String, String>>,
}

// ── Plan flow response types ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClarifyingQuestion {
    pub id: String,
    pub question: String,
    /// `type` is reserved in Rust, so the field is `question_type` here.
    #[serde(rename = "type", alias = "type")]
    pub question_type: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionsResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub questions: Vec<ClarifyingQuestion>,
    #[serde(default, alias = "can_skip")]
    pub can_skip: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    #[serde(default, alias = "step_number")]
    pub step_number: u32,
    /// "stop" | "substitute" | "add" | "adjust"
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub drug: String,
    #[serde(default, alias = "new_drug")]
    pub new_drug: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default, alias = "meds_after")]
    pub meds_after: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIPlanResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default, alias = "current_meds")]
    pub current_meds: Option<Vec<String>>,
    #[serde(default)]
    pub steps: Option<Vec<PlanStep>>,
    #[serde(default, alias = "final_meds")]
    pub final_meds: Option<Vec<String>>,
    #[serde(default)]
    pub error: Option<String>,
}

/// POST a body to `{pharm_service_url}/{path}` and deserialize the response.
/// Centralizes URL trimming, client build, status handling, and PHI-safe
/// error-body truncation across all three pharm-service commands.
async fn pharm_service_post<T: serde::de::DeserializeOwned>(
    pharm_service_url: &str,
    path: &str,
    body: &AnalyzeRequestBody<'_>,
) -> Result<T, CommandError> {
    if pharm_service_url.trim().is_empty() {
        return Err(CommandError::Config(
            "Pharmacotherapy service URL is not configured".into(),
        ));
    }
    let url = format!(
        "{}/{}",
        pharm_service_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );

    let client = HttpClient::builder()
        .timeout(Duration::from_secs(PHARM_SERVICE_TIMEOUT_SECS))
        .build()
        .map_err(|e| CommandError::Network(format!("HTTP client init failed: {}", e)))?;

    info!("POST {}", url);

    let response = client
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| CommandError::Network(format!("Pharm service unreachable: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        let err_body = response.text().await.unwrap_or_default();
        return Err(CommandError::Network(format!(
            "Pharm service returned {}: {}",
            status,
            truncate_error_body(&err_body, 200)
        )));
    }

    response
        .json::<T>()
        .await
        .map_err(|e| CommandError::Network(format!("Pharm service response parse failed: {}", e)))
}

fn build_pharm_body<'a>(
    med_text: &'a str,
    context: Option<&'a HashMap<String, bool>>,
    patient_age: Option<i32>,
    patient_egfr: Option<f64>,
    strategy: Option<&'a str>,
    answers: Option<&'a HashMap<String, String>>,
) -> AnalyzeRequestBody<'a> {
    AnalyzeRequestBody {
        medications: med_text,
        context,
        patient_age,
        patient_egfr,
        strategy: strategy.filter(|s| !s.is_empty()).unwrap_or("safety_first"),
        answers,
    }
}

#[tauri::command]
pub async fn analyze_medications(
    pharm_service_url: String,
    medications: Vec<MedEntry>,
    patient_age: Option<i32>,
    patient_egfr: Option<f64>,
    context: Option<HashMap<String, bool>>,
    strategy: Option<String>,
) -> Result<AnalysisResult, CommandError> {
    if medications.is_empty() {
        return Err(CommandError::Validation("Empty medication list".into()));
    }
    let med_text = medications_to_text(&medications);
    let body = build_pharm_body(
        &med_text,
        context.as_ref(),
        patient_age,
        patient_egfr,
        strategy.as_deref(),
        None,
    );

    let analysis: AnalysisResult = pharm_service_post(&pharm_service_url, "analyze", &body).await?;

    info!(
        "Analysis: {} meds → {} cards (burden ACB={:.1}, sedation={:.1})",
        medications.len(),
        analysis.cards.len(),
        analysis.burden_scores.acb_total,
        analysis.burden_scores.sedation_total
    );
    Ok(analysis)
}

#[tauri::command]
pub async fn get_plan_clarifying_questions(
    pharm_service_url: String,
    medications: Vec<MedEntry>,
    patient_age: Option<i32>,
    patient_egfr: Option<f64>,
    context: Option<HashMap<String, bool>>,
    strategy: Option<String>,
) -> Result<QuestionsResponse, CommandError> {
    if medications.is_empty() {
        return Err(CommandError::Validation("Empty medication list".into()));
    }
    let med_text = medications_to_text(&medications);
    let body = build_pharm_body(
        &med_text,
        context.as_ref(),
        patient_age,
        patient_egfr,
        strategy.as_deref(),
        None,
    );

    let questions: QuestionsResponse =
        pharm_service_post(&pharm_service_url, "plan/questions", &body).await?;

    info!(
        "Plan questions: {} returned (success={}, can_skip={})",
        questions.questions.len(),
        questions.success,
        questions.can_skip
    );
    Ok(questions)
}

#[tauri::command]
pub async fn generate_plan_with_answers(
    pharm_service_url: String,
    medications: Vec<MedEntry>,
    patient_age: Option<i32>,
    patient_egfr: Option<f64>,
    context: Option<HashMap<String, bool>>,
    strategy: Option<String>,
    answers: Option<HashMap<String, String>>,
) -> Result<AIPlanResponse, CommandError> {
    if medications.is_empty() {
        return Err(CommandError::Validation("Empty medication list".into()));
    }
    let med_text = medications_to_text(&medications);
    let body = build_pharm_body(
        &med_text,
        context.as_ref(),
        patient_age,
        patient_egfr,
        strategy.as_deref(),
        answers.as_ref(),
    );

    let plan: AIPlanResponse = pharm_service_post(&pharm_service_url, "plan", &body).await?;

    info!(
        "Plan: success={}, {} steps, {} final meds, {} answers in",
        plan.success,
        plan.steps.as_ref().map(|s| s.len()).unwrap_or(0),
        plan.final_meds.as_ref().map(|f| f.len()).unwrap_or(0),
        answers.as_ref().map(|a| a.len()).unwrap_or(0),
    );
    Ok(plan)
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
        assert!(json.contains("\"cnsDepressantCount\":0"));
        assert!(!json.contains("raw_text"));
        assert!(!json.contains("burden_scores"));
        assert!(!json.contains("cns_depressant_count"));
    }

    /// `/analyze` responses from older pharm-refactor builds may omit
    /// `cns_depressant_count`. With `#[serde(default)]` the field should
    /// fall back to 0 rather than failing the whole deserialization.
    #[test]
    fn burden_scores_back_compat_without_cns_depressant_count() {
        let body = r#"{
            "acb_total": 1.0,
            "sedation_total": 0.5,
            "constipation_total": 0.0,
            "qt_risk_count": 0,
            "serotonergic_count": 0,
            "bleeding_risk_count": 0,
            "falls_risk_count": 0,
            "nephrotoxic_count": 0,
            "hepatotoxic_count": 0,
            "hyperkalemia_count": 0
        }"#;
        let scores: BurdenScores =
            serde_json::from_str(body).expect("old response without cns_depressant_count must deserialize");
        assert_eq!(scores.cns_depressant_count, 0);
        assert_eq!(scores.acb_total, 1.0);
    }

    #[test]
    fn deserializes_plan_questions_response() {
        let body = r#"{
            "success": true,
            "questions": [
                {
                    "id": "q_sedation",
                    "question": "Is the patient currently sedated?",
                    "type": "boolean",
                    "options": ["Yes", "No"],
                    "context": "Affects opioid recommendations"
                },
                {
                    "id": "q_falls",
                    "question": "Recent falls?",
                    "type": "choice",
                    "options": ["None", "1 fall", "Multiple"]
                }
            ],
            "can_skip": true
        }"#;
        let parsed: QuestionsResponse =
            serde_json::from_str(body).expect("plan questions response must deserialize");
        assert!(parsed.success);
        assert!(parsed.can_skip);
        assert_eq!(parsed.questions.len(), 2);
        assert_eq!(parsed.questions[0].id, "q_sedation");
        assert_eq!(parsed.questions[0].question_type, "boolean");
        assert_eq!(parsed.questions[0].options, vec!["Yes", "No"]);
        assert_eq!(parsed.questions[1].question_type, "choice");
        assert_eq!(parsed.questions[1].options.len(), 3);
    }

    /// `type` (not `questionType`) must appear in serialized JSON — the
    /// frontend reads `q.type`, not the Rust field name.
    #[test]
    fn serializes_clarifying_question_with_type_field() {
        let q = ClarifyingQuestion {
            id: "q1".into(),
            question: "?".into(),
            question_type: "boolean".into(),
            options: vec!["Yes".into(), "No".into()],
            context: None,
        };
        let json = serde_json::to_string(&q).unwrap();
        assert!(json.contains("\"type\":\"boolean\""), "got: {}", json);
        assert!(!json.contains("questionType"));
        assert!(!json.contains("question_type"));
    }

    #[test]
    fn deserializes_ai_plan_response() {
        let body = r#"{
            "success": true,
            "summary": "Stop overlapping CNS depressants.",
            "current_meds": ["alprazolam 1mg", "zopiclone 7.5mg"],
            "steps": [
                {
                    "step_number": 1,
                    "action": "stop",
                    "drug": "zopiclone 7.5mg",
                    "new_drug": "",
                    "reason": "Combined with alprazolam → falls risk.",
                    "meds_after": ["alprazolam 1mg"]
                }
            ],
            "final_meds": ["alprazolam 1mg"]
        }"#;
        let parsed: AIPlanResponse =
            serde_json::from_str(body).expect("plan response must deserialize");
        assert!(parsed.success);
        assert_eq!(parsed.summary.as_deref(), Some("Stop overlapping CNS depressants."));
        let steps = parsed.steps.as_ref().expect("steps present");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].step_number, 1);
        assert_eq!(steps[0].action, "stop");
        assert_eq!(steps[0].meds_after, vec!["alprazolam 1mg"]);
        assert_eq!(parsed.final_meds.as_ref().map(|f| f.len()), Some(1));
    }

    #[test]
    fn deserializes_ai_plan_error_envelope() {
        let body = r#"{
            "success": false,
            "error": "No findings to create a plan from."
        }"#;
        let parsed: AIPlanResponse = serde_json::from_str(body).expect("must deserialize");
        assert!(!parsed.success);
        assert!(parsed.steps.is_none());
        assert_eq!(parsed.error.as_deref(), Some("No findings to create a plan from."));
    }

    /// `/plan/questions` and `/analyze` schemas have no `answers` field —
    /// sending it would risk strict-schema rejection on a future pydantic
    /// `extra='forbid'` config flip.
    #[test]
    fn analyze_request_body_omits_none_answers() {
        let body = AnalyzeRequestBody {
            medications: "metformin",
            context: None,
            patient_age: None,
            patient_egfr: None,
            strategy: "safety_first",
            answers: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(!json.contains("answers"), "answers must be omitted when None, got: {}", json);
    }

    #[test]
    fn analyze_request_body_includes_answers_when_present() {
        let mut answers = HashMap::new();
        answers.insert("q_sedation".to_string(), "Yes".to_string());
        let body = AnalyzeRequestBody {
            medications: "metformin",
            context: None,
            patient_age: Some(78),
            patient_egfr: Some(45.0),
            strategy: "safety_first",
            answers: Some(&answers),
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"answers\""), "got: {}", json);
        assert!(json.contains("\"q_sedation\":\"Yes\""), "got: {}", json);
        assert!(json.contains("\"patient_age\":78"));
        assert!(json.contains("\"patient_egfr\":45.0"));
    }
}

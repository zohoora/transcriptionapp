//! Vision SOAP Prompt Experimentation Framework
//!
//! This module provides tools to test different prompt strategies for vision SOAP
//! generation, specifically to find the optimal approach for using EHR screenshots
//! where the model should only extract details indirectly referenced in the transcript.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

use crate::llm_client::{ContentPart, ImageUrlContent, LLMClient};

/// Prompt strategy identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptStrategy {
    /// P1: Negative framing - explicitly list what NOT to include
    NegativeFraming,
    /// P2: Flip the default - "Ignore image EXCEPT for..."
    FlipDefault,
    /// P3: Two-step reasoning - identify references first, then extract
    TwoStepReasoning,
    /// P4: Prominent placement - put EHR rule FIRST
    ProminentPlacement,
    /// P5: Concrete examples - show correct vs incorrect usage
    ConcreteExamples,
    /// P6: Minimal image instruction - downplay image importance
    MinimalImage,
    /// Current production prompt (baseline)
    Current,
    /// P7: Transcript-only - completely ignore the image
    TranscriptOnly,
    /// P8: Aggressive examples with Julie - force specific name extraction
    AggressiveExamples,
    /// P9: Explicit anchor - tell model exactly what to look for
    ExplicitAnchor,
    /// P10: Hybrid - transcript first, then specific EHR lookups
    HybridLookup,
    /// P11: Step-by-step with verification
    VerifiedSteps,
}

impl PromptStrategy {
    /// Get all strategies for iteration
    pub fn all() -> Vec<PromptStrategy> {
        vec![
            PromptStrategy::Current,
            PromptStrategy::NegativeFraming,
            PromptStrategy::FlipDefault,
            PromptStrategy::TwoStepReasoning,
            PromptStrategy::ProminentPlacement,
            PromptStrategy::ConcreteExamples,
            PromptStrategy::MinimalImage,
            PromptStrategy::TranscriptOnly,
            PromptStrategy::AggressiveExamples,
            PromptStrategy::ExplicitAnchor,
        ]
    }

    /// Get phase 2 strategies (best performers + new variations)
    pub fn phase2() -> Vec<PromptStrategy> {
        vec![
            PromptStrategy::ConcreteExamples,      // Best from phase 1
            PromptStrategy::TranscriptOnly,        // New: ignore image entirely
            PromptStrategy::AggressiveExamples,    // New: aggressive with Julie
            PromptStrategy::ExplicitAnchor,        // New: explicit anchor points
        ]
    }

    /// Get phase 3 strategies (hybrid approaches)
    pub fn phase3() -> Vec<PromptStrategy> {
        vec![
            PromptStrategy::ExplicitAnchor,        // Best from phase 2
            PromptStrategy::HybridLookup,          // New: hybrid approach
            PromptStrategy::VerifiedSteps,         // New: step-by-step with verification
        ]
    }

    /// Human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            PromptStrategy::NegativeFraming => "P1: Negative Framing",
            PromptStrategy::FlipDefault => "P2: Flip Default",
            PromptStrategy::TwoStepReasoning => "P3: Two-Step Reasoning",
            PromptStrategy::ProminentPlacement => "P4: Prominent Placement",
            PromptStrategy::ConcreteExamples => "P5: Concrete Examples",
            PromptStrategy::MinimalImage => "P6: Minimal Image",
            PromptStrategy::Current => "Baseline: Current Production",
            PromptStrategy::TranscriptOnly => "P7: Transcript Only",
            PromptStrategy::AggressiveExamples => "P8: Aggressive Examples",
            PromptStrategy::ExplicitAnchor => "P9: Explicit Anchor",
            PromptStrategy::HybridLookup => "P10: Hybrid Lookup",
            PromptStrategy::VerifiedSteps => "P11: Verified Steps",
        }
    }

    /// Short identifier for filenames
    pub fn id(&self) -> &'static str {
        match self {
            PromptStrategy::NegativeFraming => "p1_negative",
            PromptStrategy::FlipDefault => "p2_flip",
            PromptStrategy::TwoStepReasoning => "p3_twostep",
            PromptStrategy::ProminentPlacement => "p4_prominent",
            PromptStrategy::ConcreteExamples => "p5_examples",
            PromptStrategy::MinimalImage => "p6_minimal",
            PromptStrategy::Current => "p0_current",
            PromptStrategy::TranscriptOnly => "p7_transcript",
            PromptStrategy::AggressiveExamples => "p8_aggressive",
            PromptStrategy::ExplicitAnchor => "p9_anchor",
            PromptStrategy::HybridLookup => "p10_hybrid",
            PromptStrategy::VerifiedSteps => "p11_verified",
        }
    }
}

/// Experiment parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentParams {
    pub strategy: PromptStrategy,
    pub temperature: f32,
    pub max_tokens: u32,
    pub image_first: bool, // true = image before text, false = text before image
}

impl Default for ExperimentParams {
    fn default() -> Self {
        Self {
            strategy: PromptStrategy::Current,
            temperature: 0.3,
            max_tokens: 2000,
            image_first: false,
        }
    }
}

/// Result of a single experiment run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResult {
    pub params: ExperimentParams,
    pub prompt_used: String,
    pub raw_response: String,
    pub parsed_soap: Option<ParsedSoap>,
    pub score: ExperimentScore,
    pub generation_time_ms: u64,
    pub timestamp: String,
}

/// Parsed SOAP note for evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSoap {
    pub subjective: Vec<String>,
    pub objective: Vec<String>,
    pub assessment: Vec<String>,
    pub plan: Vec<String>,
}

/// Scoring for an experiment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentScore {
    /// Did output include patient name (Julie)? +1
    pub has_patient_name: bool,
    /// Did output include medication name (Wegovy)? +1
    pub has_medication_name: bool,
    /// Did output correctly identify weight issue? +1
    pub has_weight_issue: bool,
    /// Did output include irrelevant EHR content? -1 each
    pub irrelevant_inclusions: Vec<String>,
    /// Is the JSON well-formed?
    pub valid_json: bool,
    /// Total score: correct_inclusions - incorrect_inclusions
    pub total_score: i32,
    /// Word count of output
    pub word_count: usize,
}

/// Generate the system prompt for a given strategy
pub fn build_experiment_prompt(strategy: PromptStrategy) -> String {
    match strategy {
        PromptStrategy::Current => {
            // Current production prompt
            r#"Medical scribe AI. Generate a SOAP note as a single JSON object from the transcript below.

Rules:
- JSON keys: subjective, objective, assessment, plan
- Only include documented information
- No fabricated data
- Stop after the closing brace
- Do not repeat any information
- The attached image is a screenshot of the EHR during the session. Only use it for very specific information (e.g. prescribed drug name, the name of the patient or specific imaging finding details) as referred to in the transcript but not explicitly stated. If it wasn't indirectly referred to, do not use the information in your note."#.to_string()
        }

        PromptStrategy::NegativeFraming => {
            r#"Medical scribe AI. Generate a SOAP note as JSON from the transcript.

CRITICAL - The attached EHR screenshot is for REFERENCE ONLY:
- Do NOT include medical history from the EHR unless discussed in the transcript
- Do NOT include medications not mentioned in the conversation
- Do NOT include allergies, family history, or past conditions unless the doctor discussed them
- ONLY use the EHR to fill in specific details mentioned but not spelled out (e.g., exact drug name, patient name, specific lab values)

JSON keys: subjective, objective, assessment, plan (arrays of strings)
Only include what was discussed. Stop after closing brace."#.to_string()
        }

        PromptStrategy::FlipDefault => {
            r#"Medical scribe AI. Generate a SOAP note as JSON from the transcript.

IGNORE the attached EHR screenshot EXCEPT when the transcript indirectly references something visible in it:
- "the medication" -> look up the specific drug name
- Patient greeting -> confirm patient name
- "lab results" -> look up specific values mentioned

The transcript is the PRIMARY source. The EHR is ONLY for filling in gaps.

JSON keys: subjective, objective, assessment, plan (arrays of strings)"#.to_string()
        }

        PromptStrategy::TwoStepReasoning => {
            r#"Medical scribe AI. You will create a SOAP note in two steps.

STEP 1 - Identify references: List any items in the transcript that reference something without being specific:
- Names mentioned without full name
- Medications mentioned without drug name
- Lab results mentioned without values

STEP 2 - Create SOAP: Generate the note using ONLY:
- Information explicitly stated in the transcript
- Specific details from the EHR ONLY for items identified in Step 1

JSON format: {"subjective":[],"objective":[],"assessment":[],"plan":[]}"#.to_string()
        }

        PromptStrategy::ProminentPlacement => {
            r#"IMPORTANT: The EHR screenshot should ONLY be used for specific details (drug names, patient names, lab values) that are referenced but not explicitly stated in the transcript. Do not extract or include any EHR information that was not discussed.

Medical scribe AI. Generate a SOAP note as JSON from the transcript below.

JSON keys: subjective, objective, assessment, plan (arrays of strings)
Only include documented information. Stop after closing brace."#.to_string()
        }

        PromptStrategy::ConcreteExamples => {
            r#"Medical scribe AI. Generate a SOAP note as JSON from the transcript.

The EHR screenshot is attached. Use it ONLY for specific details referenced in conversation:

CORRECT usage:
- Transcript says "Hi Julie" -> Use full name "Julie Cooper" from EHR
- Transcript says "weekly injection" -> Use drug name "Wegovy" from EHR
- Transcript says "your A1C looks good" -> Use value "6.2%" from EHR

INCORRECT usage (DO NOT DO):
- Adding diagnoses visible in EHR but not discussed
- Including medication lists not mentioned
- Adding allergies or history not referenced

JSON keys: subjective, objective, assessment, plan (arrays of strings)"#.to_string()
        }

        PromptStrategy::MinimalImage => {
            r#"Medical scribe AI. Generate a SOAP note as JSON.

PRIMARY SOURCE: The transcript below - include only what was discussed.
SECONDARY SOURCE: The attached image - use only if the transcript references something without specifying it (e.g., drug name, patient name).

JSON keys: subjective, objective, assessment, plan (arrays of strings)
Do not repeat information. Stop after closing brace."#.to_string()
        }

        PromptStrategy::TranscriptOnly => {
            // P7: Completely ignore the image - baseline without EHR
            r#"Medical scribe AI. Generate a SOAP note as JSON from the transcript ONLY.

IMPORTANT: Completely ignore the attached image. Use ONLY information from the transcript.

The transcript mentions:
- A patient named Julie
- Weight gain and metabolic issues
- A weekly medication (name not specified in transcript)
- Plans for bloodwork and follow-up

JSON keys: subjective, objective, assessment, plan (arrays of strings)
Only include what was discussed in the transcript. Stop after closing brace."#.to_string()
        }

        PromptStrategy::AggressiveExamples => {
            // P8: More aggressive with specific anchors
            r#"Medical scribe AI. Generate a SOAP note as JSON.

The transcript has GAPS that need filling from the EHR image:
1. "Hi Julie" -> Find the patient's full name in the EHR header (Julie ____?)
2. "weekly medication" -> Find the specific drug name (starts with W, for weight loss)
3. "bloodwork" -> May refer to labs visible in EHR

CRITICAL RULES:
- The subjective section should ONLY contain what the PATIENT reported in the transcript
- Do NOT add medical history from the EHR (cancer, IUD, vitamins, etc.) unless the patient mentioned it
- The image shows an EHR with LOTS of data - most of it is IRRELEVANT to this visit
- This visit is ONLY about: weight gain, weekly medication, bloodwork, follow-up

JSON format: {"subjective":[],"objective":[],"assessment":[],"plan":[]}
Keep it SHORT. Stop after closing brace."#.to_string()
        }

        PromptStrategy::ExplicitAnchor => {
            // P9: Tell the model exactly what to extract from the image
            r#"Medical scribe AI. Generate a concise SOAP note as JSON.

From the TRANSCRIPT, extract:
- Chief complaint: weight gain, metabolic issues
- Plan: weekly medication, bloodwork, follow-up in few weeks

From the EHR IMAGE, extract ONLY these 2 things:
1. Patient full name (look at the header - patient is named Julie)
2. Weekly medication name (look for GLP-1 agonist like Wegovy/Ozempic)

DO NOT EXTRACT from the EHR:
- Medical history
- Past conditions
- Other medications
- Family history
- Anything not discussed in the transcript

JSON: {"subjective":["..."],"objective":["..."],"assessment":["..."],"plan":["..."]}
Be BRIEF. Stop immediately after }."#.to_string()
        }

        PromptStrategy::HybridLookup => {
            // P10: Two-phase approach - first transcript, then specific lookups
            r#"Medical scribe AI. Generate a SOAP note using this two-phase process:

PHASE 1 - From TRANSCRIPT only:
The doctor greeted "Julie" and discussed:
- Weight gain due to metabolic reasons
- Starting a "weekly medication"
- Getting bloodwork
- Follow-up in a few weeks

PHASE 2 - Look up in EHR IMAGE:
Find the patient header. The patient's last name after "Julie" is: ___?
Find the medications list. The weekly weight-loss medication starting with W is: ___?

Generate JSON with the information filled in:
{"subjective":["Julie ___ presents with weight gain, metabolic concerns"],"objective":["Current weight from EHR"],"assessment":["Obesity/weight management"],"plan":["Start ___ (weekly GLP-1), bloodwork, f/u few weeks"]}

Fill in the blanks from the EHR. Ignore all other EHR data."#.to_string()
        }

        PromptStrategy::VerifiedSteps => {
            // P11: Step-by-step with explicit verification
            r#"Medical scribe AI. Follow these steps EXACTLY:

STEP 1: Read ONLY the transcript. The visit is about:
- Patient: Julie (first name only in transcript)
- Problem: weight gain, metabolic issues
- Plan: weekly medication, bloodwork, follow-up

STEP 2: Look at the EHR header ONLY. Find:
- Full patient name (should be "Julie Cooper" or similar)

STEP 3: Look at the medications section ONLY. Find:
- A weekly injectable for weight (Wegovy, Ozempic, or similar)

STEP 4: VERIFY you are NOT including:
- Past medical history from EHR
- Previous visits from EHR
- Unrelated conditions from EHR

Output a concise JSON SOAP note:
{"subjective":["..."],"objective":["Patient: [full name], Weight: [if visible]"],"assessment":["..."],"plan":["Start [medication name], bloodwork, follow-up"]}"#.to_string()
        }
    }
}

/// Expected correct terms for the test case (Julie/Wegovy weight management)
const CORRECT_INCLUSIONS: &[&str] = &[
    "julie",
    "wegovy",
    "weight",
    "metabolic",
    "weekly",
    "medication",
    "bloodwork",
];

/// Terms that indicate incorrect EHR data extraction
const INCORRECT_INCLUSIONS: &[&str] = &[
    "menstrual",
    "period",
    "iud",
    "ovarian",
    "cancer",
    "b12",
    "vitamin d",
    "ganglion",
    "cyst",
    "thumb",
    "breast",
    "colonoscopy",
    "mammogram",
    "bone density",
    "iron",
    "ferritin",
    "post-menopausal",
    "postmenopausal",
];

/// Score an experiment result
pub fn score_result(soap_text: &str) -> ExperimentScore {
    let lower = soap_text.to_lowercase();

    // Check for correct inclusions
    let has_patient_name = lower.contains("julie");
    let has_medication_name = lower.contains("wegovy");
    let has_weight_issue = lower.contains("weight") || lower.contains("metabolic");

    // Check for incorrect inclusions
    let mut irrelevant_inclusions = Vec::new();
    for term in INCORRECT_INCLUSIONS {
        if lower.contains(term) {
            irrelevant_inclusions.push(term.to_string());
        }
    }

    // Check JSON validity
    let valid_json = soap_text.contains("{") && soap_text.contains("}");

    // Calculate score
    let correct_count = [has_patient_name, has_medication_name, has_weight_issue]
        .iter()
        .filter(|&&x| x)
        .count() as i32;
    let incorrect_count = irrelevant_inclusions.len() as i32;
    let total_score = correct_count - incorrect_count;

    let word_count = soap_text.split_whitespace().count();

    ExperimentScore {
        has_patient_name,
        has_medication_name,
        has_weight_issue,
        irrelevant_inclusions,
        valid_json,
        total_score,
        word_count,
    }
}

/// Try to parse SOAP JSON from response
pub fn parse_soap_response(response: &str) -> Option<ParsedSoap> {
    // Extract JSON from response
    let json_str = extract_json(response)?;

    #[derive(Deserialize)]
    struct RawSoap {
        #[serde(default)]
        subjective: Vec<String>,
        #[serde(default)]
        objective: Vec<String>,
        #[serde(default)]
        assessment: Vec<String>,
        #[serde(default)]
        plan: Vec<String>,
    }

    match serde_json::from_str::<RawSoap>(&json_str) {
        Ok(raw) => Some(ParsedSoap {
            subjective: raw.subjective,
            objective: raw.objective,
            assessment: raw.assessment,
            plan: raw.plan,
        }),
        Err(_) => None,
    }
}

fn extract_json(response: &str) -> Option<String> {
    let text = response.replace("```json", "").replace("```", "");
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    Some(text[start..=end].to_string())
}

/// Run a single experiment
pub async fn run_experiment(
    client: &LLMClient,
    model: &str,
    transcript: &str,
    image_base64: &str,
    params: &ExperimentParams,
) -> Result<ExperimentResult, String> {
    let prompt = build_experiment_prompt(params.strategy);
    info!(
        "Running experiment: {} (temp={}, max_tokens={}, image_first={})",
        params.strategy.name(),
        params.temperature,
        params.max_tokens,
        params.image_first
    );

    // Build user content parts
    let text_part = ContentPart::Text {
        text: format!("TRANSCRIPT:\n{}", transcript),
    };
    let image_part = ContentPart::ImageUrl {
        image_url: ImageUrlContent {
            url: format!("data:image/jpeg;base64,{}", image_base64),
        },
    };

    let user_parts = if params.image_first {
        vec![image_part, text_part]
    } else {
        vec![text_part, image_part]
    };

    let start = std::time::Instant::now();

    let response = client
        .generate_vision(
            model,
            &prompt,
            user_parts,
            "vision_experiment",
            Some(params.temperature),
            Some(params.max_tokens),
            Some(1.1),  // repetition_penalty
            Some(50),   // repetition_context_size
        )
        .await?;

    let generation_time_ms = start.elapsed().as_millis() as u64;

    // Parse and score
    let parsed_soap = parse_soap_response(&response);
    let score = score_result(&response);

    Ok(ExperimentResult {
        params: params.clone(),
        prompt_used: prompt,
        raw_response: response,
        parsed_soap,
        score,
        generation_time_ms,
        timestamp: Utc::now().to_rfc3339(),
    })
}

/// Get the experiments output directory
pub fn experiments_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".transcriptionapp")
        .join("debug")
        .join("vision-experiments")
}

/// Save experiment result to disk
pub fn save_result(result: &ExperimentResult) -> Result<PathBuf, String> {
    let dir = experiments_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create experiments dir: {}", e))?;

    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!(
        "{}-{}-temp{:.1}.json",
        timestamp,
        result.params.strategy.id(),
        result.params.temperature
    );
    let path = dir.join(&filename);

    let json = serde_json::to_string_pretty(result)
        .map_err(|e| format!("Failed to serialize result: {}", e))?;

    fs::write(&path, &json).map_err(|e| format!("Failed to write result: {}", e))?;

    info!("Experiment result saved to: {:?}", path);
    Ok(path)
}

/// Load all experiment results from disk
pub fn load_results() -> Result<Vec<ExperimentResult>, String> {
    let dir = experiments_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("Failed to read dir: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    if let Ok(result) = serde_json::from_str::<ExperimentResult>(&content) {
                        results.push(result);
                    }
                }
                Err(e) => {
                    warn!("Failed to read {:?}: {}", path, e);
                }
            }
        }
    }

    Ok(results)
}

/// Generate a summary report of all experiments
pub fn generate_summary_report(results: &[ExperimentResult]) -> String {
    let mut report = String::new();
    report.push_str("# Vision SOAP Prompt Experiment Summary\n\n");
    report.push_str(&format!("Total experiments: {}\n\n", results.len()));

    // Group by strategy
    let mut by_strategy: HashMap<String, Vec<&ExperimentResult>> = HashMap::new();
    for result in results {
        by_strategy
            .entry(result.params.strategy.id().to_string())
            .or_default()
            .push(result);
    }

    report.push_str("## Results by Strategy\n\n");
    report.push_str("| Strategy | Avg Score | Patient Name | Medication | Weight | Irrelevant | Avg Time |\n");
    report.push_str("|----------|-----------|--------------|------------|--------|------------|----------|\n");

    let mut strategies: Vec<_> = by_strategy.keys().collect();
    strategies.sort();

    for strategy_id in strategies {
        let results = &by_strategy[strategy_id];
        let n = results.len() as f32;

        let avg_score: f32 = results.iter().map(|r| r.score.total_score as f32).sum::<f32>() / n;
        let pct_patient: f32 = results.iter().filter(|r| r.score.has_patient_name).count() as f32 / n * 100.0;
        let pct_med: f32 = results.iter().filter(|r| r.score.has_medication_name).count() as f32 / n * 100.0;
        let pct_weight: f32 = results.iter().filter(|r| r.score.has_weight_issue).count() as f32 / n * 100.0;
        let avg_irrelevant: f32 = results.iter().map(|r| r.score.irrelevant_inclusions.len() as f32).sum::<f32>() / n;
        let avg_time: f32 = results.iter().map(|r| r.generation_time_ms as f32).sum::<f32>() / n;

        report.push_str(&format!(
            "| {} | {:.1} | {:.0}% | {:.0}% | {:.0}% | {:.1} | {:.0}ms |\n",
            strategy_id, avg_score, pct_patient, pct_med, pct_weight, avg_irrelevant, avg_time
        ));
    }

    report.push_str("\n## Best Results\n\n");

    // Find best by score
    if let Some(best) = results.iter().max_by_key(|r| r.score.total_score) {
        report.push_str(&format!(
            "**Highest Score**: {} with score {} (temp={}, {}ms)\n",
            best.params.strategy.name(),
            best.score.total_score,
            best.params.temperature,
            best.generation_time_ms
        ));
        report.push_str(&format!(
            "- Correct: patient={}, medication={}, weight={}\n",
            best.score.has_patient_name,
            best.score.has_medication_name,
            best.score.has_weight_issue
        ));
        report.push_str(&format!(
            "- Irrelevant items: {:?}\n\n",
            best.score.irrelevant_inclusions
        ));
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_result_correct() {
        let soap = r#"{"subjective":["Julie presents for weight management"],"objective":[],"assessment":["Metabolic issues"],"plan":["Continue Wegovy"]}"#;
        let score = score_result(soap);

        assert!(score.has_patient_name);
        assert!(score.has_medication_name);
        assert!(score.has_weight_issue);
        assert!(score.irrelevant_inclusions.is_empty());
        assert_eq!(score.total_score, 3);
    }

    #[test]
    fn test_score_result_with_irrelevant() {
        let soap = r#"{"subjective":["Julie presents for weight management and menstrual issues"],"objective":["IUD in place"],"assessment":["Cancer history noted"],"plan":["Continue Wegovy, check B12"]}"#;
        let score = score_result(soap);

        assert!(score.has_patient_name);
        assert!(score.has_medication_name);
        assert!(score.has_weight_issue);
        assert!(score.irrelevant_inclusions.contains(&"menstrual".to_string()));
        assert!(score.irrelevant_inclusions.contains(&"iud".to_string()));
        assert!(score.irrelevant_inclusions.contains(&"cancer".to_string()));
        assert!(score.irrelevant_inclusions.contains(&"b12".to_string()));
        // Score: 3 correct - 4 incorrect = -1
        assert_eq!(score.total_score, -1);
    }

    #[test]
    fn test_parse_soap_response() {
        let response = r#"```json
{"subjective":["Chief complaint: weight gain"],"objective":["Weight: 249 lb"],"assessment":["Obesity"],"plan":["Start Wegovy"]}
```"#;
        let parsed = parse_soap_response(response);
        assert!(parsed.is_some());
        let soap = parsed.unwrap();
        assert_eq!(soap.subjective.len(), 1);
        assert_eq!(soap.plan.len(), 1);
    }

    #[test]
    fn test_all_strategies() {
        let strategies = PromptStrategy::all();
        assert_eq!(strategies.len(), 10); // P0-P9 plus P10, P11

        for strategy in strategies {
            let prompt = build_experiment_prompt(strategy);
            assert!(!prompt.is_empty());
            assert!(prompt.contains("JSON") || prompt.contains("json") || prompt.contains("SOAP"));
        }
    }

    #[test]
    fn test_phase3_strategies() {
        let strategies = PromptStrategy::phase3();
        assert_eq!(strategies.len(), 3); // P9, P10, P11

        // Verify these are the best-performing strategies
        assert!(strategies.contains(&PromptStrategy::ExplicitAnchor));
        assert!(strategies.contains(&PromptStrategy::HybridLookup));
        assert!(strategies.contains(&PromptStrategy::VerifiedSteps));
    }
}

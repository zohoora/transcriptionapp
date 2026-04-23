//! Tools-model-based OHIP diagnostic code resolution.
//!
//! Calls the router's `tools-model` alias with `file_lookup` over the indexed
//! OHIP diagnostic code reference. The 35B model is constrained by the
//! router-side system prompt to always use `file_lookup` for billing/formulary
//! questions, which eliminates the class of hallucinations seen on fast-model
//! (e.g., `315 Specified delays in development` returned for a fibromyalgia
//! encounter on 2026-04-22).
//!
//! Fails soft: if the call fails, times out, or returns an unparseable /
//! out-of-DB code, returns `None` and the caller falls through to the
//! existing 4-stage rule engine pipeline in `rule_engine::resolve_diagnostic_code`.

use crate::billing::{diagnostic_codes, types::ResolvedDiagnostic};
use crate::llm_client::{tasks, LLMClient};
use crate::pipeline_log::PipelineLogger;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::warn;

/// Hard cap on tools-model call latency. Typical: 2–7s. The router doc warns
/// multi-round tool calls can take 30–60s; we budget generously for tail cases.
const TOOLS_MODEL_TIMEOUT_SECS: u64 = 60;

/// The model's `description` field is intentionally ignored — it's often a
/// paraphrase ("Back pain" vs the DB's "Lumbar strain, lumbago, coccydynia,
/// sciatica"). We use the DB's authoritative description instead (looked up
/// from the validated code). Serde drops unknown fields by default.
#[derive(Debug, Deserialize)]
struct ToolsModelJson {
    #[serde(default)]
    code: String,
    #[serde(default, alias = "evidence")]
    soap_evidence: String,
    #[serde(default)]
    reasoning: String,
}

/// Ask tools-model to pick an OHIP diagnostic code for this encounter.
///
/// Returns `Some(ResolvedDiagnostic)` when the response parses, the code is
/// a valid 3-digit OHIP entry in the DB, and the DB description is
/// substituted for the model's paraphrase. Otherwise returns `None` — the
/// caller falls through to the rule-engine pipeline.
pub async fn resolve_via_tools_model(
    client: &LLMClient,
    primary_diagnosis: &str,
    conditions: &[String],
    soap_assessment: &str,
    logger: &Arc<Mutex<PipelineLogger>>,
    session_id: &str,
) -> Option<ResolvedDiagnostic> {
    if primary_diagnosis.trim().is_empty() {
        return None;
    }

    let user_prompt = build_prompt(primary_diagnosis, conditions, soap_assessment);
    // tools-model's router injects its own system prompt; clients must not send one.
    let system_prompt = "";

    let start = Instant::now();
    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(TOOLS_MODEL_TIMEOUT_SECS),
        client.generate_timed(
            "tools-model",
            system_prompt,
            &user_prompt,
            tasks::BILLING_CODES,
        ),
    )
    .await;
    let latency_ms = start.elapsed().as_millis() as u64;

    let (response, metrics) = match result {
        Ok((Ok(r), m)) => (r, m),
        Ok((Err(e), m)) => {
            warn!(
                "Tools-model diagnostic resolution failed for {}: {}",
                session_id, e
            );
            if let Ok(mut l) = logger.lock() {
                let mut ctx = serde_json::json!({
                    "session_id": session_id,
                    "error": "llm_error",
                });
                m.attach_to(&mut ctx);
                l.log_llm_call(
                    "diagnostic_tools_model",
                    "tools-model",
                    system_prompt,
                    &user_prompt,
                    None,
                    latency_ms,
                    false,
                    Some(&e),
                    ctx,
                );
            }
            return None;
        }
        Err(_) => {
            warn!(
                "Tools-model diagnostic resolution timed out for {} ({}s)",
                session_id, TOOLS_MODEL_TIMEOUT_SECS
            );
            if let Ok(mut l) = logger.lock() {
                l.log_llm_call(
                    "diagnostic_tools_model",
                    "tools-model",
                    system_prompt,
                    &user_prompt,
                    None,
                    latency_ms,
                    false,
                    Some("timeout"),
                    serde_json::json!({"session_id": session_id, "error": "timeout"}),
                );
            }
            return None;
        }
    };

    let resolved = parse_and_validate(&response);

    if let Ok(mut l) = logger.lock() {
        let mut ctx = serde_json::json!({
            "session_id": session_id,
            "resolved_code": resolved.as_ref().map(|r| r.code.clone()),
            "raw_length": response.len(),
        });
        metrics.attach_to(&mut ctx);
        l.log_llm_call(
            "diagnostic_tools_model",
            "tools-model",
            system_prompt,
            &user_prompt,
            Some(&response),
            latency_ms,
            true,
            None,
            ctx,
        );
    }

    resolved
}

fn build_prompt(primary: &str, conditions: &[String], assessment: &str) -> String {
    let conditions_str = if conditions.is_empty() {
        "(none extracted)".to_string()
    } else {
        conditions.join(", ")
    };
    // Cap assessment to keep the user message well under the router's 2 MB
    // body limit and give the model headroom for the tool-call dance.
    let assessment_trim = safe_truncate(assessment, 2000);
    format!(
        "You are a medical billing specialist assigning OHIP 3-digit diagnostic codes.\n\n\
         Use the file_lookup tool to find the best OHIP 3-digit diagnostic code for this \
         encounter. The reference library contains the authoritative OHIP diagnostic code \
         list (ohip_diagnostic_codes.md). Do NOT answer from memory — always look up the \
         code by condition name, synonym, or category.\n\n\
         **Primary diagnosis (from SOAP Assessment):** {primary}\n\n\
         **Conditions identified:** {conditions_str}\n\n\
         **SOAP Assessment section:**\n{assessment_trim}\n\n\
         After searching, respond with ONLY a JSON object in this exact format (no markdown \
         code fence, no commentary before or after):\n\n\
         {{\"code\": \"NNN\", \"description\": \"exact description from the reference\", \
         \"soap_evidence\": \"short quote from SOAP supporting this choice\", \
         \"reasoning\": \"one sentence explaining why this code fits better than alternatives\"}}\n\n\
         The code must be a 3-digit number (e.g. \"726\", \"401\", \"250\"). Do NOT include \
         decimals or suffixes. If no specific code fits, use \"799\" (Other ill-defined \
         conditions) with reasoning explaining why a more specific code couldn't be matched."
    )
}

fn safe_truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn parse_and_validate(response: &str) -> Option<ResolvedDiagnostic> {
    // The model may wrap the JSON in prose; extract the outermost {...} block.
    let start = response.find('{')?;
    let end = response.rfind('}')?;
    if end < start {
        return None;
    }
    let json_str = &response[start..=end];

    let parsed: ToolsModelJson = match serde_json::from_str(json_str) {
        Ok(p) => p,
        Err(e) => {
            warn!("Tools-model diagnostic response unparseable: {}", e);
            return None;
        }
    };

    let code = normalize_code(&parsed.code)?;
    let dc = diagnostic_codes::get_diagnostic_code(&code)?;

    Some(ResolvedDiagnostic {
        code,
        // Use the DB description as authoritative; model's `description`
        // field is often a paraphrase ("Back pain" vs "Lumbar strain,
        // lumbago, coccydynia, sciatica"). The model's value is discarded.
        description: dc.description.to_string(),
        evidence: parsed.soap_evidence.trim().to_string(),
        reasoning: parsed.reasoning.trim().to_string(),
    })
}

/// Extract the first 3-digit run from the start of the string. Handles ICD-10
/// format bleed-through where the model emits "784.0" instead of "784".
fn normalize_code(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let mut code = String::with_capacity(3);
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            code.push(ch);
            if code.len() == 3 {
                return Some(code);
            }
        } else if !code.is_empty() {
            break;
        } else if ch.is_whitespace() {
            continue;
        } else {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_trailing_decimal() {
        assert_eq!(normalize_code("784.0"), Some("784".to_string()));
        assert_eq!(normalize_code("  726  "), Some("726".to_string()));
        assert_eq!(normalize_code("401"), Some("401".to_string()));
    }

    #[test]
    fn normalize_rejects_short_or_garbage() {
        assert_eq!(normalize_code("78"), None);
        assert_eq!(normalize_code(""), None);
        assert_eq!(normalize_code("abc"), None);
    }

    #[test]
    fn parse_valid_response() {
        let resp = r#"{"code": "726", "description": "Fibromyalgia", "soap_evidence": "widespread musculoskeletal pain", "reasoning": "specific code matches"}"#;
        let r = parse_and_validate(resp).unwrap();
        assert_eq!(r.code, "726");
        // Authoritative DB description used, not the model's paraphrase
        assert!(r.description.contains("Fibromyalgia"));
        assert!(!r.evidence.is_empty());
    }

    #[test]
    fn parse_rejects_unknown_code() {
        // 999 is not in the OHIP diagnostic code DB
        let resp = r#"{"code": "999", "description": "bogus", "soap_evidence": "x", "reasoning": "y"}"#;
        assert!(parse_and_validate(resp).is_none());
    }

    #[test]
    fn parse_strips_surrounding_prose() {
        let resp = r#"Here is the answer:
        {"code": "250", "description": "Diabetes", "soap_evidence": "type 2 dm", "reasoning": "match"}
        — hope this helps!"#;
        let r = parse_and_validate(resp).unwrap();
        assert_eq!(r.code, "250");
    }
}

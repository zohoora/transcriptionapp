//! Replay LLM backend — returns recorded responses keyed by
//! (task_label, sha256(system + "\n" + user)).
//!
//! Strict mode: any prompt not in the recorded map returns an UnmatchedPrompt
//! error whose text carries the task + hash, surfaced in the mismatch report.
//!
//! SequenceOnly mode: per-task FIFO queue of responses, for tests that
//! intentionally change prompts and don't want to re-record baselines.
//!
//! Vision calls currently ignore the image payload in the hash (only the text
//! prompt portion is hashed). Bundle schema doesn't capture vision prompts
//! today; vision tests will hit UnmatchedPrompt until Phase 7 or deferred.

use super::policies::PromptPolicy;
use crate::encounter_detection::MultiPatientDetectionResult;
use crate::llm_backend::LlmBackend;
use crate::llm_client::{
    AudioEvent, CallMetrics, ContentPart, MultiPatientSoapResult, PatientSoapNote, SoapOptions,
    SpeakerContext,
};
use crate::replay_bundle::ReplayBundle;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct RecordedCall {
    pub task_label: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub response: String,
}

fn hash_prompt(system: &str, user: &str) -> String {
    let mut h = Sha256::new();
    h.update(system.as_bytes());
    h.update(b"\n");
    h.update(user.as_bytes());
    format!("{:x}", h.finalize())
}

pub struct ReplayLlmBackend {
    strict_map: HashMap<(String, String), String>,
    seq_map: Mutex<HashMap<String, VecDeque<String>>>,
    sequence_only_tasks: HashSet<String>,
}

impl ReplayLlmBackend {
    /// Build a backend from the LLM calls captured in a ReplayBundle.
    pub fn from_bundle(bundle: &ReplayBundle, policy: PromptPolicy) -> Self {
        let mut calls: Vec<RecordedCall> = Vec::new();

        for dc in &bundle.detection_checks {
            if let Some(resp) = &dc.response_raw {
                calls.push(RecordedCall {
                    task_label: "encounter_detection".into(),
                    system_prompt: dc.prompt_system.clone(),
                    user_prompt: dc.prompt_user.clone(),
                    response: resp.clone(),
                });
            }
        }

        if let Some(mc) = &bundle.merge_check {
            if let Some(resp) = &mc.response_raw {
                calls.push(RecordedCall {
                    task_label: "encounter_merge".into(),
                    system_prompt: mc.prompt_system.clone(),
                    user_prompt: mc.prompt_user.clone(),
                    response: resp.clone(),
                });
            }
        }

        for mp in &bundle.multi_patient_detections {
            if let Some(resp) = &mp.response_raw {
                calls.push(RecordedCall {
                    task_label: "multi_patient_detect".into(),
                    system_prompt: mp.system_prompt.clone(),
                    user_prompt: mp.user_prompt.clone(),
                    response: resp.clone(),
                });
            }
            if let Some(split) = &mp.split_decision {
                if let Some(resp) = &split.response_raw {
                    calls.push(RecordedCall {
                        task_label: "multi_patient_split".into(),
                        system_prompt: split.system_prompt.clone(),
                        user_prompt: split.user_prompt.clone(),
                        response: resp.clone(),
                    });
                }
            }
        }

        // NB: clinical_check, vision, and SOAP responses aren't captured with
        // their prompts in schema v3. Tests that exercise those paths will
        // need a SequenceOnly policy for those tasks, or rely on Phase 7
        // schema extension.

        Self::for_testing(calls, policy)
    }

    pub fn for_testing(calls: Vec<RecordedCall>, policy: PromptPolicy) -> Self {
        let sequence_only_tasks: HashSet<String> = match &policy {
            PromptPolicy::Strict => Default::default(),
            PromptPolicy::SequenceOnly { tasks } => tasks.iter().cloned().collect(),
        };

        let mut strict_map = HashMap::new();
        let mut seq_map: HashMap<String, VecDeque<String>> = HashMap::new();

        for c in calls {
            if sequence_only_tasks.contains(&c.task_label) {
                seq_map.entry(c.task_label.clone()).or_default().push_back(c.response);
            } else {
                let hash = hash_prompt(&c.system_prompt, &c.user_prompt);
                strict_map.insert((c.task_label.clone(), hash), c.response);
            }
        }

        Self {
            strict_map,
            seq_map: Mutex::new(seq_map),
            sequence_only_tasks,
        }
    }

    fn lookup(&self, task: &str, system: &str, user: &str) -> Result<String, String> {
        if self.sequence_only_tasks.contains(task) {
            let mut seq = self.seq_map.lock().expect("poisoned");
            match seq.get_mut(task).and_then(|q| q.pop_front()) {
                Some(r) => Ok(r),
                None => Err(format!(
                    "UnmatchedPrompt: task={} (SequenceOnly queue exhausted)",
                    task
                )),
            }
        } else {
            let hash = hash_prompt(system, user);
            match self.strict_map.get(&(task.to_string(), hash.clone())) {
                Some(r) => Ok(r.clone()),
                None => Err(format!("UnmatchedPrompt: task={} prompt_hash={}", task, hash)),
            }
        }
    }
}

#[async_trait]
impl LlmBackend for ReplayLlmBackend {
    async fn generate(
        &self,
        _model: &str,
        system: &str,
        user: &str,
        task: &str,
    ) -> Result<String, String> {
        self.lookup(task, system, user)
    }

    async fn generate_timed(
        &self,
        model: &str,
        system: &str,
        user: &str,
        task: &str,
    ) -> (Result<String, String>, CallMetrics) {
        let r = self.generate(model, system, user, task).await;
        (r, CallMetrics::default())
    }

    async fn generate_vision_timed(
        &self,
        _model: &str,
        system: &str,
        user_content: Vec<ContentPart>,
        task: &str,
        _temperature: Option<f32>,
        _max_tokens: Option<u32>,
        _repetition_penalty: Option<f32>,
        _repetition_context_size: Option<u32>,
    ) -> (Result<String, String>, CallMetrics) {
        // Extract text portion of vision prompt for hashing. ContentPart has
        // a Text variant (and Image variants we ignore for the hash).
        let text_portion = extract_vision_text(&user_content);
        let r = self.lookup(task, system, &text_portion);
        (r, CallMetrics::default())
    }

    async fn generate_multi_patient_soap_note(
        &self,
        _model: &str,
        _transcript: &str,
        _audio_events: Option<&[AudioEvent]>,
        _options: Option<&SoapOptions>,
        _speaker_context: Option<&SpeakerContext>,
        multi_patient_detection: Option<&MultiPatientDetectionResult>,
    ) -> Result<MultiPatientSoapResult, String> {
        // SOAP content is not captured in replay bundles (only the outcome's
        // success + word_count are). Return a minimal stub so the orchestrator
        // archive path succeeds. The archive comparator ignores SOAP text
        // content — only the has_soap_note flag is checked.
        let patient_count = multi_patient_detection.map(|d| d.patient_count).unwrap_or(1);
        let notes = (0..patient_count.max(1))
            .map(|i| PatientSoapNote {
                patient_label: format!("Patient {}", i + 1),
                speaker_id: String::new(),
                content: "[harness-stub SOAP]".into(),
            })
            .collect();
        Ok(MultiPatientSoapResult {
            notes,
            physician_speaker: None,
            generated_at: chrono::Utc::now().to_rfc3339(),
            model_used: "harness-stub".into(),
        })
    }
}

fn extract_vision_text(parts: &[ContentPart]) -> String {
    let mut out = String::new();
    for p in parts {
        if let ContentPart::Text { text } = p {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn strict_hit_returns_recorded_response() {
        let backend = ReplayLlmBackend::for_testing(
            vec![RecordedCall {
                task_label: "encounter_detection".into(),
                system_prompt: "sys".into(),
                user_prompt: "user".into(),
                response: "RECORDED".into(),
            }],
            PromptPolicy::Strict,
        );

        let r = backend
            .generate("fast-model", "sys", "user", "encounter_detection")
            .await;
        assert_eq!(r.unwrap(), "RECORDED");
    }

    #[tokio::test]
    async fn strict_miss_returns_unmatched_prompt_error() {
        let backend = ReplayLlmBackend::for_testing(
            vec![RecordedCall {
                task_label: "encounter_detection".into(),
                system_prompt: "sys".into(),
                user_prompt: "user".into(),
                response: "RECORDED".into(),
            }],
            PromptPolicy::Strict,
        );

        let r = backend
            .generate("fast-model", "sys", "DIFFERENT", "encounter_detection")
            .await;
        let err = r.unwrap_err();
        assert!(err.contains("UnmatchedPrompt"), "got: {}", err);
        assert!(err.contains("encounter_detection"));
    }

    #[tokio::test]
    async fn sequence_only_task_pops_in_order() {
        let backend = ReplayLlmBackend::for_testing(
            vec![
                RecordedCall {
                    task_label: "merge_check".into(),
                    system_prompt: "a".into(),
                    user_prompt: "b".into(),
                    response: "FIRST".into(),
                },
                RecordedCall {
                    task_label: "merge_check".into(),
                    system_prompt: "c".into(),
                    user_prompt: "d".into(),
                    response: "SECOND".into(),
                },
            ],
            PromptPolicy::SequenceOnly {
                tasks: vec!["merge_check".into()],
            },
        );

        let r1 = backend
            .generate("m", "ignored", "ignored", "merge_check")
            .await
            .unwrap();
        let r2 = backend
            .generate("m", "also ignored", "also ignored", "merge_check")
            .await
            .unwrap();
        assert_eq!(r1, "FIRST");
        assert_eq!(r2, "SECOND");
    }
}

//! Trait wrapping LLMClient so run_continuous_mode (and encounter_pipeline)
//! can be driven with a mock in tests.
//!
//! The production impl is a thin forwarding wrapper on LLMClient — no behavior
//! change. The test impl (ReplayLlmBackend in src/harness/) returns recorded
//! responses keyed by prompt hash.
//!
//! Methods on the trait are only those reached during a continuous-mode run:
//! detection/merge/clinical/billing via generate_timed, SOAP via
//! generate_multi_patient_soap_note, vision name extraction via
//! generate_vision_timed. Other LLMClient methods (e.g. `generate_soap_note`
//! used by the session-mode IPC command) stay concrete — not part of the
//! orchestrator's execution graph, so not worth expanding the trait surface.

use crate::encounter_detection::MultiPatientDetectionResult;
use crate::llm_client::{
    AudioEvent, CallMetrics, ContentPart, LLMClient, MultiPatientSoapResult, SoapOptions,
    SpeakerContext,
};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait LlmBackend: Send + Sync + 'static {
    async fn generate(
        &self,
        model: &str,
        system_prompt: &str,
        user_content: &str,
        task: &str,
    ) -> Result<String, String>;

    async fn generate_timed(
        &self,
        model: &str,
        system_prompt: &str,
        user_content: &str,
        task: &str,
    ) -> (Result<String, String>, CallMetrics);

    async fn generate_vision_timed(
        &self,
        model: &str,
        system_prompt: &str,
        user_content: Vec<ContentPart>,
        task: &str,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        repetition_penalty: Option<f32>,
        repetition_context_size: Option<u32>,
    ) -> (Result<String, String>, CallMetrics);

    async fn generate_multi_patient_soap_note(
        &self,
        model: &str,
        transcript: &str,
        audio_events: Option<&[AudioEvent]>,
        options: Option<&SoapOptions>,
        speaker_context: Option<&SpeakerContext>,
        multi_patient_detection: Option<&MultiPatientDetectionResult>,
    ) -> Result<MultiPatientSoapResult, String>;
}

#[async_trait]
impl LlmBackend for LLMClient {
    async fn generate(&self, model: &str, system: &str, user: &str, task: &str) -> Result<String, String> {
        LLMClient::generate(self, model, system, user, task).await
    }

    async fn generate_timed(
        &self, model: &str, system: &str, user: &str, task: &str,
    ) -> (Result<String, String>, CallMetrics) {
        LLMClient::generate_timed(self, model, system, user, task).await
    }

    async fn generate_vision_timed(
        &self,
        model: &str,
        system: &str,
        user_content: Vec<ContentPart>,
        task: &str,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        repetition_penalty: Option<f32>,
        repetition_context_size: Option<u32>,
    ) -> (Result<String, String>, CallMetrics) {
        LLMClient::generate_vision_timed(
            self, model, system, user_content, task,
            temperature, max_tokens, repetition_penalty, repetition_context_size,
        ).await
    }

    async fn generate_multi_patient_soap_note(
        &self,
        model: &str,
        transcript: &str,
        audio_events: Option<&[AudioEvent]>,
        options: Option<&SoapOptions>,
        speaker_context: Option<&SpeakerContext>,
        multi_patient_detection: Option<&MultiPatientDetectionResult>,
    ) -> Result<MultiPatientSoapResult, String> {
        LLMClient::generate_multi_patient_soap_note(
            self, model, transcript, audio_events, options, speaker_context, multi_patient_detection,
        ).await
    }
}

// Blanket impl so Arc<L> passes through as an LlmBackend when L: LlmBackend.
#[async_trait]
impl<T: LlmBackend + ?Sized> LlmBackend for Arc<T> {
    async fn generate(&self, model: &str, system: &str, user: &str, task: &str) -> Result<String, String> {
        (**self).generate(model, system, user, task).await
    }
    async fn generate_timed(
        &self, model: &str, system: &str, user: &str, task: &str,
    ) -> (Result<String, String>, CallMetrics) {
        (**self).generate_timed(model, system, user, task).await
    }
    async fn generate_vision_timed(
        &self,
        model: &str,
        system: &str,
        user_content: Vec<ContentPart>,
        task: &str,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        repetition_penalty: Option<f32>,
        repetition_context_size: Option<u32>,
    ) -> (Result<String, String>, CallMetrics) {
        (**self).generate_vision_timed(
            model, system, user_content, task,
            temperature, max_tokens, repetition_penalty, repetition_context_size,
        ).await
    }
    async fn generate_multi_patient_soap_note(
        &self,
        model: &str,
        transcript: &str,
        audio_events: Option<&[AudioEvent]>,
        options: Option<&SoapOptions>,
        speaker_context: Option<&SpeakerContext>,
        multi_patient_detection: Option<&MultiPatientDetectionResult>,
    ) -> Result<MultiPatientSoapResult, String> {
        (**self).generate_multi_patient_soap_note(
            model, transcript, audio_events, options, speaker_context, multi_patient_detection,
        ).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check that LLMClient implements LlmBackend.
    fn _assert_llm_client_is_backend() {
        fn takes_backend(_: impl LlmBackend) {}
        let _ = |c: LLMClient| takes_backend(c);
    }

    fn _assert_arc_llm_client_is_backend() {
        fn takes_backend(_: impl LlmBackend) {}
        let _ = |c: Arc<LLMClient>| takes_backend(c);
    }

    fn _assert_dyn_works() {
        fn takes_dyn(_: &dyn LlmBackend) {}
        let _ = |c: LLMClient| takes_dyn(&c);
    }
}

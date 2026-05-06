//! Shared test scaffolding for continuous-mode unit tests.
//!
//! - `ArchiveDirGuard`: RAII wrapper around `TRANSCRIPTIONAPP_ARCHIVE_DIR`.
//!   Tests that use it must be `#[serial]` because the env var is
//!   process-wide.
//! - `test_ctx_with_archive`: build a `RecordingRunContext` rooted at a
//!   given archive path, with a no-op `ReplayLlmBackend`. The continuous-mode
//!   splitter / flush-on-stop don't pull from `ctx.llm()` directly, so the
//!   stub backend is sufficient.
//! - `seed_transcript_buffer`: push N synthetic segments into a
//!   `ContinuousModeHandle`'s transcript buffer for splitter/flush exercising.
//!   `speaker_id = None` keeps post-drain word counts deterministic — the
//!   splitter prepends `"Speaker N: "` to segments with a speaker label,
//!   which inflates `wc` and would silently couple test assertions to label
//!   formatting.

#![cfg(test)]

use crate::continuous_mode::ContinuousModeHandle;
use crate::harness::policies::PromptPolicy;
use crate::harness::recording_run_context::RecordingRunContext;
use crate::harness::replay_llm_backend::ReplayLlmBackend;
use crate::llm_backend::LlmBackend;
use crate::server_config::compiled_defaults;
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;

pub(crate) struct ArchiveDirGuard {
    prev: Option<String>,
    tmp: tempfile::TempDir,
}

impl ArchiveDirGuard {
    pub(crate) fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let prev = std::env::var("TRANSCRIPTIONAPP_ARCHIVE_DIR").ok();
        std::env::set_var("TRANSCRIPTIONAPP_ARCHIVE_DIR", tmp.path());
        Self { prev, tmp }
    }

    pub(crate) fn path(&self) -> &Path {
        self.tmp.path()
    }
}

impl Drop for ArchiveDirGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => std::env::set_var("TRANSCRIPTIONAPP_ARCHIVE_DIR", v),
            None => std::env::remove_var("TRANSCRIPTIONAPP_ARCHIVE_DIR"),
        }
    }
}

pub(crate) fn test_ctx_with_archive(archive_root: &Path) -> RecordingRunContext {
    let llm: Arc<dyn LlmBackend> =
        Arc::new(ReplayLlmBackend::for_testing(vec![], PromptPolicy::Strict));
    RecordingRunContext::new(
        compiled_defaults(),
        llm,
        archive_root.to_path_buf(),
        Utc::now(),
        vec![],
    )
}

pub(crate) fn seed_transcript_buffer(
    handle: &ContinuousModeHandle,
    words_per_segment: usize,
    n_segments: usize,
) {
    let mut buf = handle
        .transcript_buffer
        .lock()
        .expect("transcript buffer lock");
    for i in 0..n_segments {
        let text = (0..words_per_segment)
            .map(|w| format!("w{}", w))
            .collect::<Vec<_>>()
            .join(" ");
        buf.push(
            text,
            (i as u64) * 100,
            ((i as u64) * 100) + 50,
            None,
            None,
            0,
        );
    }
}

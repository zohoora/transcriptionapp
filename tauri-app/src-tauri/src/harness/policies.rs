//! Per-test strictness policies for the harness.

/// What counts as "equivalent behavior" when comparing a test run to the
/// recorded baseline.
#[derive(Debug, Clone, Default)]
pub enum EquivalencePolicy {
    /// Compare archive state (metadata, file presence) only. Default.
    #[default]
    ArchiveStructural,
    /// Archive-structural + emitted event sequence comparison.
    EventSequence,
}

/// How strictly LLM prompts must match the recorded baseline.
#[derive(Debug, Clone)]
pub enum PromptPolicy {
    /// Default. LLM lookups match by (task_label, sha256(system + user)) exactly.
    /// Any prompt mismatch surfaces as UnmatchedPrompt in the report.
    Strict,
    /// Per-task opt-out. Listed tasks replay by call-sequence order (FIFO);
    /// tasks not listed stay Strict.
    SequenceOnly { tasks: Vec<String> },
}

impl Default for PromptPolicy {
    fn default() -> Self {
        PromptPolicy::Strict
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policies_are_strict() {
        assert!(matches!(EquivalencePolicy::default(), EquivalencePolicy::ArchiveStructural));
        assert!(matches!(PromptPolicy::default(), PromptPolicy::Strict));
    }
}

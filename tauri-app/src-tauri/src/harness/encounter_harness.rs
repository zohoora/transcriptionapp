//! Public API for per-encounter equivalence tests.
//!
//! Usage:
//!
//! ```ignore
//! #[tokio::test(flavor = "current_thread", start_paused = true)]
//! async fn my_encounter_test() {
//!     EncounterHarness::new("tests/fixtures/encounter_bundles/2026-04-14/xyz.json")
//!         .run()
//!         .await
//!         .expect_equivalent();
//! }
//! ```

use super::archive_comparator::ArchiveComparator;
use super::driver::drive_encounter_bundle_smoke;
use super::mismatch_report::{MismatchKind, MismatchReport};
use super::policies::{EquivalencePolicy, PromptPolicy};
use crate::replay_bundle::ReplayBundle;
use std::path::{Path, PathBuf};

/// Return the baseline sidecar path for a given bundle path.
/// Example: "foo/bar.json" → "foo/bar.baseline.json"
fn baseline_path_for(bundle_path: &Path) -> PathBuf {
    let parent = bundle_path.parent().unwrap_or(Path::new("."));
    let stem = bundle_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("bundle");
    parent.join(format!("{}.baseline.json", stem))
}

pub struct EncounterHarness {
    bundle: ReplayBundle,
    bundle_path: PathBuf,
    test_id: String,
    #[allow(dead_code)] // Used when EventSequence comparator arrives (Phase 5)
    equivalence: EquivalencePolicy,
    #[allow(dead_code)] // Used when PromptPolicy plumbing arrives (Phase 5 expansion)
    prompt: PromptPolicy,
}

impl EncounterHarness {
    pub fn new(bundle_path: impl Into<PathBuf>) -> Self {
        let bundle_path = bundle_path.into();
        let body = std::fs::read_to_string(&bundle_path).expect("bundle path exists");
        let bundle: ReplayBundle = serde_json::from_str(&body).expect("bundle parses");

        let test_id = bundle_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("encounter")
            .to_string();

        Self {
            bundle,
            bundle_path,
            test_id,
            equivalence: Default::default(),
            prompt: Default::default(),
        }
    }

    pub fn with_policy(mut self, p: EquivalencePolicy) -> Self {
        self.equivalence = p;
        self
    }

    pub fn with_prompt_policy(mut self, p: PromptPolicy) -> Self {
        self.prompt = p;
        self
    }

    pub async fn run(self) -> MismatchReport {
        let path_str = self.bundle_path.to_string_lossy().to_string();

        // Drive the orchestrator
        let outcome = match drive_encounter_bundle_smoke(&self.bundle).await {
            Ok(o) => o,
            Err(e) => {
                return MismatchReport::divergent(
                    &self.test_id,
                    &path_str,
                    MismatchKind::OrchestratorPanic { message: e },
                    vec![],
                );
            }
        };

        if let Err(e) = outcome.run_result {
            return MismatchReport::divergent(
                &self.test_id,
                &path_str,
                MismatchKind::OrchestratorPanic { message: e },
                outcome.ctx.captured_events(),
            );
        }

        // Snapshot-style comparison. Baseline sidecar file:
        //   tests/fixtures/encounter_bundles/seed/2026-04-01_03ffd0eb.baseline.json
        // First run (or HARNESS_RECORD_BASELINES=1) records; subsequent runs
        // verify the orchestrator still produces the same archive state.
        let baseline_path = baseline_path_for(&self.bundle_path);
        let comparator = ArchiveComparator::default();
        let compare_res = comparator.compare_snapshot(outcome.archive_dir.path(), &baseline_path);

        match compare_res {
            Ok(mismatches) if mismatches.is_empty() => {
                MismatchReport::equivalent(&self.test_id, &path_str)
            }
            Ok(mut mismatches) => {
                let kind = mismatches.remove(0);
                let preceding: Vec<_> = outcome
                    .ctx
                    .captured_events()
                    .into_iter()
                    .rev()
                    .take(3)
                    .rev()
                    .collect();
                MismatchReport::divergent(&self.test_id, &path_str, kind, preceding)
            }
            Err(infra_err) => MismatchReport::divergent(
                &self.test_id,
                &path_str,
                MismatchKind::OrchestratorPanic { message: infra_err },
                vec![],
            ),
        }
    }
}

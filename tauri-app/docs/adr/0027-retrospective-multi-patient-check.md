# ADR-0027: Retrospective Multi-Patient Check

## Status

Accepted (Apr 2026)

## Context

ADR-0024 (Hybrid Encounter Detection) and the encounter merge loop (`encounter_merge.rs`) together produce a single merged transcript when the system initially splits an encounter and then decides the split was wrong. After merge-back, the result is a single session that may actually contain:

- **One patient with a long complex visit** — the merge-back was correct.
- **Two patients with a companion** — e.g., a couples visit where both were discussed substantively. Billable as two separate encounters.
- **A patient and their accompanying parent/spouse** — discussed briefly but not as a separate patient.

The LLM's encounter-merge logic is intentionally conservative (it prefers to merge-back false splits), so long merged sessions need a *retrospective* check to rescue genuinely multi-patient encounters back into separate SOAP notes.

## Decision

After every merge-back, if the merged transcript is at least `MULTI_PATIENT_CHECK_WORD_THRESHOLD` (2,500) words:

1. Run a **multi-patient detection** LLM call. Returns `{patient_count: 1-4, confidence}`.
2. If `patient_count >= 2`, run a **split-point detection** LLM call. Returns `{line_index, confidence, reason}`.
3. Apply a **size gate**: both halves of the proposed split must be at least `MULTI_PATIENT_SPLIT_MIN_WORDS` (500) words. This suppresses splits where a companion was discussed briefly.
4. If the size gate passes, split the archived session and regenerate per-patient SOAP notes and billing.

### Rationale for thresholds

- **2,500 words for detection**: below this, the odds of two substantive patient discussions are low, and running the LLM eats latency for likely-single-patient sessions.
- **500 words per half for split**: a 500-word encounter represents roughly 4–5 minutes of substantive clinical conversation — enough to generate a meaningful SOAP note. Below that, it's probably a companion or a brief sidebar, not a separate billable visit.

These thresholds are constants in `encounter_detection.rs` and are sanity-checked by tests:

```rust
assert!(MULTI_PATIENT_SPLIT_MIN_WORDS < MULTI_PATIENT_CHECK_WORD_THRESHOLD);
assert!(MULTI_PATIENT_DETECT_WORD_THRESHOLD < MULTI_PATIENT_CHECK_WORD_THRESHOLD);
```

### Three LLM calls involved

| Prompt | Purpose | System prompt location |
|--------|---------|------------------------|
| Multi-patient check (gate) | Binary "≥ 2 patients?" | `multi_patient_check_prompt()` |
| Multi-patient detect | Count patients (1–4) | `multi_patient_detect_prompt()` |
| Multi-patient split | Find boundary `line_index` | `multi_patient_split_prompt()` |

All three are server-overridable via `PromptTemplates.multi_patient_*`.

### Split prompt strategy

The split prompt is intentionally different from the standard encounter-detection prompt: it focuses on **name transitions** rather than farewell markers, because couples/family visits rarely have an explicit "goodbye" between them. Prompt text in `encounter_detection.rs:563-...`.

### Post-split actions

When a split is accepted:

1. `split_session()` in `commands/archive.rs` atomically writes:
   - Primary session: segments 0 to `line_index`
   - Secondary session: segments `line_index+1` to end
   - Both get new IDs; the original `.bak` is retained
2. Per-patient SOAP + billing are regenerated from scratch. The original's SOAP/billing are discarded (they described the wrong thing).
3. `replay_bundle_v3` captures the `MultiPatientSplitDecision` so the replay CLI can regression-test the boundary.
4. Both sessions appear in the review tab, visually adjacent.

### Validated behavior (Mar 6 2026)

After landing, the retrospective check was run against historical sessions. Findings:

- Correctly splits couples visits and family visits.
- Blocks companions (brief discussions of a spouse's unrelated complaint) via the 500-word size gate.
- False-negative rate low but non-zero: when a patient's own history has two distinct topics (e.g., diabetes + a new cardiac complaint discussed in depth) the detect LLM occasionally flags it as 2 patients. The split prompt usually recovers by returning an empty `{}` (no boundary), which rejects the split. Monitoring ongoing.

## Consequences

### Positive

- **Rescues genuine multi-patient encounters** that the conservative merge-back would otherwise keep glued together.
- **Per-patient SOAP and billing** for couples/family visits — each patient gets their own note and OHIP codes.
- **Deterministic size gate** guards against the LLM's topic-shift false positives.

### Negative

- **Three LLM calls on long sessions** — the gate + detect + split path adds ~10–20s per long encounter. Acceptable because it only fires above 2,500 words.
- **Cost of being wrong is high** — a false-positive split fragments a SOAP note. The size gate + confidence thresholds are calibrated conservatively, but edge cases exist.
- **Prompt drift risk** — three separate prompts, each server-overridable. A `test_replay_day_py_has_current_detection_prompt`-style drift test would be prudent for the split prompt specifically. (Currently only the encounter-detection prompt has such a test.)

## References

- `tauri-app/src-tauri/src/encounter_detection.rs` — all three prompts and parsers
- `tauri-app/src-tauri/src/encounter_pipeline.rs::check_multi_patient_and_split()` — orchestration
- `tauri-app/src-tauri/src/commands/archive.rs::split_session()` — the atomic split
- `tauri-app/src-tauri/src/replay_bundle.rs` — `MultiPatientSplitDecision` capture
- `docs/benchmarks/multi-patient-detection.md` and `multi-patient-split.md` — benchmark fixtures
- ADR-0024: Hybrid Encounter Detection — preceded this in the pipeline

---
name: forensic-review
description: Forensic analysis of a clinic day's sessions — pulls archive + per-session pipeline logs, identifies defect classes, drafts GT labels under tests/fixtures/labels/
user-invocable: true
disable-model-invocation: true
arguments:
  - name: date
    description: "Clinic day to review in YYYY-MM-DD form (defaults to today)"
    default: today
---

# Forensic Review of a Clinic Day

Replays the workflow established across the 2026-04-29 / 2026-04-30 / 2026-05-01 reviews. Goal: classify the day's defects into reusable classes, draft GT labels, and surface concrete fix targets.

## When to Use

After a clinic day where the user reports SOAP / billing / vision / detection issues. The user typically pastes a per-session GT summary alongside the invocation — read that first.

## Phase 1: Resolve the date

If `date == today`, use `2026-MM-DD` from the current local date. Otherwise use the user-supplied value.

## Phase 2: Inventory the archive

Run in parallel:

```bash
ls -la ~/.transcriptionapp/archive/YYYY/MM/DD/
cat ~/.transcriptionapp/archive/YYYY/MM/DD/day_log.jsonl | head -200
```

Note any `notes-test-*` dirs — those are unrelated test artifacts; ignore them.

For each non-test session dir, dump:
- metadata.json (key fields: started_at, ended_at, word_count, patient_name, patient_dob, charting_mode, has_soap_note, encounter_number, merge_back_count)
- the contents (which files exist? Missing files = orphan partial-archive)
- billing.json (codes, time_entries, dx)
- pipeline_log.jsonl summary (step counts: vision_extraction, encounter_detection, multi_patient_detect, clinical_content_check, soap_generation, billing_extraction)

## Phase 3: Reconstruct the day timeline

Walk `day_log.jsonl` chronologically:

```python
import json
with open('/Users/backoffice/.transcriptionapp/archive/YYYY/MM/DD/day_log.jsonl') as f:
    for line in f:
        e = json.loads(line)
        # Extract: continuous_mode_started, encounter_split, clinical_check_result,
        # soap_generated, billing_extracted, encounter_merged, idle_buffer_cleared,
        # continuous_mode_stopped, retrospective_multi_patient_split,
        # multi_patient_detected, negative_gap_pairs_found
```

Map each event to:
- which session ID is alive at that moment
- whether it was eventually merged-away (no archive dir)
- which encounter_number it belongs to in the current run

Compare against the user's GT list — sessions in GT that aren't in the day_log are likely cross-machine (recorded on iMac Room 2 or via process_mobile / iOS).

## Phase 4: Classify defects

Use the established defect-class taxonomy (extend it if a new pattern appears):

| Class | Pattern | Typical signal |
|-------|---------|----------------|
| A | Phone-call vision: chart pre-load defeats recency-weighted vote | >= 3 distinct names in pipeline_log vision_extraction stream within one encounter |
| B | Vision system silent for entire encounter | 0 vision_extraction events on a >5-min clinical encounter |
| C | Multi-patient session not separated as history rows | metadata.patient_count > 1 but patient_labels.json absent |
| D | Multi-patient billing fan-out misroutes codes | sub-patient billing has codes inappropriate for the patient population (e.g. P003A on toddler) |
| E | Merge-back silently drops Q310A | post-merge billing has no time_entries despite ≥15 min duration |
| F | Multi-patient detection didn't fire | session has ≥2 patients per GT, no multi_patient_detected event |
| G | Cross-machine session: SOAP checkmark but no body | metadata.has_soap_note=true but local soap_note.txt missing/empty |
| H | negative_gap_pair_scan banner false-positive | user accepted merge but post-hoc shows unrelated sessions |
| I | Billing hallucination guard fired (positive finding — pin) | activity log: "dropped N hallucinated condition(s)" |

For each defect identified, capture:
- session_id (8-char prefix + full UUID)
- patient name from GT
- evidence (specific log lines, vision call sequence, timing)
- proposed fix or "no fix — needs repro"

## Phase 5: Cross-check vs prior reviews

Read memory for prior forensic-review summaries:
- `~/.claude/projects/-Users-backoffice-transcriptionapp/memory/project_2026_04_29_forensic_review.md`
- `~/.claude/projects/-Users-backoffice-transcriptionapp/memory/project_2026_04_30_forensic_review.md`
- `~/.claude/projects/-Users-backoffice-transcriptionapp/memory/project_2026_04_30_fixes_implemented.md`

If a defect class was supposedly fixed in v0.10.67/68/69 but reappears today, flag it as a regression — that's the highest-priority finding.

## Phase 6: Draft GT labels

For every reviewed session, write a label under `tauri-app/src-tauri/tests/fixtures/labels/YYYY-MM-DD_<short_session_id>.json`. Schema:

```json
{
  "session_id": "<full-uuid>",
  "date": "YYYY-MM-DD",
  "labeled_at": "<today's UTC datetime>",
  "labeled_by": "Dr Z (YYYY-MM-DD clinic-day forensic review)",
  "labels": {
    "split_correct": true,
    "merge_correct": true,
    "clinical_correct": true,
    "patient_count_correct": true,
    "procedure_section_correct": true,
    "billing_codes_expected": ["A007A", "Q310A"],
    "billing_codes_unexpected": [],
    "diagnostic_code_expected": "...",
    "notes": "<concise: patient context + defect class refs + fix target>"
  }
}
```

Fields are all optional except session_id + date — only include the assertions you've actually verified. For sessions without a clean session_id (Class G missing-archive cases), use a `UNKNOWN-<patient-slug>-YYYY-MM-DD` placeholder.

## Phase 7: Validate the fixtures

```bash
for f in tauri-app/src-tauri/tests/fixtures/labels/YYYY-MM-DD_*.json; do
  python3 -c "import json; json.load(open('$f'))" && echo "OK $f" || echo "FAIL $f"
done
```

## Phase 8: Report

Output a forensic report to the user with:
1. Day shape (how many continuous-mode runs, encounter counts)
2. Defect classes table (one row per class with affected sessions)
3. Counts (fully archived / orphans / merged-away / GT-but-no-trace)
4. List of new GT labels written
5. Recommended fix scope (which classes are coded vs. need investigation)

End with: "Want me to fix everything?" or specific follow-ups.

## Useful primitives

- `~/.transcriptionapp/archive/YYYY/MM/DD/day_log.jsonl` — chronological day events
- `~/.transcriptionapp/archive/YYYY/MM/DD/<sid>/pipeline_log.jsonl` — per-session LLM call log (step + prompt + response_raw + context)
- `~/.transcriptionapp/archive/YYYY/MM/DD/<sid>/replay_bundle.json` — self-contained encounter test case
- `~/.transcriptionapp/archive/YYYY/MM/DD/performance_summary.json` — per-step latency p50/p90/p99 + scheduling/network split
- `~/.transcriptionapp/logs/activity.log.YYYY-MM-DD` — structured log; UTC timestamps

If profile-service is reachable (curl 100.119.83.76:8090/health), also pull cross-machine sessions to identify GT patients with no local trace.

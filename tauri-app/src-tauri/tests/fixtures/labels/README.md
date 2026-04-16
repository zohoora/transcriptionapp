# Ground Truth Labels

This directory contains human-reviewed labels for replay bundles in `~/.transcriptionapp/archive/`.
Each label file is a JSON document that asserts the "correct" answer for a specific encounter.

## When to add a label

After reviewing a bundle and confirming production made the right (or wrong) decision, write a label file. The `labeled_regression_cli` runs all label files against the corresponding bundles and reports any divergence between current production behavior and the labeled "correct" answer.

## File naming convention

`{date}_{short_session_id}.json` where:
- `date` = YYYY-MM-DD
- `short_session_id` = first 8 chars of the session UUID

Example: `2026-04-15_00aa31d4.json`

## Schema

```json
{
  "session_id": "full-uuid-here",
  "date": "2026-04-15",
  "labeled_at": "2026-04-16T...",
  "labeled_by": "Dr Z",
  "labels": {
    "split_correct": true,
    "merge_correct": true,
    "clinical_correct": true,
    "patient_count_correct": true,
    "billing_codes_expected": ["A007A", "Q310A"],
    "diagnostic_code_expected": "311",
    "notes": "Clean headache encounter, well-coded"
  }
}
```

All fields except `session_id` and `date` are optional. The CLI checks each provided label against the bundle and reports mismatches.

## How to add a label after a clinic day

1. Open the History window, review each session's transcript + SOAP + billing
2. For sessions where production was right: copy the file template, set fields to `true` and copy the actual codes
3. For sessions where production was wrong: set the bool to `false` or change the codes/dx
4. The next CLI run will report regressions if production behavior changes

## Running

```bash
cd tauri-app/src-tauri
cargo run --bin labeled_regression_cli -- --all
cargo run --bin labeled_regression_cli -- --all --fail-on-regression
```

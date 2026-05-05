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
    "billing_codes_unexpected": ["G372A"],
    "billing_quantity_expected": {"K005A": 2},
    "diagnostic_code_expected": "311",
    "diagnostic_code_acceptable": ["311.0", "799"],
    "expected_failures": ["billing_codes", "diagnostic_code"],
    "notes": "Clean headache encounter, well-coded"
  }
}
```

All fields except `session_id` and `date` are optional. The CLI checks each provided label against the bundle and reports mismatches.

### `expected_failures` (added 2026-05-05)

Names of checks that are **currently expected to fail** for this session. Used by `labeled_regression_cli --fail-on-regression` to distinguish known-bad cases (waiting on a code fix) from new regressions:

- A check listed here that fails → counted as "expected", does not block CI.
- A check NOT listed here that fails → counted as a regression, fails CI with exit code 2.
- A check listed here that **passes** → counted as "drift", surfaced as `↑ tighten {name}` so you know to drop it from the list and tighten the gate.

Stable check names emitted by the CLI:

| Name | Source field | Notes |
|------|-------------|-------|
| `clinical_classification` | `clinical_correct` | One per label. |
| `billing_codes` | `billing_codes_expected` | Subset test — single check even when the list has multiple codes. |
| `billing_codes_unexpected:{CODE}` | `billing_codes_unexpected` | One per code in the unexpected list. |
| `billing_quantity:{CODE}` | `billing_quantity_expected` | One per code in the quantity map. |
| `diagnostic_code` | `diagnostic_code_expected` | Honors `diagnostic_code_acceptable` synonyms. |
| `billing_json_missing` | (synthetic) | Fires when a billing-related expectation exists but `billing.json` doesn't. |
| `date_parse` | (synthetic) | Fires on a malformed `date`. |

## How to add a label after a clinic day

1. Open the History window, review each session's transcript + SOAP + billing
2. For sessions where production was right: copy the file template, set fields to `true` and copy the actual codes
3. For sessions where production was wrong: set the bool to `false` or change the codes/dx
4. The next CLI run will report regressions if production behavior changes

## Running

```bash
cd tauri-app/src-tauri
cargo run --bin labeled_regression_cli -- --all
cargo run --bin labeled_regression_cli -- --all --fail-on-regression       # CI gate: exit 2 on NEW regressions
cargo run --bin labeled_regression_cli -- --all --bootstrap-expected-failures  # Re-pin the baseline
cargo run --bin labeled_regression_cli -- 2026-04-30_d7039e4c.json --verbose  # Single label, full detail
```

`--bootstrap-expected-failures` rewrites label files in place — replaces each label's `expected_failures` array with the names of the checks that are currently failing. One-time migration when the corpus has drifted (e.g. fresh forensic-review labels added). Cannot be combined with `--fail-on-regression`.

Updates from `--bootstrap-expected-failures` only modify the `expected_failures` field; field order, comments, and adjacent fields are preserved (the CLI uses `serde_json` with `preserve_order` to round-trip cleanly).

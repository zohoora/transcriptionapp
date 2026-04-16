# ADR-0021: FHO+ Billing Engine

## Status

Accepted (Apr 2026)

## Context

Ontario family-health-organization (FHO) physicians bill OHIP for visits using a schedule of ~235 fee codes plus 562 ICD-8 diagnostic codes, with complex rules about what can be combined, per-year caps on counselling codes, in-basket vs out-of-basket classification, companion codes, age brackets, and after-hours premiums. Manually transcribing a SOAP note into a billing code set is error-prone and slow.

We wanted the app to suggest billing codes automatically after SOAP generation, review-ready in the UI, with deterministic explanations that a physician can audit before submitting.

Constraints:

1. **Rules must be deterministic and explainable** — a physician needs to know *why* a code was suggested, not just trust a black box.
2. **LLMs are good at language, bad at numeric tables** — asking an LLM to "generate the correct OHIP codes" directly produces hallucinated codes, wrong per-year caps, and non-deterministic diagnostic code assignment.
3. **Schedule of Benefits changes periodically** — the app must be able to update the code table and rules without a full rebuild (see ADR-0023).
4. **Privacy** — all code suggestion must happen locally; no OHIP-related data leaves the clinic network.
5. **Must handle multi-patient encounters** — couples/family visits produce separate bills.

## Decision

Implement a **two-stage billing engine**: an LLM extracts *clinical features* from the SOAP note, then a deterministic Rust *rule engine* maps those features to OHIP codes.

### Stage 1 — LLM clinical feature extraction

`extract_clinical_features()` calls the `fast-model` alias with a schema-constrained prompt that returns:

| Field | Type | Purpose |
|-------|------|---------|
| `visitType` | one of 23 enum values | Assessment type (intermediate, minor, counselling, etc.) |
| `procedures` | subset of 79 enum values | Procedures performed (pap, PMS-injection, biopsy, cryotherapy, etc.) |
| `conditions` | subset of 14 enum values | Chronic-disease management or modifier conditions (e.g., `OpioidWithdrawalManagement`, `DiabetesT2Management`) |
| `primaryDiagnosis` | free text | The condition being addressed in the visit |
| `suggestedDiagnosticCode` | optional 3-digit string | LLM's guess at the OHIP diagnostic code — cross-validated below |
| `counsellingMinutes` | optional integer | Face-to-face counselling duration for K013A/K033A |
| `afterHours` | optional bool | Visit outside core hours |

Enum-only for enumerable fields means the LLM can't hallucinate a code like "Q999A" — the worst it can do is pick the wrong enum value.

### Stage 2 — Deterministic rule engine

`rule_engine.rs` takes the extracted features and produces a `BillingRecord` with zero LLM calls. It:

1. Maps `visitType` + `procedures` + `conditions` → base assessment code (e.g., `A007A`) and add-on codes.
2. Auto-adds companion codes: `E542A` (tray fee) when certain procedures are present, `E430A` (pap tray), `E079A` (smoking cessation), etc.
3. Applies base+add-on quantity logic (`G370` → `G371` for subsequent units, `G384` → `G385`).
4. Handles **K013A → K033A overflow**: K013A (counselling) is capped at 3 units/year per patient. If the session duration maps to >3 units or the physician's yearly cap is exhausted, the overflow quantity moves to `K033A` (out-of-basket).
5. Applies Q310–Q313 time tracking (after-hours premiums) with 14-hour/day and 240-hour/28-day caps.
6. Resolves the diagnostic code via a confidence-tiered policy (Apr 16 2026 revision — see Consequences below for the audit that motivated this):
   - `confidence ≥ 0.90` (DX_TRUST_CONFIDENCE): accept the LLM's `suggestedDiagnosticCode` directly. The LLM's semantic reasoning over narrative clinical text is more reliable than substring matching for the primary complaint.
   - `0.50 ≤ confidence < 0.90`: keep the original literal-word cross-validation guardrail — require at least one significant word (≥4 chars) from the code's description to appear in `primaryDiagnosis`, else fall through.
   - `confidence < 0.50` (DX_MIN_CONSIDER): treat as noise, ignore entirely and fall through.
   - **Fall-through stages**: text-match `primaryDiagnosis` against the 562-code database by substring, then billing-code-implied diagnosis (e.g., K030A → 250), then the K133A/K125A IDD constraint.

Every suggested code carries the reasoning path for audit ("Selected A007A because visitType=intermediate and after_hours=false; added E430A because procedures includes pap_smear").

### Data sources

| File | Contents |
|------|----------|
| `billing/ohip_codes.rs` | 235 OHIP codes (145 in-basket + 90 out-of-basket). Source: April 2026 Schedule of Benefits. `test_ohip_code_count` pins the count to catch drift. |
| `billing/diagnostic_codes.rs` | 562 OHIP diagnostic codes (ICD-8). `DIAGNOSTIC_CODE_COUNT` constant + `test_unique_codes` pin the count. |
| `billing/clinical_features.rs` | The 23/79/14 enum definitions used in Stage 1. |
| `billing/rule_engine.rs` | Stage 2 mapping logic. |
| `billing/exclusions.rs` | 21 mutual-exclusion groups (e.g., "only one assessment per visit"). |

### Multi-patient handling

In continuous mode, if an encounter is flagged as multi-patient (2–4 patients), `encounter_pipeline::extract_and_archive_billing()` runs Stage 1+2 per patient and writes one `billing.json` per patient to the session directory, keyed by patient index.

### Server-configurable rules

Both the OHIP codes and the rule engine accept `Option<&BillingData>` — when the profile service supplies a billing override via `PUT /config/billing`, it replaces the compiled defaults at runtime. See ADR-0023.

### Context toggles

The Settings drawer exposes billing context that modifies Stage 2 output:

- Visit setting (office / hospital / long-term care)
- Hospital-based (changes applicable premiums)
- After-hours (set automatically for clear cases, user-overridable)
- Referral present
- K013 yearly cap exhausted (forces K013A→K033A overflow)
- Patient age bracket — auto-populated from `patient_dob` when vision extraction succeeds

### UI

`src/components/billing/BillingTab.tsx` renders the suggestion list with inline edit, diagnostic code search, and a "Confirm" button. `DailySummaryView` and `MonthlySummaryView` aggregate billings for end-of-day review. `CapProgressBar` visualizes Q310–Q313 daily and 28-day totals.

## Consequences

### Positive

- **Auditable**: Every suggested code has a deterministic reasoning path; the LLM never "decides" the final code.
- **Hallucination-resistant**: Enum-only LLM outputs mean a new code can't be invented — only an existing enum can be mis-applied.
- **Updatable**: Schedule of Benefits changes propagate via a server config push (no app rebuild for rule tweaks) or a code edit + new release (for structural changes).
- **Multi-patient-aware**: Fits the continuous-mode pipeline naturally.
- **Age-aware**: DOB from vision flows into billing context automatically.

### Negative

- **LLM latency doubles for SOAP-plus-billing** — each encounter pays ~5–10s extra for the feature extraction call. Acceptable because billing is a post-SOAP step not on the critical path of the next encounter.
- **Cap tracking is per-patient** — requires looking up the patient's prior K013A usage. Currently simplified to "this session's duration"; longitudinal cap tracking is future work.
- **The enum in `clinical_features.rs` must stay in sync with the rule engine's match arms** — adding a new `ConditionType` without updating `rule_engine.rs` silently does nothing. A test pins every enum variant against at least one rule-engine arm.

### Risks

- **Schedule of Benefits drift**: OHIP updates the SOB periodically. Policy is to update `ohip_codes.rs` + test + comment header together; CI fails if the count-pinning test disagrees.
- **Diagnostic cross-validation false negatives** (mitigated Apr 16 2026): The Apr 16 Room 6 forensic audit showed the literal-word cross-validation was rejecting 7/10 correct LLM suggestions on a normal clinic day, because the LLM writes clinical-narrative language ("knee and back pain") while OHIP descriptions use formal terms ("Lumbar strain, lumbago, coccydynia") — zero words overlap → LLM suggestion rejected → text-match then matched on a secondary comorbidity (e.g., "atrial fibrillation" → 427 cardiac) instead of the primary musculoskeletal complaint. The confidence-tiered policy above now trusts the LLM when it reports high confidence (empirically always ≥0.90 today) and reserves cross-validation for the mid-confidence band. Net effect on a normal day: recovers ~7/10 correct dx codes; edge case regression on ~1/10 where text-match happened to improve on an LLM suggestion (e.g., LLM 311 depression → text-match 300 anxiety for an anxiety visit).

## References

- `tauri-app/src-tauri/src/billing/` — implementation
- `tauri-app/src-tauri/src/commands/billing.rs` — 9 Tauri commands
- `tauri-app/src/components/billing/` — UI
- ADR-0023: Server-Configurable Data (Phase 1) — how billing rules are overridden at runtime
- `CODE_REVIEW_FINDINGS.md` — April 2026 audit that added the epidural/nerve block codes

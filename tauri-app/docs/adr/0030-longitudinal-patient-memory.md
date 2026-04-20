# ADR-0030: Longitudinal Patient Memory — Confirm-and-Dual-Write

## Status

Accepted (Apr 2026, v0.10.46+)

## Context

Continuous-mode encounters land in the local archive + profile-service session store keyed by `(physician_id, date, session_id)`. They have never reached the Medplum FHIR EMR because continuous mode's vision-extracted patient name isn't trustworthy enough to auto-push to a record-of-truth (the Apr 20 2026 Room 2 Shelley/Richard mislabel is the canonical failure mode). Meanwhile session mode *does* upload to Medplum via the existing `medplum_quick_sync` command, but that's a one-shot flow that doesn't survive across visits.

Three consequences shape this ADR:

1. **No longitudinal record per patient.** Each session exists in isolation. Future sessions can't inject "last visit's SOAP + active problems" into the SOAP prompt when the same patient returns.
2. **Clinician knowledge doesn't reach the source of truth.** When the clinician looks at the History Window and types a corrected name + DOB, only the local archive + profile-service metadata hear about it. Medplum never learns.
3. **Replay / simulation / debugging tools have to cross two stores.** Patient identity lives nowhere coherent — some records in Medplum (from session mode), some implicit via `patient_name` / `patient_dob` fields on profile-service session metadata, nothing cross-session.

## Decision

Add a clinician-triggered **confirm-and-dual-write** flow:

- New Tauri command `confirm_session_patient(session_id, date, patient_name, patient_dob, soap_note?, transcript?, session_started_at, session_duration_ms)` runs three stores in order: local archive → Medplum (if authenticated) → profile-service patient index. Returns per-store status to the UI.
- New frontend dialog `ConfirmPatientDialog` in the History Window, reachable from `CleanupActionBar` when a single single-patient session is selected. Prefills name + DOB from the session's vision-extracted metadata; DOB is required.
- New profile-service resource: `PatientRecord`, keyed on `(physician_id, name_normalized, dob)`. Idempotent confirm — repeated calls append `session_id` to the record's session list. Persisted to `patients.json` via atomic write.
- New Medplum method `sync_continuous_session(name, dob, soap, transcript, started_at, duration)` that orchestrates upsert Patient (search → create-or-match on name+DOB) → create Encounter → attach SOAP DocumentReference (LOINC 11506-3) → attach transcript DocumentReference (LOINC 75476-2) → complete Encounter with the real visit period.

**Dual-write is the contract.** Both stores carry the same `medplum_patient_id` as the canonical cross-reference. When Medplum is unreachable, profile-service writes a UUID fallback and reconciles to the real Medplum FHIR ID on the next successful sync.

### What's NOT in this ADR

- **Auto-sync on continuous-mode encounter completion.** Vision alone doesn't warrant EMR writes. Confirmation is the trust boundary. Revisit only with evidence that clinicians systematically confirm every session.
- **Prior-SOAP injection into the SOAP prompt.** The read-side API (`fetch_encounters_for_patient` in Medplum, `GET /physicians/:id/patients` in profile-service) is delivered with the write-side so the follow-up PR has no blocker. The actual prompt wiring waits until enough confirmed patients accumulate to validate against.
- **Multi-patient session confirmation.** Sessions with `patient_labels.json` (couples/family visits) are gated out of the UI. Follow-up could let each sub-patient be confirmed independently.

### Idempotency rules

- **Profile-service store.** `(physician_id, normalize_patient_name(name), dob)` is the primary key. Hit → append session_id (deduped), refresh `medplum_patient_id` if supplied. Miss → create new record.
- **Medplum.** `upsert_patient_by_name_dob` searches with `name:contains={given} {family}`, filters to exact `birthDate == dob`, exact normalized name match. Hit → reuse existing `Patient`. Miss → POST a new `Patient` tagged `meta.tag = "confirmed-patient"` for source tracking.

### Name normalization

Tauri-side `patient_name_tracker::normalize_patient_name` handles `"Last, First Middle"` → `"First Middle Last"` + title-casing. The profile-service store duplicates the same function in `store/patients.rs::normalize_patient_name`, with a parity test (`normalization_parity_with_tauri_client`) asserting byte-equivalence for a known input corpus. A cross-crate shared utility was considered and rejected — the function is ~15 lines and the parity test catches divergence at CI time.

### Failure modes

| Failure | Behavior |
|---|---|
| Medplum unauthenticated | Step B skipped cleanly; profile-service still fires. `medplum_patient_id` stays None; can be reconciled later. |
| Medplum network error | Step B records error; profile-service still fires. UI shows `·` on the Medplum status line and the error message in the details dropdown. |
| Profile-service error | Step C records error; local archive already wrote. UI shows per-store status; clinician can retry. |
| Medplum sub-doc upload fails | `sync_continuous_session` returns success on Patient + Encounter creation even if SOAP/transcript doc attach failed. Errors bubble up as non-fatal warnings. |
| Both remote stores fail | Local archive has the truth. User can retry. |

### Metadata schema changes

`ArchiveMetadata` gains two optional fields (both `#[serde(default, skip_serializing_if = "Option::is_none")]`):

- `patient_confirmed_at: Option<String>` — RFC3339 timestamp of the confirmation
- `medplum_patient_id: Option<String>` — canonical Medplum FHIR ID for cross-referencing

Profile-service `patch_metadata` already accepts untyped JSON merge, so no server-side schema change is required for session metadata.

### Event + audit log

`confirm_patient_begin` → per-step success/failure log lines → `confirm_patient_complete` with counts. Structured tracing fields (`event`, `component`) match the `activity_log.rs` convention. PHI-safe: patient names never land in tracing spans; `truncate_error_body()` applied to all Medplum HTTP error bodies.

## Consequences

**Enabled:**
- Clinician knowledge (confirmed name + DOB) reaches the EMR without automatic vision-based writes.
- Replay / simulation / debugging tools can resolve a patient from either store using the shared `medplum_patient_id`.
- The read-side API `fetch_encounters_for_patient` + `GET /patients?name=&dob=` unblocks the follow-up SOAP-context feature.

**Not yet enabled (by design):**
- Auto-sync from continuous mode.
- Prior-SOAP injection into LLM context.
- Multi-patient session confirmation.
- Duplicate-Patient reconciliation in Medplum when the clinician enters a DOB typo.

**Cost:**
- One extra file written per confirmation (`patients.json`, append-write mode).
- One Medplum round-trip per confirmation (~1-3 seconds; runs serial, not in the UI hot path).
- No impact on continuous-mode hot paths — confirmation happens asynchronously from the History Window.

## References

- Frontend: `src/components/ConfirmPatientDialog.tsx`, `src/components/cleanup/CleanupActionBar.tsx` (new button), `src/components/HistoryWindow.tsx` (dialog wiring)
- Tauri command: `src-tauri/src/commands/archive.rs::confirm_session_patient`
- Medplum: `src-tauri/src/medplum.rs::{upsert_patient_by_name_dob, sync_continuous_session, fetch_encounters_for_patient}`
- Profile-service: `profile-service/src/store/patients.rs`, `profile-service/src/routes/patients.rs`
- Types: `PatientRecord`, `ConfirmPatientRequest`, `ConfirmPatientResult` mirrored in both backends + `src/types/index.ts`
- Parity test: `profile-service/src/store/patients.rs::normalization_parity_with_tauri_client`
- Related: ADR-0008 (Medplum EMR integration — original session-mode flow), ADR-0012 (Multi-patient SOAP — out of scope here), Apr 20 2026 Room 2 Shelley mislabel (memory file `project_forward_merge.md`)

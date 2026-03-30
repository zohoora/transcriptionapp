# Multi-Patient Sidebar Flattening

**Date:** 2026-03-30
**Status:** Design approved

## Problem

When a session contains multiple detected patients (e.g., a phone call where both Cheryl and Paul are discussed), the history sidebar shows one entry with tabs in the detail view. Physicians expect each patient to appear as a separate sidebar entry — the tab-based display is confusing and doesn't match the mental model of "one row per patient."

## Design

### Approach: Frontend-only flattening with backend enrichment

The session archive data model stays unchanged (one session = one directory). The backend enriches the session summary with patient labels. The frontend expands multi-patient sessions into separate sidebar rows at render time.

### Backend Changes

**`LocalArchiveSummary` (Rust + TypeScript):** Add two fields:

- `patient_count: Option<u32>` — from `metadata.json` (already stored by `save_multi_patient_soap`)
- `patient_labels: Option<Vec<String>>` — from `patient_labels.json` in the session directory

Populated during `get_local_sessions_by_date` when listing sessions. For single-patient sessions, both fields are `None`.

### Sidebar Rendering

When `patient_labels` has >1 entries, render one row per patient instead of one row for the session:

- Each row shows: time, encounter number, **patient name** (from label), badges
- Subtle visual grouping: thin shared left-border accent or hairline connector between entries from the same encounter — just enough to show they're related
- Selection state tracked as `(session_id, patient_index)` instead of just `session_id`

Example for a 2-patient encounter:

```
12:11 PM
│ Encounter #1 — Cheryl        AUTO-CHARTED  SOAP
│ Encounter #1 — Paul           AUTO-CHARTED  SOAP
```

Single-patient sessions render exactly as today (no change).

### Detail View

When a patient-specific entry is selected:

- **SOAP tab:** Show only that patient's SOAP note. No patient tabs. The header can show "Patient 1 of 2" as context.
- **Transcript tab:** Show the full shared transcript with a label: "Shared transcript (2 patients in this encounter)".
- **Feedback:** Scoped to the selected patient index (existing `PatientContentFeedback.patientIndex` mechanism).

### SOAP Regeneration

When "Regenerate SOAP" is clicked for a flattened patient entry:

- The LLM call is scoped to that single patient. The prompt includes the full transcript but instructs the model to generate a SOAP for **only** the specified patient (identified by label/speaker).
- This prevents re-detection of multiple patients — the physician already validated the patient split by selecting a specific entry.

### Cleanup Tools

**Delete:**
- Deleting a patient entry removes that patient's SOAP file (`soap_patient_{index}.txt`) and updates `patient_labels.json`.
- If only one patient remains after deletion, the session reverts to a single-patient session (rename `soap_patient_{index}.txt` → `soap_note.txt`, remove `patient_labels.json`, clear `patient_count` from metadata).
- If no patients remain, delete the entire session directory.

**Rename:**
- Updates the patient label in `patient_labels.json` for that specific patient.
- Does not modify the session-level `patient_name` in metadata.

**Split:**
- Not applicable to patient-level entries. Split operates on the shared transcript.
- The split action is available only when viewing the transcript, not from a patient-specific SOAP view.

**Merge (cross-session):**
- Merging a patient entry from one session with a different session works like today's merge.
- Only the selected patient's SOAP is carried into the merged result.

**Merge (same-session patients — patient detection correction):**

This is a correction to the LLM's patient detection, not a simple concatenation. When the physician selects two or more patient entries from the same encounter and clicks merge:

1. Show a confirmation dialog explaining: "These patients were detected in the same encounter. Merging will combine them into one patient note."
2. The LLM call receives:
   - The full original transcript
   - All detected patient labels and their current SOAP notes
   - Which patients the physician selected to merge (e.g., "The physician has determined that Patient 2 (Cheryl) and Patient 4 (Jim) are actually the same patient")
   - Which patients remain separate (the unselected ones)
   - Instruction: regenerate a single SOAP for the merged patients, incorporating clinical details from both original notes
3. The result replaces the merged patients with one entry. Remaining patients keep their original SOAPs.
4. `patient_labels.json` and individual SOAP files are updated accordingly. If the merge reduces to a single patient, revert to single-patient session format.

This handles the family visit correction case: 5 detected patients → physician merges 2 → result is 4 patients with corrected SOAPs.

### Data Flow

```
Archive on disk (unchanged):
  session_dir/
    metadata.json        ← has patient_count
    patient_labels.json  ← has [{index, label}, ...]
    soap_patient_1.txt
    soap_patient_2.txt
    transcript.txt       ← shared

Backend summary loading:
  get_local_sessions_by_date()
    → reads metadata.json for patient_count
    → reads patient_labels.json for labels
    → returns LocalArchiveSummary with patient_count + patient_labels

Frontend flattening:
  sortedSessions.flatMap(session =>
    session.patient_labels?.length > 1
      ? session.patient_labels.map((label, i) => virtualEntry(session, i, label))
      : [session]
  )

Selection state:
  selectedSession: { session_id: string, patientIndex: number | null }
```

### Files to Modify

| File | Change |
|------|--------|
| `src-tauri/src/local_archive.rs` | Add `patient_count`, `patient_labels` to summary; load from disk |
| `src-tauri/src/commands/archive.rs` | Pass new fields through to frontend |
| `src/types/index.ts` | Add fields to `LocalArchiveSummary` |
| `src/components/HistoryWindow.tsx` | Flatten sidebar, patient-scoped detail view, selection as tuple |
| `src/styles.css` | Subtle grouping indicator for same-encounter entries |
| `src-tauri/src/commands/ollama.rs` | Single-patient SOAP regeneration param |
| `src-tauri/src/llm_client.rs` | Single-patient regeneration prompt variant |
| `src/components/cleanup/MergeConfirmDialog.tsx` | Same-session patient merge dialog + LLM correction prompt |
| `src-tauri/src/local_archive.rs` | Delete/rename per-patient, single-patient revert logic |

### Out of Scope

- Changing the archive directory structure (stays one dir per session)
- Changing how multi-patient detection works during continuous mode
- Server sync changes (profile service sees the same session structure)

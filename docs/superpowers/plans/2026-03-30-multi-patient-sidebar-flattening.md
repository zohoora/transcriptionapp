# Multi-Patient Sidebar Flattening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Flatten multi-patient sessions into separate sidebar entries per patient in the history view, replacing the current tab-based display.

**Architecture:** Backend enriches session summaries with `patient_count` and `patient_labels`. Frontend flatMaps multi-patient sessions into virtual per-patient rows. Selection tracked as `(session_id, patientIndex)`. Detail view shows single-patient SOAP with shared transcript. Cleanup tools operate per-patient.

**Tech Stack:** Rust (Tauri backend), React + TypeScript (frontend), CSS

---

### Task 1: Backend — Add patient labels to ArchiveSummary

**Files:**
- Modify: `tauri-app/src-tauri/src/local_archive.rs:157-181` (ArchiveSummary struct)
- Modify: `tauri-app/src-tauri/src/local_archive.rs:573-589` (summary construction in list_sessions_by_date)

- [ ] **Step 1: Add fields to ArchiveSummary struct**

In `local_archive.rs`, add two fields after `room_name` (line 180):

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_labels: Option<Vec<String>>,
}
```

- [ ] **Step 2: Populate fields in list_sessions_by_date**

In `list_sessions_by_date`, after reading metadata (line 570), add patient_labels.json loading before the `sessions.push`:

```rust
        // Load patient labels for multi-patient sessions
        let patient_count = metadata.patient_count;
        let patient_labels = if patient_count.unwrap_or(0) > 1 {
            let labels_path = session_dir.join("patient_labels.json");
            if labels_path.exists() {
                match fs::read_to_string(&labels_path) {
                    Ok(json) => {
                        match serde_json::from_str::<Vec<serde_json::Value>>(&json) {
                            Ok(entries) => {
                                let labels: Vec<String> = entries.iter()
                                    .map(|e| e["label"].as_str().unwrap_or("Patient").to_string())
                                    .collect();
                                if labels.len() > 1 { Some(labels) } else { None }
                            }
                            Err(_) => None,
                        }
                    }
                    Err(_) => None,
                }
            } else {
                None
            }
        } else {
            None
        };
```

Then add the fields to the `ArchiveSummary` construction (after `room_name`):

```rust
            room_name: metadata.room_name,
            patient_count,
            patient_labels,
```

- [ ] **Step 3: Run cargo check**

Run: `cd tauri-app/src-tauri && cargo check`
Expected: Compiles clean (new fields have serde defaults so existing tests still pass)

- [ ] **Step 4: Run Rust tests**

Run: `cd tauri-app/src-tauri && cargo test archive`
Expected: All existing archive tests pass

- [ ] **Step 5: Commit**

```bash
git add tauri-app/src-tauri/src/local_archive.rs
git commit -m "feat: add patient_count and patient_labels to ArchiveSummary"
```

---

### Task 2: Frontend types — Add patient fields to LocalArchiveSummary

**Files:**
- Modify: `tauri-app/src/types/index.ts:683-699`

- [ ] **Step 1: Add fields to LocalArchiveSummary interface**

After `room_name` (line 698), add:

```typescript
  patient_count?: number | null;
  patient_labels?: string[] | null;
```

- [ ] **Step 2: Run TypeScript check**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: Clean (new optional fields don't break existing code)

- [ ] **Step 3: Commit**

```bash
git add tauri-app/src/types/index.ts
git commit -m "feat: add patient_count and patient_labels to LocalArchiveSummary type"
```

---

### Task 3: Sidebar flattening — Render per-patient rows

**Files:**
- Modify: `tauri-app/src/components/HistoryWindow.tsx`

- [ ] **Step 1: Add FlattenedSession type and selection state**

Near the top of the file (after existing type imports), add a type for flattened entries. Replace the `selectedSessionId` state (line 71) with a tuple:

```typescript
/** A sidebar row — either a normal session or one patient from a multi-patient session */
interface FlattenedSession extends LocalArchiveSummary {
  /** Index into patient_labels for multi-patient entries; null for single-patient */
  patientIndex: number | null;
  /** Display name override for multi-patient entries */
  flattenedPatientName: string | null;
  /** Whether this is the first entry in a multi-patient group (for visual grouping) */
  isGroupFirst: boolean;
  /** Whether this is the last entry in a multi-patient group */
  isGroupLast: boolean;
}
```

Replace the `selectedSessionId` state:
```typescript
const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
const [selectedPatientIndex, setSelectedPatientIndex] = useState<number | null>(null);
```

- [ ] **Step 2: Add flattening logic**

Add a `useMemo` that flattens `sortedSessions` into `flattenedSessions`:

```typescript
const flattenedSessions: FlattenedSession[] = useMemo(() => {
  return sortedSessions.flatMap((session) => {
    const labels = session.patient_labels;
    if (labels && labels.length > 1) {
      return labels.map((label, i) => ({
        ...session,
        patientIndex: i,
        flattenedPatientName: label,
        isGroupFirst: i === 0,
        isGroupLast: i === labels.length - 1,
      }));
    }
    return [{
      ...session,
      patientIndex: null,
      flattenedPatientName: null,
      isGroupFirst: false,
      isGroupLast: false,
    }];
  });
}, [sortedSessions]);
```

- [ ] **Step 3: Update sidebar rendering**

Replace the `sortedSessions.map` in the session list with `flattenedSessions.map`. Update the key and click handler:

```typescript
{flattenedSessions.map((entry) => {
  const key = entry.patientIndex !== null
    ? `${entry.session_id}:${entry.patientIndex}`
    : entry.session_id;
  const isSelected = selectedSessionId === entry.session_id
    && selectedPatientIndex === entry.patientIndex;
  const groupClass = entry.patientIndex !== null
    ? `multi-patient-group${entry.isGroupFirst ? ' group-first' : ''}${entry.isGroupLast ? ' group-last' : ''}`
    : '';

  return (
    <div key={key} className={`session-item${isSelected ? ' selected' : ''} ${groupClass}`}>
      {/* ... existing checkbox for cleanup mode ... */}
      <button
        className="session-item-body"
        onClick={() => {
          setSelectedPatientIndex(entry.patientIndex);
          fetchSessionDetails(entry, entry.patientIndex);
        }}
      >
        <div className="session-info">
          <span className="session-time">
            {formatLocalTime(entry.started_at || entry.date)}
          </span>
          <span className="session-name">
            {entry.charting_mode === 'continuous' && entry.encounter_number != null
              ? `Encounter #${entry.encounter_number}${entry.flattenedPatientName ? ` — ${entry.flattenedPatientName}` : (entry.patient_name ? ` — ${entry.patient_name}` : '')}`
              : entry.word_count > 0
                ? `${entry.word_count} words`
                : 'Scribe Session'}
          </span>
        </div>
        {/* ... existing badges ... */}
      </button>
    </div>
  );
})}
```

- [ ] **Step 4: Update fetchSessionDetails to accept patientIndex**

Modify the `fetchSessionDetails` function to accept and use `patientIndex`:

```typescript
const fetchSessionDetails = useCallback(async (session: LocalArchiveSummary, patientIndex: number | null = null) => {
  // ... existing fetch logic ...
  // After setting soapResult, auto-select the patient:
  if (patientIndex !== null) {
    setActivePatient(patientIndex);
  } else {
    setActivePatient(0);
  }
  setSelectedSessionId(session.session_id);
}, [/* existing deps */]);
```

- [ ] **Step 5: Run TypeScript check**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: Clean

- [ ] **Step 6: Commit**

```bash
git add tauri-app/src/components/HistoryWindow.tsx
git commit -m "feat: flatten multi-patient sessions into separate sidebar rows"
```

---

### Task 4: Detail view — Single-patient SOAP display

**Files:**
- Modify: `tauri-app/src/components/HistoryWindow.tsx`

- [ ] **Step 1: Hide multi-patient tabs when viewing a flattened entry**

Replace the multi-patient tabs section (lines 1115-1139) with conditional logic:

```typescript
{/* Multi-patient info — only show tabs when NOT viewing a flattened patient entry */}
{isMultiPatient && selectedPatientIndex === null && (
  <div className="multi-patient-soap">
    {/* ... existing tab UI unchanged ... */}
  </div>
)}
{/* Single patient context when viewing a flattened entry */}
{isMultiPatient && selectedPatientIndex !== null && (
  <div className="multi-patient-soap">
    <div className="patient-info">
      <span className="physician-label">
        Physician: {soapResult.physician_speaker || 'Not identified'}
      </span>
      <span className="patient-count">
        Patient {selectedPatientIndex + 1} of {soapResult.notes.length}
      </span>
    </div>
  </div>
)}
```

- [ ] **Step 2: Add shared transcript label**

In the transcript display section, when viewing a flattened multi-patient entry, add a label:

```typescript
{selectedPatientIndex !== null && isMultiPatient && (
  <div className="shared-transcript-label">
    Shared transcript ({soapResult?.notes.length} patients in this encounter)
  </div>
)}
```

- [ ] **Step 3: Run TypeScript check**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: Clean

- [ ] **Step 4: Commit**

```bash
git add tauri-app/src/components/HistoryWindow.tsx
git commit -m "feat: single-patient SOAP display for flattened entries"
```

---

### Task 5: CSS — Subtle grouping indicator

**Files:**
- Modify: `tauri-app/src/styles.css`

- [ ] **Step 1: Add multi-patient group styles**

Add after the existing `.session-item` styles:

```css
/* Multi-patient sidebar grouping — subtle left accent connecting related entries */
.session-item.multi-patient-group {
  border-left: 2px solid var(--accent-idle);
  margin-left: 4px;
}

.session-item.multi-patient-group.group-first {
  border-top-left-radius: 4px;
  margin-top: 2px;
}

.session-item.multi-patient-group.group-last {
  border-bottom-left-radius: 4px;
  margin-bottom: 2px;
}

/* Remove gap between grouped entries */
.session-item.multi-patient-group + .session-item.multi-patient-group {
  margin-top: 0;
}

/* Shared transcript label */
.shared-transcript-label {
  font-size: 12px;
  color: var(--text-tertiary);
  padding: 6px 12px;
  background: var(--bg-secondary);
  border-radius: 4px;
  margin-bottom: 8px;
}
```

- [ ] **Step 2: Commit**

```bash
git add tauri-app/src/styles.css
git commit -m "feat: subtle grouping indicator for multi-patient sidebar entries"
```

---

### Task 6: Single-patient SOAP regeneration

**Files:**
- Modify: `tauri-app/src-tauri/src/llm_client.rs` (add single-patient prompt builder)
- Modify: `tauri-app/src-tauri/src/commands/ollama.rs` (add patient_label parameter)
- Modify: `tauri-app/src/components/HistoryWindow.tsx` (pass patient context to regeneration)

- [ ] **Step 1: Add single-patient SOAP prompt in llm_client.rs**

Find `build_per_patient_user_content` (or the multi-patient SOAP generation). Add a new public function:

```rust
/// Build a SOAP prompt scoped to a single patient within a multi-patient transcript.
/// Used when the physician regenerates SOAP for one specific patient.
pub fn build_single_patient_soap_prompt(
    transcript: &str,
    patient_label: &str,
    options: &SoapOptions,
) -> String {
    let base = build_simple_soap_prompt(transcript, options);
    format!(
        "{}\n\nIMPORTANT: This transcript contains multiple patients. \
         Generate a SOAP note ONLY for the patient identified as \"{}\". \
         Ignore clinical content belonging to other patients in the transcript.",
        base, patient_label
    )
}
```

- [ ] **Step 2: Add patient_label parameter to generate_soap_note command**

In `commands/ollama.rs`, modify the `generate_soap_note` command to accept an optional `patient_label`:

```rust
#[tauri::command]
pub async fn generate_soap_note(
    transcript: String,
    detail_level: Option<u32>,
    format: Option<String>,
    patient_label: Option<String>,  // NEW: scope to single patient
    state: tauri::State<'_, AppState>,
) -> Result<String, CommandError> {
```

In the function body, when `patient_label` is `Some`, use `build_single_patient_soap_prompt` instead of the default prompt builder.

- [ ] **Step 3: Update frontend regeneration to pass patient label**

In `HistoryWindow.tsx`, modify the SOAP regeneration handler. When `selectedPatientIndex !== null`, pass the patient label:

```typescript
const patientLabel = selectedPatientIndex !== null && soapResult
  ? soapResult.notes[selectedPatientIndex]?.patient_label ?? null
  : null;

const result = await invoke<string>('generate_soap_note', {
  transcript: selectedSession.transcript,
  detailLevel: /* existing */,
  format: /* existing */,
  patientLabel: patientLabel,
});
```

After receiving the result, save it as the per-patient SOAP (update `soap_patient_{index}.txt`), not the session-level `soap_note.txt`.

- [ ] **Step 4: Run cargo check and tsc**

Run: `cd tauri-app/src-tauri && cargo check && cd .. && npx tsc --noEmit`
Expected: Clean

- [ ] **Step 5: Commit**

```bash
git add tauri-app/src-tauri/src/llm_client.rs tauri-app/src-tauri/src/commands/ollama.rs tauri-app/src/components/HistoryWindow.tsx
git commit -m "feat: single-patient SOAP regeneration for flattened entries"
```

---

### Task 7: Per-patient delete

**Files:**
- Modify: `tauri-app/src-tauri/src/local_archive.rs` (add `delete_patient_from_session`)
- Modify: `tauri-app/src-tauri/src/commands/archive.rs` (add command)
- Modify: `tauri-app/src/components/HistoryWindow.tsx` (route delete to per-patient when applicable)

- [ ] **Step 1: Add delete_patient_from_session in local_archive.rs**

```rust
/// Delete a single patient's SOAP from a multi-patient session.
/// If only one patient remains, reverts to single-patient format.
/// If no patients remain, deletes the entire session.
pub fn delete_patient_from_session(
    session_id: &str,
    date_str: &str,
    patient_index: u32,
) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    if !session_dir.exists() {
        return Err(format!("Session not found: {}", session_id));
    }

    let labels_path = session_dir.join("patient_labels.json");
    if !labels_path.exists() {
        return Err("Not a multi-patient session".to_string());
    }

    // Read current labels
    let labels_json = fs::read_to_string(&labels_path)
        .map_err(|e| format!("Failed to read patient_labels.json: {}", e))?;
    let mut labels: Vec<serde_json::Value> = serde_json::from_str(&labels_json)
        .map_err(|e| format!("Failed to parse patient_labels.json: {}", e))?;

    // Remove the target patient's SOAP file
    let soap_file = session_dir.join(format!("soap_patient_{}.txt", patient_index));
    if soap_file.exists() {
        fs::remove_file(&soap_file)
            .map_err(|e| format!("Failed to delete patient SOAP: {}", e))?;
    }

    // Remove from labels array
    labels.retain(|l| l["index"].as_u64().unwrap_or(0) as u32 != patient_index);

    if labels.is_empty() {
        // No patients left — delete entire session
        return delete_session(session_id, date_str);
    }

    if labels.len() == 1 {
        // One patient left — revert to single-patient format
        let remaining_index = labels[0]["index"].as_u64().unwrap_or(1) as u32;
        let remaining_soap = session_dir.join(format!("soap_patient_{}.txt", remaining_index));
        let single_soap = session_dir.join("soap_note.txt");
        if remaining_soap.exists() {
            fs::rename(&remaining_soap, &single_soap)
                .map_err(|e| format!("Failed to rename SOAP file: {}", e))?;
        }
        let _ = fs::remove_file(&labels_path);

        // Update metadata: clear patient_count
        update_metadata_field(session_id, date_str, |m| {
            m.patient_count = None;
        })?;
    } else {
        // Multiple patients still remain — update labels file
        let updated_json = serde_json::to_string_pretty(&labels)
            .map_err(|e| format!("Failed to serialize labels: {}", e))?;
        fs::write(&labels_path, updated_json)
            .map_err(|e| format!("Failed to write labels: {}", e))?;

        // Update metadata patient_count
        let new_count = labels.len() as u32;
        update_metadata_field(session_id, date_str, |m| {
            m.patient_count = Some(new_count);
        })?;
    }

    Ok(())
}

/// Helper: read metadata, apply a mutation, write back.
fn update_metadata_field(
    session_id: &str,
    date_str: &str,
    mutate: impl FnOnce(&mut ArchiveMetadata),
) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    let metadata_path = session_dir.join("metadata.json");
    let content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;
    let mut metadata: ArchiveMetadata = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;
    mutate(&mut metadata);
    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
    fs::write(&metadata_path, json)
        .map_err(|e| format!("Failed to write metadata: {}", e))?;
    Ok(())
}
```

- [ ] **Step 2: Add Tauri command in commands/archive.rs**

```rust
#[tauri::command]
pub async fn delete_patient_from_session(
    session_id: String,
    date: String,
    patient_index: u32,
) -> Result<(), CommandError> {
    local_archive::delete_patient_from_session(&session_id, &date, patient_index)?;
    Ok(())
}
```

Register in `lib.rs` invoke_handler.

- [ ] **Step 3: Update frontend delete handler**

In `HistoryWindow.tsx`, when deleting a selected entry with `selectedPatientIndex !== null`:

```typescript
if (selectedPatientIndex !== null) {
  // Per-patient delete
  const patientIndex = soapResult?.notes[selectedPatientIndex]
    ? (details.patientNotes?.[selectedPatientIndex]?.index ?? selectedPatientIndex + 1)
    : selectedPatientIndex + 1;
  await invoke('delete_patient_from_session', {
    sessionId: session.session_id,
    date: session.date,
    patientIndex,
  });
} else {
  // Full session delete (existing logic)
  await invoke('delete_local_session', { sessionId: session.session_id, date: session.date });
}
```

- [ ] **Step 4: Run cargo check and tsc**

Run: `cd tauri-app/src-tauri && cargo check && cd .. && npx tsc --noEmit`
Expected: Clean

- [ ] **Step 5: Commit**

```bash
git add tauri-app/src-tauri/src/local_archive.rs tauri-app/src-tauri/src/commands/archive.rs tauri-app/src-tauri/src/lib.rs tauri-app/src/components/HistoryWindow.tsx
git commit -m "feat: per-patient delete for multi-patient sessions"
```

---

### Task 8: Per-patient rename

**Files:**
- Modify: `tauri-app/src-tauri/src/local_archive.rs` (add `rename_patient_label`)
- Modify: `tauri-app/src-tauri/src/commands/archive.rs` (add command)
- Modify: `tauri-app/src/components/HistoryWindow.tsx` (route rename to per-patient)

- [ ] **Step 1: Add rename_patient_label in local_archive.rs**

```rust
/// Rename a patient label in a multi-patient session.
pub fn rename_patient_label(
    session_id: &str,
    date_str: &str,
    patient_index: u32,
    new_label: &str,
) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    let labels_path = session_dir.join("patient_labels.json");
    if !labels_path.exists() {
        return Err("Not a multi-patient session".to_string());
    }

    let labels_json = fs::read_to_string(&labels_path)
        .map_err(|e| format!("Failed to read labels: {}", e))?;
    let mut labels: Vec<serde_json::Value> = serde_json::from_str(&labels_json)
        .map_err(|e| format!("Failed to parse labels: {}", e))?;

    for entry in &mut labels {
        if entry["index"].as_u64().unwrap_or(0) as u32 == patient_index {
            entry["label"] = serde_json::json!(new_label);
            break;
        }
    }

    let updated = serde_json::to_string_pretty(&labels)
        .map_err(|e| format!("Failed to serialize labels: {}", e))?;
    fs::write(&labels_path, updated)
        .map_err(|e| format!("Failed to write labels: {}", e))?;

    Ok(())
}
```

- [ ] **Step 2: Add Tauri command and register**

```rust
#[tauri::command]
pub async fn rename_patient_label(
    session_id: String,
    date: String,
    patient_index: u32,
    new_label: String,
) -> Result<(), CommandError> {
    local_archive::rename_patient_label(&session_id, &date, patient_index, &new_label)?;
    Ok(())
}
```

Register in `lib.rs`.

- [ ] **Step 3: Update frontend rename handler**

When the rename dialog is confirmed for a flattened entry, call the per-patient rename:

```typescript
if (selectedPatientIndex !== null) {
  await invoke('rename_patient_label', {
    sessionId: session.session_id,
    date: session.date,
    patientIndex: /* archive index from patientNotes */,
    newLabel: newName,
  });
} else {
  // Existing session-level rename
  await invoke('update_session_patient_name', { ... });
}
```

- [ ] **Step 4: Run cargo check and tsc**

Run: `cd tauri-app/src-tauri && cargo check && cd .. && npx tsc --noEmit`
Expected: Clean

- [ ] **Step 5: Commit**

```bash
git add tauri-app/src-tauri/src/local_archive.rs tauri-app/src-tauri/src/commands/archive.rs tauri-app/src-tauri/src/lib.rs tauri-app/src/components/HistoryWindow.tsx
git commit -m "feat: per-patient rename for multi-patient sessions"
```

---

### Task 9: Same-session patient merge (detection correction)

**Files:**
- Modify: `tauri-app/src/components/cleanup/MergeConfirmDialog.tsx` (detect same-session merge, show correction dialog)
- Modify: `tauri-app/src-tauri/src/llm_client.rs` (add patient merge correction prompt)
- Modify: `tauri-app/src-tauri/src/commands/ollama.rs` (add merge_patient_soaps command)
- Modify: `tauri-app/src-tauri/src/local_archive.rs` (add merge_patients_in_session)

- [ ] **Step 1: Add merge_patients_in_session in local_archive.rs**

```rust
/// Merge multiple detected patients into one within the same session.
/// Replaces the merged patients' SOAP files with a single regenerated one.
/// Keeps remaining patients' SOAPs unchanged.
pub fn merge_patients_in_session(
    session_id: &str,
    date_str: &str,
    merged_indices: &[u32],
    new_label: &str,
    new_soap_content: &str,
) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    let labels_path = session_dir.join("patient_labels.json");
    if !labels_path.exists() {
        return Err("Not a multi-patient session".to_string());
    }

    let labels_json = fs::read_to_string(&labels_path)
        .map_err(|e| format!("Failed to read labels: {}", e))?;
    let mut labels: Vec<serde_json::Value> = serde_json::from_str(&labels_json)
        .map_err(|e| format!("Failed to parse labels: {}", e))?;

    // Delete SOAP files for merged patients
    for &idx in merged_indices {
        let soap_file = session_dir.join(format!("soap_patient_{}.txt", idx));
        if soap_file.exists() {
            let _ = fs::remove_file(&soap_file);
        }
    }

    // Remove merged entries from labels, keep the first merged index as the survivor
    let survivor_index = merged_indices[0];
    labels.retain(|l| {
        let idx = l["index"].as_u64().unwrap_or(0) as u32;
        !merged_indices.contains(&idx) || idx == survivor_index
    });

    // Update survivor label
    for entry in &mut labels {
        if entry["index"].as_u64().unwrap_or(0) as u32 == survivor_index {
            entry["label"] = serde_json::json!(new_label);
        }
    }

    // Write merged SOAP to survivor's file
    let merged_soap_path = session_dir.join(format!("soap_patient_{}.txt", survivor_index));
    fs::write(&merged_soap_path, new_soap_content)
        .map_err(|e| format!("Failed to write merged SOAP: {}", e))?;

    if labels.len() == 1 {
        // Revert to single-patient format
        let single_soap = session_dir.join("soap_note.txt");
        fs::rename(&merged_soap_path, &single_soap)
            .map_err(|e| format!("Failed to rename to single SOAP: {}", e))?;
        let _ = fs::remove_file(&labels_path);
        update_metadata_field(session_id, date_str, |m| {
            m.patient_count = None;
            m.patient_name = Some(new_label.to_string());
        })?;
    } else {
        // Update labels file and metadata count
        let updated = serde_json::to_string_pretty(&labels)
            .map_err(|e| format!("Failed to serialize labels: {}", e))?;
        fs::write(&labels_path, updated)
            .map_err(|e| format!("Failed to write labels: {}", e))?;
        update_metadata_field(session_id, date_str, |m| {
            m.patient_count = Some(labels.len() as u32);
        })?;
    }

    Ok(())
}
```

- [ ] **Step 2: Add LLM prompt for patient merge correction in llm_client.rs**

```rust
/// Build a prompt for merging incorrectly split patients within one encounter.
pub fn build_patient_merge_correction_prompt(
    transcript: &str,
    all_patient_labels: &[(u32, String, String)], // (index, label, soap_content)
    merged_indices: &[u32],
    options: &SoapOptions,
) -> (String, String) {
    let mut context = String::new();
    context.push_str("The following patients were detected in this encounter:\n\n");

    for (idx, label, soap) in all_patient_labels {
        let status = if merged_indices.contains(idx) {
            "TO BE MERGED"
        } else {
            "correct, keep separate"
        };
        context.push_str(&format!("--- Patient {} ({}) [{}] ---\n{}\n\n", idx, label, status, soap));
    }

    let merged_names: Vec<&str> = all_patient_labels.iter()
        .filter(|(idx, _, _)| merged_indices.contains(idx))
        .map(|(_, label, _)| label.as_str())
        .collect();

    let system = format!(
        "You are a medical scribe assistant. The physician has reviewed automatically detected \
         patient notes from a multi-patient encounter and determined that the following patients \
         are actually the SAME person and should be merged: {}.\n\n\
         Generate a single unified SOAP note for this patient, incorporating clinical details \
         from all the notes marked TO BE MERGED. Do not include content from patients marked \
         as 'correct, keep separate'.",
        merged_names.join(", ")
    );

    let user = format!(
        "TRANSCRIPT:\n{}\n\nDETECTED PATIENTS AND THEIR CURRENT SOAP NOTES:\n{}\n\n\
         Generate a single merged SOAP note for the patients marked TO BE MERGED.",
        transcript, context
    );

    (system, user)
}
```

- [ ] **Step 3: Add Tauri command for merge_patient_soaps**

In `commands/ollama.rs`:

```rust
#[tauri::command]
pub async fn merge_patient_soaps(
    session_id: String,
    date: String,
    merged_indices: Vec<u32>,
    new_label: String,
    transcript: String,
    all_patients: Vec<serde_json::Value>, // [{index, label, content}]
    detail_level: Option<u32>,
    format: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String, CommandError> {
    // Build correction prompt
    let patient_data: Vec<(u32, String, String)> = all_patients.iter()
        .map(|p| (
            p["index"].as_u64().unwrap_or(0) as u32,
            p["label"].as_str().unwrap_or("Patient").to_string(),
            p["content"].as_str().unwrap_or("").to_string(),
        ))
        .collect();

    let options = SoapOptions {
        detail_level: detail_level.unwrap_or(4),
        format: format.unwrap_or_else(|| "comprehensive".to_string()),
        ..Default::default()
    };

    let (system, user) = build_patient_merge_correction_prompt(
        &transcript, &patient_data, &merged_indices, &options
    );

    // Call LLM
    let client = /* get LLM client from state */;
    let soap_model = /* get model */;
    let merged_soap = client.generate(&soap_model, &system, &user, "patient_merge").await
        .map_err(|e| CommandError::from(format!("Patient merge SOAP generation failed: {}", e)))?;

    // Save to archive
    local_archive::merge_patients_in_session(
        &session_id, &date, &merged_indices, &new_label, &merged_soap
    )?;

    Ok(merged_soap)
}
```

Register in `lib.rs`.

- [ ] **Step 4: Update MergeConfirmDialog for same-session detection**

In `MergeConfirmDialog.tsx`, detect when all selected sessions share the same `session_id` (same-session patient merge):

```typescript
const isSameSessionMerge = sessions.length > 1
  && sessions.every(s => s.session_id === sessions[0].session_id);

// Show different UI for same-session patient merge:
if (isSameSessionMerge) {
  return (
    <div className="merge-dialog">
      <h3>Merge Patient Notes</h3>
      <p>These patients were detected in the same encounter. Merging will combine them into one patient note.</p>
      {/* List the patient labels being merged */}
      <button onClick={handleSameSessionMerge}>Merge</button>
      <button onClick={onCancel}>Cancel</button>
    </div>
  );
}
```

The `handleSameSessionMerge` calls the `merge_patient_soaps` command with the transcript and all patient data.

- [ ] **Step 5: Run cargo check and tsc**

Run: `cd tauri-app/src-tauri && cargo check && cd .. && npx tsc --noEmit`
Expected: Clean

- [ ] **Step 6: Commit**

```bash
git add tauri-app/src-tauri/src/local_archive.rs tauri-app/src-tauri/src/llm_client.rs tauri-app/src-tauri/src/commands/ollama.rs tauri-app/src-tauri/src/lib.rs tauri-app/src/components/cleanup/MergeConfirmDialog.tsx
git commit -m "feat: same-session patient merge with LLM correction prompt"
```

---

### Task 10: Version bump, test, and push

**Files:**
- Modify: `tauri-app/src-tauri/tauri.conf.json`
- Modify: `tauri-app/package.json`

- [ ] **Step 1: Run full test suite**

Run: `cd tauri-app/src-tauri && cargo test && cd .. && pnpm test:run`
Expected: All tests pass

- [ ] **Step 2: Run TypeScript check**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: Clean

- [ ] **Step 3: Bump version to 0.8.0**

Update version in both `tauri.conf.json` and `package.json` from `0.7.6` to `0.8.0`.

- [ ] **Step 4: Commit and tag**

```bash
git add -A
git commit -m "feat: multi-patient sidebar flattening (v0.8.0)

Flatten multi-patient sessions into separate sidebar entries per patient.
Each patient gets their own row with subtle visual grouping. Detail view
shows single-patient SOAP with shared transcript label. Per-patient
delete, rename, and same-session merge with LLM correction prompt."

git tag v0.8.0
git push origin main --tags
```

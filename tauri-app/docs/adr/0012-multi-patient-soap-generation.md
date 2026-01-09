# ADR-0012: Multi-Patient SOAP Note Generation

## Status

Accepted (January 2025)

## Context

Clinical visits often involve multiple patients, especially in:
- **Couples counseling**: Two patients seen together
- **Family visits**: Parent with child, or multiple family members
- **Group appointments**: Multiple patients for vaccinations or health screenings

The original SOAP note generation assumed a single patient per recording session. This created problems:
1. Physicians had to manually separate notes for each patient
2. Information from different patients could be mixed
3. No way to associate the correct SOAP note with each patient in the EMR

We needed to support multi-patient visits (up to 4 patients) while:
- Maintaining backward compatibility with single-patient sessions
- Automatically identifying who is the physician vs patients
- Generating accurate, separated SOAP notes for each patient
- Syncing each patient to their own encounter in Medplum

## Decision

Implement **LLM-based auto-detection** of patients and physician from transcript context, generating separate SOAP notes for each patient in a single LLM call.

### Key Design Choices

1. **No manual speaker mapping**: The LLM analyzes conversation context to determine who is the physician (asks questions, examines, diagnoses) vs patients (describe symptoms, answer questions)

2. **Single LLM call**: All patient SOAP notes are generated in one request for efficiency and context consistency

3. **Speaker 1 is NOT assumed to be physician**: The LLM determines this from conversation content, not speaker order

4. **Dynamic patient count**: Returns 1-4 SOAP notes as detected, backward compatible with single patient

5. **Structured JSON output**: LLM returns JSON with physician identification and patient array for reliable parsing

### Data Structures

```rust
// Backend (Rust)
pub struct MultiPatientSoapResult {
    pub notes: Vec<PatientSoapNote>,
    pub physician_speaker: Option<String>,
    pub generated_at: String,
    pub model_used: String,
}

pub struct PatientSoapNote {
    pub patient_label: String,    // "Patient 1", "Patient 2"
    pub speaker_id: String,       // "Speaker 2", "Speaker 3"
    pub soap: SoapNote,           // Standard S/O/A/P
}
```

### LLM Prompt Strategy

The multi-patient prompt:
1. Instructs LLM to analyze conversation for physician vs patient roles
2. Explicitly states NOT to assume Speaker 1 is physician
3. Requires one SOAP note per patient with ONLY that patient's information
4. Uses anti-hallucination rules (only explicit information)
5. Returns structured JSON with `physician_speaker` and `patients` array

### Commands

| Command | Purpose |
|---------|---------|
| `generate_soap_note_auto_detect` | Generate SOAP for 1-4 patients with auto-detection |
| `medplum_multi_patient_quick_sync` | Sync multi-patient session to Medplum |

### UI Implementation

- **Single Patient (1 note)**: Display unchanged - single S/O/A/P view
- **Multi-Patient (2+ notes)**:
  - Patient tabs with speaker identification
  - Each tab displays that patient's S/O/A/P
  - Physician identified at top of section
  - Copy button copies active patient's SOAP

### Medplum Integration

For multi-patient visits:
1. Create N placeholder patients (one per detected patient)
2. Create N encounters (one per patient)
3. Upload transcript to all encounters (shared context)
4. Upload each patient's SOAP to their respective encounter
5. Upload audio to first encounter

## Alternatives Considered

### Manual Speaker-to-Role Mapping
- UI where user assigns "Speaker 1 = Physician, Speaker 2 = Patient"
- **Rejected**: Adds friction, error-prone, physician may not remember speaker order

### Separate LLM Call Per Patient
- Generate each patient's SOAP in a separate LLM request
- **Rejected**: Loses cross-patient context, more latency, higher cost

### Fixed Speaker Assumptions
- Always assume Speaker 1 is physician, others are patients
- **Rejected**: Not reliable in real clinical scenarios

### Pre-Classification Step
- First LLM call to classify speakers, second to generate SOAP
- **Rejected**: Double latency, inconsistent context between calls

## Consequences

### Positive

- **Zero friction**: Works automatically without user configuration
- **Accurate separation**: Each patient gets only their relevant clinical information
- **Scalable**: Handles 1-4 patients with same code path
- **EMR ready**: Each patient has their own encounter in Medplum
- **Backward compatible**: Single patient sessions work identically to before
- **Physician identification**: Transcript review shows who was the physician

### Negative

- **LLM dependency**: Accuracy depends on LLM quality and prompt engineering
- **Ambiguous cases**: If conversation doesn't clearly indicate roles, results may be inaccurate
- **Max 4 patients**: Arbitrary limit, though rarely exceeded in practice
- **Longer prompts**: Multi-patient prompt is more complex than single-patient
- **No correction UI**: If LLM misidentifies roles, user cannot override

## Implementation Notes

### Files Modified

**Backend**:
- `ollama.rs`: Types, prompt builder, parser
- `commands/ollama.rs`: New command
- `commands/medplum.rs`: Multi-patient sync
- `lib.rs`: Command registration

**Frontend**:
- `types/index.ts`: TypeScript types
- `useSoapNote.ts`: Updated hook
- `useMedplumSync.ts`: Multi-patient sync
- `useSessionState.ts`: `soapResult` state
- `ReviewMode.tsx`: Patient tabs UI
- `styles.css`: Tab styling

### Testing

- All 429 frontend tests passing
- All 346 Rust tests passing
- Updated tests: `ReviewMode.test.tsx`, `useSessionState.test.ts`, `useSoapNote.test.ts`

## References

- [ADR-0009: Ollama SOAP Generation](./0009-ollama-soap-generation.md)
- [ADR-0008: Medplum EMR Integration](./0008-medplum-emr-integration.md)
- [Plan file: Multi-Patient SOAP Note Generation](/Users/backoffice/.claude/plans/wobbly-coalescing-toucan.md)

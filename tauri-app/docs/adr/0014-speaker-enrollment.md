# ADR 0014: Speaker Enrollment System

## Status
Accepted

## Context
The diarization system automatically clusters speakers as "Speaker 1", "Speaker 2", etc. While accurate for separating voices, this provides no semantic information about who is speaking. In clinical settings, knowing that "Dr. Smith" (the attending physician) said something vs "Speaker 2" is valuable for:
- More accurate SOAP note attribution
- Better clinical context for the LLM
- Improved transcript readability

## Decision
Implement a speaker enrollment system that:
1. Allows users to create speaker profiles with name, role, and description
2. Records voice samples to extract 256-dimensional ECAPA-TDNN embeddings
3. Pre-loads enrolled speakers at session start
4. Matches incoming audio against enrolled speakers first (higher threshold)
5. Falls back to auto-clustering for unrecognized voices
6. Injects speaker context into SOAP prompts

### Data Model
```rust
pub struct SpeakerProfile {
    pub id: String,           // UUID
    pub name: String,         // "Dr. Smith"
    pub role: SpeakerRole,    // Physician, PA, RN, MA, Patient, Other
    pub description: String,  // "Attending physician, internal medicine"
    pub embedding: Vec<f32>,  // 256-dim voice embedding
    pub created_at: i64,
    pub updated_at: i64,
}
```

### Recognition Flow
1. Session starts → load enrolled speaker profiles
2. Audio segment → extract embedding
3. Compare against enrolled speakers (threshold: 0.6)
4. If match found → use enrolled name
5. Else → fall back to auto-clustering ("Speaker N")

### SOAP Integration
Speaker context injected before transcript:
```
SPEAKER CONTEXT:
- Dr. Smith: Attending physician, internal medicine
- Speaker 2: Unidentified speaker

TRANSCRIPT:
[Dr. Smith]: How are you feeling today?
[Speaker 2]: I've had a headache for two days.
```

## Consequences

### Positive
- Named speakers in transcripts improve readability
- LLM receives semantic context about who is speaking
- Higher similarity threshold reduces false positives for enrolled speakers
- Profiles persist across sessions

### Negative
- Requires initial enrollment effort from users
- Voice embeddings add ~1KB per profile to storage
- Similarity matching adds minimal latency per utterance

### Files Added/Modified
- `src-tauri/src/speaker_profiles.rs` - Storage layer
- `src-tauri/src/commands/speaker_profiles.rs` - Tauri commands
- `src-tauri/src/diarization/clustering.rs` - Enrolled speaker support
- `src-tauri/src/diarization/provider.rs` - load_enrolled_speakers(), extract_embedding()
- `src-tauri/src/pipeline.rs` - Load profiles at session start
- `src-tauri/src/llm_client.rs` - Speaker context injection
- `src/hooks/useSpeakerProfiles.ts` - Frontend hook
- `src/components/SpeakerEnrollment.tsx` - Enrollment UI
- `src/types/index.ts` - TypeScript interfaces

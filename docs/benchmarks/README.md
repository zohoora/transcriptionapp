# LLM Benchmark Specifications

Benchmark specs for the 5 LLM tasks used in continuous charting mode (beyond final SOAP note generation). Each spec is self-contained — a separate benchmarking app can use it to evaluate model candidates without access to this codebase.

## Architecture Context

This is a clinical ambient scribe for physicians. In **continuous mode**, a microphone records all day in a medical office. The system must automatically:

1. **Detect** when one patient encounter ends and another begins
2. **Merge** incorrectly split segments back together
3. **Classify** whether a transcript segment is clinical or non-clinical
4. **Detect** if a merged transcript contains multiple patients (retrospective check)
5. **Find** the boundary line between patients in a multi-patient transcript

These 5 tasks form a pipeline that runs in real-time alongside transcription. All currently use the `fast-model` alias (typically a ~7B parameter model). The SOAP note generation task (which runs after encounter detection) has its own separate benchmark.

## Pipeline Flow

```
Audio → STT → Transcript Buffer
                    │
                    ▼
            ┌───────────────┐
            │  Encounter    │ ← Runs every ~2 min or on sensor trigger
            │  Detection    │   (encounter-detection.md)
            └───────┬───────┘
                    │ split detected
                    ▼
            ┌───────────────┐
            │  Encounter    │ ← Checks if split was correct
            │  Merge Check  │   (encounter-merge.md)
            └───────┬───────┘
                    │
           ┌────────┴────────┐
           │ merged          │ split confirmed
           ▼                 ▼
    ┌──────────────┐  ┌──────────────┐
    │ Multi-Patient│  │  Clinical    │
    │ Detection    │  │  Content     │
    │ (if ≥2500w)  │  │  Check       │
    └──────┬───────┘  └──────────────┘
           │ multiple patients
           ▼
    ┌──────────────┐
    │ Multi-Patient│
    │ Split Point  │
    └──────────────┘
```

## Tasks

| Spec | Task | When It Runs | Current Model | Fixture File |
|------|------|-------------|---------------|--------------|
| [encounter-detection.md](encounter-detection.md) | Detect transition between patient encounters | Every ~2 min, or on sensor trigger, or on word count threshold | `fast-model` (with optional `/nothink`) | `tauri-app/src-tauri/tests/fixtures/benchmarks/encounter_detection.json` |
| [encounter-merge.md](encounter-merge.md) | Check if split segments are same visit | After every encounter split | `fast-model` | `tauri-app/src-tauri/tests/fixtures/benchmarks/encounter_merge.json` |
| [clinical-content-check.md](clinical-content-check.md) | Classify transcript as clinical vs non-clinical | After split confirmed (not merged back) | `fast-model` | `tauri-app/src-tauri/tests/fixtures/benchmarks/clinical_content.json` |
| [multi-patient-detection.md](multi-patient-detection.md) | Detect multiple patients in merged transcript | After merge-back, if merged text ≥ 2500 words | `fast-model` | `tauri-app/src-tauri/tests/fixtures/benchmarks/multi_patient_detection.json` |
| [multi-patient-split.md](multi-patient-split.md) | Find boundary line between patients | After multi-patient detection confirms multiple patients | `fast-model` | `tauri-app/src-tauri/tests/fixtures/benchmarks/multi_patient_split.json` |

Run a single benchmark: `cd tauri-app/src-tauri && cargo run --bin benchmark_runner -- <task_name> --trials 3`. The replay regression CLIs (`merge_replay_cli`, `clinical_replay_cli`, `multi_patient_replay_cli`, `multi_patient_split_replay_cli`) re-issue captured prompts from production replay bundles instead of curated fixtures — see `docs/TESTING.md` for the full picture.

## Shared Conventions

### LLM Call Pattern

All tasks follow the same pattern:
1. System prompt (static or with minor context injection)
2. User prompt (contains the transcript text)
3. Response must be a single JSON object
4. No markdown, no explanations, no preamble

### Response Parsing Pipeline

All tasks share a common parsing pipeline that benchmark harnesses should replicate:

1. **Strip `<think>...</think>` tags** — Models may emit thinking tags even with `/nothink`. For closed tags, remove them entirely. For unclosed `<think>` (no `</think>`), keep whichever side of the tag contains JSON.
2. **Strip markdown code fences** — Remove `` ```json\n...\n``` `` wrappers.
3. **Extract first balanced JSON object** — Use brace-counting with string escape awareness to find `{...}`.
4. **Fallback: key-prefix search** — If outer JSON parse fails, search for the expected key (e.g., `{"complete"`) and try extracting from that position.

### Transcript Input Format

Segments are formatted as:
```
[0] (Speaker 1 (87%)): Good morning, how are you feeling today?
[1] (Speaker 2 (92%)): Hi doctor. I've been having headaches.
[2] (Unknown): [ambient noise or unspeakable segment]
```

Format: `[segment_index] (speaker_label): text`

- `segment_index` — monotonic u64, may have gaps
- `speaker_label` — `"Speaker N (XX%)"` with diarization confidence, or `"Speaker N"` without confidence, or `"Unknown"` if no speaker detected
- Segments are newline-separated

### Scoring Dimensions

Each benchmark should evaluate:

1. **Correctness** — Did the model make the right decision? (binary or within tolerance)
2. **Confidence calibration** — Are confidence scores meaningful? (high confidence = correct, low confidence = uncertain)
3. **Parsing robustness** — Can the response be parsed despite formatting quirks?
4. **Latency** — Response time matters for real-time operation (target < 5s for detection, < 10s for others)
5. **Consistency** — Same input should produce same output across runs (test with 3 trials at temp=0.3)

### Constants Reference

From `encounter_detection.rs`:

| Constant | Value | Purpose |
|----------|-------|---------|
| `FORCE_CHECK_WORD_THRESHOLD` | 3,000 | Trigger detection regardless of timer |
| `FORCE_SPLIT_WORD_THRESHOLD` | 5,000 | Force-split if consecutive no-split ≥ limit |
| `FORCE_SPLIT_CONSECUTIVE_LIMIT` | 3 | Consecutive non-split cycles before force-split |
| `ABSOLUTE_WORD_CAP` | 25,000 | Unconditional force-split (hard safety valve) |
| `MIN_WORDS_FOR_CLINICAL_CHECK` | 100 | Minimum words for clinical content check |
| `MULTI_PATIENT_CHECK_WORD_THRESHOLD` | 2,500 | Minimum merged words for multi-patient check |
| `MULTI_PATIENT_SPLIT_MIN_WORDS` | 500 | Minimum words per half for split acceptance |

### Confidence Gate (Applied to Encounter Detection)

Dynamic threshold based on buffer age and merge-back history:

| Buffer Age | Base Threshold |
|-----------|---------------|
| < 20 minutes | 0.85 |
| ≥ 20 minutes | 0.70 |

Escalation: each merge-back adds +0.05 to threshold (capped at 0.99). Resets to base when a split "sticks" (is not merged back).

Formula: `threshold = min(base + merge_back_count * 0.05, 0.99)`

### Production Failure Modes

Common issues observed in production that benchmarks should cover:

1. **STT hallucination loops** — Whisper repeats phrases, inflating word counts (e.g., "fractured fractured fractured..." × 100)
2. **Non-determinism** — Same prompt at temp=0.3 can flip decisions ~40% of the time
3. **Think tag leakage** — Models emit `<think>` even when told not to
4. **Wrapper objects** — Models wrap JSON like `{return {"complete": false}}`
5. **Markdown fences** — Models wrap in `` ```json ... ``` ``
6. **Companion vs patient confusion** — Family member who speaks extensively mistaken for separate patient
7. **Topic-shift false positives** — Doctor discusses multiple conditions for same patient, incorrectly split
8. **Sensor false departures** — Hand washing, supply runs, injections trigger departure signals but encounter continues

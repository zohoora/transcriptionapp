# Design: Elapsed Timestamps in Encounter Detection

**Date:** 2026-03-09
**Status:** Approved

## Problem

The encounter detection LLM receives transcript segments with no temporal information. It cannot distinguish a 30-second exam pause from a 10-minute empty room. This causes:

- **False splits on brief absences** (e.g., nurse checking BP before doctor arrives — the "farewell" at 2 minutes with 64 words looks identical to a real visit ending)
- **Missed splits on long silences** (e.g., patient waiting 30 minutes in the room before the doctor arrives — all segments look contiguous)

The mmWave sensor partially addresses this, but the LLM itself has zero temporal awareness.

## Solution

Add elapsed time (MM:SS from recording start) to every segment in the detection prompt. The LLM can then naturally observe pacing and gaps.

### Before

```
[0] (Speaker 1 (87%)): Good afternoon, sir.
[1] (Speaker 2 (65%)): I'll just check her blood pressure.
[2] (Speaker 1 (53%)): One forty-two degrees.
[3] (Speaker 2 (50%)): This morning it was 151 over 86.
[4] (Speaker 1 (68%)): I'll be in shortly for you.
```

### After

```
[0] (00:00) (Speaker 1 (87%)): Good afternoon, sir.
[1] (00:08) (Speaker 2 (65%)): I'll just check her blood pressure.
[2] (00:35) (Speaker 1 (53%)): One forty-two degrees.
[3] (01:12) (Speaker 2 (50%)): This morning it was 151 over 86.
[4] (02:58) (Speaker 1 (68%)): I'll be in shortly for you.
```

## Changes

### 1. `BufferedSegment` struct (`transcript_buffer.rs`)

Add `start_ms: u64` field. Currently only `end_ms` is stored (as `timestamp_ms`). The new field captures when speech started in the audio clock, enabling accurate elapsed time computation.

### 2. `TranscriptBuffer::push()` (`transcript_buffer.rs`)

Add `start_ms` parameter. Pass through to `BufferedSegment`.

### 3. Push call site (`continuous_mode.rs`)

Pass `segment.start_ms` in addition to existing `segment.end_ms`.

### 4. `format_for_detection()` (`transcript_buffer.rs`)

Compute elapsed as `segment.start_ms - first_segment.start_ms`. Format:
- Under 60 minutes: `MM:SS` (e.g., `05:30`)
- 60 minutes or more: `H:MM:SS` (e.g., `1:05:30`)

### 5. Detection system prompt (`encounter_detection.rs`)

Add to the system prompt in `build_encounter_detection_prompt()`:

```
Each segment includes elapsed time (MM:SS) from the start of the recording.
Large gaps between timestamps may indicate silence, examination, or the room being empty between patients.
```

### 6. Tests

- Update `push()` calls in `transcript_buffer.rs` tests to include `start_ms`
- Update format assertions to expect `(MM:SS)` prefix
- Update E2E test format expectations if they check detection format

## What does NOT change

- `format_for_transcript()` — plain transcript for SOAP/archive stays unchanged
- Clinical content check, merge check, multi-patient prompts — these use plain text, not detection format
- Sensor context injection — still appended separately, unaffected

## Motivation from production (Mar 9 clinic)

Allan's visit: sensor departure at 10:03 AM triggered a check at 64 words. The LLM said `complete=true` (confidence 0.92) on a 2-minute nurse BP check. With timestamps, the LLM would see the "farewell" happened at `02:58` with only 5 segments — strong temporal evidence this is not a real encounter conclusion.

Dolores waiting room: 30 minutes of chatter before the doctor arrived. Without timestamps, the LLM sees contiguous segments with no hint of timing. With timestamps, a `00:00`→`28:45` span of non-clinical chatter followed by clinical content starting at `30:12` gives the LLM a clear signal.

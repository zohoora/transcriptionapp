# Elapsed Timestamps in Encounter Detection — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add elapsed time (MM:SS) to every segment in the encounter detection prompt so the LLM can observe pacing and gaps.

**Architecture:** Add `start_ms` field to `BufferedSegment`, thread it through `push()` from the pipeline, compute elapsed time in `format_for_detection()`, and mention timestamps in the detection system prompt. Only `format_for_detection()` changes — plain transcript formatting stays untouched.

**Tech Stack:** Rust (Tauri backend), no new dependencies.

**Design doc:** `docs/plans/2026-03-09-detection-timestamps.md`

---

### Task 1: Add `start_ms` to `BufferedSegment` and update `push()`

**Files:**
- Modify: `tauri-app/src-tauri/src/transcript_buffer.rs:11-26` (struct)
- Modify: `tauri-app/src-tauri/src/transcript_buffer.rs:69` (push signature + body)

**Step 1: Write failing test**

Add to the `tests` module at the bottom of `transcript_buffer.rs`:

```rust
#[test]
fn test_buffered_segment_has_start_ms() {
    let mut buffer = TranscriptBuffer::new();
    buffer.push("Hello".to_string(), 500, 1500, None, None, 0);
    let drained = buffer.drain_through(0);
    assert_eq!(drained[0].start_ms, 500);
    assert_eq!(drained[0].timestamp_ms, 1500);
}
```

**Step 2: Run test to verify it fails**

Run: `cd tauri-app/src-tauri && cargo test test_buffered_segment_has_start_ms`
Expected: Compile error — `push()` takes 5 args, 6 given; no field `start_ms`

**Step 3: Implement the changes**

In `BufferedSegment` struct (line 11), add after `index`:

```rust
/// Pipeline audio clock: when speech started (milliseconds from recording start)
pub start_ms: u64,
```

In `push()` (line 69), change signature to:

```rust
pub fn push(&mut self, text: String, start_ms: u64, timestamp_ms: u64, speaker_id: Option<String>, speaker_confidence: Option<f32>, generation: u64) {
```

In the `BufferedSegment` construction inside `push()` (line 73), add:

```rust
start_ms,
```

**Step 4: Fix all existing tests to compile**

Every existing `push()` call in the test module passes 5 args. Each needs a `start_ms` inserted as the 2nd argument (before `timestamp_ms`). Use `0` for all existing tests since they don't test elapsed time:

| Test | Line(s) | Change |
|------|---------|--------|
| `test_transcript_buffer_push_and_read` | 188-189 | Add `0,` before `1000,` and `0,` before `2000,` |
| `test_transcript_buffer_full_text` | 200-201 | Add `0,` before `1000,` and `0,` before `2000,` |
| `test_transcript_buffer_drain_through` | 209-211 | Add `0,` before each timestamp |
| `test_transcript_buffer_get_text_since` | 226-228 | Add `0,` before each timestamp |
| `test_transcript_buffer_format_for_detection` | 237-238 | Add `0,` before each timestamp |
| `test_transcript_buffer_format_for_detection_no_confidence` | 248 | Add `0,` before `1000,` |
| `test_transcript_buffer_full_text_with_speakers` | 257-259 | Add `0,` before each timestamp |
| `test_transcript_buffer_stale_generation_rejected` | 269-270 | Add `0,` before each timestamp |

Pattern for each: `buffer.push("text".to_string(), 1000,` → `buffer.push("text".to_string(), 0, 1000,`

**Step 5: Run all tests to verify they pass**

Run: `cd tauri-app/src-tauri && cargo test transcript_buffer`
Expected: All 9 tests pass (8 existing + 1 new)

**Step 6: Commit**

```bash
git add tauri-app/src-tauri/src/transcript_buffer.rs
git commit -m "feat: add start_ms to BufferedSegment for elapsed time tracking"
```

---

### Task 2: Pass `start_ms` from pipeline to buffer

**Files:**
- Modify: `tauri-app/src-tauri/src/continuous_mode.rs:377-383` (push call site)

**Step 1: Verify it fails to compile**

Run: `cd tauri-app/src-tauri && cargo check`
Expected: Compile error at `continuous_mode.rs:377` — `push()` now expects 6 args, 5 given.

**Step 2: Fix the call site**

At line 377-383, change:

```rust
buffer.push(
    segment.text.clone(),
    segment.end_ms,
    segment.speaker_id.clone(),
    segment.speaker_confidence,
    pipeline_generation,
);
```

To:

```rust
buffer.push(
    segment.text.clone(),
    segment.start_ms,
    segment.end_ms,
    segment.speaker_id.clone(),
    segment.speaker_confidence,
    pipeline_generation,
);
```

**Step 3: Verify it compiles**

Run: `cd tauri-app/src-tauri && cargo check`
Expected: No errors.

**Step 4: Run full test suite**

Run: `cd tauri-app/src-tauri && cargo test`
Expected: All tests pass (the `Segment` struct in `transcription.rs` already has `start_ms`).

**Step 5: Commit**

```bash
git add tauri-app/src-tauri/src/continuous_mode.rs
git commit -m "feat: pass segment start_ms through to transcript buffer"
```

---

### Task 3: Add elapsed time to `format_for_detection()`

**Files:**
- Modify: `tauri-app/src-tauri/src/transcript_buffer.rs:142-151` (format_for_detection)

**Step 1: Write failing test**

Add to the `tests` module:

```rust
#[test]
fn test_format_for_detection_includes_elapsed_time() {
    let mut buffer = TranscriptBuffer::new();
    // Segments at 0s, 8s, 35s, 72s (1:12), 178s (2:58)
    buffer.push("Good afternoon.".to_string(), 0, 500, Some("Speaker 1".to_string()), Some(0.87), 0);
    buffer.push("Check blood pressure.".to_string(), 8_000, 9_000, Some("Speaker 2".to_string()), Some(0.65), 0);
    buffer.push("One forty-two.".to_string(), 35_000, 36_000, Some("Speaker 1".to_string()), Some(0.53), 0);
    buffer.push("Was 151 over 86.".to_string(), 72_000, 73_000, Some("Speaker 2".to_string()), Some(0.50), 0);
    buffer.push("I'll be in shortly.".to_string(), 178_000, 179_000, Some("Speaker 1".to_string()), Some(0.68), 0);

    let formatted = buffer.format_for_detection();
    assert!(formatted.contains("[0] (00:00) (Speaker 1 (87%)): Good afternoon."));
    assert!(formatted.contains("[1] (00:08) (Speaker 2 (65%)): Check blood pressure."));
    assert!(formatted.contains("[2] (00:35) (Speaker 1 (53%)): One forty-two."));
    assert!(formatted.contains("[3] (01:12) (Speaker 2 (50%)): Was 151 over 86."));
    assert!(formatted.contains("[4] (02:58) (Speaker 1 (68%)): I'll be in shortly."));
}

#[test]
fn test_format_for_detection_hour_plus() {
    let mut buffer = TranscriptBuffer::new();
    buffer.push("Start.".to_string(), 0, 500, Some("Speaker 1".to_string()), Some(0.90), 0);
    // 1 hour, 5 minutes, 30 seconds = 3_930_000 ms
    buffer.push("Still here.".to_string(), 3_930_000, 3_931_000, Some("Speaker 1".to_string()), Some(0.85), 0);

    let formatted = buffer.format_for_detection();
    assert!(formatted.contains("[0] (00:00) (Speaker 1 (90%)): Start."));
    assert!(formatted.contains("[1] (1:05:30) (Speaker 1 (85%)): Still here."));
}

#[test]
fn test_format_for_detection_empty_buffer() {
    let buffer = TranscriptBuffer::new();
    assert_eq!(buffer.format_for_detection(), "");
}
```

**Step 2: Run test to verify it fails**

Run: `cd tauri-app/src-tauri && cargo test test_format_for_detection_includes_elapsed`
Expected: FAIL — output has `[0] (Speaker 1 (87%)):` without `(00:00)`

**Step 3: Implement `format_for_detection()` with elapsed time**

Replace the `format_for_detection` method (lines 142-151):

```rust
/// Format segments for the encounter detector prompt (numbered, with elapsed time and speaker confidence)
pub fn format_for_detection(&self) -> String {
    let first_start_ms = self.segments.first().map(|s| s.start_ms).unwrap_or(0);

    self.segments
        .iter()
        .map(|s| {
            let elapsed_ms = s.start_ms.saturating_sub(first_start_ms);
            let total_secs = elapsed_ms / 1000;
            let hours = total_secs / 3600;
            let minutes = (total_secs % 3600) / 60;
            let seconds = total_secs % 60;

            let elapsed = if hours > 0 {
                format!("{}:{:02}:{:02}", hours, minutes, seconds)
            } else {
                format!("{:02}:{:02}", minutes, seconds)
            };

            let speaker_label = format_speaker_label(s.speaker_id.as_deref(), s.speaker_confidence);
            format!("[{}] ({}) ({}): {}", s.index, elapsed, speaker_label, s.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
```

**Step 4: Fix the existing format assertion tests**

Two existing tests check the exact detection format:

`test_transcript_buffer_format_for_detection` (line 235): update assertions to include `(00:00)`:

```rust
assert!(formatted.contains("[0] (00:00) (Dr. Smith (92%)): Hello"));
assert!(formatted.contains("[1] (00:00) (Unknown): Hi there"));
```

`test_transcript_buffer_format_for_detection_no_confidence` (line 246): update:

```rust
assert!(formatted.contains("[0] (00:00) (Speaker 1): Hello"));
```

Note: These show `(00:00)` because existing tests use `start_ms: 0` for all segments.

**Step 5: Run all tests**

Run: `cd tauri-app/src-tauri && cargo test transcript_buffer`
Expected: All 12 tests pass (9 existing + 3 new)

**Step 6: Commit**

```bash
git add tauri-app/src-tauri/src/transcript_buffer.rs
git commit -m "feat: include elapsed MM:SS timestamps in detection format"
```

---

### Task 4: Update detection system prompt to explain timestamps

**Files:**
- Modify: `tauri-app/src-tauri/src/encounter_detection.rs:58-90` (system prompt)

**Step 1: Write failing test**

Add at the bottom of `encounter_detection.rs` test module (or create one if no test module exists — check first):

```rust
#[test]
fn test_detection_prompt_mentions_elapsed_time() {
    let (system, _user) = build_encounter_detection_prompt("test transcript", None);
    assert!(system.contains("elapsed time"));
    assert!(system.contains("Large gaps between timestamps"));
}
```

**Step 2: Run test to verify it fails**

Run: `cd tauri-app/src-tauri && cargo test test_detection_prompt_mentions_elapsed`
Expected: FAIL — system prompt doesn't contain "elapsed time"

**Step 3: Add timestamp context to system prompt**

In `build_encounter_detection_prompt()`, in the `system` string (line 58-90), add after the line `"- Short exchanges or greetings with no substantive clinical content yet"` and before `"If you find a transition point"`:

```
- Discussion of multiple body parts or conditions for the SAME patient (one visit can cover many topics)

Each segment includes elapsed time (MM:SS) from the start of the recording.
Large gaps between timestamps may indicate silence, examination, or the room being empty between patients.

If you find a transition point
```

So the two new lines go between the "not a transition" list and the JSON return instructions.

**Step 4: Run test to verify it passes**

Run: `cd tauri-app/src-tauri && cargo test test_detection_prompt_mentions_elapsed`
Expected: PASS

**Step 5: Run full test suite**

Run: `cd tauri-app/src-tauri && cargo test`
Expected: All tests pass. No other prompt functions reference this text.

**Step 6: Commit**

```bash
git add tauri-app/src-tauri/src/encounter_detection.rs
git commit -m "feat: explain elapsed timestamps in detection system prompt"
```

---

### Task 5: Check for any other `push()` call sites and E2E format references

**Files:**
- Search: all `.rs` files for `buffer.push(` and `format_for_detection`

**Step 1: Search for all `push()` call sites**

Run: `cd tauri-app/src-tauri && grep -rn 'buffer\.push(' src/ --include='*.rs' | grep -v test | grep -v '#\[cfg(test)\]'`

Expect: Only `continuous_mode.rs:377` (already fixed in Task 2). If others exist, update them with `start_ms` parameter.

**Step 2: Search for E2E tests that check detection format**

Run: `cd tauri-app/src-tauri && grep -rn 'format_for_detection\|\\[0\\] (' src/e2e_tests.rs`

If any E2E tests assert on the detection format string, update them to expect `(MM:SS)`.

**Step 3: Run cargo check**

Run: `cd tauri-app/src-tauri && cargo check`
Expected: No errors.

**Step 4: Run full test suite**

Run: `cd tauri-app/src-tauri && cargo test`
Expected: All tests pass.

**Step 5: Commit (only if changes were needed)**

```bash
git add -A
git commit -m "fix: update remaining push() call sites for start_ms parameter"
```

---

### Task 6: Final verification

**Step 1: TypeScript check (should be unaffected — no frontend changes)**

Run: `cd tauri-app && npx tsc --noEmit`
Expected: No errors.

**Step 2: Frontend tests (should be unaffected)**

Run: `cd tauri-app && pnpm test:run`
Expected: All tests pass.

**Step 3: Rust tests**

Run: `cd tauri-app/src-tauri && cargo test`
Expected: All tests pass.

**Step 4: Build**

Run: `cd tauri-app && pnpm tauri build --debug 2>&1 | tail -5`
Expected: Build succeeds.

---

## What does NOT change

- `format_for_transcript()` — doesn't exist separately; plain transcript for SOAP/archive uses `full_text_with_speakers()` which is untouched
- `full_text()`, `full_text_with_speakers()`, `get_text_since()` — no elapsed time needed
- Clinical content check, merge check, multi-patient prompts — these use plain text, not detection format
- Sensor context injection — still appended separately, unaffected
- Frontend — no TypeScript changes needed

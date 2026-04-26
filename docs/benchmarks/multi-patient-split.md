# Benchmark: Multi-Patient Split Point Detection

## Overview

**Task**: Given a transcript confirmed to contain multiple patient encounters, find the LINE NUMBER where the first patient's encounter ends and the second patient's encounter begins.

**When it runs**: Only after the multi-patient detection task confirms `multiple_patients: true`. The transcript is split into individual lines, and the model must return the boundary line number.

**Why it matters**: This task enables automatic splitting of incorrectly merged transcripts. The split point accuracy directly affects the quality of the resulting SOAP notes — a bad split means clinical content from Patient A ends up in Patient B's note and vice versa.

**Current model**: `fast-model`

---

## Exact Prompts

### System Prompt

```
You MUST respond in English with ONLY a JSON object. No other text.

You are analyzing a clinical transcript that was recorded continuously in a medical office.
This transcript has been confirmed to contain MULTIPLE DISTINCT patient encounters.

Your task: find the line where the FIRST patient's encounter ends and the SECOND patient's encounter begins.

Look for:
- A different patient name being introduced or addressed
- The doctor beginning a new clinical assessment for a different person
- Someone saying "next patient" or introducing another person by name
- A shift from one person's medical issues to another person's medical issues

IMPORTANT: In family visits, the transition may be subtle — no formal farewell, just a name switch ("Mercedes is next", "how about Jim's labs"). Focus on WHICH PATIENT is being clinically assessed, not conversational flow.

Return the LINE NUMBER of the LAST line of the FIRST patient's encounter.

Return a JSON object (or empty object {} if no clear boundary):
{"line_index": <line number>, "confidence": <0.0-1.0>, "reason": "<brief explanation>"}

Respond with ONLY the JSON.
```

### User Prompt Template

The transcript is formatted as numbered lines:

```
0: Speaker 1: Lynn, your blood work came back.
1: Speaker 2: I figured. I've been feeling tired.
2: Speaker 1: I'd like to increase your levothyroxine.
...
15: Speaker 1: Jim, let me pull up your chart.
16: Speaker 3: My knees have been bothering me.
```

Each line is prefixed with its line number (0-indexed), followed by a colon and space, then the transcript line content.

---

## Input Format

Lines are formatted as:
```
{line_number}: {original_line_text}
```

- `line_number` — 0-indexed integer, sequential with no gaps
- `original_line_text` — raw transcript line including speaker labels
- Lines are newline-separated

This is different from the segment format used in encounter detection (`[index] (speaker): text`). The multi-patient split uses a simpler line-number format because the input is a plain text transcript that has been split by newlines.

---

## Expected Output Schema

```json
{
  "line_index": 14,             // usize — the last line of the FIRST encounter
  "confidence": 0.9,            // f64, 0.0-1.0 — optional
  "reason": "brief explanation" // string — optional
}
```

### Field Details

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `line_index` | `usize \| null` | Yes (null only if no boundary found) | Last line number of the first patient's encounter |
| `confidence` | `f64 \| null` | No | Model's confidence (0.0-1.0) |
| `reason` | `string \| null` | No | Brief explanation of the boundary |

### Special Case: No Clear Boundary

If the model cannot find a clear boundary, it should return an empty object `{}`. This is explicitly allowed by the prompt.

### Validity Rules

- `line_index` must be > 0 (can't split before the first line)
- `line_index` must be < total line count (can't split after the last line)
- `confidence` should be between 0.0 and 1.0 (clamped in production)
- Both halves resulting from the split must have ≥ 500 words (`MULTI_PATIENT_SPLIT_MIN_WORDS`) — this is enforced by the caller, not the model, but the model should place the boundary where a natural split occurs

---

## Scoring Criteria

### Primary: Split Point Accuracy

| Metric | Definition | Target |
|--------|-----------|--------|
| **Exact match** | `line_index` matches ground truth exactly | > 60% |
| **Within ±2 lines** | `line_index` within 2 lines of ground truth | > 85% |
| **Within ±5 lines** | `line_index` within 5 lines of ground truth | > 95% |
| **Correct side** | Each patient's primary content is in the correct half | > 95% |

**Note:** Exact match is hard because transition zones are often 2-3 lines wide (e.g., "Take care Lynn. / Now Jim, let me pull up your chart."). Being within ±2 lines is usually clinically equivalent.

### Secondary: Size Gate Pass Rate

After the split, both halves must have ≥ 500 words. If the model places the boundary too close to either end, the split is rejected.

| Metric | Definition | Target |
|--------|-----------|--------|
| **Size gate pass** | Both halves ≥ 500 words after split | > 90% |

### Tertiary: Empty Object Appropriateness

When the model returns `{}`, this should only happen when there truly is no clear boundary.

| Metric | Definition | Target |
|--------|-----------|--------|
| **True no-boundary** | Empty object returned when no clear split exists | > 80% |
| **False no-boundary** | Empty object returned when boundary IS clear | < 5% |

---

## Test Cases

### TC-1: Clear Name Switch Boundary (Easy)

**Input:**
```
0: Speaker 1: Lynn, your blood work came back. Your thyroid levels are still elevated.
1: Speaker 2: I figured. I've been feeling tired and gaining weight again.
2: Speaker 1: I'd like to increase your levothyroxine from 50 to 75 micrograms.
3: Speaker 2: Okay. Should I still take it on an empty stomach?
4: Speaker 1: Yes, first thing in the morning. We'll recheck your TSH in six weeks.
5: Speaker 2: Sounds good.
6: Speaker 1: Now Jim, how about you? Let me pull up your chart.
7: Speaker 3: Well, my knees have been really bothering me. Both of them.
8: Speaker 1: When did that start getting worse?
9: Speaker 3: About three weeks ago.
10: Speaker 1: Let me examine your knees. I see some swelling.
11: Speaker 1: I'd like to get X-rays and start anti-inflammatory medication.
12: Speaker 3: Okay, whatever you think is best.
```

**Expected:** `{"line_index": 5, "confidence": ≥0.9, "reason": "Lynn's assessment ends at line 5, Jim's begins at line 6"}`

**Difficulty:** Easy — clear name switch with "Now Jim" at line 6.

### TC-2: Mother-Daughter Transition (Medium)

**Input:**
```
0: Speaker 1: Jocelyn, your lab results look normal. Iron levels are fine.
1: Speaker 2: Oh good. The supplements worked.
2: Speaker 1: We can stop those. Your blood pressure is 118 over 76, great.
3: Speaker 2: No other concerns. But Mercedes has been having issues.
4: Speaker 1: Mercedes, come sit down. Your mom mentioned stomach problems?
5: Speaker 3: Yeah, I've been getting cramps after eating, especially with dairy.
6: Speaker 1: How long has this been going on?
7: Speaker 3: About two months.
8: Speaker 1: I'd like to test for lactose intolerance. Try avoiding dairy for two weeks.
9: Speaker 1: Mercedes, come back in two weeks. Jocelyn, six months.
```

**Expected:** `{"line_index": 3, "confidence": ≥0.8, "reason": "Jocelyn's visit concludes at line 3, Mercedes's assessment begins at line 4"}`

**Acceptable range:** `line_index` 3 or 4 — the transition spans lines 3-4. Line 3 is preferred because Jocelyn mentions Mercedes but the clinical focus hasn't shifted yet.

**Difficulty:** Medium — transition is in the middle of a conversation, no clear farewell.

### TC-3: No Clear Boundary (Edge Case)

**Input:**
```
0: Speaker 1: Your blood pressure looks good today.
1: Speaker 2: Thanks doctor.
2: Speaker 1: We'll continue your current medications.
3: Speaker 2: Sounds good.
4: Speaker 1: Any questions about your diabetes management?
5: Speaker 2: No, I think I understand the new diet plan.
6: Speaker 1: Great. See you in three months.
```

**Expected:** `{}` (empty object — this is actually a single patient, no boundary exists)

**Note:** This test case verifies the model returns an empty object when asked to find a boundary that doesn't exist. In production, this should not happen (multi-patient detection confirmed multiple patients), but it tests the model's ability to express uncertainty.

**Difficulty:** Easy — there's clearly only one patient. The model should recognize this and return empty.

### TC-4: Subtle Transition — Same Topic Continuation (Hard)

**Input:**
```
0: Speaker 1: Danika, your thyroid levels have stabilized nicely at 2.8.
1: Speaker 2: I feel so much better. The fatigue is almost gone.
2: Speaker 1: I want to keep you on the current dose and recheck in three months.
3: Speaker 2: My sister's been having similar symptoms.
4: Speaker 3: Yeah, I've been exhausted. Could it be thyroid too?
5: Speaker 1: It could be hereditary. Let me do a quick exam. Can you swallow for me?
6: Speaker 3: Like this?
7: Speaker 1: I can feel some enlargement. Let me order thyroid labs for you.
8: Speaker 1: Danika, keep taking your medication. We'll call your sister with results.
```

**Expected:** `{"line_index": 3, "confidence": ≥0.7, "reason": "Danika's visit ends at line 3, sister's assessment begins at line 4-5"}`

**Acceptable range:** `line_index` 3-4 — the transition is gradual (sister mentions symptoms at 4, doctor starts examining at 5).

**Difficulty:** Hard — same medical topic (thyroid), same family, organic transition. The transition zone is blurry.

### TC-5: Farewell Then New Patient (Easy)

**Input:**
```
0: Speaker 1: So we'll continue the current plan. See you in three months.
1: Speaker 2: Thank you doctor. Take care.
2: Speaker 1: You too, Mrs. Johnson.
3: Speaker 1: Hello, come on in. I'm Doctor Smith. What brings you in today?
4: Speaker 3: Hi doctor. I've been having this rash on my arms.
5: Speaker 1: When did you first notice it?
6: Speaker 3: About a week ago. It's itchy.
7: Speaker 1: Let me take a look. Can you roll up your sleeves?
```

**Expected:** `{"line_index": 2, "confidence": ≥0.95, "reason": "Mrs. Johnson's visit ends at line 2 with farewell, new patient begins at line 3"}`

**Difficulty:** Easy — classic farewell-greeting pattern.

### TC-6: Back-to-Back With Staff Instructions Between (Medium)

**Input:**
```
0: Speaker 1: Alright, your prescription is at the front desk. Take care.
1: Speaker 2: Thanks doctor.
2: Speaker 1: Sarah, can you bring in the next patient?
3: Speaker 3: Sure, they're in room two.
4: Speaker 1: Thanks.
5: Speaker 1: Good afternoon. I see you're here for your annual physical.
6: Speaker 4: Yes, it's been about a year since my last one.
7: Speaker 1: Let's start with your vitals.
```

**Expected:** `{"line_index": 1, "confidence": ≥0.85, "reason": "First patient leaves at line 1, staff interaction at 2-4, new patient begins at line 5"}`

**Acceptable range:** `line_index` 1-4 — the staff interaction (lines 2-4) is a gray zone. Line 1 is most correct (last line of the first patient's encounter). Lines 2-4 are administrative, not part of either encounter.

**Difficulty:** Medium — staff interaction between patients creates ambiguity about exact boundary.

---

## Edge Cases & Failure Modes

### Known Production Failures

1. **Size Gate Rejection** (Mar 6 clinic): Danika + sister — multi-patient check correctly detected two patients, but the split point produced one half with < 500 words (sister's assessment was brief). The size gate rejected the split. Benchmark should include test cases near the 500-word boundary.

2. **Transition Zone Width**: Real transitions often span 2-4 lines. The model needs to pick a single line, but the "correct" answer is a range. Scoring should use ±2 line tolerance.

3. **End-of-Visit Ambiguity**: When the doctor addresses both patients at the end ("Danika, keep taking your medication. We'll call your sister with results."), the final line references both patients. The split should be at the LAST line of the first patient's core assessment, not at the jointly-addressed wrap-up.

### Edge Cases to Test

- **Transition at very beginning** (line 1-2) — almost all content is second patient, first patient's portion is tiny
- **Transition at very end** — almost all content is first patient, second patient barely started
- **Multiple transition candidates** — staff chat between patients creates multiple possible boundaries
- **No speaker labels** — all `Unknown`, boundary must be inferred from content alone
- **Three or more patients** — prompt asks for first boundary only
- **Very long transcript** (5,000+ words) — model may lose attention over long context

---

## Prompt Variants

This task has no variants — the same system prompt is always used.

---

## Parsing Robustness

The parsing for this task has some differences from other tasks:

| Issue | Example | How It's Handled |
|-------|---------|-----------------|
| Think tags | `<think>analyzing</think>{"line_index": 5}` | `strip_think_tags()` then `extract_first_json_object()` |
| Empty object | `{}` | Valid — means no clear boundary found |
| Missing optional fields | `{"line_index": 5}` | `confidence` defaults to 0.5, `reason` defaults to empty string |
| Out-of-range line_index | `{"line_index": 100}` (only 20 lines) | Filtered out (treated as no result) |
| Zero line_index | `{"line_index": 0}` | Filtered out (can't split before first line) |
| Confidence > 1.0 | `{"line_index": 5, "confidence": 1.5}` | Clamped to 1.0 |
| Confidence < 0.0 | `{"line_index": 5, "confidence": -0.1}` | Clamped to 0.0 |
| Array response | `[{"line_index": 5}]` | Parser tries single object first, falls back to array, uses first element |
| Markdown fences | `` ```json\n{"line_index": 5}\n``` `` | Fences stripped before parsing |

### Parser

`parse_multi_patient_split()` in `encounter_detection.rs` uses the shared `parse_llm_json_response()` helper (same as other tasks). The local struct:

```rust
pub struct MultiPatientSplitResult {
    pub line_index: Option<usize>,
    pub confidence: Option<f64>,
    pub reason: Option<String>,
}
```

All fields are `Option` — the model may return a subset. `line_index: None` or `line_index: 0` or `line_index > max_line` all result in the split being rejected. Confidence is clamped to `[0.0, 1.0]` post-parse.

**Benchmark should test:** Verify parsing handles all the above edge cases, especially empty objects, out-of-range values, and missing fields.

### Fixture and Replay Tools

- **Curated benchmark fixture**: `tauri-app/src-tauri/tests/fixtures/benchmarks/multi_patient_split.json` — TC-1, TC-2, etc., with `expected_line_index` and tolerance.
- **Production replay**: `cargo run --bin multi_patient_split_replay_cli -- --all` — re-issues archived split prompts from production replay bundles (schema v5+, back-compatible with v3 and v4) and compares `line_index` within `±2 lines` (configurable via `--tolerance`). Synthetic mode (`--synthetic`) builds a split prompt from scratch for any bundle with ≥2-patient detection. See `tools/multi_patient_split_replay_cli.rs`.

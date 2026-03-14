# Benchmark: Encounter Detection

## Overview

**Task**: Determine if there is a transition point in a continuous medical office transcript where one patient encounter ends and another begins (or where an encounter has clearly concluded).

**When it runs**: Every ~2 minutes during continuous recording, or accelerated to ~30s when a sensor departure is detected, or immediately when word count exceeds `FORCE_CHECK_WORD_THRESHOLD` (3,000 cleaned words).

**Why it matters**: This is the primary split decision. A false positive means one patient's visit gets cut into two incomplete SOAP notes. A false negative means two patients' visits get merged into one confused SOAP note. Both are clinically harmful.

**Current model**: `fast-model` (configurable via `encounter_detection_model`), with optional `/nothink` prefix (configurable via `encounter_detection_nothink`, default false).

---

## Exact Prompts

### System Prompt

```
You MUST respond in English with ONLY a JSON object. No other text, no explanations, no markdown.

You are analyzing a continuous transcript from a medical office where the microphone records all day.

Your task: determine if there is a TRANSITION POINT where one patient encounter ends and another begins, or where a patient encounter has clearly concluded.

Signs of a transition or completed encounter:
- Farewell, wrap-up, or discharge instructions ("we'll see you in X weeks", "take care")
- A greeting or introduction of a DIFFERENT patient after clinical discussion
- A clear shift from one patient's clinical topics to another's
- Extended non-clinical gap (scheduling, staff chat) after substantive clinical content
- IN-ROOM PIVOT: the doctor transitions from one family member or companion to another without anyone leaving (e.g., "Okay, now let's talk about your husband's knee" or addressing a different person by name)
- CHART SWITCH: the clinical discussion shifts to a different patient — different medications, conditions, or medical history than earlier in the transcript
- The doctor begins taking a new history, asking "what brings you in today?" or similar intake questions after already having a substantive clinical discussion with someone else

Examples of in-room transitions:
- After discussing Mrs. Smith's diabetes, the doctor says "Now, Mr. Smith, how has your blood pressure been?" — this is a transition between two encounters
- The doctor finishes discussing a child's ear infection with the mother, then asks the mother about her own back pain — this is a transition
- The doctor says "Let me pull up your chart" after already having a full discussion about a different patient's condition — likely a transition

This is NOT a transition:
- Brief pauses, phone calls, or sidebar conversations DURING an ongoing patient visit
- The very beginning of the first encounter (no prior encounter to split from)
- Short exchanges or greetings with no substantive clinical content yet
- Discussion of multiple body parts or conditions for the SAME patient (one visit can cover many topics)

If you find a transition point or completed encounter, return:
{"complete": true, "end_segment_index": <last segment index of the CONCLUDED encounter>, "confidence": <0.0-1.0>}

If the current discussion is still one ongoing encounter with no transition, return:
{"complete": false, "confidence": <0.0-1.0>}

Respond with ONLY the JSON object.
```

### User Prompt Template

```
Transcript (segments numbered with speaker labels):
{formatted_segments}{context_section}
```

Where `{formatted_segments}` is the numbered transcript (see Input Format) and `{context_section}` is an optional suffix.

### Context Variants (Sensor Signals)

When a presence sensor reports departure, this is appended to the user prompt:

```

Real-time context signals:
CONTEXT: The presence sensor detected possible movement away from the room. Note: brief departures during medical visits are common (hand washing, supplies, injection preparation, bathroom). Evaluate the TRANSCRIPT CONTENT to determine if the encounter has actually concluded — a sensor departure alone is not sufficient.
```

When the sensor confirms someone is still in the room (and NOT departed):

```

Real-time context signals:
CONTEXT: The presence sensor confirms someone is still in the room. Topic changes or pauses within the same visit are NOT transitions. Only split if there is strong evidence of a different patient (new name, new history intake, greeting a new person).
```

When neither signal is active (or no sensor is available), no context section is appended.

### Optional `/nothink` Prefix

When `encounter_detection_nothink` is enabled, the system prompt is prepended with `/nothink\n`. This is a model-specific directive for Qwen3 models to disable their thinking mode.

---

## Input Format

Transcript segments formatted as:
```
[0] (Speaker 1 (87%)): Good morning, how are you feeling today?
[1] (Speaker 2 (92%)): Hi doctor. I've been having headaches for about two weeks now.
[2] (Speaker 2 (92%)): They're mostly on the right side and they get worse in the afternoon.
```

Format per line: `[{segment_index}] ({speaker_label}): {text}`

- `segment_index` — monotonic u64 starting from 0, may have gaps (segments can be dropped)
- `speaker_label` — `"Speaker N (XX%)"` with diarization confidence, or `"Speaker N"` without, or `"Unknown"`
- Lines are newline-separated
- Before being sent to the LLM, hallucination phrases are stripped from the transcript (two-phase filter: single-word then n-gram phrase repetitions)

---

## Expected Output Schema

```json
{
  "complete": true,              // boolean — required
  "end_segment_index": 15,       // u64 — required when complete=true, absent/null when complete=false
  "confidence": 0.95             // f64, 0.0-1.0 — optional (serde default)
}
```

### Field Details

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `complete` | `bool` | Yes | Whether a transition/completion was detected |
| `end_segment_index` | `u64 \| null` | When complete=true | Last segment index of the CONCLUDED encounter |
| `confidence` | `f64 \| null` | No (defaults to None) | Model's confidence in the decision (0.0-1.0) |

### Validity Rules

- `end_segment_index` must reference a segment index that exists in the input
- `confidence` should be between 0.0 and 1.0
- When `complete=false`, `end_segment_index` should be absent or null

---

## Scoring Criteria

### Primary: Split Decision Accuracy

| Metric | Definition | Target |
|--------|-----------|--------|
| **True Positive Rate** | Correctly identifies transitions | > 90% |
| **False Positive Rate** | Incorrectly splits ongoing encounters | < 10% |
| **Segment Accuracy** | `end_segment_index` within ±2 of ground truth | > 80% when complete=true |

### Secondary: Confidence Calibration

| Metric | Definition | Target |
|--------|-----------|--------|
| **High confidence correct** | Decisions with confidence ≥ 0.85 are correct | > 95% |
| **Low confidence uncertain** | Incorrect decisions have confidence < 0.7 | > 70% |

### Tertiary: Parsing Robustness

| Metric | Definition | Target |
|--------|-----------|--------|
| **Parse success rate** | Response can be parsed after cleanup pipeline | > 99% |

---

## Test Cases

### TC-1: Clear Farewell Transition (Easy)

**Input:**
```
[0] (Speaker 1): Good morning, how are you feeling today?
[1] (Speaker 2): Hi doctor. I've been having these headaches for about two weeks now.
[2] (Speaker 2): They're mostly on the right side and they get worse in the afternoon.
[3] (Speaker 1): I see. On a scale of one to ten, how would you rate the pain?
[4] (Speaker 2): Usually about a six or seven. Sometimes it goes up to an eight.
[5] (Speaker 1): Are you experiencing any nausea, vision changes, or sensitivity to light?
[6] (Speaker 2): A little bit of light sensitivity, but no nausea.
[7] (Speaker 1): Have you tried any over-the-counter medications?
[8] (Speaker 2): I've been taking ibuprofen but it only helps for a couple of hours.
[9] (Speaker 1): Based on what you're describing, this sounds like tension headaches or possibly migraines. I'd like to start you on sumatriptan.
[10] (Speaker 1): Schedule a follow-up in two weeks. If the headaches get worse, come back sooner.
[11] (Speaker 2): Thank you doctor. I'll see you in two weeks then.
[12] (Speaker 1): Take care. We'll see you soon.
```

**Expected:** `{"complete": true, "end_segment_index": 12, "confidence": ≥0.85}`

**Difficulty:** Easy — clear farewell with discharge instructions.

### TC-2: Ongoing Visit, No Transition (Easy)

**Input:**
```
[0] (Speaker 1): Good morning. What brings you in today?
[1] (Speaker 2): I've been having this knee pain for about a month now.
[2] (Speaker 1): Which knee? Can you point to where it hurts?
[3] (Speaker 2): The right one, mostly on the inside.
[4] (Speaker 1): Let me take a look. Does it hurt when I press here?
[5] (Speaker 2): Yes, right there.
[6] (Speaker 1): How about when you bend it like this?
[7] (Speaker 2): That's okay, it's more when I go up stairs.
[8] (Speaker 1): I'd like to get an X-ray of that knee. Let me also check your blood pressure while you're here.
```

**Expected:** `{"complete": false, "confidence": ≥0.85}`

**Difficulty:** Easy — ongoing visit, multi-topic but same patient.

### TC-3: In-Room Pivot — Couple Visit (Hard)

**Input:**
```
[0] (Speaker 1): Lynn, your blood work came back. Your thyroid levels are still elevated.
[1] (Speaker 2): I figured. I've been feeling tired and gaining weight again.
[2] (Speaker 1): I'd like to increase your levothyroxine from 50 to 75 micrograms.
[3] (Speaker 2): Okay. Should I still take it on an empty stomach?
[4] (Speaker 1): Yes, first thing in the morning, thirty minutes before eating. We'll recheck your TSH in six weeks.
[5] (Speaker 2): Sounds good.
[6] (Speaker 1): Now Jim, how about you? Let me pull up your chart.
[7] (Speaker 3): Well, my knees have been really bothering me. Both of them.
[8] (Speaker 1): When did that start getting worse?
[9] (Speaker 3): About three weeks ago. I can barely go up the stairs now.
[10] (Speaker 1): Let me examine your knees. Any swelling you've noticed?
```

**Expected:** `{"complete": true, "end_segment_index": 5, "confidence": ≥0.7}`

**Difficulty:** Hard — no farewell, in-room pivot from Lynn to Jim. The transition marker is the doctor addressing Jim by name and switching charts at segment 6.

**What makes it tricky:** Couple visits have no departure, no greeting ritual — just a name switch. Models often miss this because there's no "traditional" encounter boundary.

### TC-4: Topic Change, Same Patient — NOT a Transition (Medium)

**Input:**
```
[0] (Speaker 1): Let's look at your diabetes management first.
[1] (Speaker 2): My blood sugars have been running a little high in the mornings.
[2] (Speaker 1): What are your fasting readings?
[3] (Speaker 2): Usually around 140 to 160.
[4] (Speaker 1): We may need to adjust your metformin. I'd like to add a bedtime dose.
[5] (Speaker 2): Okay.
[6] (Speaker 1): Now, you also mentioned some back pain on your intake form?
[7] (Speaker 2): Yes, my lower back has been hurting for about two weeks.
[8] (Speaker 1): Is it constant or does it come and go?
[9] (Speaker 2): It comes and goes, worse when I've been sitting for a while.
[10] (Speaker 1): Let me examine your back. Can you lean forward for me?
```

**Expected:** `{"complete": false, "confidence": ≥0.7}`

**Difficulty:** Medium — the topic shift at segment 6 ("Now, you also mentioned...") looks like a transition but is the same patient with multiple complaints.

**What makes it tricky:** The doctor uses transitional language ("Now...") that mimics patient-switch phrasing but is really just addressing another complaint.

### TC-5: Farewell + New Patient Greeting (Easy)

**Input:**
```
[0] (Speaker 1): So we'll continue the current medications and I'll see you in three months.
[1] (Speaker 2): Thank you doctor.
[2] (Speaker 1): Take care Mrs. Johnson.
[3] (Unknown): [silence]
[4] (Speaker 1): Come on in. Hi, I'm Doctor Smith. What brings you in today?
[5] (Speaker 3): Hi doctor, I've been having this rash on my arms for about a week.
[6] (Speaker 1): Let me take a look. When did you first notice it?
```

**Expected:** `{"complete": true, "end_segment_index": 2, "confidence": ≥0.9}`

**Difficulty:** Easy — clear farewell at segment 2 followed by new patient greeting at segment 4.

### TC-6: Sensor-Departed, Encounter NOT Actually Over (Medium)

**Context section appended:**
```

Real-time context signals:
CONTEXT: The presence sensor detected possible movement away from the room. Note: brief departures during medical visits are common (hand washing, supplies, injection preparation, bathroom). Evaluate the TRANSCRIPT CONTENT to determine if the encounter has actually concluded — a sensor departure alone is not sufficient.
```

**Input:**
```
[0] (Speaker 1): Good morning Tracy. Let me look at your chart here.
[1] (Speaker 2): Hi doctor.
[2] (Speaker 1): So you're here for your thyroid follow-up. How have you been feeling?
[3] (Speaker 2): A little better since you adjusted my medication last time.
[4] (Speaker 1): Good. Let me check your neck. I'm going to step out for just a moment to wash my hands.
[5] (Speaker 2): Sure, take your time.
```

**Expected:** `{"complete": false, "confidence": ≥0.7}`

**Difficulty:** Medium — sensor says departure but the doctor explicitly said they're stepping out briefly. The transcript shows no farewell, no discharge, no wrap-up.

**What makes it tricky:** The sensor signal is accurate (someone did leave) but the encounter is not over. Models must weight transcript content over sensor signals.

### TC-7: Sensor-Present, Back-to-Back Encounter in Room (Hard)

**Context section appended:**
```

Real-time context signals:
CONTEXT: The presence sensor confirms someone is still in the room. Topic changes or pauses within the same visit are NOT transitions. Only split if there is strong evidence of a different patient (new name, new history intake, greeting a new person).
```

**Input:**
```
[0] (Speaker 1): Jocelyn, your labs look great. Everything's within normal limits.
[1] (Speaker 2): That's such a relief.
[2] (Speaker 1): Keep doing what you're doing. I'll see you in six months for a routine check.
[3] (Speaker 2): Thank you doctor.
[4] (Speaker 1): Now Mercedes, you're next. Let me pull up your chart.
[5] (Speaker 3): Hi doctor.
[6] (Speaker 1): So what brings you in today?
[7] (Speaker 3): I've been having these stomach cramps after eating.
```

**Expected:** `{"complete": true, "end_segment_index": 3, "confidence": ≥0.7}`

**Difficulty:** Hard — sensor says someone is present (which biases toward no-split), but there's clear evidence of a patient switch at segment 4. The model must override the sensor's conservative bias.

**What makes it tricky:** The sensor-present prompt explicitly says "Topic changes or pauses within the same visit are NOT transitions" and "Only split if there is strong evidence." The model must recognize this IS strong evidence despite the conservative framing.

### TC-8: Phone Call During Visit — NOT a Transition (Medium)

**Input:**
```
[0] (Speaker 1): Let me examine your shoulder. Can you lift your arm?
[1] (Speaker 2): Like this? Ow, it hurts right there.
[2] (Speaker 1): I see. Hold on, I need to take this call.
[3] (Speaker 1): Yes, this is Doctor Williams. Uh huh. Yes, increase the dosage to 40mg. Thanks.
[4] (Speaker 1): Sorry about that. Now, where were we? Let me check your range of motion.
[5] (Speaker 2): No problem. It hurts most when I reach overhead.
```

**Expected:** `{"complete": false, "confidence": ≥0.8}`

**Difficulty:** Medium — the phone call at segment 3 discusses a different patient's medication, which could look like a clinical topic shift.

### TC-9: Only Beginning of Visit, No Prior Encounter (Easy)

**Input:**
```
[0] (Speaker 1): Good morning. What brings you in today?
[1] (Speaker 2): I've been having chest pain on and off for the past week.
[2] (Speaker 1): Can you describe the pain? Where exactly do you feel it?
```

**Expected:** `{"complete": false, "confidence": ≥0.9}`

**Difficulty:** Easy — this is the very beginning of a first encounter. There's nothing to split from.

### TC-10: Extended Non-Clinical Gap After Clinical Content (Medium)

**Input:**
```
[0] (Speaker 1): So we'll start the antibiotics today. Take them for ten days.
[1] (Speaker 2): Okay, thanks doctor.
[2] (Speaker 1): Feel better. Take care.
[3] (Unknown): [pause]
[4] (Speaker 1): Hey Sarah, did you see the game last night?
[5] (Speaker 3): Oh my god, yes! That last-minute goal was insane.
[6] (Speaker 1): I know right. Can't believe they pulled it off.
[7] (Speaker 3): Your next patient is in room three whenever you're ready.
```

**Expected:** `{"complete": true, "end_segment_index": 2, "confidence": ≥0.85}`

**Difficulty:** Medium — clear farewell at segment 2, followed by non-clinical staff chat. The split point should be at the farewell, not deep into the staff conversation.

---

## Edge Cases & Failure Modes

### Known Production Failures

1. **STT Hallucination Inflation**: Whisper repeats phrases like "the the the the..." or "Thank you. Thank you. Thank you." hundreds of times. This inflates word count past `FORCE_CHECK_WORD_THRESHOLD` (3,000) or even `FORCE_SPLIT_WORD_THRESHOLD` (5,000), triggering premature detection checks or force-splits. The hallucination filter runs before the LLM call, but models may still see residual repetitions.

2. **False Splits on Long Single Visits**: A 45-minute procedure visit (injections, exam, discussion) with no clear farewell. The system calls detection ~20 times. Even at 5% false positive rate, the cumulative chance of at least one false split is high (~64%).

3. **Sensor-Departed False Positives**: Doctor steps out to wash hands, get supplies, or prepare an injection. Sensor correctly reports departure, but encounter is not over. The V2_soft prompt explicitly lists these scenarios but models sometimes still split.

4. **Family Member Companion Confusion**: A spouse speaks extensively about their own health while accompanying the patient. This is NOT a separate encounter — they're providing context about the patient. But their speech about their own symptoms can look like a patient transition.

5. **Non-Determinism at Temperature 0.3**: Full-day simulation showed that ~40% of detection decisions flip between runs with identical prompts. This means marginal prompt changes are unreliable — only systematic regressions are detectable.

### Edge Cases to Test

- **Very short transcript** (< 100 words) — should return `complete: false`
- **Only non-clinical content** (staff chatting) — should return `complete: false` (no encounter to complete)
- **Transcript with all `Unknown` speakers** — no diarization info, harder to detect patient switches
- **Multiple segment gaps** (e.g., `[0], [5], [12]`) — index gaps from dropped segments
- **Very long transcript** (> 5,000 words) — model may truncate or lose context
- **Mixed languages** — patient speaks non-English, doctor speaks English

---

## Prompt Variants

| Variant | Condition | Change |
|---------|-----------|--------|
| No context | No sensor, default | System prompt only, no context section |
| Sensor departed | Sensor reports absence | Appends V2_soft departure context (lists false departure scenarios) |
| Sensor present | Sensor confirms presence, no departure | Appends conservative presence context ("NOT transitions") |
| `/nothink` prefix | Config enabled | Prepends `/nothink\n` to system prompt |

---

## Parsing Robustness

Models should ideally output clean JSON, but the parser handles these common issues:

| Issue | Example | How It's Handled |
|-------|---------|-----------------|
| Think tags (closed) | `<think>reasoning</think>{"complete": false}` | Tags stripped entirely |
| Think tags (unclosed) | `<think> {"complete": false}` | Keep side containing JSON |
| Markdown fences | `` ```json\n{"complete": false}\n``` `` | Fences stripped |
| Wrapper objects | `{return {"complete": false}}` | Inner JSON extracted via fallback key prefix `{"complete"` |
| Surrounding text | `Based on analysis: {"complete": true} Done.` | First balanced `{...}` extracted |
| Combined issues | `<think> ```json\n{"complete": false}\n``` ` | Think stripped, then fences stripped, then JSON extracted |

**Benchmark should test:** For each test case, also test with the response wrapped in each of the above formats and verify parsing still succeeds.

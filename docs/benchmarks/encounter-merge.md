# Benchmark: Encounter Merge Check

## Overview

**Task**: Determine if two consecutive transcript excerpts — the tail of a previous encounter and the head of the next — are actually from the same patient visit that was incorrectly split.

**When it runs**: Immediately after every encounter split, before SOAP generation. If the merge check says "same encounter," the split is reversed and the segments are merged back.

**Why it matters**: The encounter detector has a non-trivial false positive rate (especially on long visits). Without merge-back, false splits produce incomplete SOAP notes. The merge check is the safety net. A false positive (incorrectly merging different encounters) is worse than a false negative (keeping an incorrect split), because merged transcripts can still be retrospectively checked.

**Current model**: `fast-model`

---

## Exact Prompts

### System Prompt Template

```
You are reviewing two consecutive transcript excerpts from a medical office where a microphone records all day.

The system split these into two separate encounters, but they may actually be the SAME patient visit that was incorrectly split (e.g., due to a pause, phone call, or silence during an examination).

Determine if both excerpts are from the SAME patient encounter or DIFFERENT encounters.

Signs they are the SAME encounter:
- Same patient name or context referenced
- Continuation of the same clinical discussion
- No farewell/greeting between them
- Natural pause (examination, looking at charts) rather than patient change
- Same medical condition being discussed from different angles

Signs they are DIFFERENT encounters:
- Different patient names or contexts
- A farewell followed by a new greeting
- Clearly different clinical topics with no continuity{patient_context}

Return JSON:
{"same_encounter": true, "reason": "brief explanation"}
or
{"same_encounter": false, "reason": "brief explanation"}

Return ONLY the JSON object, nothing else.
```

### Patient Name Context (Optional)

When a patient name is available (from vision-based extraction), this is inserted at `{patient_context}`:

```

CONTEXT: The patient being seen is {name}. If both excerpts reference this patient or the same clinical context, they are almost certainly the same encounter.
```

When no patient name is available, `{patient_context}` is empty (no extra text inserted).

### User Prompt Template

```
EXCERPT FROM END OF PREVIOUS ENCOUNTER:
{prev_tail}

---

EXCERPT FROM START OF NEXT ENCOUNTER:
{curr_head}
```

Where:
- `{prev_tail}` — last ~500 words of the previous encounter's transcript (plain text with speaker labels, not numbered segments)
- `{curr_head}` — first ~500 words of the new encounter's transcript

---

## Input Format

Unlike encounter detection, the merge check receives **plain text excerpts** (not numbered segments):

```
Speaker 1: Based on what you're describing, this sounds like tension headaches.
Speaker 2: Should I stop taking ibuprofen?
Speaker 1: Yes, switch to the sumatriptan. Schedule a follow-up in two weeks.
Speaker 2: Thank you doctor. I'll see you in two weeks then.
```

The two excerpts are separated by `---` in the user prompt. Each excerpt is ~500 words, taken from the boundary region of the split.

---

## Expected Output Schema

```json
{
  "same_encounter": true,      // boolean — required
  "reason": "brief explanation" // string — optional (serde default)
}
```

### Field Details

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `same_encounter` | `bool` | Yes | Whether both excerpts are from the same patient visit |
| `reason` | `string \| null` | No | Brief explanation of the decision |

---

## Scoring Criteria

### Primary: Merge Decision Accuracy

| Metric | Definition | Target |
|--------|-----------|--------|
| **True Same (Recall)** | Correctly identifies same-encounter splits | > 90% |
| **True Different (Specificity)** | Correctly identifies genuine patient transitions | > 95% |
| **Overall Accuracy** | Correct decisions / total | > 92% |

**Priority:** False merges (merging different patients) are worse than false splits (keeping an incorrect split), because:
- False splits → detected by retrospective multi-patient check or manual review
- False merges → wrong clinical information mixed into one SOAP note

### Secondary: Reason Quality

- Reason should reference specific evidence (patient name, farewell, topic continuity)
- Reason should not be generic ("seems like same encounter")

### Tertiary: Patient Name Impact

- With patient name context → accuracy should improve by ≥5% on ambiguous cases
- Without patient name → baseline accuracy still > 85%

---

## Test Cases

### TC-1: Same Encounter — Examination Pause (Easy)

**Prev tail:**
```
Speaker 1: Let me examine your knee. Can you bend it for me?
Speaker 2: Like this? It hurts right here.
Speaker 1: I see some swelling on the medial side. Let me check your range of motion.
```

**Curr head:**
```
Speaker 1: Okay, I've finished the exam. Your knee shows signs of early osteoarthritis.
Speaker 2: Is that serious?
Speaker 1: It's manageable. I'd recommend physical therapy and we'll get an X-ray.
```

**Patient name:** None

**Expected:** `{"same_encounter": true, "reason": "Continuation of knee examination..."}`

**Difficulty:** Easy — clear continuity, same knee topic, no farewell/greeting.

### TC-2: Different Encounters — Farewell + Greeting (Easy)

**Prev tail:**
```
Speaker 1: I'd like to start you on sumatriptan as needed and schedule a follow-up in two weeks. If the headaches get worse or you develop new symptoms, come back sooner.
Speaker 2: Thank you doctor. I'll see you in two weeks then.
```

**Curr head:**
```
Speaker 1: Good morning. What brings you in today?
Speaker 3: Hi doctor, I've been having this pain in my lower back for about a week.
Speaker 1: Can you describe the pain? Is it sharp or dull?
```

**Patient name:** None

**Expected:** `{"same_encounter": false, "reason": "Farewell followed by new patient greeting..."}`

**Difficulty:** Easy — textbook farewell then new greeting with different clinical topic.

### TC-3: Same Encounter — Doctor Returns After Brief Absence (Medium)

**Prev tail:**
```
Speaker 1: I'd like to start you on sumatriptan as needed and schedule a follow-up in two weeks. If the headaches get worse or you develop new symptoms, come back sooner.
Speaker 2: Thank you doctor. I'll see you in two weeks then.
```

**Curr head:**
```
Speaker 1: Take care. We'll see you soon.
Speaker 2: Thanks again doctor.
Speaker 1: Now let me update the chart with your visit notes.
```

**Patient name:** "Test Patient"

**Expected:** `{"same_encounter": true, "reason": "Continuation of same visit..."}`

**Difficulty:** Medium — the "Thank you doctor" in prev_tail looks like a farewell, but curr_head shows the doctor is still wrapping up the same visit.

### TC-4: Different Encounters — Topic Shift Without Farewell (Hard)

**Prev tail:**
```
Speaker 1: So your cholesterol is well controlled. Keep up the statin and the diet changes.
Speaker 2: Okay, thanks doctor.
Speaker 1: I'll see you in six months.
```

**Curr head:**
```
Speaker 1: Alright, next up. Let me look at the chart here.
Speaker 3: Hi doctor.
Speaker 1: So you're here about your shoulder?
Speaker 3: Yes, it's been bothering me for about three weeks.
```

**Patient name:** None

**Expected:** `{"same_encounter": false, "reason": "Different clinical topics, new patient greeting..."}`

**Difficulty:** Hard — the farewell is minimal ("Okay, thanks doctor") and could be interpreted as a transition within a visit. The "next up" is the key signal.

### TC-5: Same Encounter With Patient Name Context (Medium)

**Prev tail:**
```
Speaker 1: Your hemoglobin A1C is 7.2, which is slightly above target.
Speaker 2: I've been trying to watch my diet but it's been hard.
Speaker 1: I understand. Let's talk about adjusting your medication.
```

**Curr head:**
```
Speaker 1: I also want to discuss your foot exam results.
Speaker 2: Oh right, the nurse did that when I came in.
Speaker 1: Yes. Everything looks good, no signs of neuropathy.
```

**Patient name:** "Buckland, Deborah Ann"

**Expected:** `{"same_encounter": true, "reason": "Same patient Deborah Ann Buckland, continuation of diabetes management visit..."}`

**Difficulty:** Medium — without patient name, the topic shift from A1C to foot exam could look like different patients. The patient name context makes this unambiguous.

### TC-6: Different Encounters — Same Medical Topic (Hard)

**Prev tail:**
```
Speaker 1: Your thyroid levels are elevated. I'm increasing your levothyroxine to 75.
Speaker 2: Should I still take it on an empty stomach?
Speaker 1: Yes. We'll recheck in six weeks. Take care, Lynn.
```

**Curr head:**
```
Speaker 1: Jim, your thyroid is actually looking better.
Speaker 3: That's good to hear.
Speaker 1: We can keep the same dose for now. Your TSH is 3.2 which is right in the normal range.
```

**Patient name:** None

**Expected:** `{"same_encounter": false, "reason": "Different patients — Lynn and Jim — with separate thyroid assessments..."}`

**Difficulty:** Hard — both excerpts discuss thyroid, which could look like continuity. But different patient names (Lynn vs Jim) make these different encounters. This is the couple visit pattern from Mar 6 clinic.

### TC-7: Same Encounter — Phone Call Interruption (Medium)

**Prev tail:**
```
Speaker 1: Let me check your blood pressure.
Speaker 2: Sure.
Speaker 1: Hold on, I need to take this call. Sorry.
Speaker 1: Yes, pharmacy? Go ahead with the refill for the metformin. Thanks.
```

**Curr head:**
```
Speaker 1: Sorry about that. Your blood pressure is 128 over 82.
Speaker 2: Is that okay?
Speaker 1: It's slightly elevated. I'd like to monitor it.
```

**Patient name:** None

**Expected:** `{"same_encounter": true, "reason": "Phone call interruption during ongoing exam..."}`

**Difficulty:** Medium — the phone call discusses different medication (metformin) for potentially a different patient, but the conversation clearly resumes with the same in-room patient.

---

## Edge Cases & Failure Modes

### Known Production Failures

1. **Topic Shift Without Patient Name**: When the same medical topic spans two patients (e.g., husband and wife both have thyroid issues), the merge check incorrectly says "same encounter" because the clinical content is continuous. Patient name context eliminates this failure (33% → 100% accuracy in experiments).

2. **False Farewell Detection**: "Thank you" or "Thanks doctor" mid-visit (e.g., when the doctor explains something) can look like a farewell. The merge check should look for the full farewell pattern (farewell + departure + new greeting), not just a thank-you.

3. **Split During Silence**: When the encounter detector splits during a long exam silence, both excerpts reference the same patient and topic, but the tail may end with an unfinished thought and the head picks up mid-sentence. This should always merge back.

### Edge Cases to Test

- **Very short excerpts** (< 50 words each) — less context makes the decision harder
- **Both excerpts are non-clinical** — staff chatting, should these be "same encounter"?
- **Different speaker labels but same person** — diarization sometimes assigns different IDs to the same speaker across excerpts
- **Patient name provided but not mentioned in either excerpt** — model must still use the name context
- **Empty patient name** (empty string) — should behave same as no patient name

---

## Prompt Variants

| Variant | Condition | Change |
|---------|-----------|--------|
| No patient name | Vision unavailable or no name extracted | `{patient_context}` is empty |
| With patient name | Vision extracted a name | CONTEXT section added with patient name |
| Empty patient name | Name is empty string | Treated same as no patient name (no context added) |

---

## Parsing Robustness

Same parsing pipeline as encounter detection:

| Issue | Example | How It's Handled |
|-------|---------|-----------------|
| Think tags | `<think>reasoning</think>{"same_encounter": true}` | Tags stripped |
| Markdown fences | `` ```json\n{"same_encounter": true}\n``` `` | Fences stripped |
| Wrapper objects | `{return {"same_encounter": false}}` | Fallback key prefix `{"same_encounter"` |
| Surrounding text | `My analysis: {"same_encounter": true} End.` | First balanced `{...}` extracted |

**Benchmark should test:** Wrap each expected response in the above formats and verify parsing succeeds.

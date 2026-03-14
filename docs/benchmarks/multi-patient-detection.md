# Benchmark: Multi-Patient Detection

## Overview

**Task**: Determine if a clinical transcript contains separate visits with DIFFERENT patients — specifically, whether the doctor conducted distinct clinical assessments for different individuals.

**When it runs**: After a merge-back produces a large transcript (≥ 2,500 words, `MULTI_PATIENT_CHECK_WORD_THRESHOLD`). This is a retrospective safety check: if the encounter detector split two encounters, and the merge check said "same encounter," this task verifies that the merged result doesn't actually contain two different patients.

**Why it matters**: In couple/family visits, the encounter detector may split at the patient transition, and the merge check may incorrectly merge them back (especially when both patients have related conditions, like a husband and wife both with thyroid issues). The retrospective multi-patient check catches these cases and auto-splits the merged transcript.

**Current model**: `fast-model`

---

## Exact Prompts

### System Prompt

```
You MUST respond in English with ONLY a JSON object. No other text.

You are reviewing a clinical transcript to determine if the DOCTOR conducted separate clinical visits with DIFFERENT patients in this recording.

IMPORTANT DISTINCTION:
- A companion/partner/family member who ACCOMPANIES a patient and provides context about that patient's health is NOT a separate patient visit, even if they speak extensively. They are part of the same visit.
- A separate patient visit means the doctor conducts a distinct clinical assessment: separate history-taking, separate physical findings, separate treatment plan for a DIFFERENT individual.

Multiple patients = the doctor addresses different individuals as patients at different points, with separate clinical assessments (e.g., "Lynn, your blood work shows..." then later "Jim, your thyroid levels...").

Single patient = one person receives clinical assessment, even if others speak, provide history, ask questions, or discuss their own concerns in passing.

Return: {"multiple_patients": true/false, "confidence": <0.0-1.0>, "reason": "<brief explanation>"}
Respond with ONLY the JSON.
```

### User Prompt Template

```
{transcript_text}
```

The full merged transcript text is sent directly (not numbered segments, not truncated).

---

## Input Format

Plain text transcript, typically 2,500+ words. Includes speaker labels:

```
Speaker 1: Lynn, your blood work came back. Your thyroid levels are still elevated.
Speaker 2: I figured. I've been feeling tired and gaining weight.
...
Speaker 1: Now Jim, let me pull up your chart.
Speaker 3: My knees have been bothering me.
...
```

---

## Expected Output Schema

```json
{
  "multiple_patients": true,     // boolean — required
  "confidence": 0.95,            // f64, 0.0-1.0 — optional (serde default)
  "reason": "brief explanation"  // string — optional (serde default)
}
```

### Field Details

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `multiple_patients` | `bool` | Yes | Whether transcript contains separate patient visits |
| `confidence` | `f64 \| null` | No | Model's confidence in the decision (0.0-1.0) |
| `reason` | `string \| null` | No | Brief explanation of the evidence |

---

## Scoring Criteria

### Primary: Detection Accuracy

| Metric | Definition | Target |
|--------|-----------|--------|
| **Multi-Patient Recall** | Correctly identifies transcripts with multiple patients | > 90% |
| **Single-Patient Specificity** | Correctly identifies transcripts with only one patient | > 95% |
| **Overall Accuracy** | Correct classifications / total | > 92% |

**Priority:** False positives (claiming multiple patients when there's only one) lead to unnecessary split attempts, which are caught by the size gate (both halves must be ≥ 500 words). False negatives (missing multiple patients) mean a confused SOAP note covering two patients. Both are bad, but false negatives are clinically worse.

### Key Distinction to Evaluate

The **companion vs. patient** distinction is the most critical scoring dimension:

- A spouse who speaks 30% of the transcript about the patient's health → single patient
- A spouse who gets their own clinical assessment → multiple patients
- A child who provides history about their elderly parent → single patient
- A parent who also gets examined → multiple patients

### Secondary: Confidence Calibration

| Metric | Definition | Target |
|--------|-----------|--------|
| **High confidence correct** | Decisions with confidence ≥ 0.85 are correct | > 95% |
| **Low confidence uncertain** | Incorrect decisions have confidence < 0.7 | > 70% |

---

## Test Cases

### TC-1: Couple Visit — Both Patients Assessed (Easy)

**Input:**
```
Speaker 1: Lynn, your blood work came back. Your thyroid levels are still elevated at 8.2.
Speaker 2: I figured. I've been feeling tired and gaining weight again.
Speaker 1: I'd like to increase your levothyroxine from 50 to 75 micrograms. We'll recheck your TSH in six weeks.
Speaker 2: Okay.
Speaker 1: Also, your cholesterol is borderline at 215. Let's talk about diet modifications.
Speaker 2: I've been trying to cut back on the fried foods.
Speaker 1: Good, keep that up. We'll recheck in three months.
Speaker 2: Sounds good.
Speaker 1: Now Jim, how about you? Let me pull up your chart.
Speaker 3: Well, my knees have been really bothering me. Both of them.
Speaker 1: When did that start getting worse?
Speaker 3: About three weeks ago. I can barely go up the stairs now.
Speaker 1: Let me examine your knees. I see some swelling on the right side.
Speaker 3: It's worse on the right, yeah.
Speaker 1: I'd like to get X-rays of both knees and start you on some anti-inflammatory medication. We should also check your uric acid levels.
Speaker 3: Okay, whatever you think is best.
Speaker 1: I'll put in the orders. Come back in two weeks and we'll review the results.
```

**Expected:** `{"multiple_patients": true, "confidence": ≥0.9, "reason": "Lynn receives thyroid and cholesterol assessment, Jim receives separate knee evaluation"}`

**Difficulty:** Easy — clear patient names, distinct assessments, explicit chart switch.

### TC-2: Single Patient With Talkative Spouse (Medium)

**Input:**
```
Speaker 1: Tracy, how have you been feeling since we adjusted your medication?
Speaker 2: Much better. My energy is up and I'm sleeping better.
Speaker 3: I've noticed a big difference too. She used to be exhausted by lunchtime.
Speaker 1: That's great to hear. Tracy, are you still taking the levothyroxine every morning?
Speaker 2: Yes, on an empty stomach like you said.
Speaker 3: I make sure she takes it before breakfast. And her hair has been growing back too.
Speaker 1: Excellent. Let me check your thyroid. The levels look good on the latest labs.
Speaker 2: What about my cholesterol? Last time it was a little high.
Speaker 1: It's come down nicely to 198. The diet changes are working.
Speaker 3: I've been cooking more at home. Less takeout.
Speaker 1: That's making a real difference. Tracy, I'd like to see you again in three months.
Speaker 2: Sounds good. Thank you doctor.
```

**Expected:** `{"multiple_patients": false, "confidence": ≥0.85, "reason": "Single patient Tracy, spouse provides supportive context but receives no clinical assessment"}`

**Difficulty:** Medium — the spouse (Speaker 3) speaks extensively and even mentions their own actions (cooking), but is NOT being assessed clinically. Tracy is the only patient.

### TC-3: Family Visit — Mother + Daughter (Hard)

**Input:**
```
Speaker 1: Jocelyn, let's go over your lab results. Your blood count looks normal.
Speaker 2: Oh good. I was worried about the anemia.
Speaker 1: Your iron levels are actually fine now. The supplements worked. We can stop those.
Speaker 2: Okay, that's a relief.
Speaker 1: Your blood pressure is 118 over 76 which is great. Any other concerns?
Speaker 2: No, I feel good. Mercedes has been having some issues though.
Speaker 1: Mercedes, come sit down. Your mom mentioned you've been having stomach problems?
Speaker 3: Yeah, I've been getting cramps after eating, especially with dairy.
Speaker 1: How long has this been going on?
Speaker 3: About two months now.
Speaker 1: I'd like to test you for lactose intolerance. We'll do a hydrogen breath test. In the meantime, try avoiding dairy for two weeks and see if the symptoms improve.
Speaker 3: Okay.
Speaker 1: I'll order the test. Mercedes, come back in two weeks. Jocelyn, we'll see you in six months.
```

**Expected:** `{"multiple_patients": true, "confidence": ≥0.85, "reason": "Jocelyn receives lab review and blood pressure check, Mercedes receives separate GI assessment"}`

**Difficulty:** Hard — the transition is subtle (mother mentions daughter, doctor pivots). No farewell between them. Both assessments happen in the same visit flow.

### TC-4: Patient With Caregiver Discussing Their Own Health in Passing (Hard)

**Input:**
```
Speaker 1: Mrs. Rodriguez, how has your father been doing with the new medication?
Speaker 2: He's doing much better. The tremors have reduced a lot.
Speaker 1: That's good. Is he eating well?
Speaker 2: Yes, his appetite is back. He's gained about five pounds.
Speaker 1: Good. We want him at a healthy weight. Has he had any falls?
Speaker 2: No falls, but I've been really stressed taking care of him. I haven't been sleeping well.
Speaker 1: I'm sorry to hear that. You might want to talk to your doctor about that. Caregiver burnout is real.
Speaker 2: I should. Anyway, his memory seems about the same.
Speaker 1: That's expected with this stage. I'd like to continue the current medication and see him again in two months.
Speaker 2: Okay, I'll bring him back.
```

**Expected:** `{"multiple_patients": false, "confidence": ≥0.8, "reason": "Single patient (Mrs. Rodriguez's father), caregiver mentions own stress but is not being assessed"}`

**Difficulty:** Hard — the caregiver discusses her own health issues (stress, sleep), which could look like a second patient. But the doctor explicitly redirects her to her own doctor and continues assessing the father.

### TC-5: Two Patients, Same Family, Related Conditions (Hard)

**Input:**
```
Speaker 1: Danika, your thyroid levels have stabilized nicely at 2.8. The dose adjustment is working.
Speaker 2: I feel so much better. The fatigue is almost gone.
Speaker 1: Great. I want to keep you on the current dose and recheck in three months.
Speaker 2: My sister's been having similar symptoms. She's been really tired too.
Speaker 3: Yeah, I've been exhausted. Could it be thyroid too?
Speaker 1: It could be hereditary. Let me do a quick exam. Can you swallow for me?
Speaker 3: Like this?
Speaker 1: I can feel some enlargement. Let me order thyroid labs for you. We'll check your TSH and free T4.
Speaker 3: Okay.
Speaker 1: I should have the results in a day or two. We'll call you. In the meantime, Danika, keep taking your medication as prescribed.
```

**Expected:** `{"multiple_patients": true, "confidence": ≥0.8, "reason": "Danika receives thyroid follow-up, sister receives separate thyroid assessment with physical exam"}`

**Difficulty:** Hard — same medical topic (thyroid), family relationship, the transition is organic (sister mentions symptoms, doctor pivots to examining her). The sister's involvement starts as a "companion" comment but escalates to a clinical assessment.

### TC-6: Very Long Single Patient Visit (Medium)

Use a transcript of ~3,000 words covering a single patient with multiple complaints (headaches, diabetes follow-up, blood pressure, medication reconciliation, and vaccination discussion) — all for the same patient. The model should recognize that topic diversity does not imply multiple patients.

**Expected:** `{"multiple_patients": false, "confidence": ≥0.85, "reason": "Single patient with multiple complaints addressed in one visit"}`

**Difficulty:** Medium — the transcript length and topic diversity could confuse models into thinking there are multiple patients.

---

## Edge Cases & Failure Modes

### Known Production Failures

1. **Couple with Related Conditions** (Mar 6 clinic): Lynn and Jim, both with thyroid concerns. Merge check said "same encounter" because clinical content was continuous. Multi-patient check correctly caught this and split them.

2. **Family Visit Transitions** (Mar 6 clinic): Jocelyn and Mercedes, mother and daughter. The transition was subtle — no farewell, just mother mentioning daughter's symptoms. Multi-patient check correctly identified both.

3. **Size Gate Blocking** (Mar 6 clinic): Danika and sister — the multi-patient split was blocked because one half was < 500 words (`MULTI_PATIENT_SPLIT_MIN_WORDS`). The multi-patient check correctly detected multiple patients, but the split couldn't be executed due to the size gate.

### Edge Cases to Test

- **Three or more patients** — the prompt asks for true/false, so three patients should still return `true`
- **Patient calls in to office** — doctor takes a phone call from a patient during another visit (should NOT count as multiple patients in THIS encounter)
- **Very short merged transcript** (right at 2,500 word threshold) — minimal context for each patient
- **Interpreter-mediated visit** — interpreter speaks on behalf of patient, could look like a separate person
- **Student/resident observing** — they may ask clinical questions but are not a patient

---

## Prompt Variants

This task has no variants — the same system prompt is always used.

---

## Parsing Robustness

Same parsing pipeline as other tasks:

| Issue | Example | How It's Handled |
|-------|---------|-----------------|
| Think tags | `<think>reviewing</think>{"multiple_patients": true}` | Tags stripped |
| Markdown fences | `` ```json\n{"multiple_patients": false}\n``` `` | Fences stripped |
| Wrapper objects | `{return {"multiple_patients": true}}` | Fallback key prefix `{"multiple_patients"` |
| Missing optional fields | `{"multiple_patients": false}` | Valid — `confidence` and `reason` default to `None` |

**Benchmark should test:** Verify parsing with and without optional fields.

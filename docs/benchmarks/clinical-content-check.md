# Benchmark: Clinical Content Check

## Overview

**Task**: Classify a transcript segment as either clinical (containing a patient encounter) or non-clinical (personal conversation, staff chat, phone calls unrelated to patient care, silence/noise).

**When it runs**: After an encounter split is confirmed (not merged back), before SOAP generation. Also gated by `MIN_WORDS_FOR_CLINICAL_CHECK` (100 words) — encounters below this threshold are automatically treated as non-clinical without an LLM call.

**Why it matters**: Non-clinical segments (lunch break chat, staff discussions, phone calls) should NOT get SOAP notes generated. Generating a SOAP note from non-clinical content wastes LLM resources and produces confusing results. However, the transcript is still archived for record-keeping.

**Current model**: `fast-model`

---

## Exact Prompts

### System Prompt

```
You MUST respond in English with ONLY a JSON object. No other text, no explanations, no markdown.

You are reviewing a segment of transcript from a medical office where the microphone records all day.

Your task: determine if this transcript contains a clinical patient encounter (examination, consultation, treatment discussion) OR if it is non-clinical content (personal conversation, staff chat, phone calls unrelated to patient care, silence/noise).

If it contains ANY substantive clinical content (history-taking, physical exam, diagnosis discussion, treatment planning), return:
{"clinical": true, "reason": "brief description of clinical content found"}

If it is entirely non-clinical (personal chat, administrative only, no patient care), return:
{"clinical": false, "reason": "brief description of why this is not clinical"}

Respond with ONLY the JSON object.
```

### User Prompt Template

```
Transcript to evaluate:
{truncated_text}
```

### Truncation Rule

For long transcripts (> 2,000 words), the text is truncated to preserve the first 1,000 and last 1,000 words:

```
{first_1000_words}
[... {omitted_count} words omitted ...]
{last_1000_words}
```

For transcripts ≤ 2,000 words, the full text is used.

---

## Input Format

Plain text transcript (not numbered segments). Speaker labels are included if present:

```
Speaker 1: Good morning, how are you feeling today?
Speaker 2: Hi doctor. I've been having headaches for two weeks.
Speaker 1: I see. On a scale of one to ten, how would you rate the pain?
```

---

## Expected Output Schema

```json
{
  "clinical": true,                         // boolean — required
  "reason": "Patient history and exam"      // string — optional (serde default)
}
```

### Field Details

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `clinical` | `bool` | Yes | Whether the transcript contains clinical content |
| `reason` | `string \| null` | No | Brief explanation of the classification |

---

## Scoring Criteria

### Primary: Classification Accuracy

| Metric | Definition | Target |
|--------|-----------|--------|
| **Clinical Recall** | Clinical encounters correctly classified as clinical | > 98% |
| **Non-Clinical Precision** | Non-clinical classifications are actually non-clinical | > 95% |
| **Overall Accuracy** | Correct classifications / total | > 95% |

**Priority:** Missing a clinical encounter (false negative → clinical marked as non-clinical) is much worse than a false positive (non-clinical marked as clinical). A false negative means no SOAP note for a real patient visit. A false positive just wastes an LLM call on SOAP generation for non-clinical text.

### Secondary: Reason Quality

- Reason should identify specific clinical or non-clinical indicators
- Should be useful for human review of edge cases

---

## Test Cases

### TC-1: Clear Clinical Encounter (Easy)

**Input:**
```
Speaker 1: Good morning, how are you feeling today?
Speaker 2: Hi doctor. I've been having these headaches for about two weeks now. They're mostly on the right side and they get worse in the afternoon.
Speaker 1: I see. On a scale of one to ten, how would you rate the pain?
Speaker 2: Usually about a six or seven. Sometimes it goes up to an eight.
Speaker 1: Are you experiencing any nausea, vision changes, or sensitivity to light?
Speaker 2: A little bit of light sensitivity, but no nausea.
Speaker 1: Have you tried any over-the-counter medications?
Speaker 2: I've been taking ibuprofen but it only helps for a couple of hours.
Speaker 1: Based on what you're describing, this sounds like it could be tension headaches or possibly migraines. I'd like to start you on sumatriptan as needed and schedule a follow-up in two weeks.
Speaker 2: Thank you doctor. I'll see you in two weeks then.
Speaker 1: Take care. We'll see you soon.
```

**Expected:** `{"clinical": true, "reason": "Patient presents with headaches, clinical assessment and treatment plan discussed"}`

**Difficulty:** Easy — textbook clinical encounter.

### TC-2: Staff Personal Conversation (Easy)

**Input:**
```
Speaker 1: Hey, did you watch the game last night?
Speaker 2: Oh my god, yes! That last-minute goal was incredible.
Speaker 1: I couldn't believe it. I was screaming at the TV.
Speaker 2: Me too! My neighbors probably hate me now.
Speaker 1: So what are you doing this weekend? Want to grab lunch?
Speaker 2: Sure, how about Saturday? There's that new Thai place on Main Street.
Speaker 1: Sounds great. I'll text you.
Speaker 2: Your next patient is ready in room three, by the way.
```

**Expected:** `{"clinical": false, "reason": "Personal conversation about sports and weekend plans"}`

**Difficulty:** Easy — no clinical content at all. The last line mentions "patient" but is purely administrative.

### TC-3: Phone Call About a Different Patient (Medium)

**Input:**
```
Speaker 1: Yes, this is Doctor Williams. I'm returning your call about the lab results for Mrs. Henderson.
Speaker 1: Her hemoglobin A1C came back at 8.1, which is above her target of 7.0.
Speaker 1: I'd like to increase her metformin to 1000 milligrams twice daily.
Speaker 1: Yes, and please schedule a follow-up in three months.
Speaker 1: Thank you. Bye.
```

**Expected:** `{"clinical": true, "reason": "Phone consultation about patient lab results and medication adjustment"}`

**Difficulty:** Medium — this is a clinical phone call (discussing specific patient care), NOT the same as a personal phone call. It contains clinical decision-making.

### TC-4: Administrative/Scheduling Only (Medium)

**Input:**
```
Speaker 1: Can we move Mrs. Johnson's appointment from Tuesday to Thursday?
Speaker 2: Let me check the schedule. Thursday at 2pm is open.
Speaker 1: That works. And I need to send a referral to the orthopedic surgeon for Mr. Chen.
Speaker 2: I'll get that faxed today. Do you want me to include the X-ray results?
Speaker 1: Yes, please include the imaging report from last week.
Speaker 2: Got it. Also, the medical records request for the Patel family came in.
Speaker 1: I'll review those this afternoon.
```

**Expected:** `{"clinical": false, "reason": "Administrative scheduling and referral coordination, no patient examination or treatment discussion"}`

**Difficulty:** Medium — references patients by name and medical topics (X-rays, referrals), but no actual clinical encounter is taking place. This is office administration.

### TC-5: Minimal Clinical Content (Hard)

**Input:**
```
Speaker 1: Come on in.
Speaker 2: Hi.
Speaker 1: Have a seat. The nurse will be in shortly to take your vitals.
Speaker 2: Okay, thank you.
```

**Expected:** `{"clinical": true, "reason": "Beginning of clinical encounter — patient intake"}` OR `{"clinical": false}` — this is genuinely ambiguous.

**Difficulty:** Hard — very little content, technically the beginning of a clinical encounter but no substantive clinical discussion yet. In production, encounters this short (< 100 words) are automatically treated as non-clinical without an LLM call. This test case is for transcripts that pass the word count threshold but have minimal clinical depth.

**Acceptable answers:** Both `true` and `false` are defensible. The scoring should treat this as a "soft" test case where either answer is acceptable but `true` is preferred (avoids missing a clinical encounter).

### TC-6: Mixed Clinical and Personal Content (Medium)

**Input:**
```
Speaker 1: So your blood pressure is 130 over 85 today.
Speaker 2: Is that okay?
Speaker 1: It's a little high. Are you taking your medication regularly?
Speaker 2: Yes, every morning. By the way, I heard you're running the marathon this year?
Speaker 1: Ha, yes! Training has been rough though. But back to your blood pressure — have you been watching your salt intake?
Speaker 2: I've been trying. It's hard with the holidays.
Speaker 1: I understand. Let's keep monitoring it. I'll see you back in a month.
```

**Expected:** `{"clinical": true, "reason": "Blood pressure assessment and medication compliance discussion"}`

**Difficulty:** Medium — contains personal chitchat interspersed with clinical content. The instruction says "ANY substantive clinical content" means clinical.

### TC-7: Background Noise / Silence (Easy)

**Input:**
```
Unknown: [silence]
Unknown: [ambient noise]
Unknown: [papers rustling]
```

**Expected:** `{"clinical": false, "reason": "No speech content, only ambient noise"}`

**Difficulty:** Easy — no actual speech.

### TC-8: Patient Education Without Exam (Medium)

**Input:**
```
Speaker 1: So insulin works by helping your cells absorb glucose from the blood.
Speaker 2: I see. So that's why my blood sugar goes up when I forget my injection?
Speaker 1: Exactly. When you miss a dose, the glucose stays in your bloodstream.
Speaker 2: And that's bad for my kidneys, right?
Speaker 1: Yes, over time high blood sugar can damage the kidneys, eyes, and nerves. That's why consistent medication is so important.
Speaker 2: I'll try to be better about it. Thank you for explaining.
```

**Expected:** `{"clinical": true, "reason": "Patient education about insulin and diabetes management"}`

**Difficulty:** Medium — no physical exam or new diagnosis, but this IS clinical content (patient education is part of treatment).

---

## Edge Cases & Failure Modes

### Known Production Failures

1. **Short Non-Clinical Segments**: A quick greeting or scheduling exchange that passes the 100-word threshold but has no clinical substance. The LLM sometimes says "clinical" because it sees patient names or medical scheduling terms.

2. **Staff Huddle About Patients**: Morning staff meeting discussing patient schedules and care plans — clinical content is referenced but no encounter is happening. Should be non-clinical.

3. **Voicemail / Automated Messages**: Pharmacy callbacks, insurance hold music with automated messages, appointment reminders playing on speaker. These should be non-clinical.

### Edge Cases to Test

- **Transcript with truncation marker** — `[... 500 words omitted ...]` — model must handle this gracefully
- **All `Unknown` speakers** — no speaker differentiation
- **Non-English clinical content** — patient speaks another language, doctor in English
- **Mental health encounter** — no physical exam, entirely conversational — must still be classified as clinical
- **Nurse-only interaction** — vitals check without physician, still clinical

---

## Prompt Variants

This task has no variants — the same system prompt is always used. The only variation is whether the transcript is truncated (> 2,000 words) or not.

---

## Parsing Robustness

Same parsing pipeline as other tasks:

| Issue | Example | How It's Handled |
|-------|---------|-----------------|
| Think tags | `<think>analyzing</think>{"clinical": true, "reason": "exam"}` | Tags stripped |
| Markdown fences | `` ```json\n{"clinical": false}\n``` `` | Fences stripped |
| Wrapper objects | `{return {"clinical": true}}` | Fallback key prefix `{"clinical"` |
| Missing reason | `{"clinical": false}` | Valid — `reason` defaults to `None` |

**Benchmark should test:** Verify that responses without the optional `reason` field still parse correctly.

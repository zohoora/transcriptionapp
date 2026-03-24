# AMI Assist — Experiment Log

A structured record of every detection, merge, naming, and pipeline strategy that was tried, what the hypothesis was, the result, and what was learned. **Check this file before proposing algorithmic changes** to avoid repeating failed approaches.

---

## How to Use This File

Before proposing a change to encounter detection, merge logic, patient naming, or any clinical pipeline component:

1. Search this file for the relevant system (Detection, Merge, Naming, etc.)
2. Check the "Failed Approaches" section — your idea may have already been tried
3. Check the "Edge Cases" section — your idea may break on known cases
4. If proceeding, add an entry to this log with: hypothesis, approach, result, lesson

---

## 1. Encounter Detection

### Production Strategy (as of Mar 2026)

**"Transition-point detection"** — ask the LLM "is there a TRANSITION POINT where one encounter ends and another begins?" rather than "is this encounter complete?" This framing enables detecting in-room pivots (couples, families) where nobody physically leaves.

Key elements:
- V3 "Clean" prompt: short, focused on "clinical discussion + concluding plan"
- Elapsed `(MM:SS)` timestamps on each segment (transformative — fixed nurse pre-visit false splits entirely)
- In-room pivot language with explicit examples of chart switches and couple visits
- Dynamic confidence gate: 0.85 (<20 min) / 0.7 (20+ min), merge-back escalation +0.05 each (cap 0.99)
- Sensor context: sensor-present suppresses splits, sensor-departed uses V2_soft prompt listing common false departures

### Tried and Worked

| Date | What | Result | Why It Worked |
|------|------|--------|---------------|
| Feb 23 | "Transition-point" reframing (from "is encounter complete?") | Enabled couple/family detection | Old framing biased toward detecting endings; new framing catches in-room pivots |
| Feb 23 | Elapsed `(MM:SS)` timestamps on segments | Eliminated nurse pre-visit false splits (5/5 → 0/5) | Without temporal info, LLM couldn't distinguish 30-second pause from 10-minute empty room |
| Feb 23 | V3 "Clean" prompt (shorter, less prescriptive) | Same-day validation: 0/5 false splits, 5/5 correct splits | Verbose checklists confused the model; concise framing was clearer |
| Mar 2 | Dynamic confidence gate with merge-back escalation | Prevented repeated false splits on long sessions | Flat threshold couldn't adapt to session length; escalation learns from failed splits |
| Mar 5 | `hybrid_confirm_window_secs` 75 → 180 | Prevented false splits during procedures (injections, hand washing) | Doctor stepping away for >1 min during procedure triggered premature sensor timeout at 75s |
| Mar 5 | Sensor-departed V2_soft prompt listing common false departures | LLM evaluates transcript content rather than blindly trusting sensor | Previous prompt didn't contextualize the sensor signal |
| Feb 27 | `cleaned_word_count` for force-split thresholds | Prevented STT hallucination loops from triggering force-splits | Buckland incident: phrase repetition inflated raw count past 5K |
| Feb 27 | `ABSOLUTE_WORD_CAP` 10K → 25K | Supports legitimate 5+ hour sessions | 10K was hit by long afternoon sessions with 2-3 patients |
| Mar 5 | `hybrid_min_words_for_sensor_split` = 500 | Eliminated micro-splits (120-word, 229-word encounters) | Sensor flicker during departures triggered wakeups on trivially small buffers |

### Tried and Failed

| Date | What | Why It Failed | Lesson |
|------|------|---------------|--------|
| Feb | P1 "Conservative" prompt (require farewell BY NAME) | Too strict for family visits with no farewell | Farewells are unreliable markers — many visits end with "see you in X weeks" without naming the patient |
| Feb | P3 "Patient-Name-Aware" detection (inject known name) | False anchoring with couples — anchored on first patient's name, refused to split when second patient arrived | Using names in detection biases the model toward seeing everything as one patient |
| Feb | Vision-triggered splits (chart name change → force split) | EMR chart state is unreliable — doctor opens family members, doesn't open chart, or vision parses same name differently | **Demoted to metadata-only (Mar 2026).** Codex review agreed: "Full-screen chart-name extraction is too noisy to be a hard trigger" |
| Feb | Flat confidence threshold (single value for all encounters) | Too many false splits on short visits, not enough on long visits | Duration-based dynamic threshold was the fix |
| Feb | Force-split counter incrementing on any non-split response | Mid-visit force-splits when LLM confidently said "no split" 3 times | Changed to only count errors/timeouts; confident "complete: false" resets counter to 0 |
| Mar 5 | Marginal prompt tuning (small wording changes to sensor-departed prompt) | LLM non-determinism (~40% flip at temp=0.3) dominates marginal differences | Only systematic regressions are detectable; small prompt changes are noise |

### Known Edge Cases

| Case | Behavior | Status |
|------|----------|--------|
| Back-to-back patients, no sensor gap (Mar 24 Phil→Jason) | Sensor shows present throughout; LLM sees topical continuity; split lands mid-conversation → merge reunifies | **UNSOLVED** — biggest failure mode |
| Couple visit (same exam room) | In-room pivot language helps but relies on name switch in transcript | Works when doctor says new name; fails when transition is implicit |
| Nurse pre-visit then doctor visit (same room) | Timestamps distinguish 30-second vitals check from 10-minute gap | Solved by elapsed timestamps |
| Doctor steps away for injection prep (sensor sees absence) | V2_soft prompt + 180s confirm window prevent false split | Solved |
| Phone consultation during visit (different patient discussed) | LLM may see topic discontinuity | Partially handled by sensor-present context |
| 5+ hour continuous session with one patient | 25K word cap + hallucination filter + confidence escalation | Solved |

### Key Insight

> "The next gains will not come primarily from prompt tuning. The next gains will come from better boundary detection under uncertainty." — Codex Review
>
> "Prefer deterministic weak signals before adding another model." — Codex Review

---

## 2. Encounter Merge

### Production Strategy

**M1 (Patient-Name-Weighted)** — merge prompt includes the patient name when available. Achieved 33% → 100% accuracy on ambiguous cases in experiments.

Critical prompt line: "CONTEXT: The patient being seen is {name}. If both excerpts reference this patient or the same clinical context, they are almost certainly the same encounter."

Design is **asymmetric**: false merges (combining different patients) are considered worse than false splits (unnecessary separation), because false splits can be caught by the retrospective multi-patient check.

### Tried and Worked

| Date | What | Result |
|------|------|--------|
| Feb | M1 patient-name-weighted merge | 33% → 100% accuracy on ambiguous same-topic cases |
| Mar 2 | Confidence escalation on merge-back | Prevents repeated split-merge cycles |

### Tried and Failed

| Date | What | Why It Failed |
|------|------|---------------|
| Feb | M0 baseline (no patient name context) | When same medical topic spans two patients (e.g., husband and wife both have thyroid issues), merge incorrectly says "same encounter" |
| Feb | M2 hallucination-filtered (no name, clean text) | Not enough improvement over M0 to justify |

### Known Edge Cases

| Case | Behavior | Status |
|------|----------|--------|
| Split lands mid-conversation, same topic on both sides (Mar 24 hydromorphone) | Merge sees topic continuity and reunifies, even though they're different patients | **UNSOLVED** — merge only sees boundary, not the full 3700-word encounter A |
| No patient name available (vision failed) | Falls back to M0 behavior | Degrades gracefully but less accurate |
| Different speaker labels for same person across split boundary | Merge may see different "patients" | Rare, not blocking |

---

## 3. Patient Name Tracking

### Production Strategy

**Recency-weighted voting** from vision screenshots. Screenshot N gets weight N (linear ramp). Stale vote suppression within 90s of split. Names used for **metadata labeling only**, NOT split decisions.

### Tried and Worked

| Date | What | Result |
|------|------|--------|
| Feb | Recency-weighted voting (late chart opens get more weight) | Chart opened at minute 2 of 4: correct name wins 72% vs 28% |
| Mar | Name normalization ("Surname, Given" → "Given Surname") | Fixed "Claudia split" — different name formats no longer treated as name changes |
| Mar | Stale vote suppression (90s grace after split) | Prevents first screenshots of new encounter from voting for old patient |
| Mar 23 | Tracker snapshot before reset (not after) | Replay bundle now correctly shows votes; was showing empty due to read-after-reset |

### Tried and Failed

| Date | What | Why It Failed |
|------|------|---------------|
| Feb | Flat majority vote (1 vote per screenshot) | Wrong patient wins when chart opened late — early screenshots outnumber late ones |
| Feb | Vision as split trigger (name change → force detection check) | EMR chart state too unreliable — doctor opens family members, wrong tab, chart not visible |
| Mar 20-24 | Vision extraction in production (~230 calls, ~2 successes) | EMR chart consistently not the foreground window during encounters | **Vision is effectively non-functional** — screenshots show the Scribe/AMI app, not the EMR |

### Key Insight

> "EMR chart state is unreliable — doctor may open family members, not open chart, or vision may parse same name differently." — commit 9c74ede
>
> Vision was demoted to metadata-only. It should NOT influence detection decisions.

---

## 4. Clinical Content Check

### Production Strategy

Two-pass gate: (1) skip LLM if <100 words, (2) `fast-model` classifies clinical vs non-clinical. 30s timeout, fail-open (assume clinical on error). Transcripts >2000 words truncated to first 1000 + last 1000.

### Known Edge Cases

| Case | Behavior | Status |
|------|----------|--------|
| Phone consultation about a patient (no patient present) | Classified as clinical | Acceptable — better to generate unnecessary SOAP than miss a consult |
| 354-word phone call about antibiotics (Mar 20 Enc 4) | False positive — classified as clinical | **Known weakness** for short phone calls |
| Staff huddle about patient care plans | Correctly non-clinical | Working |
| Flush of background noise (154 words, Mar 24 Enc 4) | SOAP generated but empty | **No pre-archive clinical check on flush path** |

---

## 5. Retrospective Multi-Patient Check

### Production Strategy

3-step pipeline after merge-back produces >=2500 words:
1. Detect multiple patients (distinguishes companions from separate visits)
2. Find boundary via name transitions (not farewells — subtle in family visits)
3. Size gate: both halves >= 500 words (catches companion false positives)

### Validation (Mar 6, clinic)

| Case | Result |
|------|--------|
| Lynn + Jim couple | Correctly split at name transition |
| Tracy with spouse (companion) | Correctly NOT split |
| Jocelyn + Mercedes mother/daughter | Correctly split |
| Danika + non-patient sister | Detected as multi-patient but **blocked by size gate** (sister's portion <500 words) |

---

## 6. Hybrid Detection (Sensor + LLM)

### Production Strategy

Sensor provides early warning, LLM confirms. Sensor Present→Absent sets `sensor_absent_since`, accelerates next LLM check (~30s vs ~8 min timer). Sensor timeout force-splits after 180s sustained absence + 500 words minimum. Graceful degradation to LLM-only on sensor failure.

### Key Design Decisions

- Sensor triggers do NOT force-split — they accelerate LLM checks
- 180s confirm window (was 75s — raised after procedure false splits)
- 500-word minimum for sensor-triggered checks (prevents micro-splits from sensor flicker)
- Sensor-present context in detection prompt ("topic changes within same visit are NOT transitions")

### Simulated Impact (from design doc, 2026-02-25)

On a 5-hour, 12-encounter clinic day: 4 encounters improved (43-44 minutes faster detection), 1 false split prevented, 7 unchanged, 0 degraded.

---

## 7. SOAP Generation

### Production Strategy

Explicit section definitions (added Mar 23, 2026):
- **S**: Patient-reported info, symptoms, history, medications, social/family, ROS, AND historical test results recounted from previous visits
- **O**: ONLY today's exam findings — vitals, physical exam, new test results. Empty `[]` if no exam performed
- **A**: Clinical impressions, diagnoses, differentials
- **P**: Treatments, prescriptions, referrals, procedures, follow-up — only what was stated

### Previous Issue (Mar 20-23 forensic audit)

O section was used as a catch-all for patient-reported symptoms, historical results, and social context. Root cause: prompt had no section definitions — LLM guessed what "objective" meant.

---

## 8. Duration & Timestamp Accuracy

### Bugs Found (Mar 23 forensic audit)

- `duration_ms` wrong after merge: computed from orphan's start time, not surviving encounter's start
- `ended_at` not updated after merge: caused timestamp overlaps
- Replay bundle name tracker: read after reset (always empty)

### Fixes Applied

- `merge_encounters()` recomputes duration from surviving encounter's `started_at` → now
- `merge_encounters()` updates `ended_at` to current time
- Tracker snapshot captured before reset, used for replay bundle

---

## Open Problems (as of Mar 24, 2026)

1. **Back-to-back patients with no sensor gap** — the biggest unsolved failure mode. Sensor stays present, LLM sees topical continuity, split lands mid-conversation, merge reunifies. No current mechanism detects the patient boundary without a sensor signal or a clear name change in transcript.

2. **Vision is effectively non-functional** — EMR chart is not the foreground window during encounters. 230+ vision calls, 2 successes. The entire name extraction pipeline is a dead path in practice.

3. **Merge only sees the boundary** — when a split lands mid-topic, the merge correctly sees topic continuity across the boundary but has no visibility into the full encounter history. A 3700-word encounter with two patients looks like one topic at the split point.

4. **Flush creates spurious sessions** — no minimum word threshold or clinical check on the flush-on-stop path.

---

## Adding New Entries

When you try a new approach, add an entry with:

```markdown
### [Date] [System] — [Brief Description]

**Hypothesis**: What you expected to happen and why.
**Approach**: What you changed (code, prompt, threshold).
**Result**: What actually happened (with data — session IDs, word counts, accuracy).
**Lesson**: What was learned. What edge case was discovered.
**Status**: Adopted / Reverted / Partially adopted
```

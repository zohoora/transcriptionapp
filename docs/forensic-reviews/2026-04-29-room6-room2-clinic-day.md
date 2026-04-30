# 2026-04-29 Forensic Review — Room 6 + Room 2 clinic day

**Reviewer:** Claude (Opus 4.7) on behalf of Dr Z
**Scope:** 14 sessions across Room 2 (morning) + Room 6 (afternoon) under physician 60e4f1de — full day from 14:22 UTC (10:22am EDT) through 19:54 UTC (3:54pm EDT)
**Trigger:** Dr Z's per-session feedback (SOAP / dx / billing notes), evaluated against archived `replay_bundle.json`, `pipeline_log.jsonl`, `segments.jsonl`, `billing.json`, `metadata.json`, `soap_note.txt`, `patient_labels.json`, `soap_patient_*.txt`.
**Status:** All 7 defect classes + 4 infra gaps have been implemented (see "Implementation Status" at the end of this doc). 14 ground-truth label files were written under `tauri-app/src-tauri/tests/fixtures/labels/2026-04-29_*.json` and the `labeled_regression_cli` was extended to verify them. Changes are uncommitted on `main`; review the diff and bump SOAP/BILLING_PROMPT_VERSION + tauri.conf.json/package.json/Cargo.toml versions when ready to release.

---

## TL;DR

Today's 13 problem reports collapse to **7 generalized defect classes**. Five of them touch a single hot prompt — `build_simple_soap_prompt` (procedure section + drug-name handling) and the v0.10.61 procedure-section workflow — so a single, careful prompt change addresses ~half of today's complaints. The other classes (vision chart-stuck, telephone detection, multi-patient SOAP regen, dx-code semantic untethering, OHIP code-set audit) each need their own focused intervention.

The replay/simulation infrastructure (schema v5 captures SOAP + billing prompts) is sufficient to validate prompt changes offline — but two pieces are missing: (a) **no SOAP/billing experiment CLI** to actually replay archived bundles through new prompts, and (b) the `labeled_regression_cli` **doesn't yet check `procedure_section_correct`**, so the v0.10.61 problem class is invisible to the regression suite even though the field is in the schema. Fixing both is the highest-leverage infra investment.

---

## Index of sessions

| Time (EDT) | Patient (label) | Room | Session ID | Defects |
|---|---|---|---|---|
| 10:22 | Ruth Marie Doherty | 2 | edea83d5 | Procedure-section overcapture |
| 10:48 | Allan Wesley Nicholls | 2 | 12b5e886 | Procedure-section overcapture |
| 11:31 | Irene Martel-Jeddry | 2 | e8720971 | Procedure-section bracket leak; Dx wildly wrong (491 vs ~729); Billing missed nerve-block; OHIP code dropdown gap |
| 12:58 | Hussein Esmaili Seyghalahi | 6 | 406f0785 | ✅ correctly non-clinical |
| 1:22 | Daniel Santos Chua | 6 | 7e71d61f | Dx-code 314 (childhood ADHD) for adult patient |
| 1:28 | Angelika Marie Jaskot | 2 | 813255ff | Procedure-section overcapture; pap smear procedure missed in billing |
| 1:59 | Catherine Devouge (true) | 6 | 4a1fa43d | ✅ merge-back of 2:15pm short follow-up was correct |
| 2:19 | Carter Young-watling | 2 | 5c807cc6 | Dx-code 701 (hyperkeratosis) for an acne case — pure tools-model hallucination |
| 2:35 | "Catherine Deveuge" → actually **Shirley Rice** | 6 | f3b3de39 | Vision chart-stuck mislabel; Procedure-section overcapture; Dx-code 249 vs 715 |
| 2:49 | Janice Carey | 2 | fb2bb8d4 | Telephone visit not detected (A004A vs A102A); Drug-name hallucination (Dayvigo→estradiol) |
| 2:50 | "Sara Slote" → actually **Jaden Slote** | 6 | c42927fb | Vision multi-chart confusion (Shirley → Catherine → Jaden → Sara) |
| 3:18 | Mihai Beres | 6 | c50e7fae | ✅ clean baseline |
| 3:28 | Slote multi-patient (Jaden + Sara) | 6 | 71e3459a | Per-patient SOAP regen produced identical adult content for both |
| 3:48 | Sara Slote (label leakover) | 6 | 2e12553f | ✅ correctly non-clinical |

---

## Defect classes — generalized solutions

### Class 1 — Procedure-section overcapture (5 of 14 sessions)

**Affected:** Ruth, Allan, Irene, Angelika, Catherine 2:35/Shirley Rice. Cross-references the same failure mode in the 2026-04-27 forensic review (Heike, Kaden, Cathy, Tammy, Jim).

**Pattern:** v0.10.61's procedure-section workflow asks the LLM to identify "EVERY procedure mention" and stance-filter through `procedure_candidates` → `procedure[]`. The stance machinery works (correctly tags `OFFERED` vs `COMPLETED`), but the prompt **never defines what counts as a billable procedure** — so the LLM treats any past-tense doctor action as a candidate. Today's `procedure[]` outputs:

| Session | Action | Reality |
|---|---|---|
| Ruth | "Performed chest auscultation" | Part of physical exam |
| Allan | "reviewed blood work results" | Documentation, not procedure |
| Angelika | "Performed physical examination of both knees and hips" | Part of physical exam |
| Catherine 2:35 | "Reviewed blood work and ECG" / "Provided printed copy" / "Completed DTC form" | None are procedures |
| Irene (correct procedure!) | "performed ultrasound-guided cervical numbing injection" | Real — but with `[transcript: ...]` bracket leak |

Plus a **secondary defect** affecting all of these: the rendered SOAP includes `[transcript: "..."]` quoting the raw audio (see `llm_client.rs:2805`). This was added intentionally for billing-audit traceability, but it ends up in the user-visible SOAP body — visual clutter the user is correctly objecting to.

**Generalized fix (single prompt edit + 2-line render change):**

1. **Define a billable procedure** in the SOAP prompt (`build_simple_soap_prompt`, `llm_client.rs:1857`) — add an explicit *positive* and *negative* list. Mirror the existing `clinical_features.rs` procedure-type vocabulary so SOAP and billing share the same definition:
   - **POSITIVE (procedure[] qualifies)**: injections (joint/trigger/nerve-block/IM/SC/IV/intralesional), pap smear, IUD insertion/removal, biopsy, lesion excision/cryo/cautery, suturing/laceration repair, abscess drainage, nail avulsion/debridement, ear syringing, foreign-body removal, sigmoidoscopy/anoscopy, immunizations, intravenous administration, tonometry, epistaxis cautery/packing, hemorrhoid I&D.
   - **NEGATIVE (NEVER goes in procedure[])**: physical examination components (auscultation, palpation, ROM, BP/HR), reviewing labs/imaging/ECG, providing copies of results, completing forms (DTC, return-to-work letters, sick notes), prescription renewal, telephone call itself, patient education, counselling, "discussed", "explained", "advised".

2. **Hide transcript_quote from rendered SOAP** (`llm_client.rs:2805`). Keep `transcript_quote` in the JSON output for billing audit and replay forensics, but render only `• {action}` in the user-visible SOAP. The `[transcript: ...]` line was a forensic affordance that leaked into the clinical document.

3. **Cross-validate procedure[] against the billing vocabulary**: after JSON parse, drop any procedure[] entry whose action text doesn't fuzzy-match any `ProcedureType` in `clinical_features.rs:248-310`. This is a deterministic post-filter (no extra LLM call) that catches "reviewed blood work" / "completed DTC form" before they appear in either SOAP or billing.

**Replay/simulation evaluation:** All 14 sessions today have replay bundles (Room 2 sessions are missing replay_bundle.json on the profile-service side, but the segments + pipeline_log are sufficient to reconstruct). With a SOAP experiment CLI (currently missing — see Infrastructure section), this prompt change can be replayed against the SOAP-prompt-included bundles (schema v5) to confirm no regression on previously-correct procedure sections (e.g., today's Mihai, Daniel; pap smears in prior days).

**Risk:** Tightening procedure[] definition might miss novel real procedures. Mitigated by (a) the existing v0.10.61 stance filter still applying, (b) the user's existing custom instruction "Do not put procedures under 'O' and instead put them in either the 'P' section or a separate procedure note" already separating procedures, and (c) a `procedure_section_correct` regression check (see Infrastructure).

### Class 2 — Diagnostic code semantic untethering (5 of 14 sessions)

**Affected:** Irene (491 chronic bronchitis for cervicogenic headache), Daniel (314 hyperkinetic of childhood for adult ADHD), Carter (701 hyperkeratosis for acne), Catherine 2:35/Shirley (249 pre-diabetes for knee OA visit), Janice (311 depression for HTN/sleep call).

**Pattern:** The tools-model returns a code `C`. The pipeline (`diagnostic_tools_model.rs`) validates `C` is in the 562-entry DB and substitutes the DB description as authoritative. **But it never cross-checks the model's reasoning text against the DB description.** Carter's billing.json captures the smoking gun:

```json
"diagnosticCode": "701",
"diagnosticDescription": "Hyperkeratosis, scleroderma, keloid",     // from DB
"diagnosticEvidence": "Acne vulgaris with psychosocial distress",   // from model
"diagnosticReasoning": "Code 701 is the specific OHIP diagnostic code for acne, which directly matches the primary diagnosis of acne vulgaris..."   // model HALLUCINATION
```

The model's reasoning ("701 = acne") is internally contradictory with the DB description ("Hyperkeratosis, scleroderma, keloid"). Acne maps to **706**, which the existing Stage 2 text-matcher would correctly find via primary_diagnosis="acne vulgaris" if Stage 0 (tools-model) hadn't blocked the cascade.

**Generalized fix:** add a semantic guard between Stage 0 and the rest of the pipeline.

1. After tools-model returns `(code, description, evidence, reasoning)`, compute lexical overlap between `primary_diagnosis` (lowercased, stop-words removed, ≥4-char tokens) and `description` (same normalization). If overlap is **zero significant words**, reject the tools-model output and fall through to Stage 2 text-match. Today's failures all have zero overlap:
   - "Acne vulgaris" vs "Hyperkeratosis, scleroderma, keloid" → 0 words
   - "Cervicogenic headache" vs "Chronic bronchitis" → 0 words
   - "Knee osteoarthritis" vs "Pre-diabetes" → 0 words
   - "Hypertension management" vs "Depression / non-psychotic disorders" → 0 words
2. For age-mismatched codes (codes whose description contains "childhood" / "of children" / "infant" / "neonatal" applied to a patient with `patient_dob` indicating adult), add an extra penalty. Daniel's 314 ("Hyperkinetic syndrome of CHILDHOOD") for a 26-year-old fails this check.

**Replay/simulation evaluation:** add the four wrong-dx labels to the regression baseline (already done — see `tests/fixtures/labels/2026-04-29_*.json`). The `labeled_regression_cli` flagged 8 dx regressions across the day; after the semantic-guard fix, these should pass on re-extraction. Stage 2 (text match) reliably returns 706 for "acne vulgaris", 715 for "knee osteoarthritis", and 401 for "hypertension management" — already validated by historical labels.

**Risk:** Some clinical-narrative phrases legitimately have zero literal overlap with the DB (e.g. "PV bleeding" → 626 "menstrual disorder"). Mitigated by the existing `DX_TRUST_CONFIDENCE` ≥0.90 LLM-suggestion path which retains current behavior at high confidence. The guard only fires when tools-model and primary_diagnosis disagree — primary_diagnosis itself is a model output, so a paranoid alternative is to run the guard against the SOAP Assessment section text.

### Class 3 — Vision chart-stuck-on-wrong-patient (2 of 14 sessions)

**Affected:** Catherine 2:35 (was Shirley Rice), Sara 2:50 (was Jaden Slote).

**Pattern:** Vision OCR can only see what the EMR is showing. When the clinician doesn't switch the chart for the next patient (Shirley case) or has multiple charts open during one visit (Jaden case), vision picks up the wrong name. Today's vision sequences:

- **2:35pm Shirley visit** (15 min): 12 of 14 votes were "Catherine Deveuge" (the prior patient's chart). Early-stop fired at K=5 with the wrong name. DOB invalidation never fired because no vision call returned a DOB. The re-sample throttle finally fired at 18:51 → "Shirley Joanne Rice" — but the encounter ended at 18:50.
- **2:50pm Jaden visit** (38 min): 7×Shirley → NONE → 3×Catherine → 3×Jaden → 2×Sara (final). Recency-weighted majority picked the LAST chart open (Sara, the mother whose own visit was about to start) even though the clinical content is unambiguously the 3-year-old.

**Generalized fix (defense in depth — 3 layers):**

1. **Audio-side name extraction** (new). Parse the transcript's first 200 words for greeting patterns: `Hi/Hello/Good morning/afternoon, [Name]`, `[Name], how are you`, `Mr./Mrs./Ms. [Name]`. The doctor's first turn typically establishes who the patient is. Cross-check vision-derived name against this — if mismatch, lower vision confidence and surface a "verify patient" UI flag.
2. **Multi-name-per-encounter heuristic** (cheap). When `PatientNameTracker.unique_names.len() ≥ 3` within one encounter, the encounter is suspicious — flag for clinician review rather than picking the recency-weighted majority. Both today's bad cases would have triggered (Shirley case had 3 unique names: Devogela / Devoege / Deveuge; Jaden case had 4: Shirley / Catherine / Jaden / Sara).
3. **Encounter-to-encounter name continuity check** (new). When an encounter's vision-derived name matches the *previous* encounter's name AND the current encounter started after a sensor-confirmed person change, suspect chart-stale. Today's Catherine→Shirley transition was sensor-confirmed at 18:35; vision still claimed Catherine. The signal is there in `replay_bundle.json::sensor_transitions`.

**Replay/simulation evaluation:** Both today's bundles have full `vision_results[]` arrays with timestamps and parsed names. A simple offline analysis script (Python over `replay_bundle.json`) can compute the proposed heuristic across the existing labeled corpus (~68 days of bundles) and report false-flag rate before any code change ships. Suggested implementation: extend `vision_experiment_cli.rs` with a `--name-stability` mode.

**Risk:** Audio-side greeting parsing is heuristic — STT homophone errors will cause some name mismatches even when chart and audio agree. Mitigation: use it as a confidence signal, not a hard override. The vision-derived name still wins; greetings just lower the certainty score.

### Class 4 — Telephone visit not detected (1 of 14 sessions)

**Affected:** Janice Carey (2:49pm).

**Pattern:** Janice's transcript opens with "Hi, Diana. This is Doctor Zohor Kalin." — a textbook phone-visit greeting (clinician introduces themselves by phone). The clinical_features prompt at `clinical_features.rs:412` lists phone signals as *"calling, we'll call you back, phone visit, over the phone"* — none of which appeared in the transcript. So `setting=in_office`, `visitType=general_reassessment`, billing emits A004A instead of A102A (telephone visit, ~$30 vs ~$25 — and more importantly, the wrong service code for OHIP audit).

**Generalized fix:** expand the phone-detection signal vocabulary in the clinical_features extraction prompt (`build_billing_extraction_prompt`):

- Doctor self-introduction: `this is Dr X`, `Dr X here`, `it's Dr X`
- Doctor initiates remote contact: `I'm calling`, `I'm just calling`, `calling you about`, `calling to follow up`
- Patient verification: `Can you hear me?`, `Are you there?`, `Is this [Name]?`
- One-sided audio cues: only one speaker has audio (single-speaker dominance in segments) — but that's an audio metric, not a transcript-level signal; lift it from the `segments.jsonl`/diarization layer if needed.

Add `virtual_phone` examples to the prompt's example set — currently only one phone example (`telephone_remote` for after-hours anxiety call).

**Replay/simulation evaluation:** Today's Janice bundle has the transcript captured. Extending the regression CLI to check `setting` (currently it only checks billing codes / dx) would surface this regression class going forward. Validation path: on a SOAP-experiment CLI re-run with the expanded prompt, Janice should resolve to `setting="telephone_remote"` + `visitType="virtual_phone"` and the rule engine emits A102A.

**Risk:** Over-broad phone signals could mis-classify in-person visits where the doctor is on a Bluetooth earpiece talking to a colleague mid-encounter. Mitigation: require **two** phone signals (greeting + content cue), or anchor on the very first 30 seconds of the transcript only.

### Class 5 — Multi-patient SOAP regeneration produces identical content (1 session)

**Affected:** Slote 3:28pm (mother + 3-year-old child).

**Pattern:** `run_multi_patient_detection` correctly identified 2 patients with summaries (Child/Young Patient + Adult Female). The composite `soap_note.txt` correctly separates the content. But the per-patient `soap_patient_1.txt` and `soap_patient_2.txt` **both contain the adult's iron-deficiency content** — the child's diet content was lost.

Root cause: `build_per_patient_soap_prompt` (`llm_client.rs:2862`) and `build_per_patient_user_content` (line 2905) take the FULL transcript + the patient label + the patient summary + a description of all patients. When the user later regenerates a single per-patient SOAP via the History Window, the system uses `build_single_patient_soap_prompt` (line 2885) — which only takes the patient_label, no summary. With dominantly-adult content (~800 words vs ~50 for the child) and a generic label like "Child/Young Patient", the LLM defaults to dominant content for both regens.

**Generalized fix (two-part):**

1. **Persist per-patient summary alongside labels.** Currently `patient_labels.json` stores only `[{index, label}]`. Extend the schema to `[{index, label, summary}]` where `summary` is the per-patient summary from `MultiPatientDetectionResult.patients[].summary`. The summary is computed at detection time (before SOAP) — just save it. Update `commands/archive.rs` regen path to read `summary` and pass it to `build_single_patient_soap_prompt` via a new parameter (default fallback empty).
2. **Add child-vs-adult age signal to the prompt.** When the multi-patient detection produces labels containing "Child" / "Young" / "Adult" / "Female" / "Male", surface them as anatomical cues:
   - "Patient 1 is a CHILD — extract only pediatric content (diet, growth, daycare, immunizations, parents speaking on behalf of child)"
   - "Patient 2 is an ADULT FEMALE — extract only adult content (menses, gynecologic, work history)"

**Replay/simulation evaluation:** Slote's `soap_result.response_raw` (schema v5) contains both patient SOAPs concatenated with `\n---\n`. A SOAP-experiment CLI that re-runs the per-patient prompt with the summaries injected should produce divergent content for the child vs adult. Compare the new outputs against the existing composite (which got it right) — composite-vs-per-patient agreement is the validation criterion.

**Risk:** The summary persisted in `patient_labels.json` is a model output and may itself be wrong. Mitigated by including the summary as guidance, not as an authoritative subset of the transcript — the LLM still sees the full transcript and decides what's relevant.

### Class 6 — OHIP code-set audit gap (1 session)

**Affected:** Irene (occipital nerve block).

**Pattern:** User reported the dropdown lacked G291 / G264 for occipital nerve block. Verified — neither code is in our 235-code DB (`ohip_codes.rs`). The DB has G231A (Peripheral nerve block, one site), which is the SOB-current code per the v0.10.55 audit. Either the user's expected codes are from a different SOB version (older or different jurisdiction), or our v0.10.55 audit missed an occipital-specific entry.

A separate but related defect: the LLM's `clinical_features.procedures` extraction missed `nerve_block_peripheral` for Irene's session entirely. The SOAP procedure section explicitly listed "performed ultrasound-guided cervical numbing injection" — but the billing extraction (running on `soap_content + transcript + context_hints`) didn't pick up the nerve-block keyword.

**Generalized fix:**

1. **Re-audit the SOB for occipital-specific codes.** Run `scripts/audit_ohip_codes.py` against the latest SOB (`docs/billing/references/`) and verify whether G291/G264 are valid current codes. If yes, add. If no, document the dropdown's correct mapping (G231A) for clinician training.
2. **Improve dropdown fuzzy-search**: currently `search_ohip_codes` matches by code prefix or description substring. Add synonym mapping: searching "occipital" should surface G231A (peripheral nerve block) and G225A/G228A (cranial / paravertebral); searching "epidural" should surface G246A/G117A/G119A/G918A. This is a one-time data-driven enrichment of the OHIP code search index.
3. **Tighten the procedure→billing handoff.** The SOAP's procedure[] now contains structured `{action, transcript_quote}` — if the action contains "injection" / "nerve block" / "numbing", the billing extraction should be biased to consider `nerve_block_peripheral` / `nerve_block_paravertebral` / `joint_injection` / `intralesional_*`. Today's billing extraction missed the signal entirely. A post-extraction safety net: after `clinical_features` parse, scan SOAP procedure[] for canonical procedure verbs and warn if `procedures: []` was returned despite obvious procedure language in SOAP.

**Replay/simulation evaluation:** Compare today's billing to expected G231A inclusion. With the SOAP-experiment CLI replay through the same billing prompt but with the procedure[] cross-check post-filter active, Irene should resolve to `procedures: ["nerve_block_peripheral"]` and bill `G231A`.

**Risk:** Adding codes to the DB requires SOB verification — incorrect additions create rejected claims. The audit script (already exists per CLAUDE.md) handles this rigorously.

### Class 7 — Drug-name STT/LLM hallucination (1 session)

**Affected:** Janice (Dayvigo → estradiol; Razipan, Lemicil also questionable).

**Pattern:** STT misheard "Dayvigo" (lemborexant — insomnia). LLM "interpreted" the misheard variant as "Davigo" and added "(estradiol)" — a completely unrelated sex hormone. Estradiol is in the LLM's training-data drug knowledge, Dayvigo is newer and less common; the LLM substituted a familiar-sounding drug.

**Generalized fix:** add a **verbatim-only rule for unrecognized drug names** to the SOAP prompt and the clinical_features prompt:

> If you encounter a drug name in the transcript that you cannot positively identify, write it verbatim in quotes and append "(transcribed; verify spelling)". Do NOT substitute a different drug, even one that sounds similar. Do NOT add a generic-name annotation unless the transcript itself states the generic.

**Replay/simulation evaluation:** Janice's bundle and a few others can be re-run through the SOAP prompt with this rule added. Check that "Davigo" stays as "'Davigo' (transcribed; verify spelling)" and is NOT annotated with estradiol. Cross-check against a small Canadian-Rx corpus (~200 most common drugs) — if the LLM emits a drug not in the corpus AND not matched verbatim to the transcript, flag.

**Risk:** None significant. This is a strictly conservative rule that won't introduce errors; worst case is the LLM omits a correct annotation. Clinical safety perspective: under-annotation is far safer than substitution.

---

## Infrastructure findings

### F1 — `labeled_regression_cli` doesn't check `procedure_section_correct`

The label schema (`tests/fixtures/labels/README.md`) lists `procedure_section_correct: bool` and prior labels (e.g. `2026-04-27_b1d59dd8.json` Heike) populate it. But `tools/labeled_regression_cli.rs` only consumes `clinical_correct`, `billing_codes_expected`, `diagnostic_code_expected` — `procedure_section_correct` is silently ignored. Six of today's 14 labels assert `procedure_section_correct: false`; without CLI support, the v0.10.61 problem class is invisible to the regression baseline.

**Fix:** extend `labeled_regression_cli.rs` to read `replay_bundle.json::soap_result.response_raw`, parse `procedure[]`, and compare against the label. The check becomes "does the SOAP have a non-empty procedure[]" → matches `procedure_section_correct: false` (true means production thought there was a procedure; label says no).

### F2 — No SOAP/billing experiment CLI

The replay infrastructure has:
- `detection_replay_cli` for `evaluate_detection()`
- `merge_replay_cli` for merge-check
- `clinical_replay_cli` for clinical-content check
- `multi_patient_replay_cli` + `multi_patient_split_replay_cli`
- `vision_experiment_cli`
- `encounter_experiment_cli`

**There is no SOAP experiment CLI and no billing experiment CLI.** Schema v5 added `system_prompt` + `user_prompt` + `response_raw` to `SoapResult` and `BillingResult` *specifically* for this — but no consumer.

The natural next CLI is `soap_experiment_cli`:
1. Walk archived `replay_bundle.json` files
2. For each, build a *new* SOAP prompt using current `build_simple_soap_prompt` + proposed prompt template overrides
3. Re-issue the LLM call with the SAME user content (from `response_raw` provenance)
4. Diff the new SOAP against the archived SOAP and against any ground-truth label
5. Report deltas (procedure-section count, dx code, billing codes after rule-engine re-run)

`scripts/replay_day.py replay <date> <config>` is the closest existing tool — it does end-to-end re-transcription + re-detection + re-SOAP, but it doesn't compare against labels and is per-day not per-session.

### F3 — No SOAP/billing benchmark fixture

`tests/fixtures/benchmarks/` has 5 task suites (clinical_content_check, encounter_detection, encounter_merge, multi_patient_detection, multi_patient_split) consumed by `benchmark_runner`. **No SOAP suite, no billing suite.** The five worst SOAP failures of today (and Apr 27, and earlier days) are exactly the kind of curated case the benchmark suite was designed for — they would catch regressions on prompt edits before they hit production.

### F4 — Profile-service file allowlist excludes `replay_bundle.json` synced uploads

Inspecting today's pulls: replay_bundle.json wasn't synced to the profile service for any session, even though `is_allowed_session_file` in `profile-service/src/store/sessions.rs:40` includes it. Looking at `tauri-app/src-tauri/src/server_sync.rs::SYNCED_AUX_FILES` (per CLAUDE.md), this list defines what the tauri-side actually uploads. Either replay_bundle isn't in `SYNCED_AUX_FILES`, or there's a path-mismatch bug. This means **forensic review of Room 2 sessions today couldn't access the replay bundle** — only segments + pipeline_log were available.

**Fix:** verify `SYNCED_AUX_FILES` includes `replay_bundle.json`. If yes, debug the sync path. The forensic-review process requires bundles; missing them means we can't replay 6 of today's sessions through new prompts.

---

## Per-session forensic notes

### Ruth Doherty — 4070-word general reassessment, dx 786, A004A. Procedure section over-captured "chest auscultation"
- **Bundle:** Room 2; replay_bundle.json not synced.
- **SOAP fidelity:** Subjective + Objective + Assessment + Plan are clinically rich and accurate (cough, hyponatremia, post-op anemia, neuropathy, DDD, MD).
- **Procedure leak:** `[transcript: "Can I have a quick listen to your chest..."]` rendered in the SOAP body itself.
- **Action:** Apply Class 1 fix.

### Allan Nicholls — 1611 words, dx 250, A007A + K030A + Q040A. "Reviewed blood work" listed as procedure
- **SOAP fidelity:** Multi-issue but well-organized (post-extraction, diabetes med stop, hand trauma, mental wellbeing).
- **Procedure leak:** `reviewed blood work results [transcript: "The blood work is Super average..."]` — a false procedure.
- **Action:** Apply Class 1 fix.

### Irene Martel-Jeddry — 4864 words. Real procedure (cervical numbing injection). DX 491 wildly wrong; injection NOT BILLED; OHIP G291/G264 not in dropdown
- **SOAP fidelity:** Captures the procedure ("performed ultrasound-guided cervical numbing injection [transcript: ...]"), the polypharmacy concerns, the cervicogenic headache. But also leaks transcript brackets.
- **DX:** 491 (Chronic bronchitis) for a cervical pain visit — pure tools-model untethering. No respiratory complaint anywhere in the SOAP. Apply Class 2.
- **Billing:** Expected G231A (peripheral nerve block, ~$45) plus the assessment code — got only A007A. Per Class 6, the procedure→billing handoff is broken.
- **Drug-name issues:** Lemicil, Razipan, cimetidine-as-acetaminophen — three suspect drug names. Apply Class 7.

### Hussein Esmaili Seyghalahi — 391 words, ✅ correctly non-clinical
- **Action:** None. Used as positive baseline label.

### Daniel Santos Chua — 3692 words, dx 314 (childhood ADHD) for adult patient
- **SOAP fidelity:** Accurate (ADHD on Concerta, dose-increase request, DTC, psychiatry referral).
- **DX:** 314 description literally says "Hyperkinetic syndrome of CHILDHOOD" — inappropriate for a 26-year-old. Closest adult-ADHD codes (313, 311, 309) are all imperfect. Apply Class 2 (age-based penalty).

### Angelika Jaskot — 2775 words, dx 715 (knee OA) — pap smear DONE but not billed
- **SOAP fidelity:** Multi-issue (knee OA, sleep, menopause, mental health, screening); mentions pap-test plan AND examination. Pap was done.
- **Procedure leak:** Lists "Performed physical examination of both knees and hips" as procedure (Class 1). MORE IMPORTANTLY: pap smear was actually done but NOT in procedure[] and NOT in billing — billing has only A007A. Expected G365A (pap smear) + E430A (pap tray, auto-companion).
- **Why missed:** The SOAP mentions pap in the Plan ("schedule follow-up for Pap test") AND the Subjective ("History of abnormal Pap tests") — but the v0.10.61 stance filter would only include pap if the doctor said it in past tense ("did the pap smear"). Need to inspect the transcript to see whether the pap was actually verbalized as performed.

### Catherine Devouge 1:59pm — ✅ correctly merged with 2:15pm short follow-up
- **Action:** None — the small-orphan merge-back coordinator did the right thing. User explicitly confirmed.

### Carter Young-watling — 1724 words, dx 701 (hyperkeratosis) for acne case
- **SOAP fidelity:** Clean acne presentation with prescribed cream.
- **DX:** Pure tools-model hallucination. The reasoning text says "Code 701 is the specific OHIP diagnostic code for acne" — internally contradictory with DB description. Apply Class 2.

### "Catherine Deveuge" 2:35pm = Shirley Rice — Vision mislabel + procedure overcapture + dx wrong
- **SOAP content:** Bilateral knee OA with significant functional impairment + DTC application + DM monitoring discussion. Clinically accurate for whoever the patient was.
- **Patient identity:** WRONG. Vision saw the leftover Catherine Deveuge chart for 12 of 15 minutes; early-stop locked it in; DOB invalidation didn't fire because vision returned no DOB. Re-sample throttle finally read "Shirley Rice" at 18:51 — past encounter end (18:50). Apply Class 3.
- **Procedure overcapture:** 3 false procedures (review, copy provision, DTC form). Apply Class 1.
- **DX:** 249 pre-diabetes is a side-issue (HbA1c 6.7%), not the primary visit reason (knee OA + DTC). Apply Class 2.

### Janice Carey — Telephone visit not detected; drug-name hallucination
- **Action:** Apply Class 4 + Class 7.
- **DX:** 311 depression vs expected 401 hypertension or sleep-focused. Subordinate to Class 2.

### "Sara Slote" 2:50pm = Jaden Slote — Vision multi-chart confusion
- **SOAP fidelity:** Clearly captures the 3-year-old's presentation (diarrhea, juice/dairy, daycare). Content is correct, only the patient name is wrong.
- **Vision:** 4 distinct names within one encounter (Shirley → Catherine → Jaden → Sara). Recency-weighted majority picks Sara because the clinician opened her chart last in preparation for the next encounter. Apply Class 3 (multi-name flag).
- **DX:** 787 is GI-symptoms-broad; 009 (Diarrhea, gastro-enteritis) is the specific match.

### Mihai Beres — ✅ clean baseline
- **Action:** None.

### Slote multi-patient 3:28pm — Per-patient SOAP regen broken
- **Composite SOAP:** Correctly separates Patient 1 (child diet) vs Patient 2 (adult anemia). Multi-patient detection worked.
- **Per-patient files:** soap_patient_1.txt and soap_patient_2.txt BOTH contain adult iron-deficiency content — child content lost. Apply Class 5.
- **Billing:** Only A007A, no time entries. Likely the per-patient billing pipeline stalled; investigate whether multi-patient billing is per-patient or aggregate.

### Sara Slote 3:48pm — ✅ correctly non-clinical (vision label leftover from 3:28)
- **Action:** None — the non-clinical detector caught the 220-word tail correctly. The Sara label is a cosmetic vision-stickiness artifact.

---

## Recommended order of work

If Dr Z and the engineering process want to address today's defects in priority order (highest user-value-per-LOC first):

1. **Class 1 fix — procedure-section discipline** (~50 lines: prompt edit + render-quote-strip + post-filter): 5 sessions today, ~10 sessions across Apr 27 + today, single hot prompt. **Highest leverage.**
2. **F1 + F2 — extend regression CLI to check procedure_section_correct, add soap_experiment_cli** (~150 lines): unblocks all subsequent prompt-engineering work; without these, every prompt change is unverified.
3. **Class 2 fix — DX semantic guard** (~30 lines in `rule_engine.rs::resolve_diagnostic_code`): 5 sessions today, simple lexical-overlap check.
4. **Class 4 fix — telephone signal expansion** (~10 lines in `clinical_features.rs` prompt): 1 session today, low risk, rare phone calls but high billing-accuracy impact when they happen.
5. **Class 5 fix — persist multi-patient summaries** (~40 lines schema + plumbing): 1 session today, but the failure mode is bad enough that any future mother+child visit risks the same outcome.
6. **Class 3 fix — vision name multi-signal cross-check** (~80 lines): 2 sessions today, more invasive (audio greeting parser is new code); pair with operational fix (clinician switches chart) since vision can't fully solve the chart-stale problem.
7. **Class 7 fix — drug-name verbatim rule** (~5 lines prompt addition): 1 session today, near-zero risk.
8. **Class 6 — OHIP code audit** (research task, no LOC estimate): re-run `audit_ohip_codes.py` against current SOB; address procedure→billing handoff gap.

Items 1–4 in one prompt-engineering pass touch ~80 LOC and would have changed the user's verdict on **9 of today's 14 sessions**.

---

## Ground-truth labels written today

14 files in `tauri-app/src-tauri/tests/fixtures/labels/`:

```
2026-04-29_edea83d5.json   Ruth Doherty
2026-04-29_12b5e886.json   Allan Nicholls
2026-04-29_e8720971.json   Irene Martel-Jeddry
2026-04-29_406f0785.json   Hussein (non-clinical baseline)
2026-04-29_7e71d61f.json   Daniel Santos Chua
2026-04-29_813255ff.json   Angelika Jaskot
2026-04-29_4a1fa43d.json   Catherine 1:59 (merge-correct baseline)
2026-04-29_5c807cc6.json   Carter Young-watling
2026-04-29_f3b3de39.json   Catherine 2:35 / Shirley Rice
2026-04-29_fb2bb8d4.json   Janice Carey
2026-04-29_c42927fb.json   Sara 2:50 / Jaden Slote
2026-04-29_c50e7fae.json   Mihai Beres (clean baseline)
2026-04-29_71e3459a.json   Slote multi-patient
2026-04-29_2e12553f.json   Sara 3:48 (non-clinical baseline)
```

`labeled_regression_cli --all` baseline: **Labels: 114, Checks: 265, Pass: 244, Regressions: 18.** Today's labels added 8 of those 18 regressions (intentional — they assert the *correct* answer; production currently fails them, so future prompt changes can be validated by watching these flip from REGRESSION to OK). The other 10 regressions are pre-existing carryover from prior days (Apr 15 dx errors, Apr 24 over-split artifacts, Apr 27 K005A dedup + a30cd034 cross-machine merge bug). Clinical_correct semantic on the two non-clinical sessions was corrected in a follow-up edit (`false` = "encounter is non-clinical", not "production was right").

---

## Implementation Status (post-fix)

All 7 defect classes and all 4 infrastructure gaps have been implemented. **1339 lib tests pass** (28 added). **Regression CLI** runs against today's labels: 254/278 checks pass; the 24 outstanding regressions assert post-fix expected outputs that production will deliver after the prompt-version bump and rebuild ship to the running app.

### Files modified

| Class / Gap | File(s) | Tests added |
|---|---|---|
| F4 (sync parity) | `src/server_sync.rs` | 1 |
| F1 (CLI procedure check) | `tools/labeled_regression_cli.rs`, `src/feedback_to_label.rs` | 11 |
| Class 7 (drug verbatim) | `src/llm_client.rs` (prompt) | 1 |
| Class 4 (phone signals) | `src/billing/clinical_features.rs` (prompt) | 1 |
| Class 2 (dx guard) | `src/billing/rule_engine.rs` (`dx_description_matches_primary` + Stage 0 wrapper) | 13 |
| Class 1 (proc discipline) | `src/llm_client.rs` (prompt + render + post-filter), `src/billing/procedure_vocab.rs` (new) | 16 + existing |
| Class 5 (multi-patient summary) | `src/local_archive.rs` (schema), `src/llm_client.rs` (regen prompt), `src/commands/ollama.rs` (lookup), `src/encounter_pipeline.rs` (writer plumbing) | 1 |
| Class 3 (vision cross-check) | `src/patient_name_tracker.rs` (greeting parser, multi-name flag, vision-vs-audio match) | 15 |
| Class 6 (proc→billing) | `src/billing/clinical_features.rs` (`augment_procedures_from_soap_text`), `src/encounter_pipeline.rs` (wire-up + `extract_soap_procedure_section`), `src/commands/billing.rs` (search synonyms) | 8 |
| F2 (experiment CLIs) | `tools/soap_experiment_cli.rs`, `tools/billing_experiment_cli.rs`, `tools/soap_diff_cli.rs` (verified pre-existing + still building) | n/a |
| F3 (benchmark fixtures) | `tests/fixtures/benchmarks/soap.json`, `tests/fixtures/benchmarks/billing.json`, `tools/benchmark_runner.rs` (added `soap` + `billing` task dispatchers, extended TestCase schema) | 10 + 10 fixture cases |

Prompt versions bumped: `SOAP_PROMPT_VERSION` v0.10.61 → v0.10.67 in `llm_client.rs`; `BILLING_PROMPT_VERSION` v0.10.61 → v0.10.67 in `billing/clinical_features.rs`. App version (`tauri.conf.json` / `package.json` / `Cargo.toml`) NOT yet bumped — do that as the final commit before tagging the release.

### What this means for next clinic day

Once the rebuilt app is deployed to Room 2 + Room 6 (or auto-update fires), the regression CLI's "Regressions: 24" should drop substantially as the new prompts/rules produce outputs matching today's labeled expected codes. Run after each clinic day:

```bash
cd tauri-app/src-tauri
cargo run --bin labeled_regression_cli -- --all --fail-on-regression
cargo run --bin benchmark_runner -- soap --trials 3 --fail-on-regression
cargo run --bin benchmark_runner -- billing --trials 3 --fail-on-regression
cargo run --bin benchmark_runner -- --all --trials 3 --fail-on-regression  # all 7 task suites
```

For prompt iteration without re-running clinic-day data, use the experiment CLIs:

```bash
cargo run --bin soap_experiment_cli -- --date 2026-04-29 \
    --variant v0_10_61 --variant prompts/my_test.txt --seeds 3
cargo run --bin billing_experiment_cli -- --date 2026-04-29 \
    --variant baseline --variant visit_dx --seeds 3
```

### Outstanding work — RESOLVED 2026-04-29

All four follow-up items have been implemented:

- **Class 3 production wiring** ✅ — `screenshot_task.rs` now emits `ChartStaleSuspected{reason="multi_chart"}` once per encounter when `tracker.is_chart_likely_stale()` becomes true (≥3 unique vision-derived names), with reset on `last_split_time` bump. `continuous_mode_splitter.rs` runs `extract_greeting_name_candidates` over the just-split transcript at encounter end and emits `ChartStaleSuspected{reason="audio_mismatch"}` when `cross_check_vision_vs_audio` returns `Mismatch`, also stamping `chart_stale_suspected: true` onto `metadata.json` via the new `local_archive::patch_metadata_value` helper. New event variant added to `ContinuousModeEvent`. 5 unit tests for the metadata-patch helper.
- **Class 5 cross-day regen** ✅ — `generate_soap_note` IPC now accepts `session_date: Option<String>`, plumbed through to `lookup_patient_summary`. `HistoryWindow.tsx` regen call passes `formatDateForApi(selectedDate)`. Falls back to today's date when caller omits it (legacy backward-compat). Frontend `npx tsc --noEmit` passes.
- **Class 6 OHIP DB audit** ✅ — Audited SOB 2026-03-27 PDF: confirmed G264 + G265 + G291 + G292 are valid current codes specifically for occipital nerve blocks (G264 = first block per day, max 1/day & 16/year; G265 = each additional, $17.10; G291/G292 are IC-billed above-cap variants). Added 4 codes to `ohip_codes.rs` (count: 235 → 239), added 2 new `ProcedureType` variants (`NerveBlockOccipital`, `NerveBlockOccipitalAdditional`) mapped to G264A / G265A in `rule_engine.rs`, updated `augment_procedures_from_soap_text` to prefer occipital-specific signals over generic peripheral, updated dropdown synonym map to surface G264A/G265A/G291A/G292A first when searching "occipital". Updated billing prompt schema + `test_ohip_code_count`. Added regression test `test_occipital_nerve_block_codes_present`.
- **F2 experiment CLI offline mode** ✅ — Both `soap_experiment_cli` and `billing_experiment_cli` accept `--replay-only`, which reads `response_raw` from `replay_bundle.json` (schema v5) instead of issuing live LLM calls. `--help` shows the new flag. Lets prompt-edit re-scoring against the labeled corpus run without the LLM Router.

**Final verification:** `cargo check` clean. `cargo test --lib` — 1345 passed (6 added beyond the 1339 baseline). `npx tsc --noEmit` clean. `labeled_regression_cli --all` baseline still 254/278 pass — no regressions vs the post-fix state from the original implementation pass.

### Cross-machine sync follow-up (#3) — added 2026-04-29

`patient_labels.json` now syncs cross-room. The previous fix persisted per-patient summaries in `patient_labels.json` locally but didn't cross machine boundaries — so a multi-patient session created on Room 2 and regen'd from Room 6's History Window still fell back to label-only behavior. Closed by:

- Adding `"patient_labels.json"` to both `tauri/src-tauri/src/server_sync.rs::SYNCED_AUX_FILES` AND `profile-service/src/store/sessions.rs::ALLOWED_SESSION_FILES`. The parity test in `server_sync.rs` is updated; a sibling allowlist test (`allowlist_accepts_patient_labels`) added on the profile-service side.
- Refactoring `lookup_patient_summary` to extract a pure `lookup_patient_summary_from_bytes` parser (case-insensitive label match, returns None for missing/empty summary or invalid JSON). 6 unit tests cover the parser end-to-end including legacy-schema (no summary field) and garbage-bytes resilience.
- Wiring the IPC caller (`commands/ollama.rs::generate_soap_note`) to attempt local first, then fall back to `ProfileClient::download_session_file(phys_id, sid, "patient_labels.json")` when local returns None. Both `SharedProfileClient` and `SharedActivePhysician` are injected via `tauri::State` — frontend doesn't need to change.

`cargo test --lib` — 1351 passed (6 added). `cargo test` on profile-service — all 7 allowlist tests pass.

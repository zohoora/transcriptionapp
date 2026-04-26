# Billing-prompt engineering simulation — Apr 20–24, 2026

Frozen artifacts from the SOAP+billing prompt-engineering loop that produced the v0.10.61 prompt changes (visit-type calibration guide + dx chain-of-thought) and the `condition_keyword_guard` in `rule_engine.rs`.

## What's here

```
billing_sim_2026_04_24/
├── prompts/
│   ├── billing_v0_10_59_baseline.txt   # billing prompt before the round
│   └── soap_v0_10_61.txt               # SOAP prompt used to regenerate inputs
├── results/
│   ├── round1.json                     # 5 variants × 13 sessions × 2 seeds
│   ├── round2.json                     # 5 variants × 13 × 2 (visit guide / dx CoT introduced)
│   ├── round3.json                     # 4 variants × 13 × 3 (winner: V_VISIT_DX 30/39)
│   └── round4.json                     # 4 variants × 13 × 3 (refinement; no improvement over R3)
├── sim/
│   ├── regen_soaps.py                  # phase-1 SOAP regeneration with v0.10.61 prompt
│   └── round{1,2,3,4}.py               # phase-2 billing variants × seeds
├── soap_prompt_engineering/            # earlier SOAP-prompt loop (C1 + Procedure section)
│   ├── results/c1_e2e.json             # end-to-end SOAP→billing on this week
│   └── results/iter{1,2,3}.json        # 12 SOAP-prompt variants × seeds
└── README.md
```

## Method (billing loop)

1. **Stage** transcripts + production billing.json + metadata for 13 sessions covering all ground-truth categories from the Apr 20–24 clinic week.
2. **Regenerate SOAPs** through `soap-model-fast` with the v0.10.61 SOAP prompt (the one shipped in `llm_client.rs::build_simple_soap_prompt`). These act as the controlled input for the billing loop.
3. **Run billing variants** against those SOAPs. Variants explored:
   - `V_BASE` — production billing prompt + SOAP + transcript (the shipped baseline)
   - `V_SOAP_ONLY` — same prompt, transcript dropped
   - `V_DURATION` — SOAP + duration field
   - `V_STRICT` — SOAP + Procedure-section binding rider
   - `V_PROC_BIND` — deterministic procedure binding from SOAP.procedure[]
   - `V_VISIT` — SOAP-only + visit-type calibration guide
   - `V_DX_COT` — SOAP-only + dx chain-of-thought
   - `V_VISIT_DX` — both calibration + CoT (round-2 winner)
   - `V_VISIT_DX_STRICT` — `V_VISIT_DX` + strict condition rider
   - `V_VISIT_DX_PHONE` — adds telephone-detection rider
   - `V_VISIT_DX_BIND` — adds deterministic procedure binding
   - Round 4: `V_R4_PHONE`, `V_R4_DIAB`, `V_R4_FULL` (riders that didn't improve)
4. **Score** against per-session ground truth: visit_type, procedures (OR-set), conditions (no hallucinations), diagnostic code (acceptable codes per session).

## Headline result

| Round | Champion | All-correct | Hallucinations |
|---|---|---|---|
| 1 (baseline) | V_BASE | 13/26 (50%) | 0 (small sample) |
| 2 | V_VISIT_DX | 20/26 (77%) | 1 |
| 3 (3 seeds) | V_VISIT_DX | 30/39 (77%) | 3 (Alexander × 3) |
| Final (R3 + code-side post-filter) | V_VISIT_DX + `condition_keyword_guard` | 30/39 + **0/39 hallucinations** | **0** |

The chosen production combination is the v0.10.61 prompt (`build_billing_extraction_prompt` in `clinical_features.rs`, which now contains the visit guide + dx CoT) plus the SOAP-text keyword guard in `rule_engine.rs::condition_keyword_guard`.

## Re-running

The scripts under `sim/` reference local paths (`/tmp/sim_billing/...`) where the input transcripts and SOAPs were staged during the original run. These are NOT checked in (they contain PHI). To re-run end-to-end:

1. Set `BASE` to your archive root and stage transcripts + billing.json into the working dir.
2. Run `regen_soaps.py` to produce `soap_v061_<sid>.json`.
3. Run `round1.py` through `round4.py` to reproduce the variant comparisons.

Or, more directly, the methodology is portable to any clinic week: pick 10-15 sessions covering ground-truth categories (correct, condition-hallucination, procedure-hallucination, dx-hallucination, telephone, multi-issue), label them, and run the variants. The scripts under `sim/` are the template.

## Ground truth schema (in `sim/round1.py`)

```python
GROUND_TRUTH = {
    "<short_session_id>": dict(
        name="<patient short name>",
        visit_type="general_reassessment",      # or None for "any acceptable"
        procedures={"nerve_block_paravertebral", "joint_injection"},  # OR-set: any one OK
        procedures_optional={"im_injection_with_visit"},              # also OK if present
        forbidden_conditions={"diabetic_assessment", "smoking_cessation"},
        ok_dx={"724", "722"},                                          # any of these acceptable
        notes="...",
    ),
    ...
}
```

The schema is intentionally loose (OR-sets, optional procedures) because most clinical decisions have multiple valid answers.

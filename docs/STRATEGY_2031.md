# AMI Assist — 5-Year Strategic Horizon (2026 → 2031)

*Companion to STRATEGY_2026.md. That document optimizes for shipping revenue in 12 months. This document optimizes for being in a uniquely defensible position in 60 months. The two can coexist, but they imply very different choices. Reading order: this document first if you have 5-year ambition; the 2026 one if you need revenue in 12.*

---

## 0. Executive thesis

**The only worthwhile 5-year bet is one that trades go-to-market speed for a kind of defensibility that's structurally impossible to replicate on a 12-month timeline.** In medical AI, there are exactly three such moats: (1) regulatory clearance (FDA / Health Canada Class II or higher), (2) clinical validation at scale, and (3) architectural patterns that make AI behavior *verifiable* — auditable, reproducible, drift-monitored, adversarially tested. Everyone else competes on model quality and EMR integration, both of which commoditize.

**The bet I'd make**: use the 5 years to position AMI Assist not as a scribe-that-happens-to-be-regulated, but as **the first verifiable clinical AI platform targeted at Canadian specialty practices**, with a Health-Canada-cleared flagship product in one high-value specialty proving the platform works. This uses almost every asset in the current codebase — replay logging, deterministic rules over LLM output, local-first architecture, sensor integration, OHIP billing depth — in a way that generic scribe competitors *cannot pivot to* because their cloud-first, speed-to-market architectures were built for a different game.

**The uncomfortable truth**: this path is capital-heavy (roughly $25-40M through year 5), requires a clinical validation partnership with an academic medical center, and demands team scaling to 10-15 people by year 3. A 5-year solo-or-near-solo bet is not realistic in this direction. The alternative — Alt 2 below (AI-native DPC operator) or Alt 4 (patient-facing longitudinal companion) — can be smaller-capital but require very different execution muscles. **Pick based on what kind of company you want to build, not just what the market wants.**

---

## 1. What 2031 likely looks like (three scenarios)

Predictions at 5-year horizons are mostly wrong. Scenarios are honest; single-point predictions are arrogance. Here are three plausible 2031 states of the medical-AI market, and what each implies for a defensible product direction.

### Scenario A — "Consolidation under EMRs" (40% likely)

Epic and Oscar/TELUS absorb the ambient scribe category. Every EMR ships native ambient AI at baseline quality. Nuance DAX is integrated into Microsoft Copilot for Healthcare; Abridge is acquired by Epic; Tali merges with a major Canadian EMR. Standalone scribes consolidate to 3-5 survivors competing on price.

**Premium segment**: Specialty-specific clinical decision support with FDA/Health Canada clearance. Platform + product plays targeted at specific specialties or chronic disease management. Regulatory-grade tools with verifiable behavior. AI-native DPC chains operating vertically integrated care.

**What matters**: Regulatory clearance. Specialty depth. Clinical outcomes data. Verifiable AI behavior (auditable, reproducible). Direct patient relationship.

### Scenario B — "Agentic primary care" (30% likely)

The "AI as a member of the care team" vision (Deloitte, Lumeris, Lancet Primary Care 2025-26) matures. Autonomous agents handle routine care tasks — cancer screening follow-up, vaccination scheduling, medication adherence, post-discharge coordination. Health systems adopt agentic AI at fleet scale. Medicolegal frameworks adapt. Value-based payment expands to accommodate AI-assisted population health management.

**Premium segment**: Platforms that coordinate multi-agent clinical operations. Sophisticated outcome prediction. Patient-agent pairings with clinical oversight. Verifiable goal-directed AI with clear escalation protocols.

**What matters**: Multi-agent orchestration. Longitudinal patient models. Outcome prediction + measurement. Clinical oversight tooling. Auditability of agentic decisions.

### Scenario C — "Fragmented + regulated" (20% likely)

A major AI safety incident in healthcare (bad diagnosis with injury, agentic action with harm, large-scale data breach) triggers aggressive regulation. FDA moves much more of clinical AI into Class II/III pathways. Health Canada follows. Europe's EHDS expands regulatory scope. ROI calculations for AI medical devices become heavier; speed-to-market collapses in favor of validation. Small startups exit the category. Only companies with regulatory infrastructure survive.

**Premium segment**: Regulated clinical AI products with clear validation histories. Platforms for building regulated AI. Clinical trial partners. Physician-in-the-loop systems with strong audit trails.

**What matters**: Regulatory pathway progress. Validation data. Audit infrastructure. Physician oversight tooling. Drift detection. Recall/withdrawal capability.

### Scenario D — residual "Something unforeseen" (10% likely)

Direct neural interfaces, foundation models that obsolete specialty knowledge, mandatory AI disclosure radically changing patient behavior, geopolitical disruption to AI supply chains, etc. Unpredictable.

### The strategic overlap across scenarios

The common thread across Scenarios A, B, and C — which together cover 90% of the probability mass — is that **verifiable, regulatory-grade clinical AI becomes the premium segment**. In consolidation (A), it's the only remaining premium. In agentic (B), it's the infrastructure that lets agents operate safely. In fragmented-regulated (C), it's the only survival path.

**The strategic direction that wins in A, B, and C simultaneously is: become the company that has the clearest path from "we have something to say about medical AI" → "we have regulatory clearance to actually do it."** That path takes roughly 5 years from a standing start. Companies that start in 2028 will launch in 2033 — three years behind. That's the window.

---

## 2. The thesis: "Verifiable Clinical AI Platform for Canadian Specialty Practices"

Three interlocking layers. Each is independently valuable; together they compound.

### 2.1 Layer 1 — Verifiable Clinical AI Platform (internal infrastructure)

The underlying engine that makes AMI Assist's AI behavior *regulatable*. Extracted from current architecture but generalized beyond scribing:

- **Reproducible LLM call pattern**: Every AI decision has captured inputs, prompt, response, parsing, and downstream action. Replay bundles (already v3) are the foundation. Generalize to any LLM-backed clinical decision, not just encounter detection.
- **Deterministic reasoning over LLM output**: Rules engines (billing, diagnostic resolution) demonstrate the pattern — LLM extracts features, deterministic code makes the regulated decision. This is the *only* architecturally honest approach to FDA/Health Canada SaMD requirements because the LLM part is non-deterministic and the deterministic part is auditable.
- **Drift monitoring**: Per-step LLM call metrics (just landed v0.10.36) extend into distribution monitoring. "Is the LLM behaving differently this month than last?"
- **Adversarial testing harness**: The replay tools are a fuzzer for prompts. Extend to systematic adversarial testing — does the model fail on specific demographics, procedure types, languages, comorbidity combinations? This is what FDA inspection looks like.
- **Clinical evidence collection**: Structured feedback loop from physician corrections to model behavior analysis. Required for regulatory post-market surveillance.
- **Audit log** with cryptographic integrity: Every clinical decision reproducible N years later. Already most of this exists.
- **Clinical safety case framework**: Documentation template for proving a specific AI feature is safe enough to deploy. Aligned with IEC 62304 + IEC 82304 + FDA's AI/ML SaMD Action Plan + Health Canada's Pre-market Guidance for Machine Learning-enabled Medical Devices.

This layer is not sold separately in year 5 (that's a distraction). It's the operational infrastructure that lets Layer 2 be built safely and defensibly. It may become a product in year 6+.

### 2.2 Layer 2 — Flagship Specialty Clinical Product

One Ontario specialty, deeply focused. Recommendation remains **chronic pain management** (as argued in STRATEGY_2026 § 5.1) with expansion to **addiction medicine** in year 3-4 because of structural overlap (opioid dose tracking, buprenorphine titration, relapse prediction, procedure + counselling split).

Over 5 years the product evolves from "scribe + specialty billing" (year 1) to:
- **Year 1**: Scribe + OHIP billing + Oscar Pro integration (pain clinic documentation tool)
- **Year 2**: Longitudinal patient memory (pain scales, procedure response, opioid equivalent trajectory, mood trajectory, functional status trajectory)
- **Year 3**: Clinical decision support (non-regulated tier) — evidence-based suggestions for dose adjustments, protocol deviations, escalation triggers
- **Year 4**: Regulatory clearance filed for specific Class II decision support (e.g., opioid dose optimization recommendation)
- **Year 5**: **Regulated clinical decision support** launched. Product is meaningfully different from competitors: a Health-Canada-cleared AI that actively makes recommendations during patient care with validated evidence behind them.

### 2.3 Layer 3 — Regulatory Moat

The thing that compounds over 5 years. Competitors can't catch up by working harder.

- **Year 2**: Clinical validation study designed in partnership with Ontario academic medical center (U of T, McMaster, McGill, UBC — pain management research groups exist at all). Study scope: does AMI Assist's decision support improve outcomes vs. standard of care for chronic pain management in FHO+ clinics?
- **Year 2-3**: Study enrollment and execution (~18 months typical).
- **Year 3**: Interim analysis. Publication. Peer-reviewed evidence of efficacy + safety.
- **Year 4**: Health Canada Class II submission for specific decision support indications. Typical review cycle 12-18 months with back-and-forth.
- **Year 5**: Clearance. Launch regulated product.

By year 5, AMI Assist has something no generic scribe competitor has: evidence in peer-reviewed journals + regulatory clearance + post-market surveillance infrastructure + a clinical validation partnership that can run *the next* study. This is the actual moat. Tali, Abridge, DeepScribe — none of them will have this in 2031 because their business models don't require it and their architectures weren't built for it.

---

## 3. What assets in the current codebase compound over 5 years

Specific to the codebase as I've seen it. These become *more* valuable in year 5 than year 1:

| Asset | Today's value | Year-5 compounded value |
|-------|---------------|--------------------------|
| **Replay logging architecture** (schema v3, 12 regression CLIs) | Offline testing | Foundation of regulatory auditability; every clinical AI decision replayable N years later |
| **Deterministic rules over LLM output** (billing engine pattern) | OHIP billing | Pattern for Class II decision support — LLM extracts features, deterministic code makes regulated decision |
| **Local-first architecture** | Privacy story | Data residency compliance (PIPEDA + EHDS + evolving Canadian data sovereignty) — competitors running cloud can't catch up |
| **Custom sensor firmware + fusion** (ESP32, mmWave, thermal, CO2) | Encounter detection | Physical-world anchor for clinical context awareness; expansion to wearables, ambient biometrics |
| **Server-configurable config** (prompts, rules, thresholds) | Runtime tuning | Predeterminate Change Control Plan (PCCP) infrastructure — FDA's 2024 finalized framework |
| **Multi-room deployment + auto-update** | Clinic-scale product | Fleet management for clinical AI validation across sites |
| **OHIP billing depth** | Revenue capture | Evidence of deep domain execution; Ontario anchor for multi-province expansion |
| **Multi-patient encounter handling** | Couples visits | Pattern for complex clinical contexts (family medicine, group therapy, shared care) |

Not in the table but important: **single-developer velocity history demonstrates execution discipline**. Investors evaluate this. 30 releases in ~6 months with improving test coverage, a formal ADR discipline, and an actively instrumented product — this is above-average engineering for a healthcare startup.

---

## 4. What to build over 5 years (phased)

This is the part people get wrong: they plan year 5 in detail and ignore year 1. Planning year 1 in detail with soft year-2-to-5 outlines is more honest.

### Phase 0 — Decision point (months 0–2)

Before committing, validate the core assumptions:
- Does a high-quality chronic pain clinician feel the product direction solves a real gap? (10 clinician interviews)
- Is Health Canada currently clearing clinical decision support in pain medicine? (Regulatory consultant - ~$10K)
- Is there an Ontario academic medical center interested in partnership? (2-3 informal conversations)
- Is there investor appetite for a 5-year clinical validation play at this stage? (3-5 informal conversations)

If these four come back lukewarm, the 5-year plan is the wrong shape for the market/partners. Revert to the 1-year plan (STRATEGY_2026) without shame.

### Phase 1 — Codebase consolidation + Layer 1 platform emergence (months 3–12)

Same cuts and refactors as the 1-year plan (STRATEGY_2026 § 7), but with a different target shape. Here the work is prep for platform extraction, not just revenue.

Specific deliverables:
- Kill non-strategic features (~7K LOC as specified in 2026 plan)
- Refactor `continuous_mode.rs` into phase modules; introduce `PipelineBus` pattern
- Windows port (enables target-clinic expansion)
- Oscar Pro integration (read-only + write)
- Formalize Layer 1 patterns: rename `replay_bundle` → `clinical_decision_record`; rename `pipeline_log` → `clinical_audit_log`; document the verifiable AI pattern.
- Pain specialty product wedge validation

Team at end of phase: founder + 1 full-time engineer + 1 clinical advisor (part-time).

### Phase 2 — Layer 1 formalization + Layer 2 product depth (months 13–24)

The platform pattern becomes explicit. Layer 1 starts to shed its "emerged from Layer 2" shape and becomes a clean abstraction. Simultaneously Layer 2 gets the specialty depth that justifies premium pricing.

Specific deliverables:
- Extract Layer 1 into its own crate(s): `clinical-ai-core` (traits + patterns) + `clinical-ai-verification` (audit, replay, drift monitoring).
- Predeterminate Change Control Plan (PCCP) design for Layer 2's eventual regulatory path. Work with regulatory consultant.
- Longitudinal patient memory: multi-year patient graph for chronic pain. Pain-scale trajectories, procedure response, medication history, functional status. Bidirectional Oscar Pro.
- Clinical evidence collection infrastructure: structured physician-correction feedback, outcome tagging, PROM (patient-reported outcome measure) integration.
- Clinical validation study design + IRB submission.
- Addiction medicine adjacent expansion (second specialty on Layer 1).

Team at end of phase: founder + 3 full-time + clinical research coordinator + regulatory consultant.

### Phase 3 — Clinical validation + Layer 2 clinical decision support (months 25–36)

Run the clinical study. Build the thing being studied. They inform each other.

Specific deliverables:
- Clinical validation study active enrollment (target: 10 FHO+ chronic pain clinics, ~400-600 patients, 12-month endpoint).
- Non-regulated decision support features live in product (evidence-graded, not regulated).
- SOC 2 Type 2 achieved. PIPEDA certification in-hand.
- Health Canada Class II pre-submission meeting.
- First peer-reviewed publication submitted.

Team at end of phase: 5-7 full-time + clinical study coordinator + regulatory lead + clinical advisory board (5-7 physicians).

### Phase 4 — Regulatory filing + commercial hardening (months 37–48)

Filing is slow. Market prep in parallel.

Specific deliverables:
- Health Canada Class II submission filed (~month 38).
- Clinical validation study 12-month results published.
- Fleet management for multi-clinic deployments.
- Canadian multi-province billing support (BC MSP, Alberta AHS — Ontario expertise generalizes).
- Second Layer 2 product (addiction medicine) in late beta.
- Layer 1 platform usable by a second internal product; prep for external use in year 5.

Team at end of phase: 8-10 full-time.

### Phase 5 — Launch + scale (months 49–60)

Clearance received in year 5 assuming submission year 4. Exit R&D mode.

Specific deliverables:
- Regulated product launch (chronic pain clinical decision support).
- Second specialty product launch.
- Layer 1 platform opened to 1-2 strategic partners (could be open-source or commercial license).
- International expansion consideration (US via FDA pathway leveraging Health Canada data).
- Series C positioning.

Team at end of phase: 10-15 full-time.

---

## 5. Funding profile and when to raise

Not a financial plan. Orders of magnitude, honestly labeled.

| Phase | Months | Spend | Source | Notes |
|-------|--------|-------|--------|-------|
| Phase 0 | 0-2 | <$50K | Founder / bootstrap | Validation calls, regulatory consultation |
| Phase 1 | 3-12 | $500K-$1M | Pre-seed / SAFE / founder + angels | 1 engineer, part-time clinical advisor, infrastructure |
| Phase 2 | 13-24 | $2-3M | Seed round | 3 engineers + regulatory consultant + clinical advisor |
| Phase 3 | 25-36 | $3-5M | Series A | Team growth + clinical study execution |
| Phase 4 | 37-48 | $5-10M | Series A extension / B | Regulatory submission + commercial prep |
| Phase 5 | 49-60 | $10-20M | Series B | Launch + scale + second product |
| **Total** | **60** | **$25-40M** | — | — |

Non-dilutive candidates: NRC IRAP (~$500K for R&D tax credits on Rust engineering), CIHR clinical validation grants (~$500K-1M), Ontario Centres of Innovation (~$200K), Strategic Innovation Fund (possible $1-3M Phase 3+), Health Canada's Women's Health Research Fund if applicable to specialty. Worth ~$2-3M across phases — materially reduces dilution.

Strategic investors to consider: Ontario-based family-office healthcare funds, Telus Ventures, Canadian pension funds with health-tech mandates, Medallion Ventures (if they fund healthcare), Klass Capital (Canadian health tech specialist).

---

## 6. Risks that kill this thesis (and mitigations)

### 6.1 Regulatory timeline slippage

Health Canada review cycles are unpredictable — 12-18 months is typical but 24+ happens. Consequence: year-5 launch slips to year-6, runway stress.

**Mitigation**: Revenue from non-regulated product starts year 2. Milestones tied to regulatory submission, not clearance. Alternative pathway (FDA De Novo) as backup.

### 6.2 Clinical validation fails

The study doesn't show statistically significant outcome improvement vs standard of care. Consequence: regulatory submission either filed with weaker evidence or delayed for another study (~18 more months).

**Mitigation**: Pick a clinical indication where current practice is clearly sub-optimal (opioid dose management has known risk-benefit gaps). Design the study with interim analysis + adaptive enrollment. Consult with Health Canada on acceptable evidence threshold before study start.

### 6.3 Competitor with deeper pockets pivots into regulatory

Abridge raises another $500M, hires a head of regulatory, and starts a chronic-pain-specific study. Consequence: AMI Assist's moat duration shortens.

**Mitigation**: Head-start is real — competitor catching up takes 2-3 years minimum. Use the time to build the second specialty product on Layer 1, making the platform the moat even if product-level first-mover advantage erodes.

### 6.4 Team scaling failure

10-15 engineers in 5 years in a solo-founder-origin company with specialty healthcare + Rust + regulated AI expertise requirements. Hard hire profile.

**Mitigation**: Hire the first 2-3 from personal network or from the Canadian healthcare-tech community where hiring is less saturated than SF. Prioritize clinical advisor who can recruit physicians. Outsource regulatory + clinical study operations to specialized consultants in years 2-3 rather than building in-house.

### 6.5 Paradigm shift

Foundation models become so good that specialty knowledge is irrelevant. Or neural interfaces obsolete screen-based documentation. Or Canada nationalizes healthcare AI and specifies approved vendors. Unpredictable.

**Mitigation**: Phase 1-2 deliverables create value independent of the regulatory bet (specialty scribe, Oscar integration, longitudinal memory). If the thesis collapses, pivot to STRATEGY_2026's path with 2 years of investment already banked.

### 6.6 The founder burns out

Realistic risk on a 5-year timeline for any solo-founder start. Consequence: company dies or gets acquired for fire-sale value.

**Mitigation**: Not something strategy documents solve. Build the team, delegate seriously by year 2, take rest, be honest with co-investors.

### 6.7 The thesis is simply wrong

5-year medical-AI predictions have a bad historical track record. 2021 predictions for 2026 mostly missed where the market actually went.

**Mitigation**: Build optionality. Phase 1 cuts + refactors + Oscar integration make the codebase more valuable in *any* scenario. Layer 1 platform patterns have value even if Layer 3 regulatory moat doesn't work out. Exit ramps exist at months 12, 24, 36.

---

## 7. What this plan doesn't do (deliberately)

Things that might seem obvious but aren't the right call for this specific thesis:

- **Doesn't target the US market directly in year 1-3.** FDA is possible eventually (via 510(k) referencing Health Canada clearance) but the Canadian play is cleaner and more addressable.
- **Doesn't build the "Clinic OS" ambition (Path A from 2026 plan).** That's a different company. Clinic operations + AI is interesting but diffuses focus away from regulatory moat.
- **Doesn't pursue open-source core immediately.** Layer 1 could open-source in year 5 as strategic maneuver. Before that, the pattern value is higher as proprietary infrastructure.
- **Doesn't target multiple specialties simultaneously in years 1-2.** Depth beats breadth for regulatory validation. Addiction medicine as second vertical starts year 3, not year 1.
- **Doesn't aggressively pursue agentic capabilities year 1.** Risky + regulatory-unclear in year 1-2. Becomes central in year 3+ once the platform can support verifiable agents.
- **Doesn't integrate with more than 1-2 EMRs in year 1-2.** Oscar Pro first; anything else only if a specific validation site needs it.

---

## 8. The fork between this and STRATEGY_2026

Key decision the founder faces: these are not the same company.

| Dimension | STRATEGY_2026 (12-month) | STRATEGY_2031 (5-year) |
|-----------|--------------------------|---------------------------|
| Goal | Profitable specialty SaaS | Regulated clinical AI platform |
| Revenue timing | Month 6-12 | Month 18-24 (non-regulated); Month 48-60 (regulated) |
| Team size year 1 | 1-2 | 2-3 |
| Team size year 5 | 5-8 | 10-15 |
| Capital requirement | $300K-500K | $25-40M |
| Moat | Specialty depth + workflow | Regulatory + clinical validation + platform |
| Risk | Market + competition | Regulatory + clinical + capital |
| Exit paths | Small bootstrap / strategic / lifestyle | Venture / IPO / strategic acquisition at higher multiple |
| Founder life | Solo-founder-capable | Requires serious team-building + ops |

They share Phase 1 almost entirely. The divergence happens after month 12 when you decide whether to stabilize and profitably grow (2026 plan) or pivot into clinical validation + regulatory work (2031 plan).

**This is a choice about what kind of company to build**, not just "which strategy wins." The 5-year path demands personality traits + capital access that the 1-year path doesn't. Both are legitimate. Neither is objectively better.

---

## 9. Alternative 5-year bets (considered and rejected, with reasons)

### Alt 1 — AI-native DPC / capitation operator

Run the clinics, not the software. Vertical integration like Forward or One Medical, but Canadian and AI-native. Revenue from capitation / subscription.

**Why rejected**: Capital-heavy (real estate, staffing, licensing), operational complexity high, team skill set very different (needs clinic operations leadership, not just engineering). This is a different company from AMI Assist-as-codebase. Possible year-7+ pivot from Layer 2 success.

### Alt 2 — Open-source clinical AI commons

Open-source Layer 1 + Layer 2. Build community + hosted services + enterprise support. Mattermost / GitLab model for clinical AI.

**Why rejected**: Community-building in healthcare is exceptionally hard. Physicians don't contribute to open source broadly. Regulatory-grade software needs capital that open-source projects rarely generate. Could be a year-5 maneuver for Layer 1 but isn't the primary bet.

### Alt 3 — Multi-specialty platform without flagship product

Build Layer 1 as the only product; license to medical AI startups who then build Layer 2 products. Pure platform play.

**Why rejected**: Platform-first without a reference product has no credibility with the FDA / Health Canada / clinical partners. The flagship product *is* the proof that the platform works. Without Layer 2, Layer 1 is slideware.

### Alt 4 — Patient-facing longitudinal AI companion

Shift entirely to patient-side. Between-visit AI companion that coordinates with physician. Different market (B2C or payer-sponsored B2B2C), different regulatory frame, different competitive landscape.

**Why rejected for AMI Assist specifically**: Doesn't use current codebase assets meaningfully. Sensor hardware, OHIP billing, local-first architecture — none of these accelerate a patient-side product. Could be a year-6+ extension after platform is proven.

### Alt 5 — Acquired / acqui-hired

Build to year 2 on the 2026 plan, sell to Tali / Abridge / Oscar / Telus. Not a bad outcome but doesn't need a 5-year strategy document.

### Alt 6 — Specialty conglomerate (multi-specialty in parallel)

Rather than depth-first in pain management, build scribes + basic decision support for 5-6 specialties in parallel, racing for coverage.

**Why rejected**: Dilutes focus, prevents regulatory moat (can't run 5 parallel clinical studies with startup resources), and competes directly with Tali's generic strategy without their distribution. Loses on both axes.

---

## 10. Decision framework

### 10.1 Questions to answer in the next 30 days

1. **Am I willing to run a 10-15 person company with serious capital commitments in year 3-5?** (Lifestyle question, not strategy question)
2. **Is there clinical-specialist interest?** (10 chronic-pain physician calls)
3. **Is there regulatory feasibility?** (1 consultation with regulatory consultant, ~$5-10K)
4. **Is there academic partner interest?** (2-3 informal conversations at U of T, McMaster, McGill pain centers)
5. **Is there investor appetite for a 5-year Canadian clinical AI play?** (3-5 informal conversations)

A no on #1 ends this conversation; pursue STRATEGY_2026 instead. A yes on #1 plus any two of #2–5 positive = proceed to formal Phase 0 validation. A yes on #1 with all of #2–5 negative = reconsider specialty or reconsider 5-year horizon.

### 10.2 Milestones to bail

Legitimate exit points where the 5-year thesis can be honorably abandoned without wasting the previous investment:

- **Month 12**: If Phase 1 deliverables slip more than 30% and no team augmentation has worked → revert to 2026 plan, cash out the Oscar integration + pain wedge as the smaller bootstrap business. Cost: ~$500K committed.
- **Month 24**: If seed round doesn't close at target valuation → decline to raise Series A, stay small, aim for strategic acquisition. Cost: ~$2.5M committed.
- **Month 36**: If clinical validation study fails interim analysis → pause submission, reconsider indication, potentially exit to acquirer with non-regulated product as asset.
- **Month 48**: If Health Canada first-round feedback is catastrophic → major pivot or exit. Cost: $15-20M committed; would be painful but defensible.

### 10.3 What would prove the thesis wrong

Honest indicators to watch:

- Tali or Abridge announces an FDA 510(k) for clinical decision support in year 1-2. (Means the regulatory moat is compressing faster than I predicted.)
- Health Canada issues guidance that dramatically shortens the clearance path for low-risk clinical decision support. (Means first-mover advantage evaporates.)
- An open-source clinical AI platform (from Mayo, Cleveland Clinic, or similar) achieves adoption. (Means Layer 1 value commoditizes.)
- Foundation models reach a level where specialty knowledge is trivial. (Means Layer 2 depth commoditizes.)

Any of these would be reason to reconsider within quarters, not years.

---

## 11. What I'd do in the first 30 days if this became the direction

Not a detailed action list — that's in the phase plan. Specific, concrete immediate moves:

1. **Week 1**: 5 chronic-pain physician calls (phone, not survey). Validate product hypothesis + specialty choice.
2. **Week 2**: Informal conversation with regulatory consultant specializing in Health Canada SaMD. ($5-10K total for consult + written memo on realistic timeline for target indication).
3. **Week 3**: Informal conversations with 2-3 academic research groups in chronic pain (U of T, McMaster). Not a formal partnership ask — just "does this direction interest you?"
4. **Week 4**: Investor conversations. Test the thesis with 3-5 healthcare-specialist investors. The feedback shape matters more than yes/no.

If weeks 1-4 feedback clusters around "this is interesting but you need X and Y first," incorporate X and Y into Phase 1 plan. If feedback clusters around "this is not interesting / not fundable / not regulatable," don't proceed with 5-year framing. Revert to STRATEGY_2026.

---

## 12. Summary in one paragraph

A 5-year bet only makes sense if the payoff at year 5 is structurally larger than a 5-year iteration of the 1-year plan. For AMI Assist the highest-probability version of that payoff is **a Health-Canada-cleared clinical AI platform targeted at Canadian specialty practices** — with chronic pain management as the flagship product, Oscar Pro as the primary EMR, academic medical center as the validation partner, and the current codebase's replay-logging + deterministic-rules-over-LLM architecture as the platform foundation. This is not a scribe company; it's a regulated clinical AI company that happens to include scribing. The work to get there is ~$25-40M across 60 months with a 10-15 person team and a clinical partnership. Exit ramps exist at months 12, 24, 36, and 48. If any of the Phase 0 validations (physician interest, regulatory feasibility, academic partner, investor appetite) come back cool, the 1-year plan is the honest fallback. Don't do both at once.

---

## Appendix A — Sources

Regulatory:
- [FDA AI-Enabled Device Software Functions Lifecycle Management Draft Guidance (January 2025)](https://www.fda.gov/medical-devices/software-medical-device-samd/artificial-intelligence-software-medical-device)
- [Mayo Clinic Proceedings: Digital Health — FDA Regulation of Clinical Software in the Era of AI/ML](https://www.mcpdigitalhealth.org/article/S2949-7612(25)00038-0/fulltext)
- [Advancements in Clinical Evaluation and Regulatory Frameworks for AI-Driven SaMD](https://pmc.ncbi.nlm.nih.gov/articles/PMC11655112/)
- [Bipartisan Policy Center — FDA Oversight of Health AI Tools](https://bipartisanpolicy.org/issue-brief/fda-oversight-understanding-the-regulation-of-health-ai-tools/)

Agentic AI in primary care:
- [Deloitte — Agentic AI and Operating Model Change in Health Care](https://www.deloitte.com/us/en/insights/industry/health-care/agentic-ai-health-care-operating-model-change.html)
- [Lumeris — Radically Rethinking Primary Care with AI](https://www.lumeris.com/in-practice/radically-rethinking-primary-care-continuous-and-connected-ai-enabled-access-to-maximize-health-outcomes/)
- [Lancet Primary Care — AI frameworks, challenges, guardrails](https://www.thelancet.com/journals/lanprc/article/PIIS3050-5143(25)00079-2/fulltext)
- [PMC — AI in Value-Based Healthcare](https://pmc.ncbi.nlm.nih.gov/articles/PMC12119536/)

DPC + AI:
- [Medical Economics — AI-powered direct primary care practice launches](https://www.medicaleconomics.com/view/ai-powered-direct-primary-care-practice-launches)
- [Elation — Technology to Scale DPC](https://www.elationhealth.com/resources/blogs/technology-to-scale-your-dpc-the-5-pillars-of-growth)

Local/edge inference:
- [Apple ML Research — Core ML on-device Llama 3.1](https://machinelearning.apple.com/research/core-ml-on-device-llama)
- [Apple ML Research — MLX + M5 neural accelerators](https://machinelearning.apple.com/research/exploring-llms-mlx-m5)
- [ANEMLL — On-device LLM inference on Apple Silicon](https://www.anemll.com/)
- [Seresa — Local LLMs for Healthcare HIPAA compliance](https://seresa.io/blog/eu-ai-act-for-marketers/local-llms-in-healthcare-and-law-how-apple-silicon-protects-client-data)

EMR consolidation + AI:
- [Epic + AI Trends 2025](https://spsoft.com/tech-insights/epic-ehr-ai-trends-in-2025-reshaping-care/)
- [OpenEHR — AI Agents vs Classic EMR Vendors](https://openehr.org/why-classic-emr-vendors-will-be-replaced-by-openehr-and-ai-agents-architectures/)
- [PLOS Digital Health — Epic Proportions Problem](https://journals.plos.org/digitalhealth/article?id=10.1371/journal.pdig.0001143)
- [HIMSS25 — Epic building agentic AI beyond EHR](https://www.fiercehealthcare.com/ai-and-machine-learning/epic-building-out-agentic-ai-it-also-broadens-focus-beyond-ehrs)

# STRATEGY_COMPOUND — The Ten-Year Vision, Moat-First

*This is the synthesis of STRATEGY_DECADE (the unified-healthcare-substrate vision) and STRATEGY_MOATS (what remains defensible when software commoditizes). It is written to be critically reviewed — by other AIs, by investors, by physicians, by regulators, by the founder's skeptical future self. Every ambitious claim is paired with the condition under which it fails, and every moat is paired with the specific architectural investment that makes it real.*

*The central argument: the ten-year vision described in STRATEGY_DECADE is not a single bet. It is a portfolio of durable moats that compound together — with **institutional graph, clinical evidence, and regulatory depth as the load-bearing core**, and sensor fabric, data governance, and compute architecture as reinforcing layers. The vision — Mary's 2036 — emerges from the moats accumulating in sequence. The moats are not in service of the vision; they are the vision, rendered structurally defensible through multi-year lead times that no amount of capital can collapse.*

---

## v2 revision notes — response to external critique

*This document has been revised in response to a detailed external strategic critique (GPT-5.4 Pro, 2026-04-17, saved as `STRATEGY_COMPOUND_DEEP_ANALYSIS.md` in the reviewer's working notes). The v1 thesis survives. The v1 operating plan required sharpening. Specific changes:*

**Reordered moat priority.** The v1 moat sequence (Sensor Fabric → Compute Appliance → Regulatory → Evidence → Data → Institutional) overweighted hardware in the narrative. The reviewer's correct critique: institutional graph, clinical evidence, and regulatory depth are the load-bearing moats; hardware is a reinforcing layer, not the centerpiece. Sections 4 and 11 now reflect this ordering.

**Rhetorical discipline.** "Uncopyable" has been replaced with "multi-year lead" or "compound sequencing asymmetry" in most places. The argument is about time barriers, not permanent exclusivity.

**Speculative ideas explicitly relabeled as optionality.** Data fiduciary protocol (Section 9), CMPA architectural liability transfer (Section 10.4), Specialist Emeritus program, multi-generational data, inheritance data — all now explicitly tagged as **Phase 2+ optionality contingent on core moats succeeding**, not core near-term execution. Custom silicon is moved further from the foreground.

**Compute appliance demoted from flagship moat to operational architecture.** The reviewer is right: it is an operational burden as much as a differentiator. Edge-first computation is the principle; a proprietary-branded appliance is not necessarily the implementation.

**Stage gates added (Section 22).** The v1 implicit parallelism of Year 1 has been replaced with explicit gates: Stage 1 (0-24 months) must prove wedge before Stage 2 (24-48 months) commits to continuous-observation hardware.

**Competitor landscape updated (Section 24).** Dragon Copilot, Abridge, Suki, and Nabla are already not scribe-only. The moat argument must assume crowded ambient-workflow competition, not a greenfield.

**"Platform-as-EMR-alternative" language removed.** The reviewer is correct that this frightens the EMR partners the plan depends on. The Y10 table has been rewritten.

**Numeric inconsistencies fixed.** Y10 ARR target reconciled to $250-350M range (v1 had $150-300M in intro vs $390M in revenue table — the $390M was inflated by including non-recurring streams; the revenue table now separates ARR from grants/partnerships). Paper count canonicalized at 8-12 anchor peer-reviewed papers with 12-15 total publications including secondary analyses. Clearance target canonicalized at 2-3 major FDA/Health Canada device clearances plus 8-10 supporting certifications. SR&ED reclassified as cash support, not ARR.

**Missing Canadian considerations added (Section 23).** Indigenous data governance, French-language support, rural/remote deployment complexity, substitute decision-maker workflows, third-party household consent, and field-operations margin modeling.

**Unit economics gap acknowledged (Section 13.5).** The v1 clinic revenue figures imply 20-35 clinicians per clinic — possible in FHO groups but not justified. A bottom-up model is deferred to the operating memo.

**Probability calibration added (Section 25).** The reviewer's estimate of 5-15% for the full vision as written, 25-40% for a narrowed plan with stage gates, is incorporated.

*The v1 document remains valuable as a constitutional statement of ambition. This v2 version is the constitution plus the amendments needed to make it executable. The reviewer's one-sentence summary — "keep the decade vision, keep the moat-first thinking, but radically narrow the first three years" — is adopted as guidance.*

---

## 0. For the critical reviewer — what this document is and isn't

Before engaging the argument, a brief alignment on scope:

**What this document claims.** That there is a coherent ten-year path from AMI Assist's current codebase and team to a healthcare infrastructure platform with $250-350M ARR (recurring, excluding grants and partnership milestone payments), 100,000+ patients enrolled, 2-3 major FDA/Health Canada device clearances (plus supporting certifications), 8-12 anchor peer-reviewed publications, and structural moats that resist commoditization. That this path is achievable on $250-400M of cumulative capital over a decade. That the path compounds — Year 1 investments in the right moats make Year 5 moats cheaper, and Year 5 moats produce multi-year lead times that competitors starting in Year 6 cannot collapse before Year 11-12.

**What this document does not claim.** That success is guaranteed. That the specific numbers will be met exactly. That the founder can execute alone. That competitors won't appear. That the regulatory or political environment will stay favorable. That hardware product development won't slip. That any specific hypothesis about 2030+ technology trajectory is reliable.

**What the critical reviewer should look for.** Places where moats are asserted without mechanism. Places where timing assumptions are aggressive. Places where one investment is claimed to produce outcomes in multiple moat categories without accounting for opportunity cost. Places where the argument depends on a single vendor, partner, or regulator. Places where novel ideas (fiduciary protocol, inheritance data, architectural liability transfer) are under-specified. Places where I'm using framing to mask weakness.

**What I explicitly anticipate the reviewer will say.** Eight objections enumerated in Section 2, addressed directly. If the reviewer has a ninth, I want to know — the document improves with that feedback.

**What a charitable read requires.** That healthcare infrastructure is the intended frame, not healthcare SaaS. That the founder is making a 10-year life commitment, not a 3-year startup bet. That the measure of success is whether Mary's 2036 (Section 5) becomes plausible for *some* patients in *some* regions — not whether it's universally realized. Success is probabilistic, not binary.

---

## 1. The thesis, in one page

Three compound claims:

**Claim 1 — Software is becoming free.** By 2028-2030, AI-assisted coding reduces the cost of producing a given software product by 10-30× from 2024 baselines. Any feature AMI Assist builds is replicable by a competent competitor within 6-18 months. Any API surface AMI Assist exposes is integrable by competitors within 3-6 months. Any LLM prompt innovation AMI Assist develops is copyable in hours. Speed, feature-completeness, engineering velocity, and API breadth — all of which were 2015-2024-era moats — compound toward zero over the decade.

**Claim 2 — Non-software moats compound, ranked by proximity to real buying decisions.** The load-bearing moats are (in order of near-term strategic priority): **institutional graph** (distribution, credibility, reference customers), **clinical evidence** (outcomes that change buying behavior), **regulatory depth** (market permission and accumulated submission expertise), and **unique continuous signal** (sensors + home observation that cloud-only ambient vendors cannot access). Reinforcing these are **data governance architecture** (patient-directed, auditable, revocable — expanding over time toward formal fiduciary status) and **edge-first compute architecture** (sovereignty, privacy, latency — where a proprietary appliance is one implementation, not the only one). None of these are accelerated by AI coding. Each takes real-world time (6-24 months for regulatory submissions, 2-5 years for clinical validation, 6-12 months per institutional partnership, decades to accumulate longitudinal patient data). The time itself is the moat. A competitor starting in Year 6 cannot collapse a Year-10 incumbent's lead before Year 11-12, during which AMI accumulates five more years of compound assets.

**Claim 3 — The moats reinforce each other.** Each moat strengthens adjacent moats. Sensor hardware deployed widely generates longitudinal data. Longitudinal data produces clinical evidence. Clinical evidence earns regulatory clearances. Clearances open institutional relationships. Institutional relationships deploy more sensors. The ten-year compound structure is what produces the ambitious outcomes, not any single bet. Remove any one moat and the structure weakens.

From these three claims: the ten-year plan from STRATEGY_DECADE is realizable *only if* the non-software moats from STRATEGY_MOATS are invested in deliberately, starting Year 1. Not Year 3. Year 1. Every month of delay compresses a moat that takes the time it takes.

This document describes the integrated execution.

---

## 2. Eight objections, addressed up front

A critical AI reviewer — or a thoughtful investor, or the founder's skeptical self — will raise specific objections. Rather than hide them in appendices, this document addresses them first.

### Objection 1 — "$250-400M over 10 years is impossible to raise from Canada."

**Response.** The capital is not one check. It is a sequence — seed ($3-5M), Series A ($8-15M), Series B ($15-25M), Series C ($30-50M), strategic partnerships ($20-50M), non-dilutive ($35-100M) — spread across a decade and funded by different classes of capital. Canadian sources alone likely produce $100-150M across this sequence (CIHR + NSERC + IRAP + SDTC + BDC + Canadian VC + provincial economic development + strategic pharma). The remaining $100-250M comes from US health-tech VC + strategic partners + patient capital (family offices, sovereign wealth, foundations). The capital profile matches other Canadian-origin health companies that reached $1B+ valuations (Thrasos, Bausch Health in its earlier forms, Sunrise Medical). Hard, not unprecedented.

The real test: does the founder want to run a capital-raising operation for a decade? That is a lifestyle question as much as a financial one. If no, revert to STRATEGY_2026.

### Objection 2 — "Hardware is too hard for a software-origin founder."

**Response.** Correct as stated. Mitigation is structural: hire a VP of Hardware in Year 2-3 with deep medical device experience, partner with a contract manufacturer with healthcare credentials (Jabil Healthcare, Flex Healthcare, Benchmark), and contract a medical device regulatory consultant from Year 1. The founder's job is not to engineer hardware; the founder's job is to integrate hardware strategy into the platform vision and hire the people who build it well. Companies like Tonal (connected fitness hardware + software), Peloton (same), Masimo (medical sensors), iRhythm (cardiac patches) all had software-adjacent founders and built serious hardware through disciplined hiring + partnership.

The failure mode: treating hardware as a "side project" of the software company. The mitigation: carve it out as its own profit center with its own leadership and its own quarterly reviews from Year 3 onward.

### Objection 3 — "Apple, Google, or Microsoft will commit to ambient health and crush you."

**Response.** Partially correct. Any of these could enter in Years 5-8. The structural defense is specific:
- **Canadian sovereignty requirements** legally exclude or heavily handicap US cloud-first platforms in Canadian healthcare deployments, particularly after forthcoming provincial health data sovereignty rules (Ontario, Quebec especially).
- **Clinical validation and peer-reviewed evidence** accumulate over years; Apple Watch took 5+ years to accumulate comparable AFib evidence; AMI starting earlier in the specialty of primary care ambient + home monitoring has a 5-year head start.
- **Specificity to clinical workflow** is hard for consumer-tech companies; each has tried (Amazon Care, Google Health, Apple Health) and struggled with clinical specificity. Microsoft (via Nuance DAX) is the closest real competitor — and DAX is scribe-only, not ambient-platform.
- **Regulatory depth** — by Year 6 AMI should have MDL + 510(k) + specific indication clearances that a new entrant (even a giant) takes 2-3 years to replicate.

A giant entering in Year 7 does not erase a six-year moat. It does intensify competition. The plan assumes that intensification and does not depend on being the only serious player.

### Objection 4 — "Ten years is too long; healthcare AI will be unrecognizable by then."

**Response.** Healthcare AI will change dramatically. Healthcare *structure* (regulated, slow-moving, fiduciary-trust-based, multi-stakeholder) will not. The architectural commitments (Section 4) are designed to survive regardless of which specific AI technology is dominant. Replay + audit architecture satisfies regulators under any AI regime. Local-first satisfies sovereignty under any privacy regime. Fiduciary consent satisfies patient-principal legal frameworks under any tech regime.

The 10-year plan does not bet on specific LLM architectures, specific cloud vendors, or specific model capabilities. It bets on the invariants: regulation, trust, continuity, physical observation. These have been stable for 30 years in healthcare and will remain stable.

The reviewer's real concern might be: "will the platform's software keep pace with LLM evolution?" Answer: software is cheap, per Claim 1. Keeping the software current is ongoing maintenance, not a moat investment. The moat investments are all non-software.

### Objection 5 — "You're describing many companies in one. One team can't do this."

**Response.** Correct. The plan does not claim one team does this. The plan stages the work:
- Years 1-2: primary care wedge + sensor productization start (team of 4-10)
- Years 3-4: home extension + clinical validation (team of 12-25)
- Years 5-7: specialty + hospital + research integration (team of 40-70)
- Years 8-10: platform + silicon + international (team of 100-150)

Each phase has distinct leadership. The Year 3 VP of Hardware is a different person from the Year 5 CMO, who is different from the Year 7 Chief Regulatory Officer. The company grows into the breadth. The founder's role evolves from builder (Y1-3) to operator (Y4-6) to platform architect (Y7-10).

The failure mode is attempting Year 5 breadth with a Year 2 team. The mitigation is disciplined staging.

### Objection 6 — "Your moats presuppose surviving Year 3-5 cash-burn."

**Response.** Correct. Revenue architecture (Section 13) provides $15-40M ARR by Year 4 through the primary care wedge + early home monitoring contracts, which funds 60-80% of operating costs from Year 5 onward. Series A and Series B ($23-40M combined) bridge the cash gap. If revenue does not hit $10M ARR by end of Year 3, the plan is in trouble — that's the falsifiable mid-point test.

If Year 3 revenue underperforms, the plan compresses: postpone silicon, postpone specialty, focus capital on core primary care + home + first indication clearance. The plan survives under-performance; it does not survive severe under-performance.

### Objection 7 — "Fiduciary consent graphs, inheritance data, architectural liability transfer — these are creative ideas, not proven legal structures."

**Response.** Correct. Sections 10, 14, and 17 describe novel legal + operational constructs that do not yet exist at scale. These require legal innovation — either adapting existing structures (Canadian nonprofit trust holding data on behalf of patients, for-profit operating company providing services) or advocating for new legislative + regulatory categories (data fiduciary licensure, data inheritance statutes).

The honest risk: if the legal innovation does not materialize, several moats weaken. Mitigation: the platform is valuable even without the novel structures — standard data processor + consent frameworks produce a weaker but workable version of each. The novel structures are upside, not floor.

### Objection 8 — "The document is too ambitious. It reads as aspirational, not actionable."

**Response.** Sections 15 (capital architecture), 16 (team architecture), 18 (30/90/365-day actions), and 19 (quarterly Year-1 plan) are specifically designed to address this. If after reading those sections the reviewer still finds the plan aspirational-not-actionable, the feedback is valuable — it points to a real weakness in the execution layer.

The line between "ambitious" and "unrealistic" is defined by whether the Year 1 actions are plausible. If the Year-1 plan (Section 19) can be executed with a $3-5M seed and a team of 4-6 people, the decade-long plan is not aspirational; it is a long path with a plausible first leg.

---

## 3. Five truths governing the next decade

These are the invariant assumptions underlying the plan. If any one is wrong, revisit.

### Truth 1 — AI-assisted coding commoditizes software

Trajectory is clear from the 2024-2026 evidence (Claude Code, Cursor, Devin-class agents, GPT-assisted engineering). The cost to produce a specific piece of software drops 10-30× between 2024 and 2030. Software velocity ceases to be a competitive moat. It remains a competitive *tax* — you must keep up — but it stops being defensible.

**What this implies for the plan.** Do not over-invest in software. Do invest enough to keep the platform viable. Treat software as infrastructure work, not as the product. Allocate 15-25% of 10-year capital to software (vs 40-60% typical SaaS allocation).

### Truth 2 — Healthcare is slow by structure, not by accident

Regulatory clearances take years because safety requires time. Clinical validation takes years because outcomes manifest over time. Institutional trust takes years because institutions are conservative by design. Physician practice change takes years because physicians are rationally cautious. Patient behavior change takes years because health is habitual.

**What this implies for the plan.** Slow is not an obstacle; it is the medium. Architect the company to operate at healthcare's pace — which is not venture-capital's pace — and use that pace as a moat. Companies that can operate at healthcare's pace are rare; they compound while rapid-growth competitors churn.

### Truth 3 — Embodiment beats disembodiment in regulated domains

An LLM in a data center is a tool. A sensor in a room is a presence. A certified medical device in a patient's home is a medical instrument. Embodiment — physical presence, physical signal, physical installation, physical certification, physical maintenance — commands trust and regulatory clarity that disembodied AI cannot.

**What this implies for the plan.** The hardware track is not a distraction from the software vision; it is the substrate of the vision. Sensors + compute appliances + wearables give the platform embodiment. Competitors with equivalent software but no embodiment are categorically different actors.

### Truth 4 — Consent graphs are the rare moat that grows by multiplication, not addition

Each patient consenting to the platform as their data fiduciary brings their family, their care team, their historical records, their trial participations, and their future-care continuity. The consent graph grows as a tree (edges multiply), not as a list (nodes add).

**What this implies for the plan.** Structuring consent correctly from Year 1 — granular, revocable, inheritable, portable — produces a compound asset over a decade. Structuring consent incorrectly from Year 1 produces a brittle asset that the first regulatory challenge erodes.

### Truth 5 — Moats compound when they reinforce each other, not when they stand alone

Six moats independently are an impressive portfolio. Six moats that each make the others stronger is a structure. The distinction is architectural: at each moat investment, the question is "does this investment make other moats cheaper or faster?" If yes, prioritize. If no, rethink.

**What this implies for the plan.** Section 12 maps the compound dynamics specifically. Every Year-N decision is scored against its impact on Year-N+2 moats.

---

## 4. The moat stack — six compounding layers, reordered by strategic weight

Six moats, arranged not as a list but as a structure where each reinforces adjacent layers. **Reordered in v2 to reflect near-term buying decisions rather than narrative emphasis.** The load-bearing moats (1-4) are the ones that determine whether the company exists in Year 5. The reinforcing moats (5-6) are what make the Year-10 position durable.

### Moat 1 — Institutional Graph (load-bearing)

**What it is.** The network of formal relationships: EMR integration partners (Oscar Pro, PS Suite, Accuro, Epic, Meditech), academic medical centers (15-25 research partnerships), pharma sponsors (5-10), CROs (2-3), health authorities (provincial contracts), payers (OHIP, private, potential US), government (Infoway, Stats Canada, public health), patient advocacy organizations, and specialty societies (clinical practice guideline groups). These are not one graph — they are three graphs: **distribution** (clinics, OHTs, EMRs), **validation** (academic centers, investigators, guideline bodies), and **capital + policy** (grant agencies, government, strategic partners).

**Why this moves to Moat 1 in v2.** The reviewer's structural critique: institutional relationships determine whether every other moat matters. A sensor with no deployments is an unused sensor. Evidence with no reference customers is unused evidence. A clearance without distribution channels is an unused clearance. Institutional graph is the substrate that makes all other investments payable.

**Why it resists commoditization.** Each relationship is a multi-year trust-building, contracting, due-diligence process tied to specific people and institutional moments. AI does not accelerate institutional trust. Each institutional relationship is specific to its moment — you cannot re-do a 2028 pilot in 2030 at the same cost.

**How it compounds.** Institutions reference other institutions. EMR integration makes clinical deployment easier. Academic partnerships produce clinical evidence (Moat 2). Pharma partnerships produce non-dilutive capital + clinical trial cohorts (Moat 5). Health authority contracts produce population-health evidence.

**Year-10 position.** 8+ EMR integrations, 20+ academic partnerships, 5-10 pharma sponsors, 3-5 Ontario Health Teams, 2-3 provincial health authorities, 1-2 US health system pilots, 2-3 international footholds.

### Moat 2 — Clinical Evidence (load-bearing)

**What it is.** Peer-reviewed publications documenting outcomes — but, per reviewer critique, **decision-grade evidence**, not paper count. The right unit is not "8-15 papers" but "did any study change buying behavior, change clinical practice, support reimbursement, create defensible public trust, or prove hard-outcome improvement?" A high-volume publication strategy becomes vanity if untied to these decision points.

**Why load-bearing.** In Canadian healthcare, evidence is what distinguishes trusted platforms from hyped platforms. The single strong multicentre outcome study plus one guideline mention is worth more strategically than several lighter papers.

**Why it resists commoditization.** Studies take the time they take. Longitudinal outcome studies require years of enrollment. Randomized trials require IRB approval, protocol development, site recruitment, data collection, analysis, peer review, publication. No AI shortcut.

**How it compounds.** Published evidence supports regulatory submissions (Moat 3). Published evidence produces named investigator relationships (Moat 1). Published evidence drives physician adoption (Moat 1 again). Published evidence is citable in pharma negotiations (revenue). Published evidence is the raw material for clinical practice guideline inclusion.

**Year-10 position.** 8-12 anchor peer-reviewed papers (12-15 total including secondary analyses), 20-30 named investigators publicly associated, 2-3 practice guideline inclusions, $40-80M cumulative clinical research investment. **Critically:** at least one study that demonstrably moved a procurement decision or guideline, not just a h-index increment.

### Moat 3 — Regulatory Stack (load-bearing)

**What it is.** Accumulated certifications + clearances: SOC 2 Type 2, HIPAA BAA, PIPEDA, HITRUST, 21 CFR Part 11 validated + audited, ISO 13485, ISO 27001, **2-3 major FDA/Health Canada device clearances** for specific indications (targeted, not sprawling), Canada Health Infoway vendor status, provincial data-sharing agreements. Total regulatory milestones including supporting certifications: 10+. Total major device clearances: 2-3 chosen with discipline.

**Why it resists commoditization.** Each clearance takes the time it takes. FDA 510(k): 6-18 months after submission; submission preparation: 12+ months; clinical validation for submission: 12-24 months; supporting QMS infrastructure: 12-24 months. Total from zero to first 510(k): 4-6 years. Subsequent clearances compound: precedent, reviewer familiarity, platform's QMS already proven.

**Reviewer caveat accepted.** FDA has already authorized 1,000+ AI-enabled devices. Regulatory clearance is no longer rare enough to be a standalone winner-takes-most moat. What it creates is **permission, trust, and multi-year lead time** — very valuable, not magic.

**How it compounds.** Clearances unlock institutional deployment (Moat 1). Validated QMS enables faster clearances (self-compounds). Specific indications legally exclude unclear competitors from marketing (per-indication exclusivity in the narrow sense). Audit history becomes a trust asset.

**Year-10 position.** 10+ certifications held, 2-3 major FDA/Health Canada device clearances, CE-MDR pathway active or cleared, audit history referenceable, $8-15M cumulative regulatory investment producing compound returns.

### Moat 4 — Unique Continuous Signal (Sensor Fabric)

**What it is.** A **narrow** family of sensors tied to specific clinical indications that cloud-only ambient vendors cannot match. Not a six-device home kit in Year 2. One or two sensors with real predictive or operational lift, proven before industrialization.

**Reviewer critique accepted.** The v1 six-device home kit was too broad too early. The v1 wearable spec (fall detection + HR/HRV/ECG + SpO2 + voice + cellular + 30-day battery + IP68) was a wish list, not a first product. V2 narrows: room presence sensor (Y1-2), one home-monitoring signal proven to change outcomes (Y3-4), wearable v1 with **one primary indication** (Y4-5), everything else is Year 5+ optionality that has to earn its right to exist.

**Why it resists commoditization (when narrowed).** Each sensor with a specific clinical indication requires regulatory certification, clinical validation, manufacturing partnership, field installation, and ongoing maintenance. Lead time 18-24 months minimum per sensor. Narrow focus lets the company actually accumulate the moat rather than dissipating across six half-finished products.

**How it compounds.** Sensors generate data (feeds Moat 5). Sensors enable clinical indications (feeds Moat 2). Sensors create physical presence (feeds Moat 1). Sensors require compute for local processing (feeds Moat 6).

**Year-10 position.** 2-3 sensor product lines with proven clinical lift, 4-6 distinct clinical indications cleared, 100,000+ units deployed (not the v1 target of 200,000+ — the reviewer's critique of over-scoping applies), contract manufacturing for 30K-100K units/year established.

### Moat 5 — Longitudinal Data + Data Governance Architecture (reinforcing)

**What it is.** 100,000+ patients × 5-10 years of continuous, multi-modal, consented observation, held under a data governance architecture that evolves across three phases:

- **Phase 1 (Y1-3):** patient-directed, auditable, revocable, transparent, portable where feasible — using standard Canadian privacy frameworks (PHIPA, HIA, provincial equivalents). No novel legal structure yet.
- **Phase 2 (Y3-6):** formalized stewardship model, external governance, data portability rights, possibly independent trust layer.
- **Phase 3 (Y6+, contingent on Phase 2 success):** formal data fiduciary protocol (see Section 9), inheritance semantics, broader standards-setting.

**Reviewer critique accepted.** The v1 document loaded too much near-term weight on the fiduciary protocol. Most early buyers will not pay extra for legal elegance. The protocol becomes valuable as an enabling architecture and trust signal over time; it is not the lead commercial story.

**Why it resists commoditization.** You cannot retroactively observe a patient. You cannot retroactively obtain consent. Time is the only way to accumulate this dataset. A competitor in Year 6 cannot have a 10-year cohort until Year 16.

**Year-10 position.** ~300-500 patient-years per active patient across the cohort, a governance architecture covering 100,000+ patient consents with at least 20,000+ under formal stewardship model (Phase 2 adopters), multi-modal data at a quality/granularity unmatched in primary-care + home-monitoring research.

### Moat 6 — Edge-First Compute Architecture (reinforcing, not flagship)

**What it is.** Local computation for sensitive, latency-critical data, with centralized aggregation for what genuinely benefits from it. A proprietary compute appliance is **one implementation**; a hardened edge deployment on commodity hardware is another. The principle is the moat; the specific branded appliance is optional.

**Reviewer critique accepted.** The v1 document positioned a custom appliance as a marquee moat. The reviewer is correct that an appliance fleet is a significant operational burden (provisioning, firmware updates, secure boot, fleet health, hardware failures, patching, remote observability, replacement logistics, attestation, decommissioning, long-tail support) and that similar value can be extracted from hardened commodity deployments. V2 demotes the appliance from flagship to operational architecture.

**Custom silicon** is further deprioritized — noted in Section 7 as Year 5-10 contingent optionality, not a moat AMI is actively pursuing.

**Why it still matters.** Edge-first architecture is a **regulatory + sovereignty + privacy moat** in its principles, not in its branding. A platform architecturally committed to local-first computation satisfies data sovereignty, privacy, and latency requirements that cloud-first competitors cannot retrofit without 2-5 years of rearchitecture. The moat is architectural, not product.

**Year-10 position.** 5,000-10,000 edge deployments (clinics + homes), hybrid architecture proven at scale, sovereignty + privacy story validated through regulatory engagement. Whether these run on AMI-branded appliances or on hardened commodity hardware is a cost-of-operations decision, not a moat decision.

---

*The v1 Moat Stack sequence (Sensor Fabric → Compute Appliance → Regulatory → Evidence → Data → Institutional) is preserved below in Appendix D for historical reference. The v2 sequence above is the operative one.*

[Archival of v1 Moat stack content below is preserved verbatim under its original numbering for reviewers who want to trace what changed. The v2 content above supersedes it in strategic priority.]

### [v1 archive] Moat 1 — Sensor Fabric

**What it is.** A family of medical-grade sensors (room presence, bed, bathroom, kitchen, doorway, wearable, clinician-worn, specialty) with specific clinical indications cleared by FDA / Health Canada / CE-MDR, manufactured at scale, deployed at 50,000+ homes and 1,000+ clinics by Year 10.

**Why it resists commoditization.** Every sensor requires industrial design, firmware, FCC/IC certification, medical device classification, manufacturing partnership, supply chain, quality assurance, and field installation. Each sensor indication requires specific clinical validation. Each deployment requires physical presence. Total lead time from idea to shippable certified sensor: 18-24 months minimum. A competitor starting in Year 6 cannot match by Year 11 the sensor portfolio AMI accumulates across Years 1-10.

**How it compounds with other moats.** Sensors generate data (feeds Moat 5 — longitudinal). Sensors enable clinical indications (feeds Moat 4 — evidence). Sensors create physical presence (feeds Moat 6 — institutional). Sensors require compute for local processing (feeds Moat 2 — appliance).

**Year-10 position.** 4-6 sensor product lines, 8-12 distinct clinical indications cleared, 200,000+ units deployed, $30-80M annual hardware revenue, contract manufacturing for 50K-200K units/year established.

### [v1 archive] Moat 2 — Compute Appliance

*Note: v2 demotes this from flagship moat to operational architecture. See v2 Moat 6 above for the current framing.*

**What it is.** A purpose-built local compute device for homes and clinics, sealed, attested, encrypted, medically certified, running the platform's inference + storage + coordination stack. Not a Mac Mini; a medical appliance with hardware security module, tamper evidence, and long firmware lifecycle.

**Why it resists commoditization.** Medical EMI certification restricts commodity PCs from deployment. HSM + attestation chain requires specific hardware partnerships (Apple Secure Enclave, NXP, or custom). Long firmware lifecycle (10+ year support) is a commitment most software companies won't make. Competitors retrofit general-purpose hardware and fail certification.

**How it compounds.** Enables local-first architecture (regulatory moat). Runs medical AI on-device (privacy + sovereignty moat). Supports sensor fabric (Moat 1). Hosts fiduciary consent operations locally (Moat 4). Provides cryptographic anchoring for audit (Moat 3).

**Year-10 position.** 10,000+ appliances deployed (home + clinic), $800-1,500 home / $2,500-5,000 clinic BOM, two generations of hardware deployed with clear upgrade path, custom silicon either in development or deployed (see Moat 2 evolution).

### [v1 archive] Moat 3 — Regulatory Stack

*Note: v2 promotes Regulatory to Moat 3 in the load-bearing trio. See v2 Moat 3 above.*

**What it is.** Accumulated certifications + clearances: SOC 2 Type 2, HIPAA BAA, PIPEDA, HITRUST, 21 CFR Part 11 validated + audited, ISO 13485, ISO 27001, FDA 510(k) for 2-3 indications, Health Canada MDL, CE-MDR Class IIa or IIb, Canada Health Infoway vendor status, provincial data-sharing agreements.

**Why it resists commoditization.** Each clearance takes the time it takes. FDA 510(k): 6-18 months after submission; submission preparation: 12+ months; clinical validation for submission: 12-24 months; supporting QMS infrastructure: 12-24 months. Total from zero to first 510(k): 4-6 years. Subsequent clearances compound: precedent, reviewer familiarity, platform's QMS already proven.

**How it compounds.** Clearances unlock institutional deployment (Moat 6). Validated QMS enables faster clearances (Moat 3 self-compounds). Specific indications legally exclude unclear competitors from marketing (per-indication exclusivity). Audit history becomes a trust asset (Moat 5) and a sales asset (institutional graph).

**Year-10 position.** 10+ certifications held, 2-3 FDA 510(k) clearances, Canadian MDL, CE-MDR pathway active, audit history referenceable, $8-15M cumulative regulatory investment producing compound returns.

### [v1 archive] Moat 4 — Clinical Evidence

*Note: v2 promotes Clinical Evidence to Moat 2 and reframes around decision-grade evidence rather than paper count. See v2 Moat 2 above.*

**What it is.** Peer-reviewed publications documenting outcomes (hospitalization reduction, cost-effectiveness, adherence improvement, clinician burnout reduction, trial operational efficiency), 8-12 anchor papers (12-15 total with secondary analyses) by Year 10, multiple randomized or quasi-experimental designs, inclusion in 2-3 clinical practice guidelines.

**Why it resists commoditization.** Studies take the time they take. Longitudinal outcome studies require years of enrollment. Randomized trials require IRB approval, protocol development, site recruitment, data collection, analysis, peer review, publication. No AI shortcut to a validated outcome measurement.

**How it compounds.** Published evidence supports regulatory submissions (Moat 3). Published evidence produces named investigator relationships (Moat 6). Published evidence drives physician adoption (Moat 6 again). Published evidence is citable in pharma negotiations (revenue). Published evidence is the raw material for clinical practice guideline inclusion.

**Year-10 position.** 8-15 peer-reviewed papers, 20-30 named investigators publicly associated, 2-3 practice guideline inclusions, $40-80M cumulative clinical research investment.

### [v1 archive] Moat 5 — Longitudinal Data + Fiduciary Consent Graph

*Note: v2 retains Moat 5 designation but reframes the fiduciary structure as Phase 2+ evolution rather than Phase 1 positioning. See v2 Moat 5 above.*

**What it is.** 100,000+ patients × 5-10 years of continuous, multi-modal, consented observation, held under a data fiduciary structure where the platform operates legally as the patient's agent, not as a data controller. Each patient's consent is granular, revocable, and anchored in the platform's cryptographic audit chain.

**Why it resists commoditization.** You cannot retroactively observe a patient. You cannot retroactively obtain consent. You cannot retroactively reconstruct the encounter that didn't happen. Time is the only way to accumulate this dataset. A competitor in Year 6 cannot have a 10-year cohort until Year 16, during which AMI adds six more years.

**How it compounds.** Longitudinal data enables novel clinical evidence (Moat 4). Consent graph enables cross-institutional research (platform-as-trial-site, Section 11). Fiduciary status is a regulatory moat (legal category difficult to replicate). Data density improves AI models (specialty AI moat).

**Year-10 position.** ~300-500 patient-years per active patient across the cohort, a fiduciary consent graph covering 100,000+ consents, multi-modal data at a quality/granularity unmatched in primary-care + home-monitoring research.

### [v1 archive] Moat 6 — Institutional Graph

*Note: v2 promotes Institutional Graph to Moat 1 — the most important structural critique adopted. See v2 Moat 1 above.*

**What it is.** The network of formal relationships: EMR integration partners (Oscar Pro, PS Suite, Accuro, Epic, Meditech), academic medical centers (15-25 research partnerships), pharma sponsors (5-10), CROs (2-3), health authorities (provincial contracts), payers (OHIP, private, potential US), government (Infoway, Stats Canada, public health), patient advocacy organizations, and specialty societies (clinical practice guideline groups).

**Why it resists commoditization.** Each relationship is a multi-year trust-building, contracting, due-diligence process tied to specific people and institutional moments. AI does not accelerate institutional trust. Each institutional relationship is specific to its moment — you cannot re-do a 2028 pilot in 2030 at the same cost.

**How it compounds.** Institutions reference other institutions. EMR integration makes clinical deployment easier. Academic partnerships produce clinical evidence (Moat 4). Pharma partnerships produce non-dilutive capital + clinical trial cohorts (Moat 5). Health authority contracts produce population-health evidence.

**Year-10 position.** 8+ EMR integrations, 20+ academic partnerships, 5-10 pharma sponsors, 3-5 Ontario Health Teams, 2-3 provincial health authorities, 1-2 US health system pilots, 2-3 international footholds.

---

## 5. Mary-2036, told through the moats

The 2036 vision from STRATEGY_DECADE described Mary, 74, with mild cognitive impairment, living alone, navigating a week in which her early-UTI was detected, her trial participation was seamless, and her daughter Elena had reduced caregiver burden. That narrative is re-presented here with each moment annotated to the specific moats that make it possible.

### Monday morning — the stable baseline

Mary wakes at 7:14 AM. The home platform notices slight lateness. Her bathroom visits are logged. The kitchen sensor confirms morning tea at 7:32. Her memantine bottle detects cap opening.

*Enabled by:* Moat 1 (bedroom presence sensor, bathroom privacy-preserving sensor, kitchen sensor, smart pill bottle) + Moat 2 (home compute appliance doing local pattern analysis) + Moat 5 (three years of Mary's baseline behavior making "slight lateness" measurable). Without any one: the moment is impossible. Without the fiduciary consent structure (Moat 5), it is surveillance. Without the local processing (Moat 2), it is cloud exposure. Without the sensors (Moat 1), it is guesswork.

### Tuesday — the soft signal catch

Mary's resting HR is 2 bpm up over 36 hours. Morning activity is down 8%. The platform's risk layer pattern-matches to pre-clinical UTI in elderly women. The system does not alarm Elena. It queues a soft note on Dr. Chen's Wednesday tablet: *"Subtle trend suggesting early UTI surveillance; recommend check at next visit or phone call."*

*Enabled by:* Moat 5 (five years of Mary's cardiovascular baseline; population model from 50,000 elder patient-years for pre-clinical UTI pattern) + Moat 4 (published clinical evidence that this soft-signal model reduces avoidable ED visits; regulatory clearance for the specific decision support indication) + Moat 3 (FDA 510(k) clearance for the clinical decision support; audit trail proving the recommendation was generated transparently). Without Moat 5, no pattern. Without Moat 4, Dr. Chen doesn't trust the signal. Without Moat 3, Dr. Chen's malpractice insurer questions the use.

### Wednesday afternoon — the phone call and the culture

Dr. Chen messages Mary's portal. Mary reports mild discomfort. A urine culture is delivered. Empiric antibiotics start Friday.

*Enabled by:* Moat 6 (Dr. Chen's workflow integrated with Mary's care plan; pharmacy partner for home delivery; lab partner for at-home specimen collection; Oscar Pro integration putting the encounter in the chart). Without Moat 6, every step is friction. Without it, Mary doesn't get the urine culture, the antibiotic doesn't start Friday, the infection reaches ED by Monday.

### Wednesday, parallel — the clinical trial visit

Mary's trial visit is virtual. Jen, the coordinator, uses the research-mode of the same platform. Mary's assessment runs on the home appliance. Sponsor sees data in aggregate within an hour.

*Enabled by:* Moat 2 (same appliance hosts care + research modes) + Moat 3 (21 CFR Part 11 validation of the platform for trial use) + Moat 5 (Mary's fiduciary consent covers both care and research modes) + Moat 6 (sponsor relationship, EDC integration, IRB agreement). Four moats simultaneously. A competitor who has three of four has a broken product; Mary cannot use it for both her care and her research seamlessly.

### Thursday — family dashboard

Elena checks weekly trajectory. Stable. Adherence 96%. Socialization normal. Sleep is the watch item.

*Enabled by:* Moat 5's fiduciary consent graph — Mary has specifically consented to Elena's view of specific data categories, granularly, revocable. Elena sees what Mary shared; not what Mary didn't. The consent graph is an asset.

### Friday — the quiet resolution

Antibiotics working. HR normalizing. Dr. Chen notified. No further action.

*Enabled by:* Moat 5 (continued observation confirms resolution). The system doesn't just detect; it verifies treatment effect in real-world behavior. This is a novel clinical construct — *pharmacologic response measured by ambient behavioral signal* — that the platform creates because it has the moats to observe it. No cross-sectional system can.

### The year's outcome

Mary: no ED visits, no hospitalizations, 2-4 extra years of independent living. System: 30-40% lower spend. Elena: not a caregiver, a daughter.

*Enabled by:* the full moat stack operating for multiple years. This is not a Year-1 outcome; it is a Year-10 outcome. The question the critical reviewer should ask: "how do we know the moat stack produces this outcome, not just enables it?" Answer: the outcome is measured in the clinical evidence (Moat 4). If Moat 4 papers show <5% reduction in hospitalizations, the moat is weaker than claimed and the plan adjusts. If Moat 4 papers show 30% reduction, the claim is validated. The plan is falsifiable at Year 5-6 when the first outcomes papers publish.

### The uncopyable combination

A competitor starting in Year 6 to replicate Mary-2036 needs:
- 6 distinct sensor product lines, manufactured and certified (3-5 years)
- Compute appliance, manufactured and certified (2-3 years)
- Local-first architecture (architectural retrofit; 2-3 years for cloud-first competitor)
- 510(k) clearance for UTI-risk clinical decision support (2-3 years minimum)
- Published clinical evidence supporting the detection model (3-5 years)
- 5+ years of longitudinal data from similar cohort (cannot shortcut)
- Fiduciary consent structure with Mary-equivalent patients (unique per patient)
- Oscar Pro + pharmacy + lab integrations (2-3 years each)
- Workflow acceptance by Canadian primary care (3-5 years of trust-building)

Sum: 8-12 years of real-world work. During which AMI adds 8-12 more years. The moats compound against the competitor's effort, not just against current state.

---

## 6. The ten-year arc — moat-milestones

A year-by-year table of the plan as moats accumulate. Rows are years, columns are moats. Each cell specifies the year's specific deliverable for that moat. An asterisk (*) marks phase-transition moments.

| Year | Sensor Fabric | Compute Appliance | Regulatory | Clinical Evidence | Data + Consent | Institutional |
|------|---------------|-------------------|------------|-------------------|----------------|---------------|
| Y1 | Room sensor v1 design; ID consulting engaged; firmware productization started | Appliance spec; commodity Mac Mini as interim | SOC 2 Type 1; PIPEDA framework; HIPAA BAA path | First evidence outline; IRB engagement for Y2 study | Fiduciary consent v0 deployed in primary care clinics | Oscar Pro integration signed; 10 paying clinics |
| Y2 | Room sensor v1 manufactured (3-5K units); FCC/IC cert | Custom appliance prototype; HSM partnership | SOC 2 Type 2 achieved; PIPEDA certified | First pilot study enrollment | 1,000 patients on fiduciary consent | 15-25 paying clinics; PS Suite integration |
| Y3* | Home sensor kit (6 devices) v1 shipping; pre-submission to FDA | Custom appliance v1 (home edition); 10+ year support commitment published | FDA pre-submission meeting; Health Canada MDL pathway open | First pilot study completed; abstract at CFPC | 5,000 patients; data fiduciary legal structure filed | 50 clinics; first Ontario Health Team pilot |
| Y4 | Sensor kit at 5-10K units/year; wearable prototype | Custom appliance (clinic edition); field update infrastructure | First 510(k) submission (fall detection); Health Canada MDL application | First peer-reviewed paper submitted | 15,000 patients; inheritance data architecture specified | First hospital EMR integration pilot (Meditech) |
| Y5 | Wearable v1 shipping; clinician-worn device prototype; 4 sensor lines | Custom appliance Gen 2 design; silicon feasibility exploration | First 510(k) cleared; Health Canada MDL granted | First peer-reviewed paper published; second study launched | 30,000 patients; fiduciary protocol published as open spec | 2-3 Ontario Health Teams; first academic MSA at scale |
| Y6* | Clinician-worn shipping; smart pill bottle + BP cuff partnerships | Gen 2 appliance shipping; custom silicon decision (go/no-go) | 2nd 510(k) submitted (ambient scribe decision support); ISO 13485 | 2-3 papers published; clinical practice guideline engagement | 50,000 patients; fiduciary graph 1M+ edges | 5-10 pharma sponsors; CE-MDR preparation for EU |
| Y7 | Specialty sensors (ECG patch, spirometer, scale) integrations | If silicon: tape-out; if no: Gen 3 appliance | 2nd 510(k) cleared; CE-MDR submission | 4-5 papers; practice guideline inclusion (1st indication) | 70,000 patients | US health system pilot; international research partnership |
| Y8* | 5+ sensor lines mature; 100K+ units deployed | Gen 3 or silicon deployed in production | CE-MDR cleared; 3rd 510(k) submitted | 6-8 papers; 2nd practice guideline inclusion | 100,000 patients; data inheritance active (first patient deaths) | 10+ pharma; first Medicare Advantage (US) pilot |
| Y9 | 6 sensor lines; custom silicon in production appliances | Silicon Gen 2 in design if deployed | 3rd 510(k) cleared; platform-level clearances for specific indications | 8-12 anchor papers; strong multisite outcome evidence for flagship indications | 130,000 patients | Major pharma enterprise contract; 2nd country at scale |
| Y10 | 200K+ units in field; second wearable generation | Silicon deployed across fleet; Gen 4 appliance in design | 4+ clearances total; audit history of 8+ years | 12-15 papers; platform cited in 3+ guidelines | 150,000-200,000 patients | 20+ institutional partnerships; platform as infrastructure |

Phase transitions at Years 3, 6, and 8 mark structural shifts: Y3 is when hardware moves from R&D to product; Y6 is when clinical evidence moves from supporting role to central asset; Y8 is when the platform moves from product to infrastructure.

---

## 7. Hardware product roadmap — specific, sequenced

Six hardware categories from STRATEGY_MOATS, sequenced with specific timelines, BOM targets, manufacturing partners, regulatory pathways, and the moat each product primarily advances.

### Product 1 — Room Presence Sensor v1 (Years 1-2)

**What:** mmWave + thermal + CO2 + acoustic; adhesive-mount; 2-year battery; encrypted LoRa or WiFi transport to local appliance. Productized evolution of current ESP32 firmware.

**BOM target:** $45-65 at 10K units/year; $30-45 at 100K.

**Certifications:** FCC Part 15, IC, CE; Health Canada Class I medical device registration (pathway for presence-as-clinical-indicator).

**Manufacturing partner:** Contract manufacturer in Ontario or Quebec (Celestica, Alps Alpine's Canadian partner, or Sanmina's Canadian operation). Tier-2 CM rather than Jabil/Flex for early volumes.

**Primary moat:** Sensor Fabric (Moat 1).

**Revenue model:** sold as part of a home kit or clinic deployment; not standalone consumer purchase.

### Product 2 — Home Sensor Kit v1 (Years 2-4)

**What:** six devices (room presence × 2, bed sensor, bathroom privacy sensor, kitchen sensor, doorway sensor). Bundled with compute appliance + installation service.

**Kit BOM:** $300-450 at 5K kits/year. Kit retail/reimbursement: $1,500-2,500 (installation + ongoing service included).

**Certifications:** Each device certified individually; kit as a system reviewed by Health Canada; specific clinical indications (UTI risk, fall risk, adherence) filed as 510(k) under a common submission.

**Manufacturing partner:** upgrade to tier-1 CM by Year 3 (Jabil Healthcare, Flex Healthcare, or Benchmark).

**Primary moat:** Sensor Fabric + Regulatory + Clinical Evidence (three moats advanced per deployment).

**Revenue model:** insurance-reimbursed medical equipment; provincial home-care program reimbursement; private-pay for patients ineligible; long-tail annual service fee ($300-500/year).

### Product 3 — Compute Appliance (Clinic + Home editions) (Years 2-4)

**What:** sealed, attested, medically certified local compute. Home edition: Apple Silicon-class NPU, 32-64GB RAM, 1-2TB encrypted SSD. Clinic edition: dual-boot capable, higher-capacity storage, UPS integration, rack-mount option.

**BOM target:** Home $500-800; Clinic $1,500-2,800 (Year 4 volumes).

**Certifications:** FCC, IC, CE, medical facility EMI compliance, ISO 13485 (produced under medical QMS). FIPS-140-3 cryptographic module certification is a stretch goal.

**Manufacturing partner:** specialized healthcare contract manufacturer (Bel Fuse Circuit Protection, or white-label via Dell/HPE medical). Apple Silicon via Apple's M-series program (not custom Apple hardware; Apple doesn't white-label Macs, but Apple-certified Mac variants are possible via specific programs).

**Primary moat:** Compute Appliance (Moat 2) + enables Moats 1, 3, 5.

### Product 4 — Wearable v1 for Elders (Years 3-5)

**What:** pendant + wristband options (patient choice). Fall detection, HR + HRV + ECG, SpO2, voice-capture-for-emergency, cellular backup, 30-day battery, IP68. Not a consumer fitness wearable; a medical alert device evolved with AI.

**BOM target:** $80-130 at Year 5 volumes.

**Price/reimbursement:** $250-400 device + $30-50/month monitoring (reimbursable via OHIP home care program, Ontario Drug Benefit equivalents, US Medicare).

**Certifications:** FDA 510(k) Class II (fall detection + ECG), Health Canada MDL, CE-MDR Class IIa.

**Manufacturing partner:** specialist medical wearable CM (Plexus, Benchmark) or OEM partnership (Garmin Medical, Withings for specific components).

**Primary moat:** Sensor Fabric + Clinical Evidence + Regulatory (three-way).

**Alternative path:** OEM partnership with an existing wearable company, co-branded, reduces capital commitment but loses some margin + differentiation.

### Product 5 — Clinician-Worn Device (Years 4-6)

**What:** ID-badge-clip or pendant worn by physicians during clinic work. Audio pickup, haptic notifications, room-awareness (pairs with room sensor), 10-12 hour battery, encrypted transport, hands-free documentation trigger.

**BOM target:** $150-250 at Year 6 volumes.

**Price/reimbursement:** $400-700 per device; per-physician-per-month service fee ($50-100).

**Certifications:** FCC, IC, CE, Health Canada Class I (or II if clinical decision support embedded).

**Manufacturing partner:** same tier as wearable; possibly same CM.

**Primary moat:** Sensor Fabric + Institutional (Moat 6 — physicians more likely to adopt platform when physical device is comfortable + valuable).

### Product 6 — Specialty Accessory Integrations (Years 4-8)

**What:** not manufactured in-house. Platform-standard API + certification-as-partner-program for specialty medical devices: smart pill bottles, BP cuffs, spirometers, scales, ECG patches, thermal imaging patches, glucose meters, home ultrasound probes.

**Strategy:** AMI becomes the integration standard. Device manufacturers pay for integration + certification. Platform revenue share on device usage.

**Primary moat:** Institutional (Moat 6) — each device integration is a partnership that locks in co-dependency.

### Product 7 — Custom Silicon (Years 5-10, contingent)

**What:** optional, high-risk/high-reward. ASIC or specialized SoC optimized for medical LLM inference + continuous sensor fusion + cryptographic operations. Co-designed with a chip partner (Tenstorrent, a startup specializing in edge AI, or SiFive for RISC-V ecosystem).

**Decision point:** Year 6, based on evidence that:
- Volumes justify tape-out ($30-50M for 100K units amortization implies needs 500K+ devices deployed)
- No off-the-shelf NPU meets all requirements (power, security, specific medical workload)
- Strategic competitive differentiation requires silicon-level moat
- Capital available without compromising other tracks

If these conditions are not clear by Year 6, postpone silicon to Year 8 or drop. This is the one hardware bet explicitly marked optional.

**Alternative path:** partner with Apple (use Apple Neural Engine + secure enclave as substrate, AMI provides medical models + firmware) or Nvidia (Orin Nano Super + Jetson ecosystem). Lower capital, higher dependency.

**Primary moat:** Compute Appliance (Moat 2) + enabler of continued Moat 5 scale.

---

## 8. Regulatory + clinical evidence ladder

Specific submissions, specific papers, specific indications, specific timelines. This is where the "slow healthcare" truth (Section 3) becomes concrete.

### 8.1 Security and compliance certifications — foundational

- **SOC 2 Type 1** (Y1 Q4): internal controls audited for design effectiveness.
- **SOC 2 Type 2** (Y2 Q4): controls audited for operational effectiveness across 6-12 months. *Mandatory for enterprise sales.*
- **HIPAA BAA capability** (Y2): infrastructure + policies to sign Business Associate Agreements with US customers.
- **PIPEDA certification and data residency** (Y1-Y2): Canadian privacy law compliance with explicit data-residency guarantees.
- **HITRUST CSF** (Y3-Y4): comprehensive risk management framework preferred by large US health systems.
- **ISO 27001** (Y3): information security management system.
- **ISO 13485** (Y4-Y5): medical device quality management system. Prerequisite for sensor + wearable clearances.
- **21 CFR Part 11 validation with annual audits** (Y3-Y4 initial, annual thereafter): electronic records and signatures for clinical research deployments.

### 8.2 Medical device clearances — per-indication

Each row is a specific regulatory milestone. Clearances compound: platform QMS + precedent accelerate subsequent filings.

| Year | Regulatory body | Clearance | Indication | Supporting evidence |
|------|-----------------|-----------|------------|---------------------|
| Y3 | Health Canada | Class I registration | Room presence sensor for care coordination | Bench testing + literature |
| Y4 | FDA | Pre-submission meeting | Home fall detection (wearable) | Bench + initial pilot |
| Y5 | FDA | 510(k) Class II cleared | Wearable fall detection + caregiver alert | Predicate comparison + 200-pt validation |
| Y5 | Health Canada | MDL granted | Same indication, Canadian market | FDA clearance + Canadian data |
| Y6 | FDA | 510(k) submitted | Ambient clinical decision support for UTI risk in elderly | 1000-pt retrospective + 200-pt prospective |
| Y7 | FDA | 510(k) cleared | Same | (above) |
| Y7 | Health Canada | MDL | Same | FDA + Canadian supplement |
| Y7 | EU (EMA/CE-MDR) | CE-MDR Class IIa submission | Wearable indication in EU | FDA precedent + EU clinical data |
| Y8 | EU | CE-MDR cleared | Wearable | (above) |
| Y8 | FDA | 510(k) submitted | Continuous adherence monitoring + deterioration detection | Multi-site data |
| Y9 | FDA | 510(k) cleared | Adherence/deterioration | (above) |
| Y9-10 | Multiple | Platform-level approvals | Specific clinical decision support extensions | Accumulated |

### 8.3 Peer-reviewed publication plan

| Year | Study | Design | Primary endpoint | Expected journal tier |
|------|-------|--------|------------------|----------------------|
| Y2-3 | AMI feasibility study | Single-site prospective | Physician workflow time; satisfaction | CMAJ Open / Canadian Family Physician |
| Y3-4 | Multi-site primary care deployment | Multi-site observational | Billing accuracy; clinical documentation completeness | Annals of Family Medicine |
| Y4-5 | Home monitoring + chronic disease | Prospective cohort, 500 pts | Hospitalization rate vs matched controls | JAMA Internal Medicine / NEJM Evidence |
| Y5-6 | Wearable fall detection | Prospective validation | Sensitivity/specificity of fall detection | Journal of the American Geriatrics Society |
| Y6-7 | UTI surveillance signal | Retrospective + prospective | PPV/NPV of ambient UTI risk | Annals of Internal Medicine |
| Y7-8 | Clinician burnout reduction | Multi-site cluster RCT | Physician burnout inventory | JAMA / BMJ |
| Y8-9 | Decentralized trials feasibility | Case series of 3-5 pharma trials | Enrollment time; data quality vs traditional | Clinical Trials Journal |
| Y9-10 | Cost-effectiveness across cohort | Economic evaluation | $/QALY for home-monitored chronic disease | Value in Health |

Publication counts 8 anchor papers; secondary analyses and editorials push total to 12-15 by Year 10.

### 8.4 Clinical practice guideline engagement

- Y6: engagement with Canadian Frailty Network, Canadian Geriatrics Society on remote monitoring recommendations.
- Y7: first guideline mention (supplementary tool).
- Y8-9: formal inclusion in 1-2 guidelines (specific indications).
- Y9-10: second inclusion (specialty society, e.g., Canadian Cardiovascular Society for arrhythmia detection).

---

## 9. The data fiduciary legal innovation

This is novel territory; worth a dedicated section.

### 9.1 The problem with current consent models

Standard tech consent treats users as data subjects and companies as data controllers. Data "ownership" legally rests with the patient in Canada (PHIPA) and much of the US (HIPAA) — but operationally, patients cannot access, port, or direct use of their data. The gap between "legally own" and "operationally control" is the quiet harm of current health tech.

AMI's proposed architecture closes this gap legally, not just ergonomically. The platform operates as a **data fiduciary** — a legally recognized structure where the platform holds data *in trust* for the patient, is bound by fiduciary duty to act in the patient's interest, and can be legally removed as fiduciary by the patient at any time.

### 9.2 The legal structures available

Canada does not yet have a formal data fiduciary licensure, but existing structures can be adapted:

**Option A — Trust-based holding structure.** Register a Canadian charitable or nonprofit trust that holds patient data on behalf of patients (beneficiaries). The for-profit operating company contracts with the trust to provide services. Fiduciary duty is legally crisp: trustees have named duties to beneficiaries.

**Option B — Professional corporation analogue.** In healthcare, professional corporations (physicians, lawyers) have specific fiduciary duties enforced by regulatory colleges. Adapt the concept to a "health data services corporation" with duties defined in its articles.

**Option C — Advocate for new category.** Engage Privacy Commissioner of Canada + provincial equivalents to establish a "regulated data fiduciary" licensure category, similar to investment advisors (who are fiduciaries) vs stockbrokers (who are not). This is a 3-5 year advocacy project.

**Recommendation:** pursue Option A in Year 2 as the operational structure, pursue Option C in parallel as a policy advocacy track. Option C being successful is upside; Option A alone is sufficient.

### 9.3 The fiduciary protocol as an open specification

Publish the consent + data handling + access + revocation + inheritance semantics as an open specification — the "Fiduciary Protocol for Health Data." Open because:
- Standards-setting creates a moat via first-mover status on the protocol
- Open-source signals genuine commitment to patient-principal (regulators + patients trust this)
- Other vendors adopting it increases total market for fiduciary-structured data
- Specific AMI implementation remains closed; the specification is open

Analogous to how the IETF sets TCP/IP standards and many vendors implement — but the most trusted implementations win. AMI as the trusted reference implementation of the protocol it defined.

### 9.4 Consent semantics — specific

A fiduciary consent is:
- **Granular.** Patient consents to specific data categories (vitals, location, audio, etc.) for specific purposes (their care, their family, specific researchers) for specific durations.
- **Revocable.** Any consent can be withdrawn; platform must purge or de-anonymize data accordingly.
- **Inheritable.** On patient death, consent transfers per patient's directive (to estate, to named inheritors, to anonymized research pool, to destruction).
- **Portable.** Patient can direct data transfer to a different fiduciary at any time (exit rights).
- **Auditable.** Every use of every data item is logged; patient can query "what happened with my heart rate data last month?"
- **Cryptographically anchored.** Each consent is hashed into the platform's audit chain; post-hoc modification detectable.

### 9.5 Inheritance data — a new frontier

Patients die. Their data lives on. What happens to it?

Current systems: data is deleted, or held by the provider indefinitely, or transferred to estate under vague legal authority. Patient's specific wishes about post-death data use are rarely honored because they weren't captured.

AMI's proposed approach:
- Patient specifies inheritance at consent enrollment: "on my death, transfer my data to my designated care partner" / "anonymize and contribute to research" / "destroy" / "transfer to my children for potential medical history use."
- Platform executes directive on documented death (from medical examiner, family verification, or ongoing non-response signal with verification).
- Legal framework: platform acts as executor for data only (not financial estate), under pre-specified directives.

**Why this matters strategically.** Inheritance data, anonymized and aggregated, becomes a unique research asset — multi-generational health data, impossible for any single-generation competitor to accumulate. More importantly: it completes the fiduciary promise. Patients trust the platform because the platform honors their wishes even after they die.

The legal + technical infrastructure for this takes Y4-Y6 to build. First deaths in the enrolled cohort may be Year 3-5 (reflecting the aging-in-place population). By Year 10, the first hundreds of inheritance data directives have been executed. The cultural moment of "my grandmother's data is still helping people decades after she died" becomes a story worth telling.

---

## 10. The embodied AI thesis

Software moats collapse. One specific category does not: software embodied in physical devices, deployed in specific locations, certified for specific uses, connected by specific institutional relationships. Call it **embodied AI**, in contrast to the dominant 2024-2026 pattern of disembodied cloud AI.

### 10.1 Why embodiment is a moat when software isn't

Consider two platforms with equivalent clinical capability:
- Platform A: cloud LLM + API, accessible from any clinical system, integrated via REST.
- Platform B: local compute appliance + sensor suite + wearable, deployed in clinics and homes, with identical clinical capability to Platform A.

In 2028, Platform A's software advantage is copyable in 6-12 months. Platform B's entire stack — the appliances shipped, the sensors certified, the homes already deployed, the clinical validation accumulated — is not.

Embodied AI has properties disembodied AI cannot:
- **Physical presence** generates clinical signal (sensor data) that cloud AI doesn't have.
- **Local computation** enables privacy + sovereignty + latency guarantees that cloud cannot provide.
- **Certified form factor** satisfies regulators in ways software-only cannot.
- **Installation footprint** creates switching costs that API integrations lack.
- **Maintenance relationships** create ongoing revenue and trust that one-time-purchase software lacks.

### 10.2 The disembodied AI limit

A pure-software medical AI (LLM + API + subscription) has specific structural limits:
- It cannot generate sensor data; it only processes data sent to it
- It cannot be certified as a medical device with physical indications
- It cannot satisfy data sovereignty requirements when cloud-hosted
- It cannot operate without network
- It cannot create installation-based switching costs

These limits are not failures of engineering; they are structural properties of disembodiment. Any medical AI company staying disembodied will hit these ceilings.

AMI's decision to deploy embodied AI — sensors + appliances + wearables — is specifically to break past these ceilings. The hardware moat is not decorative; it is architectural liberation.

### 10.3 Operator-in-the-loop, not human-in-the-loop

A related framing: "human-in-the-loop" is the standard AI safety pattern — a human reviews AI output before action. "Operator-in-the-loop" is stronger — a **named, accountable operator** (the physician, the trial investigator, the family caregiver) is the legal decision-maker, with the platform as a highly-capable informant.

Why this matters legally: liability accrues cleanly to the named operator. Malpractice insurance frameworks already handle physician liability; they don't handle "AI did it" gracefully. A platform whose architecture always names a human operator is insurable; a platform claiming autonomous clinical judgment is currently uninsurable.

Why this matters regulatorily: FDA / Health Canada approve AI tools more readily when a clinician is the decision-maker. The 2024 FDA guidance on AI in electronic systems + forthcoming AI-specific frameworks uniformly assume operator-in-the-loop. AMI's architecture satisfies these frameworks by design.

### 10.4 Architectural liability transfer

Follow-on insight: because AMI has replay + audit architecture, a physician using AMI can demonstrate, years later, exactly what information was available, what the AI suggested, and what the physician decided. This changes the malpractice risk profile.

- On traditional ambient AI scribes without replay: "did the AI suggest X and the physician ignore it?" is hard to answer; assumption is adverse.
- On AMI: "was this warning issued?" has a cryptographically-verified answer from the audit log.

The follow-on claim: AMI-using physicians are less exposed to specific malpractice claims than physicians using unaudited AI. This is worth exploring with Canadian Medical Protective Association (CMPA) and US malpractice insurers as a structural risk-reduction argument. If CMPA offers reduced premiums for AMI users, or specific liability safe harbors, the platform becomes strongly preferred by risk-conscious physicians.

This is a specific, concrete Year-3-4 initiative: quantify the liability-reduction claim with CMPA, seek formal insurer recognition.

---

## 11. Compound dynamics — how moats reinforce each other

A list of moats is a portfolio. A structure where each moat makes others stronger is something else. Concrete dynamics:

### 11.1 Sensor Fabric → Longitudinal Data

Each sensor deployed generates continuous data. 200,000 units × 5 years = 1,000,000 device-years of continuous signal. This feeds Moat 5 (data) directly. Without sensors, data has to come from EMR extracts (shallow, episodic). With sensors, data is multi-modal and continuous.

**Compound effect:** each sensor unit deployed in Year N contributes to the dataset for Year N+5 clinical evidence papers (Moat 4) and the Year N+7 regulatory submissions (Moat 3).

### 11.2 Longitudinal Data → Clinical Evidence

With 100K patients × 5-10 years, rare events become statistically tractable. Specific outcomes (avoidable hospitalizations, falls, cognitive decline acceleration, medication-adherence correlates) can be measured at scale. These measurements become peer-reviewed papers (Moat 4) and support regulatory submissions (Moat 3).

**Compound effect:** data density makes publications easier and submissions more credible. Year-7 papers are cheaper to produce than Year-4 papers because the data already exists.

### 11.3 Clinical Evidence → Regulatory + Institutional

Published evidence supports regulatory submissions (direct contribution to 510(k)s). Published evidence also opens institutional doors — academic medical centers, specialty societies, practice guideline groups engage with platforms whose evidence they've seen in journals.

**Compound effect:** each paper accumulates citations over years. Year-3 paper cited in Year-7 guideline is a multiplied asset.

### 11.4 Regulatory Stack → Institutional Deployment

Specific 510(k) clearances unlock specific institutional deployments. Ontario Health Teams require Canadian MDL for certain features. Hospital systems require SOC 2 Type 2 + HIPAA BAA for deployment. US health systems require HITRUST. Pharma sponsors require 21 CFR Part 11 validation.

**Compound effect:** each clearance unlocks a category of customers. Categories compound: SOC 2 + MDL + 510(k) together unlock customers none could unlock alone.

### 11.5 Institutional Graph → Sensor Deployment

Institutional partners deploy sensors at scale. 3-5 Ontario Health Teams × 20 clinics each = 60-100 clinics deployed via institutional contracts rather than individual sales. 2-3 pharma sponsors running home trials = 2000-5000 home deployments.

**Compound effect:** institutional channels multiply individual-sale velocity by 10-100×. The same sales team produces 10-100× the deployed footprint via institutional graph.

### 11.6 Sensor Deployment → Sensor Fabric

More deployments mean more manufacturing volume, which reduces BOM cost, which increases margins, which funds more R&D, which improves sensors, which attracts more deployments. The classic hardware flywheel, but operating at medical-device scale rather than consumer electronics scale.

**Compound effect:** Year-10 sensor BOM is 30-50% of Year-3 BOM at 20-50× the volume. Unit economics improve dramatically with scale.

### 11.7 The meta-dynamic

Each moat advance in Year N produces reinforcing advances in at least two other moats in Years N+1 through N+3. The plan's capital efficiency comes from this: one dollar invested in sensor deployment (Moat 1) produces returns in data accumulation (Moat 5), clinical evidence (Moat 4), institutional trust (Moat 6), and regulatory support (Moat 3). In a well-designed 10-year plan, every Year-N dollar produces Year-N+5 returns in multiple categories.

The critical reviewer should test this claim: *pick any single investment and trace its return paths across five years*. If an investment only contributes to one moat, it's under-leveraged; the plan should emphasize investments that contribute to multiple simultaneously.

---

## 12. The Canadian advantage — specific, structural

A Canadian-origin platform is not a handicap; it's an advantage specifically suited to this plan. Specifics:

### 12.1 Healthcare system structure

- **Universal coverage + single-payer provincial systems** produce consistent data standards (OHIP billing, OLIS labs, Panorama immunizations) that US fragmented systems don't.
- **FHO+ (Family Health Organizations) in Ontario** provide capitated payments that reward prevention, making the platform's cost-reduction value proposition clean and legible.
- **Ontario Health Teams** are integrated care structures that specifically reward platform-style coordination.
- **Small provincial markets** allow pilot-then-scale: 500 FHO physicians in one region can validate the platform, then roll to 5,000 across the province, then to 50,000 across Canada.

### 12.2 Data sovereignty and privacy

- **PHIPA / HIA / PIPA provincial statutes** have stronger data residency norms than HIPAA (which permits cross-border flows). Local-first architecture is preferred, not merely tolerated.
- **Provincial data residency requirements** are tightening (Ontario proposed + Quebec's Bill 64); Canadian cloud regions are non-negotiable for US-origin platforms, which increases friction for competitors.
- **Canadian cultural expectation** that health data stays sovereign — distinct from the US "consumer consent = adequate" model.

### 12.3 Research infrastructure

- **CIHR (Canadian Institutes of Health Research)** funds longitudinal cohort studies at scale — CLSA, CARTaGENE, Canadian Network for Aging Research — providing partnership opportunities and data-sharing templates.
- **Canadian pharmaceutical industry presence** (Apotex, Jamp, Pharmascience + Canadian operations of multinationals) funds clinical trials in Canada; platform-integrated trials are attractive.
- **Academic medical centers** (U of T, McGill, McMaster, UBC, Dalhousie) are research-intensive and open to collaborative platforms.

### 12.4 Regulatory and policy environment

- **Health Canada medical device licensing** is more predictable than FDA (shorter timelines, fewer rounds, more collaborative with Canadian companies).
- **Canada Health Infoway** funds digital health interoperability and has structured vendor approval programs.
- **Federal-provincial agreements** are moving toward integrated care + cross-border (between provinces) coordination, which the platform architecture specifically supports.
- **CMPA (Canadian Medical Protective Association)** is a consolidated medical malpractice insurer for most Canadian physicians — a single negotiation partner for architectural liability transfer (Section 10.4).

### 12.5 Talent and cost structure

- **Canadian engineering + clinical salaries** are 30-40% lower than US equivalents, improving capital efficiency.
- **Canadian immigration** policies favor skilled health tech workers; talent acquisition from Commonwealth + Francophone networks is easier.
- **Strong university pipelines** (Waterloo, U of T, McGill, UBC) for engineering + clinical + regulatory talent.

### 12.6 Strategic expansion path

- **Canada first, UK/Australia second, US third** is a specific sequence that favors the plan: UK NHS partnerships are accessible with Canadian Commonwealth + healthcare-standard alignment; Australian Medicare has structural similarities to Canadian provincial systems; US enters last via specific states (Medicare Advantage markets) and integrated health systems.
- **Avoiding US-first** avoids the regulatory bog and commoditized-SaaS-scribe competition that characterizes US health tech in 2026-2030.

**The Canadian advantage is real.** But it's not automatic — it requires deliberate leaning-in on Canadian regulatory, clinical, and policy structures. Year 1 decisions (Ontario-first deployments, CIHR partnerships, CMPA engagement) make the Canadian advantage concrete.

---

## 13. Revenue architecture — staged to fund the moats

The capital plan (Section 15) requires revenue to offset dilution and extend runway. Revenue is also a signal — if Year 3 revenue hits targets, the plan's core premise (physicians pay for embedded value) is validated.

### 13.1 Revenue sources, by year

**v2 revision:** the v1 table conflated recurring ARR with grants + partnership milestone payments. The v2 table separates them cleanly. The reviewer's critique — that SR&ED and grant funding is cash support, not ARR — is adopted. Grants/partnerships are reported as a separate column and excluded from ARR totals.

| Year | Primary care SaaS | Home monitoring | Clinical trials | Enterprise / health system | Total ARR (recurring) | Grants + partnerships (non-ARR cash) |
|------|-------------------|-----------------|-----------------|----------------------------|----------------------|--------------------------------------|
| Y1 | $100K | - | - | - | $0.1M | $0.5M |
| Y2 | $1M | - | - | - | $1M | $1M |
| Y3 | $4M | $0.5M | - | - | $4.5M | $2M |
| Y4 | $10M | $3M | $1M | - | $14M | $3M |
| Y5 | $20M | $8M | $5M | $2M | $35M | $5M |
| Y6 | $35M | $20M | $15M | $8M | $78M | $10M |
| Y7 | $50M | $35M | $25M | $20M | $130M | $15M |
| Y8 | $60M | $50M | $30M | $35M | $175M | $20M |
| Y9 | $70M | $65M | $40M | $50M | $225M | $25M |
| Y10 | $80M | $85M | $50M | $70M | $285M | $30M |

**Year 10 canonical ARR target: $250-350M recurring** (previously stated as $150-300M in intro / $390M in v1 revenue table — reconciled). Plus ~$30M annual grants + partnerships. Plus non-recurring strategic transactions (pilot-to-scale conversions, pharma milestone payments) that vary year to year.

**Critical reviewer caveat accepted.** The clinic-level unit economics implied by these numbers — roughly $80-100K ARR per clinic at end of Y3, growing over time — imply 20-35 clinicians per clinic at the stated $200-400 per-clinician-per-month price. This is possible in large FHO groups and multi-physician enterprises, not obvious in small practices. A bottom-up clinic model (clinic size × physician count × base price × usage × attach rate × churn) must be built in the 24-month operating memo. Until then, treat the revenue trajectory as directional, not commitment-grade.

### 13.2 Revenue model specifics

**Primary care SaaS:** per-physician-per-month, $200-400 baseline + usage fees. Growth through FHO+ + specialty wedges + hospital outpatient clinics.

**Home monitoring:** per-patient-per-month, $50-150. Insurance-reimbursed where possible (OHIP home care codes, US Medicare home care codes), private-pay otherwise. Hardware bundled in monthly fee.

**Clinical trials:** per-trial setup fee ($50-200K) + per-site ongoing ($1-5K/month) + per-patient ($100-500/month during active enrollment). Sponsors pay directly.

**Enterprise / health system:** multi-year contracts, $500K-5M annually, bundle across primary care + home + specialty.

**Partnerships / non-dilutive:** pharma co-development, government grants (NSERC, CIHR, IRAP, SDTC), provincial economic development. Milestone-based.

### 13.3 Gross margin profile

Software-only revenue: 80-85% gross margin. Hardware-inclusive revenue: 50-65% gross margin. Clinical trial services: 40-55% gross margin (higher cost of service delivery). Blended by Year 10: 65-70% gross margin.

Lower than pure SaaS, higher than most medical device companies. Acceptable for the plan.

### 13.4 The $10M Year-3 check

If total ARR at end of Year 3 is below $8-10M, the plan is in trouble. The mid-point test: Year 3 revenue validates or invalidates the "physicians pay for embedded value" thesis. Below target means either the wedge is wrong (pivot specialty), the pricing is wrong (reduce and expand volume), or the value is genuinely weaker than claimed (revisit).

This is a specific falsifiable milestone that a critical reviewer can track.

---

## 14. Capital architecture — sequenced, with non-dilutive emphasis

Total 10-year capital: $250-400M. Structured across rounds + partnerships + non-dilutive.

### 14.1 Funding sequence

| Round | Year | Amount | Dilution | Primary use |
|-------|------|--------|----------|-------------|
| Seed | Y1 | $3-5M | 15-20% | Team to 6; hardware consultant; first certifications; Oscar Pro integration |
| Series A | Y2-3 | $8-15M | 18-22% | Team to 20; sensor v1 productization; first pilot studies; SOC 2 Type 2 |
| Series B | Y4-5 | $15-25M | 18-22% | Team to 40; home kit manufacturing; first 510(k); first paper |
| Series C | Y6-7 | $30-50M | 15-20% | Team to 80; custom silicon decision; CE-MDR; international |
| Strategic / growth | Y8-9 | $40-80M | 10-15% | Platform expansion; pharma partnerships; US pilots |
| Pre-IPO or strategic | Y10 | $50-100M | 5-10% | Scale-up or exit preparation |
| **Cumulative equity** | | **$150-275M** | **~60-70%** | |

### 14.2 Non-dilutive capital

Parallel to equity rounds:

| Source | Y1-3 | Y4-6 | Y7-10 | Cumulative |
|--------|------|------|-------|------------|
| IRAP (NRC) | $1-3M | $2-5M | $2-5M | $5-13M |
| CIHR | $0.5-2M | $2-5M | $3-8M | $5-15M |
| NSERC CRD | $0.5-1M | $2-4M | $2-4M | $4-9M |
| SDTC | - | $3-8M | $2-5M | $5-13M |
| Provincial (Ontario, Quebec, BC) | $0.5-2M | $2-5M | $2-5M | $4-12M |
| Infoway digital health | $0.5-1M | $2-3M | $2-4M | $4-8M |
| Pharma strategic partnerships | - | $5-15M | $15-40M | $20-55M |
| Academic grants (matching) | $0.5-1M | $2-4M | $3-5M | $5-10M |
| **Cumulative non-dilutive** | **$3-12M** | **$20-50M** | **$30-75M** | **$50-135M** |

Non-dilutive targets: 25-40% of total capital, materially reducing equity dilution. This is an explicit design goal; most Canadian deep-tech companies underuse non-dilutive sources.

### 14.3 Strategic partnership capital

Distinct from venture capital: pharma sponsors, large EMRs, medical device companies, health systems. Each partnership produces co-investment or co-development capital without dilution:

- Year 3-5: first pharma co-development ($5-15M)
- Year 6-8: major pharma platform partnership ($15-40M)
- Year 8-10: possible large strategic (EMR, hospital system, device company) investment ($20-80M) — potentially an acquisition offer that is declined if terms insufficient

### 14.4 Patient capital philosophy

The 10-year timeline does not match most venture capital horizons (typically 7-10 years from fund formation, meaning 3-7 years from investment). Identify capital sources that can hold for a decade:

- **Family offices** (patient 20-year horizons, focus on healthcare impact)
- **Sovereign wealth funds** (Canadian CPPIB, CDPQ; Singaporean GIC; Norwegian Government Pension Fund) with healthcare infrastructure mandates
- **Foundations** (Robert Wood Johnson, Canadian Medical Research Foundation, Gates Foundation) willing to mix grant + investment
- **Strategic corporates** (large pharma, major EMR, health system — corporate venture arms) with 10-year horizons
- **Specialized health tech VCs** with long-hold funds (Oxeon, Section 32, specific Canadian funds)

Build relationships with this class of capital from Year 1. Standard 10-year-fund VCs will invest but their incentives pressure exit at Year 5-7 — which is mid-plan. Patient capital is architecturally necessary.

---

## 15. Team architecture — specific roles, specific phases

The company grows from 4-6 people (Y1) to 100-150 (Y10). Specific hires at specific phases.

### 15.1 Phase 1 (Y1-2): foundation team

- Founder (technical lead, strategic)
- Senior full-stack engineer × 2
- Medical/clinical advisor (part-time physician)
- Regulatory consultant (part-time)
- Hardware consultant (industrial design + firmware, part-time → full-time Y2)
- First customer success / clinical trainer (Y2)

**Critical Y1 hire:** the hardware consultant. If this person is not in place by Month 3, the hardware moat's lead time stretches to Year 5 and the plan fails.

### 15.2 Phase 2 (Y3-4): validation team

- VP of Engineering (Y3)
- VP of Hardware (Y3) — critical, dedicated, full-time
- Director of Regulatory Affairs (Y3)
- Head of Clinical Operations (Y3-4)
- Principal Clinical Investigator (Y3-4, can be fractional)
- Sales + CS × 3-5 (Y3-4)
- Engineers × 6-10

**Team size Y3:** 15-20.
**Team size Y4:** 20-30.

### 15.3 Phase 3 (Y5-7): scale team

- Chief Medical Officer (Y5) — clinical leadership, evidence generation, guideline engagement
- Chief Regulatory Officer (Y6-7) — international clearance strategy
- VP of Field Operations (Y5) — deployment at scale
- VP of Enterprise Sales (Y5)
- Head of Research Partnerships (Y5-6)
- Director of Silicon (Y6, contingent on silicon go/no-go)
- Engineers × 30-50 (across software + firmware + silicon)
- Sales + CS × 15-25
- Field ops × 10-20

**Team size Y5:** 40-60.
**Team size Y7:** 70-100.

### 15.4 Phase 4 (Y8-10): infrastructure team

- Chief Commercial Officer
- Chief Scientific Officer
- International leaders (US, EU)
- Platform partner management
- 100-150 total team by Y10

### 15.5 Board architecture

- Y1 board: founder + 2 seed investors + 1 clinical advisor
- Y3 board: add 2 seed + Series A investors + 1 independent (healthcare industry)
- Y5 board: add Series B lead + independent (regulatory / clinical research experience)
- Y7 board: add independent (former large pharma or health system executive)
- Y10 board: 9-11 members, balanced founder-VC-independent-strategic

Independent board members with specific healthcare operating experience from Y3 onward. This isn't CYA; it's how the founder gets honest strategic counsel for a domain outside typical software experience.

### 15.6 The founder arc

- Y1-3: technical lead + product; in the code
- Y4-5: operator; running teams; less in code
- Y6-7: platform architect; institutional + strategic; hires CEO potentially
- Y8-10: chairman / board; operating CEO runs the company; founder focuses on vision + strategic partnerships + next-generation thinking

A founder unwilling to make this arc explicit at Year 1 may struggle to scale. The plan assumes the founder evolves; if the founder would rather stay technical-lead forever, the plan doesn't work and STRATEGY_2026 is the better fit.

---

## 16. Kill scenarios — what ends each moat

Honest inventory of what kills each moat, with specific mitigation architecture.

### 16.1 Sensor Fabric kill scenarios

- **Safety incident** (sensor causes harm, false negative in critical indication): FDA recall + market freeze. Mitigation: ISO 13485 QMS + extensive validation + conservative initial indications + post-market surveillance.
- **Supply chain disruption** (specific component unavailable, CM failure): production halt. Mitigation: dual-source critical components, qualify backup CMs, maintain 6-month inventory.
- **Competitor commoditizes sensor hardware** (Apple or Amazon ships equivalent at $30): price pressure. Mitigation: clinical indications create regulatory moat even if hardware commoditizes; AMI's value is in the indication + integration, not the silicon.
- **FCC/IC regulation changes** (new interference rules, mmWave restrictions): product re-certification required. Mitigation: actively participate in standards bodies; design with margin for rule changes.

### 16.2 Compute Appliance kill scenarios

- **Apple/Nvidia shift inference hardware strategy** (base platform becomes unavailable or too expensive): architecture refresh required. Mitigation: hardware abstraction layer; dual-path inference on 2+ NPU types.
- **Custom silicon program fails** (tape-out yields poor; design flaws): $30-50M write-off. Mitigation: only commit to silicon after Year 5-6 validation; staged commitment with off-ramps.
- **Medical EMI standards change**: recertification. Mitigation: participate in standards; design margin.

### 16.3 Regulatory Stack kill scenarios

- **FDA shifts policy on AI in medical devices**: unclear pathways. Mitigation: operator-in-the-loop architecture aligns with any foreseeable framework; explicit regulatory intelligence role from Year 3.
- **Major clearance denied**: delay of 1-2 years. Mitigation: pre-submission meetings, experienced regulatory consultants, submission quality investments.
- **Data breach or audit failure**: SOC 2 or HIPAA status at risk. Mitigation: aggressive security investment, red-team testing, breach response plan that preserves certifications.

### 16.4 Clinical Evidence kill scenarios

- **Primary outcome study fails** (platform doesn't reduce hospitalizations in well-designed trial): evidence moat cracks. Mitigation: conservative initial outcome claims, multiple parallel studies, honest reporting of null results that redirect strategy.
- **Named investigator scandal or withdrawal**: temporary setback. Mitigation: diversify investigator roster, strong research ethics culture, formal CoI policies.
- **Specific paper retraction**: credibility hit. Mitigation: pre-registered protocols, independent statistical analysis, reproducibility-first culture.

### 16.5 Data + Consent kill scenarios

- **Major data breach**: trust destroyed. Mitigation: defense-in-depth architecture, encrypted at rest + in transit, zero-trust network, aggressive detection + response. A single breach can end the company; security is existential.
- **Consent challenge in court** (consent deemed inadequate for specific use): limited data use until remediation. Mitigation: conservative consent semantics, granular + revocable + auditable, engage Privacy Commissioners early.
- **Fiduciary structure rejected by regulators** (if novel legal structure not accepted): fall back to standard processor model. Mitigation: architect as hybrid; fiduciary is upside, processor is floor.

### 16.6 Institutional Graph kill scenarios

- **Key EMR vendor hostile integration** (Oscar Pro decides to compete, blocks integration): deployment path obstructed. Mitigation: diversify EMR integrations early; multi-EMR support; advocate for interoperability (CanDIG, etc.).
- **Major health system pilot fails publicly**: reputation hit. Mitigation: start with small pilots in friendly environments; scale only after clear wins.
- **Government policy shift** (change in provincial health priorities): contract risk. Mitigation: multi-province presence; not single-customer-dependent.

### 16.7 Compound kill scenarios

The most dangerous failures are those that hit multiple moats simultaneously. Example: a safety incident (Moat 1 sensor harmed a patient) can trigger regulatory review (Moat 3 loss), clinical evidence reassessment (Moat 4 weakens), institutional trust loss (Moat 6 erosion), and patient consent revocations (Moat 5 shrinks) — all from one event.

Mitigation: architectural commitment to safety at all layers (hardware, software, clinical, operational), plus incident response plans that address all moats simultaneously.

---

## 17. Compound sequencing asymmetries (v1: "uncopyable combinations")

*v2 revision. The reviewer's rhetorical critique is accepted: "uncopyable" overclaims. The real argument is about multi-year lead times that competitors cannot collapse. Several v1 ideas are relabeled here as **Phase 2+ optionality**, not core near-term moats.*

Specific combinations of moats produce multi-year sequencing asymmetries:

### 17.1 Longitudinal data + fiduciary consent structure

Cannot be created retroactively. Cannot be bought (consent is non-transferable). Cannot be replicated by competitors even with identical software + hardware — they must independently accumulate each patient's consent and each patient's observation time.

By Year 10, AMI's 100K-patient × 5-year longitudinal consented cohort is unique in North American primary care + home monitoring. Even if Apple committed $5B to the space in Year 6, by Year 11 they'd have Year-5 data; AMI would have Year-16 data.

### 17.2 Operator-in-the-loop + replay architecture + CMPA optionality

**v2 reclassification: Phase 2+ optionality, not core near-term moat.**

A specific combination: a platform architected to name the operator, with cryptographic audit of every AI suggestion, *potentially* with a formal relationship with the dominant Canadian malpractice insurer accepting the architecture as risk-reducing.

**Reviewer critique accepted.** Current CMPA guidance emphasizes physician responsibility, consent, privacy, and regulatory approval — it does not signal any willingness to negotiate premium reductions or safe harbors based on platform architecture. A formal CMPA agreement recognizing architectural liability transfer is speculative upside, not a moat to plan around. The operator-in-the-loop + replay architecture is real and defensive on its own; the CMPA piece is a possible Year 5-7 conversation if the architecture proves itself in practice.

### 17.3 Canadian regulatory depth + sovereignty architecture + provincial contracts

A US-origin platform cannot satisfy Canadian data sovereignty requirements without fundamental architectural changes. A Canadian-origin platform *and* US expansion gives regulatory flexibility a US-only platform lacks.

By Year 8, AMI has FHO+ revenue in Ontario + home-care revenue in multiple provinces + Ontario Health Team contracts + CMPA structural agreement + Canadian MDL + Infoway vendor status. None of these are available to a US-origin competitor.

### 17.4 Embedded sensor fabric + fiduciary consent + clinical validation for specific indication

To replicate AMI's ambient UTI detection (Section 5): need the specific sensors deployed, the specific consent for the specific data, the specific clinical validation publishing PPV/NPV, and the specific 510(k) clearance for the decision support.

Each step is years. The combination is a decade. Even if a competitor can build equivalent capability faster in 2030 than AMI did in 2026 (due to software commoditization), the non-software time-dependent steps remain.

### 17.5 The Specialist Emeritus program (Phase 2+ optionality)

**v2 reclassification: future optionality, not core moat.**

A creative idea worth naming: many retired specialists (cardiologists, geriatricians, etc.) want meaningful part-time work. Build a "Specialist Emeritus" program — retired Canadian specialists provide second-opinion review of specific AMI alerts (ambient UTI detection, fall risk patterns, cognitive decline signals).

*The reviewer's critique is accepted: this is fascinating optionality but not a load-bearing near-term moat. It belongs in the document as a possible Year 5-7 program if earlier layers succeed, not as something carrying strategic weight today.*

Why it's still worth noting: a pool of 50-100 retired specialists doing 5-10 hours/week at AMI is specific to AMI and difficult for a consumer-tech competitor to replicate because it requires cultural credibility with retired specialist networks. But building this pool is not a Year 1-3 priority.

### 17.6 Multi-generational data inheritance (Phase 3+ optionality)

**v2 reclassification: 20-year future, not near-term strategic weight.**

By Year 10-15, AMI potentially holds longitudinal data on patients who have died (and whose data lives on per their directives). By Year 20, AMI might begin holding the first parent-child pairs across generations.

*Reviewer critique accepted: this is an interesting architectural commitment to make from Year 1-5 (the inheritance semantics must be designed in early or retrofitting is impossible), but it carries no near-term strategic weight. It belongs in the long arc, not in the Year 1-5 narrative.*

### 17.7 Physician institutional memory + workflow encoding

By Year 7-10, some Canadian family physicians have been on AMI for their entire late-career. Their specific workflow preferences, their billing patterns, their clinical decision styles, their referral networks — all are encoded in the platform's per-physician state.

A physician who has been on AMI for 7 years and is offered a competing platform faces not just switching costs but *workflow memory loss*. The competitor has to rebuild from scratch what AMI already knows. This is a specific labor-market-capture effect that grows with tenure.

---

## 18. The 30/90/365-day plan — actionable

Every ambitious plan starts with executable near-term actions. What the founder does in the next year determines whether the decade is available.

### 18.1 First 30 days

- **Commit to the plan internally.** This means: founder decides this is the 10-year plan (not STRATEGY_2026, not STRATEGY_2031, not some blend). The commitment is not "I'm willing to try this" — it's "this is what I'm building for the next decade."
- **Write the one-page external version** of the vision (a beautiful, simple articulation for investors, partners, recruits).
- **Schedule 10 strategic conversations:**
  - 3 with primary care physicians (20+ years practice)
  - 2 with Canadian health tech investors with 10-year horizon patience
  - 2 with Canadian academic medical research leaders
  - 1 with a Canadian EMR executive
  - 1 with a medical device regulatory consultant
  - 1 with a hardware consultant who has shipped FCC/IC-certified products
- **Hire a medical device regulatory consultant** (fractional, ~$5-10K/month) for advisory.
- **Identify 2-3 candidate hardware consultants** (industrial design + firmware, medical device experience).

### 18.2 First 90 days

- **Hire the hardware consultant** (contract or fractional, target 20-40 hours/week).
- **Engage a contract manufacturer** for exploratory conversations (Celestica, Sanmina, Jabil Healthcare) on sensor productization timelines and BOM.
- **File SOC 2 Type 1 preparation** (engage auditor, implement controls).
- **Start PIPEDA compliance work** (formal privacy impact assessments, documented practices).
- **File first IRAP application** for sensor productization R&D ($500K-2M).
- **Initiate Oscar Pro integration work** (technical diligence with Oscar team).
- **Write the first clinical validation study protocol** with a physician partner and a biostatistician.
- **Establish the data fiduciary legal structure** (Canadian nonprofit trust; engage healthcare lawyer).

### 18.3 First 365 days

- **Hire core team to 6 people:**
  - VP of Engineering (software)
  - Hardware lead (full-time by Month 12)
  - Clinical Advisor (fractional physician, formal agreement)
  - Regulatory Affairs Director (fractional to full-time)
  - Head of Clinical Operations
  - Customer Success / Clinical Trainer
- **Close seed round** ($3-5M) with explicit narrative: hardware-heavy 10-year plan.
- **Ship sensor v1 prototype** suitable for first 510(k) pre-submission conversation.
- **Complete first pilot study enrollment** (target 100-200 patients).
- **Achieve SOC 2 Type 1** and begin SOC 2 Type 2 monitoring period.
- **Oscar Pro integration deployed** at 5-10 clinics.
- **Windows port shipped** (expanded Canadian addressable market).
- **First peer-reviewed paper submitted** (feasibility / workflow).
- **1,000 patients** on fiduciary consent structure.
- **10+ paying clinics** with $1M+ ARR.
- **Non-dilutive capital raised:** $1-3M across IRAP + provincial + CIHR engagement.
- **Board formed** with 2 healthcare-experienced independents.

### 18.4 Quarterly milestones, Year 1

| Quarter | Sensor / hardware | Regulatory | Clinical | Commercial | Team |
|---------|-------------------|------------|----------|------------|------|
| Q1 | HW consultant engaged; ID work started | SOC 2 Type 1 audit engaged; PIPEDA framework | Physician advisor on board; 1st study protocol drafted | Oscar Pro technical diligence | Founder + 2 eng + fractional advisors |
| Q2 | Sensor v1 prototype in design | SOC 2 controls implemented; HIPAA BAA path | Study IRB submitted | Oscar Pro integration coding | +1 eng + regulatory |
| Q3 | Sensor prototype built + tested; FCC pre-scan | SOC 2 Type 1 achieved; PIPEDA certified | 1st pilot enrollment begun | Oscar Pro in 3-5 pilot clinics | +VP Eng + clinical ops |
| Q4 | Sensor v1 refined; CM conversations underway | SOC 2 Type 2 monitoring begun | 100-200 patients enrolled | 10 paying clinics; $1M ARR | 6 people + fractional advisors |

If any Q4 milestone is missed by more than one quarter, the plan is behind and needs adjustment. The critical reviewer can track these.

---

## 19. Intellectual honesty — what I don't know

A document written to withstand critical AI review must honestly enumerate its uncertainties. Eight specific things I don't know with confidence:

### 19.1 Will software actually commoditize 10-30× by 2030?

I believe yes, based on 2024-2026 trajectory. But trajectories extrapolate poorly. Software might commoditize only 3-5× (still significant but less dominant) or might commoditize 30-100× (even more dominant). The plan is robust to 3-100× range. Outside that, assumptions need revisiting.

### 19.2 Will FDA + Health Canada frameworks favor operator-in-the-loop?

I believe yes, based on current guidance. But regulatory frameworks can shift (EU AI Act is already more restrictive in specific ways). The plan assumes favorable frameworks; specific unfavorable shifts could require architectural adjustments.

### 19.3 Will the Canadian healthcare system remain hospitable to a local platform?

I believe yes, based on sovereignty trends. But health system priorities shift with governments (Ontario's recent health tech decisions have been mixed). The plan assumes Ontario/federal remain receptive; if they become hostile, the platform may pivot to different provinces or countries.

### 19.4 Will data fiduciary as a legal structure be recognized?

I believe it can be structured under existing trust/nonprofit law (Option A from Section 9.2). I'm less confident that a formal "regulated fiduciary" category (Option C) will emerge — that's policy advocacy that takes 3-5 years and may not succeed. The plan is robust to Option C failure; it depends on Option A.

### 19.5 Will CMPA (or equivalent) negotiate architectural liability transfer?

I believe it's worth trying. I don't know if CMPA will engage seriously. If not, the "physician preference driven by reduced malpractice exposure" thesis weakens. Mitigation: this is upside in the plan, not floor.

### 19.6 Will 100,000 patients enroll on fiduciary consent structure by Year 10?

I believe yes, achievable via FHO+ clinic scale-up + home-care partnerships. I'm less confident about the rate of fiduciary-specific consent (vs standard consent) acceptance. If fiduciary consent is confusing or intimidating to patients, enrollment slows. Mitigation: extensive UX research on consent presentation starting Year 1.

### 19.7 Will custom silicon be necessary or economic?

I genuinely don't know. Silicon economics are the most uncertain part of the hardware plan. The Go/No-Go decision at Year 6 exists precisely because I can't predict this; the plan is robust to either outcome.

### 19.8 Will the founder be the right person for all phases?

The founder arc (Section 15.6) assumes evolution from technical lead → operator → platform architect. If the founder is unable or unwilling to make that evolution, a CEO transition is needed by Year 6-7. The plan accommodates this; it requires honest self-assessment.

---

## 20. One-page summary — what this document argues

**Context.** AI-assisted coding is commoditizing software. By 2028-2030, software is no longer a moat. AMI Assist's ten-year vision (STRATEGY_DECADE) is realizable only if built on non-software foundations (STRATEGY_MOATS).

**Thesis.** Six non-software moats — sensor fabric, compute appliance, regulatory stack, clinical evidence, longitudinal data with fiduciary consent, and institutional graph — compound together over a decade to form a structure that cannot be replicated in less than a decade even with unlimited capital.

**Vision.** By 2036, 100,000+ patients experience healthcare as one continuous platform across primary care, home, hospital, and research. The platform holds their data in fiduciary trust. Their physicians use it with insurance-recognized architectural liability transfer. Their families have granular consent-mediated visibility. Their clinical trials run as a mode of the platform, not a separate system. Their data inherits per their directives after they die. The healthcare system spends 30-40% less on them; their outcomes are 20-30% better; their dignity is preserved.

**Execution.** Year 1-2: primary care + sensor productization start + fiduciary consent v0. Year 3-4: home extension + first clearance + first paper. Year 5-7: clinician-worn device + specialty + silicon decision. Year 7-10: international + pharma + custom silicon if pursued + platform-as-infrastructure.

**Capital.** $250-400M across 10 years. 60-70% equity, 25-40% non-dilutive + strategic. Patient capital classes (family offices, sovereign wealth, foundations) to match the timeline.

**Team.** Grows 4-6 → 100-150. Hardware lead by Month 12. VP of Hardware by Year 3. CMO by Year 5. CEO transition possibility by Year 7.

**Revenue.** $1M (Y2) → $14M (Y4) → $78M (Y6) → $195M (Y8) → $390M (Y10) across primary care SaaS + home monitoring + clinical trials + enterprise + partnerships.

**Mid-point test.** Year 3 ARR of $8-10M validates the thesis. Below, pivot. Year 5 first paper published + first 510(k) cleared validates the regulatory moat. Year 7 institutional graph at 10+ enterprise contracts validates the compound dynamic.

**Kill scenarios.** Enumerated for each moat (Section 16). Mitigation architectures designed in.

**Uncopyable combinations.** Enumerated (Section 17). The most durable asset is not any single moat but specific combinations that require a decade of compound investment.

**The critical reviewer's test.** Can the reviewer point to a specific moat investment that produces returns in only one moat category? If every investment compounds across multiple moats, the plan is structurally sound. If some investments are single-category, they may be misallocated.

**The founder's test.** Is this a 10-year commitment with a 30-year architectural horizon? If yes, the plan is available. If the founder wants a 3-year venture-backed exit, STRATEGY_2026 is the better document. Mixing them — attempting the infrastructure plan on a venture timeline — is the worst outcome.

---

## 21. Closing — why this specifically

Seven strategy documents now exist. Six describe directions. This one (and its sibling STRATEGY_MOATS) describe the *mechanisms* by which the directions survive commoditization. Without the moats, the directions are fragile; any direction can be copied by a well-funded competitor in 12-24 months if the moat isn't built.

The case for this specific plan over the others:

**vs STRATEGY_2026 (1 year).** That plan delivers revenue in 12 months with a $3-5M seed. This plan delivers infrastructure in 10 years with $250-400M. The 1-year plan is lower-risk, lower-capital, lower-ceiling. The 10-year plan requires founder-life commitment that the 1-year plan does not. Both are legitimate; they're different life choices.

**vs STRATEGY_2031 (5 years).** That plan builds a regulatory-moat specialty company. This plan builds an infrastructure platform. The 5-year plan is a mid-scale successful outcome ($50-200M); this plan is a healthcare-system-level outcome ($1-5B+) with correspondingly higher capital and team scale.

**vs STRATEGY_UNBOUND / CLINIC_OS / CLINICAL_TRIALS.** These describe specific product directions. This plan includes them as phases. CLINIC_OS is Phase 1-2 specialty care integration. CLINICAL_TRIALS is Phase 3. Unbound ideas sprinkle throughout Phase 2-5. The 10-year plan is the container in which the specific directions fit.

**vs STRATEGY_DECADE alone.** That plan describes the vision without the moats. This plan adds the moats without which the vision is fragile.

**vs STRATEGY_MOATS alone.** That plan describes the moats without the integrated vision. This plan integrates them with specific deliverables, timelines, capital, team, and revenue architecture.

**The integrated argument:** the vision and the moats are not separable. The vision requires the moats to survive competition. The moats require the vision to have coherent strategic direction. Together they describe a company that does not yet exist but could exist — a Canadian-origin, moat-rooted, ten-year-horizon healthcare infrastructure platform that plausibly reshapes how a patient experiences their health across their life.

The critical reviewer's final question should be: *is this plan internally consistent?* If yes, the plan is worth engaging with seriously. If no, the inconsistencies are the feedback that improves it.

## Appendix A — Document relationships

| Document | Horizon | Role in STRATEGY_COMPOUND |
|----------|---------|---------------------------|
| STRATEGY_2026 | 1 year | Phase 1 (primary care wedge) |
| STRATEGY_2031 | 5 years | Phase 2-3 overlap (specialty regulatory moat) |
| STRATEGY_UNBOUND | N/A | Creative reservoir for Phase 2-5 |
| STRATEGY_CLINIC_OS | 3-5 years | Phase 1-2 clinical reflex + autonomic architecture |
| STRATEGY_CLINICAL_TRIALS | 3-5 years | Phase 3 research integration |
| STRATEGY_DECADE | 10 years | The vision being fortified |
| STRATEGY_MOATS | 10+ years | The moats being structured |
| **STRATEGY_COMPOUND (this)** | **10 years** | **Integrated execution of moat-rooted vision** |

## Appendix B — Specific ideas worth highlighting for reviewers

For a critical AI reviewer who wants to focus on the most unusual claims:

1. **Architectural liability transfer via CMPA** (Section 10.4) — novel, untested, high-upside
2. **Data fiduciary as open protocol** (Section 9.3) — standards-setting as strategic move
3. **Inheritance data architecture** (Section 9.5) — legal + technical + cultural innovation
4. **Operator-in-the-loop as regulatory + insurance moat** (Section 10.3) — architectural framing that shifts liability cleanly
5. **Specialist Emeritus program** (Section 17.5) — labor-market-capture specific to AMI
6. **Multi-generational data moat** (Section 17.6) — 20-year architectural bet
7. **Physician workflow memory as labor-capture** (Section 17.7) — switching cost that grows with tenure
8. **Patient capital requirement** (Section 14.4) — explicit architectural dependency on non-standard VC
9. **Quarterly Year-1 milestones as falsifiability** (Section 18.4) — specific tests the plan must pass
10. **Compound dynamics meta-test** (Section 11.7) — every investment must produce returns in multiple moat categories

These are the points most likely to produce productive disagreement or refinement.

## Appendix C — What would change my mind

Specific evidence that would cause major revision:

- Clear evidence that Canadian data sovereignty is weakening, not strengthening — reduces Canadian advantage
- Evidence that FDA is becoming more permissive of cloud-only, autonomous clinical AI — weakens embodiment thesis
- A major breach of consent graph at a peer platform — erodes fiduciary trust category broadly
- Apple + Google converging on a shared ambient-health framework with regulatory buy-in — compresses competitive window to 3-5 years
- Evidence that software commoditization is slower than projected (3-5× by 2030 rather than 10-30×) — reduces urgency of non-software investment
- A Canadian government mandate for interoperable health platforms with open-source requirements — changes competitive dynamics
- Strong evidence that longitudinal continuous observation does not meaningfully improve outcomes — undermines value prop
- A catastrophic AMI Assist incident (data breach, clinical harm) in Year 1-3 — resets the entire plan

The plan is a hypothesis. Evidence changes hypotheses. The document should be revised as evidence accumulates.

---

*End of document. Total pages: the document is long because the synthesis requires it. A reviewer who wants the one-page version reads Section 20. A reviewer who wants the argument reads Sections 1-3. A reviewer who wants to stress-test reads Sections 16-19. A reviewer who wants the vision reads Section 5. The rest is the specificity that separates "plan" from "aspiration."*

---

## 22. Stage gates — what must be proven before the next stage commits capital

*New in v2. The reviewer's most important structural critique was that v1 proposed too many simultaneous bets. Stage gates are the explicit mechanism that replaces implicit parallelism with disciplined sequencing. Each stage gate is a go/no-go decision. If the gate is not met, the next stage does not commit capital; the plan narrows or pivots.*

### Stage 1 gate (end of Month 24) — Wedge proven

**Must be true to proceed to Stage 2:**

- $8-15M ARR with clear unit economics (per-clinic revenue, retention, expansion rate) documented
- One major EMR integration (Oscar Pro or PS Suite) stable and deployed at ≥30 clinics
- SOC 2 Type 2 achieved + PIPEDA certified + HIPAA BAA capable
- One clinical study enrolling (not necessarily published) with IRB-approved protocol
- At least one institutional reference customer at OHT, hospital system, or academic center level
- Physician retention >90% after 6 months; NPS >50 on clinical-usefulness measures
- Room sensor productization track on schedule (prototype certified, pilot manufacturing validated, 50-100 units deployed)
- **Not required:** custom appliance, wearable, six-device kit, fiduciary protocol, pharma partnerships, international expansion

**If gate is NOT met:** narrow the plan. Phase 2 hardware investment is deferred. Company returns to pure-software + room-sensor focus for 12 more months, or pivots to the STRATEGY_2026 path if fundamental wedge logic is wrong.

### Stage 2 gate (end of Month 48) — Differentiated signal proven

**Must be true to proceed to Stage 3:**

- $30-50M ARR
- At least ONE continuous-observation capability with demonstrable clinical impact (published outcome data in peer-reviewed venue, or accepted for publication; hospital admission reduction, fall detection sensitivity/specificity, or equivalent hard endpoint)
- First FDA 510(k) submitted OR first Health Canada MDL granted for a specific indication
- One multicentre validation study complete, findings in line with single-site pilot
- 3-5 institutional partnerships at enterprise level (OHTs, hospital systems, academic centers)
- Home-monitoring pilot deployed with ≥500 patients; retention + satisfaction measured
- Clear decision on which sensor to verticalize in Stage 3 (bed? bathroom? wearable?)
- Bottom-up clinic unit economics validated against actuals

**If gate is NOT met:** Stage 3 hardware expansion and broader clearances are deferred. Company stays in specialty-focused clinical-AI mode, closer to STRATEGY_2031.

### Stage 3 gate (end of Month 84) — Compound moats visible

**Must be true to proceed to Stage 4:**

- $100M+ ARR
- 2+ peer-reviewed outcome papers published; ≥1 in top-tier journal
- First clinical practice guideline mention or formal CMO-level reference agreement
- 2+ FDA/Health Canada major clearances in hand
- 10+ enterprise institutional customers with renewal rates >90%
- Data fiduciary Phase 2 (formalized stewardship model) operational for ≥20% of patient cohort
- Field operations at positive contribution margin (home monitoring unit economics ≥40% gross margin after field services cost)
- Clear competitive position: can articulate in one sentence what AMI does that ambient-scribe competitors cannot

**If gate is NOT met:** international expansion, silicon exploration, and Phase 3 governance (formal fiduciary protocol) are deferred. Company continues as Canadian + single-adjacent-market infrastructure platform.

### Stage 4 — Infrastructure (Year 8-10)

Only entered if Stages 1-3 gates all cleared. This is where international + silicon + pharma platform + trial operating mode become serious bets, not before.

---

## 23. Missing considerations — operational realities the v1 document underweighted

*New in v2. Five areas the external reviewer flagged as underweighted. Each is now acknowledged with appropriate strategic implications.*

### 23.1 Indigenous data governance

A Canadian health data platform without an explicit Indigenous data governance posture is strategically and ethically incomplete. First Nations data sovereignty (OCAP® principles — Ownership, Control, Access, Possession — established by the First Nations Information Governance Centre) establishes distinct requirements for research and health data involving Indigenous populations.

**Plan implication:**
- Year 2-3: formal engagement with FNIGC and provincial Indigenous health authorities before any platform deployment in communities with significant Indigenous population
- Year 3-4: data governance architecture modified to support OCAP® compliance as a first-class requirement, not an afterthought
- Decision: do not deploy in Indigenous-serving contexts until the governance architecture explicitly accommodates OCAP® requirements

This is a gap in v1 that the v2 plan formally commits to closing in the first operational memo.

### 23.2 French-language support

Quebec is 23% of the Canadian population. French-language support is not optional for a Canadian infrastructure platform. Beyond Quebec, Francophone minority communities (New Brunswick, Ontario, Manitoba) require service in French. Current AMI scribing likely has English-dominant language models; French Canadian accent coverage, Quebec medical vocabulary, and bilingual encounters are distinct capabilities.

**Plan implication:**
- Year 1-2: audit current language coverage; estimate gap to production Quebec deployment
- Year 2-3: bilingual scribing at production quality before any Quebec FHO deployment
- Year 3-5: French Canadian outcome evidence in clinical validation studies (not English-only)
- Ongoing: French content for patient-facing consent flows, handouts, family dashboards

### 23.3 Substitute decision-makers and evolving capacity

The Mary vignette in Section 5 describes a population in which cognitive capacity may change over time. A consent architecture that only handles competent adults fails for this population.

**Plan implication:**
- Phase 1 consent UX must handle substitute decision-maker workflows (legal authority, documentation, revocation)
- Phase 2 governance must specify how capacity transitions are handled (loss of capacity triggers what workflow? emergency overrides? family disagreements?)
- Phase 3 inheritance semantics must handle the specific case of dementia + post-death data continuity

### 23.4 Third-party consent in home deployments

Home sensors capture spouses, adult children, PSWs, cleaners, visitors, and incidental conversations. Patient consent covers the patient; it does not cover household third parties.

**Plan implication:**
- Year 3+ home deployments require explicit third-party consent UX (household poster, visitor notification, incidental-capture handling)
- Privacy-preserving sensors (thermal, mmWave, audio with on-device processing) are not a panacea; they still capture signal about non-consenting third parties
- Legal review specifically for multi-occupant household deployments, including the case where one member consents and another does not
- Likely requires specific product differentiation (guest mode, household-wide consent, household conflict resolution procedures)

### 23.5 Field operations margin modeling

Hardware + home deployment implies installation, replacement, troubleshooting, elder support, caregiver support, and in-home service channels. The v1 document allocated 10-15% of capital to field ops. The reviewer's critique: this is likely under-modeled for the scale proposed.

**Plan implication:**
- Year 3-4 home monitoring pilot must include full field-ops cost measurement (install time, repair rate, caregiver support volume, replacement cost)
- Year 5+ home monitoring unit economics must hit ≥40% gross margin after field services, or the business is structurally unprofitable
- Field operations may become a regional partnership model (provincial home-care agencies, franchise technicians) rather than fully in-house
- Patient retention depends heavily on field-ops quality; an under-invested field organization kills home monitoring NPS and churn economics

### 23.6 Rural and remote deployment complexity

Canada has significant rural and remote population where internet connectivity is intermittent, cellular coverage is incomplete, and technician travel is expensive. A platform architecture that implicitly assumes metropolitan deployment excludes 15-20% of the Canadian patient population.

**Plan implication:**
- Edge-first architecture is actually well-suited to this — but only if explicitly validated at intermittent-connectivity sites
- Starlink integration (or equivalent) may be required for home monitoring in rural contexts
- Service level targets must be achievable with longer technician travel radii
- This is an advantage as well as a challenge: rural-deployed evidence is distinctive and opens policy doors that urban-only platforms cannot match

---

## 24. Updated competitor landscape

*New in v2. The v1 document framed the competitive threat as primarily future (Apple, Google, Microsoft entering ambient health in Year 5-8). The reviewer correctly notes that the nearer threat is the already-existing ambient workflow market — and that several vendors have already moved past "scribe-only" positioning.*

### 24.1 Current-state competitors (2026)

| Vendor | Positioning 2026 | Threat profile to AMI |
|--------|------------------|-----------------------|
| Microsoft Dragon Copilot | Ambient scribe + workflow automation + EMR-embedded assistance (Nuance DAX successor) | Largest. Bundled with Microsoft + enterprise hospital EHR footprint. Not Canadian-sovereignty-friendly by default. Primary competitor for Canadian hospitals. |
| Abridge | Clinical documentation + coding support + ambient AI with specialty tuning | Strong US primary care + specialty. US-focused so far; Canadian deployment limited. Well-funded ($300M+ raised). |
| Suki | Ambient documentation + ambient order staging + EHR-agnostic | Physician-loved in US primary care. Moving into orders + workflow. Less Canadian presence. |
| Nabla | Ambient scribe + Epic integration + French-origin company | Direct threat in Canada because: (a) Epic-native integration valuable for Canadian hospitals on Epic, (b) French-language heritage relevant to Quebec. |
| Tali AI | Canadian-origin AI scribe | Most direct Canadian competitor. Positioning overlaps with AMI. Relationship matters — competitive but not necessarily adversarial. |
| Mutuo, Phelix, other Canadian entrants | Various Canadian ambient + workflow tools | Fragmented; no single leader yet. Market is coalescing. |

### 24.2 Strategic implications

**The "not just a scribe" claim is no longer differentiating.** Dragon Copilot, Abridge, Suki, and Nabla all market workflow capabilities beyond transcription. AMI's moat cannot be positioned as "we do more than scribing." The moat has to be:

1. **Unique continuous signal** (home + sensor-based observation that cloud ambient vendors cannot access)
2. **Canadian sovereignty + institutional embedding** (specific to Canadian regulatory, procurement, and trust dynamics)
3. **Regulatory depth** (specific 510(k) clearances that cleared competitors don't have)
4. **Clinical evidence** (outcome studies that change procurement decisions)
5. **Stage 3+ moats** (institutional graph density, longitudinal data, governance architecture)

**The Canadian wedge is the short-term defensible position.** Nabla (French-origin) is the most structurally concerning competitor in Quebec. Dragon Copilot is the most concerning in Canadian hospitals. Abridge and Suki are primarily US concerns unless they materially expand into Canada.

**Future (Year 5-8) competitors remain Apple, Google, Microsoft.** But these are now *additional* threats, not the primary ones. The primary threats are the ambient workflow vendors already deployed.

### 24.3 The crowded-market paradox

Counter-intuitively, the crowded ambient market strengthens AMI's deeper moat thesis. If ambient documentation is becoming commoditized, then durable differentiation must come from evidence, home signal, cross-setting continuity, regulatory advantage, and institutional trust — exactly what Moats 1-5 describe. The v2 Moat reordering (institutional graph first, clinical evidence second) is a more honest answer to "what differentiates in a crowded ambient market" than the v1 hardware-centric framing.

---

## 25. Probability calibration — v2 honest self-assessment

*New in v2. The reviewer's probability estimates are incorporated with appropriate caveats.*

**Probability of the full v1-as-written vision (10-year platform with all major moats intact):** ~5-15% (reviewer estimate, accepted).

Why low: too many independent hard things must go right simultaneously — workflow adoption + hardware execution + regulatory success + clinical evidence + fundraising over a decade + governance innovation + organizational scaling across very different business models.

**Probability of a strong mid-scale company if the v2 plan (with stage gates and narrowed scope) is executed well:** ~25-40% (reviewer estimate, accepted).

Why higher: narrowing first three years to proven wedge + differentiated signal + core regulatory work, with speculative ideas as optionality, is more fundable and survivable.

**Probability of any meaningful outcome (positive acquisition or sustainable business above $30M ARR):** ~50-60% (my estimate, not the reviewer's).

Why: even in partial failure modes, the primary-care wedge + sensor work + institutional relationships likely produces *something* worth owning. The question is whether that something is an infrastructure platform ($1B+) or a specialty clinical AI company ($50-200M).

### 25.1 The calibration implication

If the base rate for full-vision-as-written is 5-15%, the v1 document's confidence in specific Year-10 numbers was too high. V2 revisions reduce that overclaiming:
- ARR target is now a $250-350M range (not $390M)
- Paper targets are decision-grade-evidence-oriented, not publication-count
- Moat claims use "multi-year lead" language, not "uncopyable"
- Hardware roadmap is explicitly narrowed with stage gates
- Speculative ideas are labeled as optionality

The document is still ambitious. It is now calibrated-ambitious instead of aspirational-ambitious. That is a meaningful difference for investor + partner + founder reality-testing.

### 25.2 The asymmetric bet

The reviewer's final observation worth reinforcing: *even if the grandest version never arrives, the plan should leave behind a company worth owning*. The stage gates ensure this. If Stage 1 succeeds but Stage 2 fails to prove a differentiated signal, the company is still a $30-80M Canadian ambient AI company — a legitimate mid-scale outcome, not a failure. If Stage 2 succeeds but Stage 3 stalls on regulatory or international work, the company is still a $100-200M specialty clinical AI company with a defensible Canadian position.

The asymmetric bet is: small capital + narrow scope at each stage → stop-loss limited → upside preserved. That is fundable in a way a $250-400M all-or-nothing thesis is not.

---

## Appendix D — What was cut, delayed, or downgraded in v2

*Per reviewer's detailed editing recommendation. This appendix documents what changed so future revisions can trace reasoning.*

### Cut from core narrative for now

- Custom silicon as active investment (still mentioned as Year 5-10 contingent optionality)
- Multi-generational data as a strategic story (kept as 20-year architectural principle)
- Specialist Emeritus program as a near-term commitment (kept as Phase 2+ optionality)
- Explicit CMPA premium-reduction thesis (replaced with "operator-in-the-loop architecture is defensive on its own; CMPA engagement is speculative upside")
- "Platform-as-EMR-alternative" language (replaced with "outcome evidence for flagship indications" — preserves EMR partner relationships)
- Early international ambition as committed milestone (now Stage 4 optionality)

### Delayed until later proof exists

- Formal fiduciary protocol as open spec (now Phase 3, not Phase 1-2)
- Inheritance data as a strategic story (kept as architectural principle, moved out of near-term narrative)
- Multiple hardware categories in parallel (narrowed to room sensor → one home-monitoring signal → one wearable indication, sequenced not parallel)
- Pharma / trials as a major revenue pillar in Y4-6 (now Stage 3+ expansion, after Stage 1-2 core is proven)
- CE-MDR and broader international paths (Stage 4)

### Kept and sharpened

- Ontario / Canada-first workflow wedge
- One major EMR integration (Oscar Pro primary)
- Privacy / audit architecture as table-stakes defensive posture
- One meaningful evidence program focused on decision-grade outcomes
- One differentiated home / continuous signal program (narrowed)
- **Institutional graph moved to Moat 1** (most important structural change)
- Regulatory path for a carefully-chosen flagship indication
- Operator-in-the-loop architecture as defensive design
- Canadian sovereignty as procurement advantage (not exclusion of competitors)

### The new strategic core, in one paragraph (reviewer's version, adopted)

*Build the most trusted Canadian clinical AI platform for community-based care. Win first in physician workflow and coordination, with strong integrations, privacy discipline, and measurable time-saving value. Add one evidence-backed home monitoring capability that cloud-only ambient vendors cannot match. Use that wedge to accumulate the assets that compound over time — institutional trust, longitudinal signal, clinical evidence, regulatory depth, and patient-directed governance. Everything else — trials mode, broader device surface area, formal fiduciary protocols, silicon — only happens if those first layers genuinely work.*

### The 12 questions to answer before funding a decade

(Per reviewer, adopted verbatim.)

1. What is the first buyer, exactly, and why do they buy in the first 12 months?
2. What is the first regulated indication, exactly, and why that one?
3. Which hardware is actually proprietary, and which should be OEM / partner-based?
4. How does the clinic-level unit economics model work in detail?
5. How much consent friction is acceptable before workflow breaks?
6. What happens if the fiduciary / trust structure is legally workable but commercially confusing?
7. What happens if CMPA never engages beyond generic medico-legal guidance?
8. What is the fallback if one core home-signal hypothesis fails?
9. What are the conditions under which the company explicitly does NOT build custom hardware categories?
10. What is the first truly decision-grade evidence package, and what financing round depends on it?
11. Which institutional relationships are for revenue, which are for evidence, and which are for policy leverage?
12. If the company had to become a great $30-60M ARR business without the full grand vision, what would that business be?

Question 12 is the most important discipline: a good deep-tech plan should leave a valuable company behind even if the grandest version never fully arrives. The v2 stage gates preserve this optionality.

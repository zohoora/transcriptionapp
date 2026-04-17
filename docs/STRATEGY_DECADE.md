# A Ten-Year Vision — A Unified Healthcare Platform

*The most ambitious framing of all the strategy documents. Synthesizes STRATEGY_UNBOUND's eight directions and six weird combinations into a single 10-year narrative. The question this document answers: what if the codebase's architecture is the substrate of a new age of healthcare — one where research, primary care, specialty care, hospital care, and home care stop being separate systems and become one connected experience centered on the patient?*

*This is aspirational. It is also grounded — every piece has precedents, the economics are calibrated, the risks are enumerated honestly. Read it as a map of a possible future, not a prediction.*

---

## 0. A note on ambition

There's a version of this document that reads like a pitch deck — all ceiling, no floor, every number rounded up, every complexity hand-waved away. I'm not writing that version. The vision below is genuinely ambitious; the decade-long arc that gets there includes hard constraints on clinical validation, regulatory timing, behavioral adoption, and the uncomfortable reality that *healthcare changes slowly, then suddenly, then slowly again*.

Read what follows as the honest answer to "what could this become if done well over ten years," not "what it definitely will become." The latter no one knows.

---

## 1. The beginning — 2036, what healthcare feels like

Imagine Mary, 74, lives alone in her home in Hamilton. She was diagnosed with mild cognitive impairment three years ago. Her husband died five years ago. Her daughter Elena lives 40 minutes away. Her family doctor, Dr. Chen, has known her for 18 years.

In 2036, this is Mary's week:

**Monday morning.** Mary wakes at 7:14 AM — slightly late, which the home platform noticed. Her bathroom visits overnight were normal; sleep fragmentation index trending stable over three months. The kitchen sensor registers her making tea at 7:32. She takes her memantine (the bottle detects the cap opening; presence sensor confirms she was in the kitchen). No reminder needed.

**Tuesday.** Her wearable detects a 2-bpm resting HR uptick compared to her baseline, sustained over 36 hours. The platform's risk-surveillance layer considers it: combined with slightly reduced morning activity (down 8% vs 90-day trend), it matches a soft pre-clinical pattern for urinary tract infection in elderly women (common, treatable). The system doesn't alarm Elena. It doesn't call Mary. It gently queues a note on Dr. Chen's tablet for her Wednesday morning huddle: *"Mary K — subtle trend suggesting early UTI surveillance; recommend check at next visit or phone call if any new symptoms."* Dr. Chen messages Mary's portal that afternoon: *"How's the week going? Any discomfort with bathroom trips? Just checking in."* Mary replies yes, a little — and a urine culture is delivered to her home and picked up Thursday. Empiric antibiotics start Friday before culture is back. The infection never progresses to a fall, an ER visit, or a hospitalization. Cost to the system: $45. Cost to Mary: zero friction.

**Wednesday.** Mary is enrolled in a Phase III Alzheimer's prevention trial (lecanemab follow-on, enrolled 14 months ago). Her trial visit this morning is virtual. She opens the research app on her tablet — same hardware as her care platform, different mode. The coordinator, Jen, runs her through her quarterly cognitive battery. Jen doesn't manually score anything; Mary's responses + response times + engagement patterns are captured and structured automatically. Jen reviews the session output, adds her clinical impressions, signs. 40 minutes, three-quarters of which was the actual assessment, not paperwork. The trial's sponsor sees Mary's data in aggregate within an hour, not in next month's data lock.

**Thursday.** Mary's trial platform + her care platform share the same data substrate. Elena, her daughter and designated care partner, checks the family dashboard: Mary's weekly trajectory looks stable. No concerns. Medication adherence 96%. Socialization proxy (people visiting, phone calls received) within normal range. Sleep is the main trend to watch.

**Friday.** The UTI antibiotics are working; Mary's resting HR has normalized. Dr. Chen sees the update on her tablet during rounds. No additional action needed. The platform's continuous observation meant the entire episode happened before anyone felt sick.

**Sunday.** Mary goes to church. The platform loses signal for four hours, as expected. When she comes home, the trip is inferred from her absence window and her return pattern. Nothing is logged from church — that's her private life.

**Over the year.** Mary has had no emergency department visits. No unplanned hospitalizations. Her daughter has not received a 3 AM panic call. Her daily life is hers, not managed. The healthcare system has spent less on her this year than the average Medicare patient her age — despite her having dementia, living alone, and participating in a clinical trial.

**Mary doesn't know any of this is happening.** She knows she likes Dr. Chen, her daughter visits more than she did five years ago, and she feels safer than she used to. She can't name the platform running in the background. That's the point.

---

## 2. Why today is broken

Mary's 2036 experience is impossible in 2026. Not because the technology doesn't exist — most of it does, in pieces, in different silos. It's impossible because **the pieces don't connect**.

In 2026:

- **Primary care** runs on Oscar Pro or PS Suite; sees Mary every 4 months for 15 minutes.
- **Home care** (if she has it) runs on a different system with different staff and no access to Dr. Chen's chart.
- **Hospital** runs on Epic; if Mary were admitted, no continuity with Dr. Chen's or home care's records.
- **Pharmacy** runs on PharmaClik; adherence is a guess.
- **Research** runs in a trial-specific EDC (Medidata); her trial data is invisible to her care team.
- **Insurance/OHIP** runs on a claims system; pays for episodes, not outcomes.
- **Family** runs on text messages and panicked phone calls.

Each silo has its own data, its own identity system, its own consent model, its own authority boundary. The patient experiences this as friction — each transition costs time, context, and trust.

The fragmentation isn't a bug; it's the historical structure of how healthcare computing evolved over 50 years. Each vendor optimized their silo. Each regulator protected their domain. Each professional protected their liability. The resulting system is Pareto-suboptimal: *everyone* spends more, *outcomes* are worse, *patient experience* is degraded, *clinician burnout* is epidemic, *research recruitment* is glacial.

Every major healthcare AI company today is building yet another silo. A better scribe. A better patient portal. A better clinical trial platform. A better remote patient monitoring product. A better care coordination app. All of them valuable. None of them solving the actual problem, which is that **health is one continuous thing and the system treats it as a series of disconnected transactions**.

A 10-year horizon can either build the next-better silo or build the thing that connects silos. This document imagines the second.

---

## 3. The unified architecture

The 10-year vision is not a product. It is **a substrate** — an architectural foundation that makes multiple products possible on one connected stack. Think of it as a healthcare-specific alternative to how Apple thinks about hardware + OS + apps, or how the internet thinks about TCP/IP + HTTP + the application layer.

### 3.1 The five-layer unified stack

The same layering as the CANS document, extended to the whole healthcare continuum:

**Layer 0 — Peripheral sensing, everywhere the patient is**
- Primary care clinic rooms: audio, presence, CO2 (current)
- Specialty clinic rooms: same
- Hospital rooms: extended with medical device integration
- Home: ambient presence + wearables + bathroom + sleep + medication + environmental
- Research settings: identical to above, with study-specific instrumentation overlaid
- Pharmacy: dispensing events, refill patterns
- Any professional setting where the patient consents to observation

One sensor fabric. One local-compute appliance per site (home, clinic, hospital ward). Same firmware family. Same data capture pipeline.

**Layer 1 — Reflex/rule layer, context-aware by setting**
- Primary care: the CANS reflexes (room turnover, billing prompts, clinical alerts)
- Hospital: different reflexes (fall risk, deterioration detection, medication timing)
- Home: different reflexes (fall detection, medication adherence, activity anomalies)
- Research: protocol-specific reflexes (visit window compliance, assessment completeness)
- Setting-specific, but same architectural pattern. Rules configured by setting.

**Layer 2 — Autonomic orchestration, continuous across settings**
- Care coordination: when Mary moves between settings (home → clinic → hospital → home), continuity is maintained automatically
- Longitudinal memory: every setting contributes to one patient graph
- Medication reconciliation: continuous, not just at admission/discharge
- Preventive care: continuous surveillance, not annual checkups
- Research participation: seamless integration with care without doubling the workload
- **This is where the silos dissolve.** The autonomic layer doesn't respect institutional boundaries — it's the patient's nervous system across their whole health experience.

**Layer 3 — Contextual awareness, respecting dignity**
- Emotional/mental state (Mary seems worried today; physician is given a soft cue)
- Life context (Mary is adjusting to her husband's death; trajectory is sensitive to this)
- Cognitive engagement (Mary's participation in her own care planning; feedback loops to clinicians)
- Family context (Elena's engagement, caregiver burden, sibling coordination)
- Never surveillance; always in service of the person being sensed

**Layer 4 — Strategic/population-level insights**
- Individual: trajectory prediction, risk stratification, outcome forecasting
- Clinic: panel health, resource allocation, financial sustainability
- Region: population-health patterns, epidemic surveillance, preventive-care gaps
- Research: real-world evidence generation, trial cohort identification, endpoint validation
- System: policy evaluation, health-outcome-per-dollar measurement

**Layer 5 — Action, verified and consented**
- Prescriptions, orders, referrals, reports, claims, communications
- Every action signed, auditable, replay-able, attributable
- Patient always has veto, always has visibility

### 3.2 The five principles

Everything else in this document derives from these five commitments. Without them, the platform is just another silo with a marketing story.

**Principle 1 — The patient is the principal.**
All data about a patient is in their service, owned by them (legally, operationally, ergonomically), and accessible to them in plain language. Consent is granular, revocable, and informed. Nothing happens to a patient's data without a clear chain back to their consent for that specific use.

**Principle 2 — Sovereignty by architecture, not by promise.**
Data stays local by default. Cloud use is opt-in and specific. Computation happens at the edge (home, clinic, hospital) where it can. The platform is designed to work if the cloud is gone, if a vendor dies, if a country changes laws. This is a structural property, not a privacy policy.

**Principle 3 — Every decision is replayable.**
AI-generated actions, human-generated decisions, automated workflows — all reproducible from audit trails years later. Not because regulators demand it, but because *trust requires it*. The family, the patient, the court, the sponsor, the payer — any of them should be able to ask "what happened?" and get a cryptographically-verified answer.

**Principle 4 — Continuity over transaction.**
The patient is one continuous experience, not a series of billable events. The platform treats primary care / hospital / home / research / pharmacy as *modes of engagement with the same patient*, not as separate products. Data, consent, identity, care plans, and care team persist across modes.

**Principle 5 — Research and care are two sides of one substrate.**
Every patient can opt into contributing to research. Every research insight feeds back into care. Trials are not separate systems requiring separate recruitment, separate consent, separate data capture — they are modes of engagement that any patient can enter and leave. Research is democratized; participation is a choice, not a burden.

---

## 4. The 10-year journey, phased

Not a Gantt chart. A narrative of what each phase is *for*, what it validates, what it delivers, and what it enables next. Phases overlap; the calendar is illustrative.

### Phase 1 — Foundation (Years 1-2)

**Purpose**: prove the core architectural patterns and ship revenue in one clinical domain. This is STRATEGY_2026 executed cleanly.

**Domain**: Canadian primary care, chronic pain or family medicine depending on initial validation.

**Deliverables**:
- Cut non-strategic features from existing codebase (~7K LOC removed)
- Refactor `continuous_mode.rs` into phase modules; introduce typed pipeline bus
- Ship Oscar Pro bidirectional integration (read-only first, then write)
- Windows port (opens addressable market to most Canadian clinics)
- Pain specialty wedge or FHO+ generalist wedge (based on physician validation)
- 10 paying clinics by end of Year 2
- Performance summary + observability infrastructure mature
- First SOC 2 Type 2 audit completed; PIPEDA certification
- Team: 4-6 engineers + 1 clinical advisor + 1 part-time regulatory consultant

**Why this matters for the 10-year arc**: everything downstream requires a working, trusted, profitable primary-care product as its foundation. Without this, Phases 2-5 are speculation.

**Capital**: $1-2M seed.

### Phase 2 — Home extension (Years 3-4)

**Purpose**: extend the platform into the patient's home. Validate that the same sensor + local-compute pattern works outside the clinic.

**Domain**: aging-in-place for patients of Phase-1 clinics. Start with patients already known to the platform.

**Deliverables**:
- Home sensor kit (adhesive-mount, elder-installation-friendly) hardware product
- Home local-compute appliance (Mac Mini equivalent or dedicated device)
- Family caregiver app (Elena's view in the Mary story)
- Wearable integration (Apple Watch, Fitbit, Oura, Garmin, medical-grade)
- Longitudinal patient memory implementation: 2+ years of continuous observation per patient
- Chronic disease management pilots (CHF, COPD, diabetes, post-op, mental health)
- Integration with Ontario Home Care + equivalent provincial services
- 500+ patients on home monitoring by end of Year 4
- First clinical evidence paper published (home sensor + primary care care-coordination outcomes)
- Team: 12-18 people + field operations support
- First major Series A raise ($8-15M)

**Why this matters**: Mary's 2036 experience requires the home to be an instrumented extension of the clinic. Phase 2 is where this capability gets built and validated.

**Economic insight**: in value-based contracts (which are growing), preventing one hospitalization saves $15-25K. Home monitoring that prevents even a small fraction of avoidable admissions pays for itself at the population level.

### Phase 3 — Research integration (Years 4-6, overlapping Phase 2)

**Purpose**: connect research into the platform. Validate that the same substrate serves both care and research — and that patients can participate seamlessly.

**Domain**: cognitive aging / dementia trials (per STRATEGY_CLINICAL_TRIALS), leveraging Phase 2's home-monitoring capability as a research asset.

**Deliverables**:
- Clinical trial site platform mode (Veritas positioning)
- 21 CFR Part 11 validation package completed; Health Canada conformance
- First academic research partnerships (CCNA network, Baycrest, Rotman)
- EDC integrations (Medidata, Veeva, REDCap)
- First pharma-sponsored trial on the platform (Year 5-6)
- Consent-based research-participation model: any patient can opt in; contribution is compensated or acknowledged
- Longitudinal research cohorts: patients who opt in become part of ongoing observational studies
- Team: 25-35 people including clinical research operations specialists
- Series B ($15-25M)

**Why this matters**: research integration is what turns the platform from "a better care tool" into "the substrate of a new healthcare experience." Mary's trial participation is the same platform as her care. Her consent manages both. Her data serves both.

**Cultural insight**: one of the quiet revolutions of this decade is **democratizing research participation**. Today, trial participation is an elite experience — you need to know about trials, know how to get into them, be able to travel to trial sites. When research is a mode on the same substrate as care, any patient can opt in from their kitchen.

### Phase 4 — Connection (Years 6-8)

**Purpose**: connect the platform's primary care node to specialty care, hospital, pharmacy, and insurance/payer nodes. Turn the stand-alone clinic/home product into a connected healthcare substrate.

**Deliverables**:
- Specialty clinic module: same reflex + autonomic layers, specialty-specific rules (cardiology, nephrology, oncology, psychiatry)
- Hospital inpatient module: rounds, medication timing, deterioration detection, discharge planning
- Pharmacy bridge: bidirectional refills, adherence signals, dispensing events
- Payer/insurance integration: outcome-based contracting, quality measure automation, prior-auth autopilot
- Care coordination layer: when a patient moves between settings, continuity is automatic — not faxes, not phone calls, not re-entering history
- Patient's unified identity layer: one consent, one data home, all settings connect
- Regional pilots in one Canadian province (Ontario or BC) with 5-10 clinics, 2 hospitals, 1 regional health authority
- Team: 50-70 people
- Series C or strategic partnership ($30-50M)

**Why this matters**: this is where the silo-breaking happens. Individual silos (primary care, hospital, home) have been validated on the platform in Phases 1-3; Phase 4 connects them. A patient admitted to hospital arrives with their full longitudinal context. A discharge back to home is a mode change, not a cliff.

**The hard part**: EMR integration across Oscar Pro + PS Suite + Epic (hospital) + provincial systems + pharmacy + payer is a multi-year interop project. Each integration is its own negotiation, its own regulatory review, its own validation cycle. There is no shortcut. Phase 4 is the slowest, most expensive, most essential phase.

### Phase 5 — The substrate (Years 8-10)

**Purpose**: the platform becomes infrastructure. Other organizations build on it. Healthcare experiences that were impossible become ordinary.

**Deliverables**:
- Platform APIs for third-party applications (care-coordination apps, disease-specific tools, mental health services, preventive care)
- Second country deployment (likely US via FDA pathway or UK via MHRA); architecture proven portable
- Open-source strategic components (sensor firmware, decentralized-trial eConsent, consent-management SDK) — for ecosystem growth
- Enterprise deployments: multi-site clinical research organizations, multi-hospital health systems, home care agencies
- Population-health contracts with provincial health authorities
- Regulated clinical decision support components (Health Canada Class II for specific indications)
- Fully consented real-world evidence contributing to multiple ongoing studies
- Team: 100-150 people
- Revenue: $150-300M ARR
- Valuation: $1-2B+ depending on strategic position

**Why this matters**: the platform is no longer a product; it's infrastructure that other products (from other companies) rely on. Success at Phase 5 means the healthcare experience described in Section 1 is plausibly normal for some patients somewhere, not just a demo.

---

## 5. Five stories from 2036

Because an architecture is abstract; people are not. Five illustrative portraits of what the unified platform enables.

### 5.1 Mary, 74, mild cognitive impairment, lives alone

*(Expanded from Section 1.)*

The platform gives Mary and Elena (her daughter) a partnership with Dr. Chen that was impossible in 2026. Mary's cognitive trajectory is visible as data, not guessed. Her daily life is hers, not managed. Her trial participation is seamless. Her prescribed medications are taken because the system notices when they aren't, gently. When she declines — which will happen eventually; MCI is a progressive condition — the decline is observed early, treated aggressively where treatable, and managed with dignity where not.

Economic: the healthcare system's annual spending on Mary is 30-40% lower than her 2026 equivalent (fewer ED visits, fewer admissions, slower functional decline). The cost savings fund the platform many times over.

Personal: Mary keeps her independence 2-4 years longer than she would have in 2026. Those years matter.

### 5.2 David, 45, chronic pain + anxiety + opioid dependency history

David has had chronic low back pain for eight years. He's tried PT, surgery consult, opioids (tapered off two years ago with difficulty), CBT, mindfulness, and three pain specialists. He's exhausted, his marriage is strained, his job is in jeopardy.

On the platform: his pain is a trajectory, not a number. His functional status (hours of sleep, steps, household activities) is continuously observed with his consent. His mood is tracked via passive signals and short validated instruments weekly. His medication is tracked; his appointments with his pain psychologist are integrated with his primary care.

His pain specialist in 2036 sees the full picture, not a 15-minute snapshot. A new experimental non-opioid chronic pain therapy enters Phase III trials — David is automatically identified as eligible based on his platform data, invited to consent, and can enroll with minimal friction. His trial participation contributes to a medication approval that helps hundreds of thousands of other people.

Economic: chronic pain costs the US healthcare system ~$600B annually; even 5% reduction via better-matched care pays for whole categories of technology investment.

Personal: David has a coordinated care team for the first time in his adult life. His wife stops being his case manager. His employer accommodations are based on measured function, not self-report.

### 5.3 Dr. Elaine Chen, family physician, 52

Dr. Chen has been in primary care for 23 years. In 2026 she was drowning in documentation, post-visit inbox work, prior-auth drudgery, and the cognitive tax of tracking 2,200 patients. She considered retirement in 2027.

In 2036, Dr. Chen sees 18 patients a day, ends her workday at 5:00 PM, and goes home without a laptop. The platform handles the operational layer. Her patients' longitudinal context is available when she needs it, invisible when she doesn't. Her panel's preventive care gaps are continuously closed without her tracking spreadsheets. Her chronic-disease patients' between-visit trajectories are monitored; she's notified when something deviates. Her research participation as an investigator is paid and meaningful — she enrolls interested patients, provides clinical judgment during studies, and her practice contributes to evidence that helps her future patients.

She will retire in 2040, on her own terms, not because she was broken by the system.

### 5.4 The Kumar family — three generations on the platform

Raj, 71, post-cardiac surgery, home-based recovery. Sunita, 68, diabetes management. Priya, 42, perimenopausal and caring for two aging parents. Arjun, 15, anxiety + ADHD.

Four family members, four different care needs, one platform. Priya (the sandwich-generation caregiver) has a single dashboard where she can see her parents' wellbeing trajectories (with their consent), manage her teenage son's mental health treatment integration with school and therapy, track her own health, and communicate with their shared family doctor.

Priya's consent-gated view doesn't include everything — she doesn't see her parents' private data they've chosen to shield, she doesn't see her son's therapist notes beyond what he's shared. But the coordination that used to consume 10 hours of her week now takes 20 minutes.

Economic: informal caregiving by adults like Priya is a ~$600B annual opportunity cost in North America. Platforms that reduce caregiver burden free up real economic capacity and prevent burnout-driven family health cascades.

Personal: Priya is a daughter and a mother again, not a case manager and a logistics coordinator.

### 5.5 A research participant, by default

In 2036, 40% of platform-enrolled patients have opted in to contribute their data to research in some form — ranging from anonymous aggregated population-health research to active clinical trial participation with full protocols.

Research isn't a thing that happens to some patients who seek it out. It's a mode any patient can enter with one toggle. The clinical trial platform is the care platform is the research platform. Trial recruitment times drop 60-80%. Trial diversity (age, socioeconomic, geographic, ethnic) improves dramatically because trials aren't limited to who can get to an academic medical center. Real-world evidence feeds regulatory submissions routinely, not as a novelty. Drug approvals accelerate.

Economic: pharma R&D productivity is a major driver of healthcare cost inflation. Platform-enabled faster/better research doesn't just help individual companies — it *lowers the cost of new treatments for everyone*.

Cultural: research becomes a civic contribution, like voting. Participation is normalized, ethically managed, economically compensated for meaningful contributions.

---

## 6. The platform's core integrations

What gets connected. This is the practical work of Phase 4 and onward. Each integration is a multi-year project and a real partnership.

### 6.1 Primary care EMRs
- Oscar Pro (Ontario's dominant Canadian primary care EMR)
- PS Suite / TELUS (second most common Ontario primary care)
- Accuro (Western Canada)
- Med Access
- PS Health / Profile / CHR
- By Year 5-7: all major Canadian primary care EMRs

### 6.2 Hospital EMRs
- Epic (largest, hardest)
- Cerner / Oracle Health
- Meditech (Canadian hospitals)
- By Year 8: major hospital EMRs covered

### 6.3 Specialty systems
- Cardiology (specific cardiac EMRs)
- Oncology (OncoEMR, Flatiron, iKnowMed)
- Mental health (practice-specific)
- Radiology (PACS integration for imaging)

### 6.4 Pharmacy
- Pharmacy dispensing systems (PharmaClik, Kroll)
- Provincial drug benefit programs (ODB, BC PharmaCare)
- Medication adherence sensors (smart bottles, pill dispensers)

### 6.5 Laboratory
- Central lab feeds (OLIS in Ontario, equivalent provincial systems)
- Point-of-care devices (home glucose, home BP, ECG via wearable)

### 6.6 Research
- EDCs (Medidata, Veeva, Oracle Clinical One, REDCap, Castor)
- CTMS systems
- Randomization systems
- Safety reporting (drug safety database integration)

### 6.7 Insurance / payer
- OHIP (Ontario billing, provincial equivalent elsewhere)
- Private insurance (claims submission, prior auth)
- US: Medicare / Medicare Advantage / commercial insurance
- Value-based contract measurement (HEDIS-equivalent, quality indicators)

### 6.8 Patient-owned records
- Apple Health / Google Fit / Samsung Health
- Direct patient portals
- Patient-controlled health records (MyChart-style, plus open-source alternatives)

### 6.9 Government / public health
- Immunization registries (Panorama in Ontario)
- Disease surveillance (communicable disease reporting)
- Vital records (birth, death)
- Public health (Infoway, Canadian Institute for Health Information)

### 6.10 Wearables and devices
- Apple Watch, Fitbit, Oura, WHOOP, Garmin
- Medical-grade CGMs (Dexcom, Libre)
- Home BP cuffs, scales, SpO2
- Specialty devices (ECG patches, sleep studies, fall detectors)

**Integration philosophy**: the platform is designed to be the *integrator*, not the replacement. Existing systems keep their roles; the platform provides the connective tissue.

---

## 7. What success looks like (10-year metrics)

### 7.1 Patient-level

- 100,000+ patients actively on the platform by Year 10 (primary enrollment via participating clinics)
- 50,000+ with continuous home monitoring
- 20,000+ actively participating in research at any time
- Average platform-enrolled patient age: 55-65 (reflects chronic-disease + aging focus)
- Patient satisfaction > 4.5/5 (primary concern: dignity + privacy + feeling heard)
- 90%+ of platform patients report the platform reduces family caregiver burden

### 7.2 Clinical-outcome-level

- 30% reduction in avoidable hospitalizations in platform cohorts vs matched controls
- 40-50% reduction in avoidable ED visits
- 20% improvement in chronic disease control measures (HbA1c, BP, depression scores)
- 2-4 years of additional independent living for elderly platform users
- 60-80% reduction in clinical trial recruitment time for platform-enrolled studies
- Multiple peer-reviewed publications establishing platform efficacy

### 7.3 Economic

- $150-300M ARR by Year 10
- Profitable by Year 7-8
- Verified cost savings per enrolled patient: $3-8K annually (hospitalization prevention, efficient care)
- Research partnership revenue: $30-50M annually by Year 10
- Platform-native APIs: 20-50 third-party applications deployed
- Valuation: $1-2B+ depending on strategic position (comparable to Aledade at primary-care-platform valuations, Medable at research-platform valuations)

### 7.4 System-level

- Canadian FHO+ clinics using the platform: 500-1,000 (10-20% of addressable market)
- Canadian home-monitored patients: 50,000+
- Canadian clinical trial sites on the platform: 50-100
- International presence: one additional country (US or UK) by Year 8, second by Year 10
- Regulatory milestones: SOC 2 Type 2, PIPEDA, HIPAA BAA, 21 CFR Part 11 validated, Health Canada Class II for specific clinical decision support
- Academic partnerships: 10+ institutions, multiple joint research programs
- Contributions to clinical evidence: 20+ published studies with platform-derived data

### 7.5 Team / company

- 100-150 employees by Year 10
- Headquartered in Canada (tax + talent + cultural fit)
- Satellite presence in US (customer proximity) and likely UK (regulatory + research)
- Founder either still operating or transitioned to chairman/board role
- Funding history: ~$200-400M across seed + A + B + C
- Clear strategic position: acquirable (large pharma, health system, EMR vendor) or IPO candidate

### 7.6 What success does NOT look like

Being honest: if any of the following are true at Year 10, the vision has failed to realize:

- Fewer than 50,000 patients on the platform (too niche to matter)
- Medical errors or safety issues traceable to the platform that weren't caught by the audit architecture (trust destroyed)
- Data breach or sovereignty violation that became a public news story (privacy promise broken)
- Platform dependency on a single cloud vendor or LLM provider that went offline (sovereignty failed architecturally)
- Founder burnout or replacement under distress (succession planning failed)
- Category commoditized by Epic or Tali acquiring AMI Assist and integrating it away (strategic-partnership mistake)

---

## 8. The hardest problems

Not fundraising. Not engineering. These are the problems that could kill the 10-year vision regardless of execution quality.

### 8.1 Consent and the tyranny of the consent form

Every person involved in this system has to understand what they're agreeing to, with enough clarity that regulators + ethicists + courts + them-in-five-years all respect the choice. Current informed consent is famously poorly understood. Multi-setting, multi-use, multi-temporal consent is harder. The platform needs an actually-working consent model — not legal CYA — or the whole thing fails ethically.

Research needed: consent quality science, adaptive consent UX, revocation semantics, inheritance (what happens to a patient's data when they die?).

### 8.2 Who pays for prevention?

Current healthcare economics reward volume. Preventing a hospitalization costs the payer $15K but pays the prevention provider zero. Value-based care fixes this; it's growing but slowly. The platform's economic model depends on how quickly value-based payment catches up.

In Canada this is the FHO+ program, Ontario Health Teams, federal bilateral agreements. In the US it's Medicare Advantage + bundled payments + ACOs. In other countries, other mechanisms. Platform economics are sensitive to this policy environment.

### 8.3 The data-owned-by-patient legal fiction

Patients "own" their health data in most jurisdictions but operationally they can't access it, can't port it, can't direct it. The platform promises to change this. But the practical reality is that data ownership is shared across providers, EMR vendors, labs, insurers, and government. Resolving this legally is slow. The platform may have to *assert* patient ownership and work backwards into the legal framework.

### 8.4 Regulatory variety across settings

Primary care has PHIPA. Research has ICH GCP + 21 CFR Part 11 + TCPS2. Hospital has medical device regulations. Home care has remote monitoring standards. Insurance has payer regulations. Each setting has a regulatory apparatus that evolved independently. The platform operating across all of them needs to satisfy all of them simultaneously, with consistent architecture.

### 8.5 Provider skepticism

Physicians in 2026 have been burned by multiple waves of health tech (first EMRs, then portals, then scribes, then AI). Getting them to trust another system is not rhetorical — they have earned skepticism. The platform must work quietly and earn trust gradually. A single high-profile failure can set Canadian primary care adoption back 3-5 years.

### 8.6 Liability when the system is right and the human is wrong

If Layer 3 surfaces a soft signal of deterioration, the clinician dismisses it, and the patient deteriorates — who's liable? If the system misses a signal it could have caught — who's liable? Current medicolegal frameworks don't have good answers. The platform will need to work with insurers, law societies, and medical regulatory colleges to establish new norms. This takes years.

### 8.7 Family dynamics are not software problems

The Kumar family story assumes Priya gets a well-designed dashboard that shows her what she needs to know about her parents. In reality, Priya might be estranged from her brother who disagrees with her role. Her mother might have told her son different things about her condition than she told her daughter. The platform exposing this asymmetry could cause harm, not healing.

Family-facing features need ethnographic research, not just UX.

### 8.8 What about the patients who don't want this?

Opt-in is the right principle. But healthcare systems increasingly *require* digital engagement (portals, apps, logins). The platform must work for patients who don't want continuous observation, don't want home sensors, don't want family visibility, don't want research participation. A minority-friendly design is architecturally required, not a nice-to-have.

### 8.9 The last mile is always harder than expected

Sensor installation in elderly patients' homes isn't software. Training clinical research coordinators on the platform isn't software. Getting pharmacy chains to integrate isn't software. Integrating with a regional health authority is three years of bureaucratic negotiation. Each phase of the 10-year plan will slip on last-mile operational problems, not engineering problems.

### 8.10 The acquirer question

At some point between Year 5 and Year 10, a large entity (Epic, an EMR vendor, a large pharma, a health system, a tech giant) will likely want to acquire the platform. The vision's survival depends on that acquisition happening well — or not happening at all — with the founder's values intact. This is a strategic governance question that starts Day 1, not Year 5.

---

## 9. Why this matters

Not the pitch-deck version. The actual reasons.

### 9.1 The fragmentation is costing lives, not just efficiency

Medication errors at care transitions kill people. Delayed cancer diagnoses from specialty-to-primary-care communication failures kill people. Avoidable hospitalizations from preventable chronic disease management failures kill people. Research participation gaps mean treatments exist later (or never) and disproportionately serve affluent populations. The continuous-observation + connected-care vision saves real lives at scale. This is not a commercial efficiency story; it's a mortality reduction story.

### 9.2 The caregiver-burden tax is enormous and invisible

Informal caregiving — adult children, spouses, partners — is ~$600B annually in North America and is the silent heart of how the elderly population actually lives. It falls disproportionately on women. It is an acute occupational and mental health crisis. A platform that meaningfully reduces this tax has social impact beyond its user count.

### 9.3 The current trajectory of healthcare AI is not toward a humane future

Most current healthcare AI investment is being poured into tools that extract more productivity from clinicians, surveil patients for payer contracts, or automate prior authorization denials. The future toward which this trajectory points is uglier than necessary. A 10-year platform built on the five principles in Section 3.2 is a structural counter-bet — an argument that healthcare AI can be built patient-first, sovereign, verifiable, and continuous *because the architecture demands it*, not because a vendor promises it.

### 9.4 Research democratization is one of this decade's great unclaimed prizes

Clinical trials today are an elite experience. Getting into one requires knowledge, geographic luck, socioeconomic stability, and time. The populations most impacted by diseases (low-income, rural, non-white, elderly, disabled) are systematically under-represented. When research is just another mode of engagement with a care platform, participation democratizes. The drugs that emerge work better for more people. This is a societal good.

### 9.5 Canadian-origin healthcare technology at scale is rare and valuable

Canada is a sophisticated healthcare market with a unique mix of universal coverage + fragmented delivery + strong clinical research infrastructure + active AI regulation + data sovereignty culture. The country produces excellent clinical research and excellent individual technologies but rarely produces globally-significant health technology platforms. A Canadian-origin platform that reaches international scale is a national asset. The current moment (post-COVID digital health investment + Infoway programs + CIHR leadership in aging research + Ontario Health Teams) is unusually supportive.

### 9.6 The architecture compounds over decades, not months

Software is usually short-lived. Specialties change. LLMs replace models. EMRs get disrupted. What persists over 20-30 years are *architectural patterns that prove themselves*: TCP/IP, HTTP, the Unix process model, the relational database. The five principles of the unified platform are architectural bets on what will still be true in 2046. Building them now — and doing the slow, patient work of making them real over 10 years — contributes to a long-lived infrastructure layer, not another product cycle.

---

## 10. What to do in 2026 to start this

Every ambitious 10-year plan is actually a 30-day plan repeated 120 times. What the founder does in the next 30 days either does or doesn't put this vision within reach.

### 10.1 Decide this is the vision (not a passing idea)

The 2026 and 2031 plans are legitimate alternatives. The clinic-OS and clinical-trials plans are legitimate alternatives. This 10-year vision is *bigger* than any of them. It requires larger capital, longer timeline, more team-building, more operational capacity. It requires accepting that most of the daily work won't feel visionary — it'll feel like integrating EMRs and validating compliance packages and negotiating IRB approvals.

The founder must decide, clearly, whether they want to build *this* — with all the slow, grinding, institutional work that actually builds it. If the answer is yes, the rest of this section is actionable. If the answer is "maybe," revert to a smaller plan.

### 10.2 Write the one-page version of this vision for external use

A PDF, a website, a pitch deck slide. Not the 700-line analysis — a beautiful, simple articulation that a physician, investor, or patient can read in 2 minutes and understand. The discipline of writing the one-pager forces clarity that a long document doesn't.

### 10.3 Have ten conversations

- 3 with primary care physicians who've been in practice 20+ years
- 2 with clinic owners running multi-physician practices
- 2 with academic researchers in aging / cognitive decline
- 2 with healthcare investors with 10-year horizon comfort
- 1 with a retired senior executive from a Canadian EMR or provincial health authority

Share the one-pager. Ask honest questions. Listen for what resonates vs what doesn't. The 10-year vision will survive or fail based on whether serious people who know healthcare see the shape.

### 10.4 Do the Phase 1 work regardless

Even if the 10-year vision ultimately isn't pursued, Phase 1 (the primary-care product, Oscar Pro integration, continuous_mode refactor, deprecating non-strategic features) is net-positive. It improves the business under any strategic framing. Start it immediately, independent of whether Phase 2-5 happen.

### 10.5 Start building the values infrastructure

The five principles (Section 3.2) are not marketing claims — they have to become architectural properties. In 2026, this means:
- Establishing a written data-ethics framework (to be published)
- Designing consent semantics at the database level, not the UI level
- Committing to an open-source licensing for specific foundational components
- Publishing the first annual transparency report (even if modest)

These commitments, made early and publicly, shape future decisions. Without them, the platform at Year 5 looks indistinguishable from the competitors it's trying to differentiate from.

### 10.6 Find the right first hires

Years 1-3 team matters disproportionately. The specific first engineering hire, first clinical advisor, first regulatory lead will shape the culture that carries the 10-year vision. Hire slowly for these. Prioritize values-alignment over skills — skills are buyable later, values are not.

### 10.7 Plan for the 30-year-old version of the company

Companies optimized for 10-year exits produce different architectures than companies optimized for 30-year longevity. The 10-year plan should be designed so that the company at Year 30 (the founder's grandchildren's era) still serves its principles — or has gracefully handed them off to a successor organization. This sounds dramatic but is actually a pragmatic design constraint: it forces choices that favor longevity over short-term capital efficiency.

---

## 11. One paragraph to close

The 10-year vision is that by 2036, Mary — or whoever she is in your life — experiences her health as one continuous thing: observed with her consent in her home, extended into the clinic when needed, connected to specialty and hospital care during transitions, integrated with research participation as a civic contribution, and communicated with her family and care team without friction. The platform that makes this real is not a better scribe, a better portal, or a better clinical trial tool — it is the substrate that connects all of them through five architectural principles (patient as principal, sovereignty by architecture, every decision replayable, continuity over transaction, research and care as one substrate). The 10-year journey to build this substrate starts with the primary-care wedge described in STRATEGY_2026 and ends with a platform deployed across primary care + home care + hospital + research + pharmacy + family, with 100,000+ patients, $150-300M ARR, peer-reviewed evidence of impact on mortality and morbidity, and a clear path to continuing to exist as healthcare infrastructure for decades beyond. It is ambitious beyond any reasonable business plan. It is also, uniquely among the strategic options considered, the one where the existing codebase's weirdness (replay bundles + deterministic reasoning + local-first + sensor firmware + longitudinal memory) isn't a burden to overcome but the exact shape of what needs to be built. The case for pursuing it isn't that it's likely to succeed — no 10-year healthcare plan is. The case is that the cost of failure is still a valuable Canadian healthcare-AI company, and the cost of success is a structural contribution to how humans experience their health for the next several generations.

---

## Appendix A — How the six strategy documents fit together

Six strategy documents now live in `docs/`, each asking a different question. Reading them together is the decision surface:

| Document | Horizon | Framing | Core question |
|----------|---------|---------|---------------|
| STRATEGY_2026 | 1 year | Scribe is product | How do I make revenue from this codebase in 12 months? |
| STRATEGY_2031 | 5 years | Scribe is product | How do I build a regulatory-moat clinical AI company over 5 years? |
| STRATEGY_UNBOUND | N/A | Scribe is feature | What could the codebase become if the scribe frame were removed? |
| STRATEGY_CLINIC_OS | 3-5 years | Scribe is one reflex | What does the full clinic-operating-system direction look like? |
| STRATEGY_CLINICAL_TRIALS | 3-5 years | Verifiability is product | What does the clinical trial platform direction look like? |
| **STRATEGY_DECADE (this)** | **10 years** | **Infrastructure for a new healthcare era** | **What does the fullest expression of this codebase's potential look like?** |

The documents are nested rather than alternative. STRATEGY_DECADE includes STRATEGY_CLINIC_OS as its Phase 1 and Phase 4 specialty-care component, includes STRATEGY_CLINICAL_TRIALS as its Phase 3, includes the aging-in-place direction as its Phase 2, etc. Each smaller document is a valid off-ramp from the decade plan if circumstances require — not a contradiction.

The founder's choice among them is fundamentally about:
- **Risk tolerance** (1-year SaaS vs 10-year platform)
- **Capital access** (bootstrap vs $300M+ across a decade)
- **Team scaling appetite** (solo-capable vs 100+ people)
- **Founder life preferences** (product builder vs healthcare infrastructure leader)
- **Time horizon comfort** (near-term outcomes vs decade-long building)

None of these choices is wrong. The worst outcome is choosing the 10-year vision without genuinely committing to it — spending the first two years with shifting ambitions, neither delivering the 2026 wedge nor building the 2036 substrate.

## Appendix B — Philosophical companion works

For the founder considering this vision, the following long-form works articulate parts of the underlying philosophy better than this document can:

- Atul Gawande, *Being Mortal* (2014) — on dignified aging, the inadequacy of current systems for the end of life, and the primacy of the patient's priorities
- Eric Topol, *Deep Medicine* (2019) — on AI as augmentation, not replacement, for the clinician-patient relationship
- Abraham Verghese, *My Own Country* (1994) and *Cutting for Stone* (2009) — on continuity, presence, and the human-scale of medicine
- Paul Farmer (various) — on equity as a non-negotiable property of health systems
- Brian Goldman, *The Power of Kindness in Medicine* (2018) — on what gets lost when healthcare becomes transactional

These books shape what "success" means for the vision. The 10-year plan is the engineering and business strategy to realize a vision of healthcare whose moral structure is already articulated in works like these.

## Appendix C — Three-generation arc

If the founder is 40 today, the 10-year plan makes them 50 when the vision reaches its peak. The first 30 years of the platform's life overlaps with their remaining career. The next 30 years — when the platform is infrastructure, not a product — is their children's generation's inheritance. The 30 years after that is their grandchildren's. Architectural choices made in 2026 affect what healthcare looks like in 2086.

This is not hyperbole. TCP/IP was designed in 1974 and still runs the internet. Unix was designed in 1969 and still shapes every operating system. Healthcare's Epic was founded in 1979. The substrate built in 2026-2036 will still be shaping healthcare in 2086 if it's built with the humility to recognize that it's building infrastructure, not just a product.

The founder's decision, at its most honest level, is: *do I want to spend a decade building a company, or a decade building infrastructure?* Both are legitimate. They're not the same thing.

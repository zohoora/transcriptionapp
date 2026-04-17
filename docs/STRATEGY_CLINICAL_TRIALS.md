# Clinical Trial Site Platform — Deep Dive

*Expansion on STRATEGY_UNBOUND § 3.4 and § 8 ("clinical trials infrastructure"). Companion to STRATEGY_CLINIC_OS.md — that document is about primary care; this one is about clinical research sites. They share more architecture than anyone expects, and differ in customer, revenue model, and regulatory posture in ways that fundamentally shape the product.*

---

## 0. The one-paragraph case

The clinical-trials world is the single domain where **verifiability is the product**, not a compliance cost. Every other market treats replay, audit, deterministic reasoning, and cryptographic integrity as expensive overhead; clinical trials treat them as purchase criteria. The codebase's two most unusual architectural assets — **replay bundles + deterministic-rules-over-LLM pattern** — are, almost by accident, purpose-built for 21 CFR Part 11 compliance. The FDA finalized new guidance on electronic systems in clinical investigations in October 2024 and is actively developing AI-specific frameworks. Canadian aging/dementia research is a credible first-customer wedge (CIHR-funded, culturally aligned, multi-site). The result is a platform direction where the codebase's weirdness is the selling point — not a bug to overcome. This document explores that direction at sufficient depth to decide whether it's worth pursuing instead of, or after, the primary-care directions.

---

## 1. The opportunity — honestly framed

**Market size** (verified from 2026 industry reports):
- Decentralized Clinical Trials (DCT) market: **$8.48B in 2026, projected $29.69B by 2035** (13.67% CAGR)
- Global clinical trials spending: ~$60B annually
- Site-level tech spending: ~$3-5B annually
- Neurology (cognitive aging included): fastest-growing segment at **16.45% CAGR**

**Why now specifically**:
- FDA October 2024 final guidance (Electronic Systems in Clinical Investigations — 29 Q&As) explicitly includes digital health technologies, AI-generated data, and electronic signatures
- ICH E6(R3) Good Clinical Practice revision (2024) emphasizes risk-based quality management — maps exactly to AMI's layered autonomy model
- EU Clinical Trials Regulation (CTR 536/2014) fully in force since January 2023 — standardizing Europe, opens export market
- Post-COVID DCT momentum has been slower than predicted but structurally real — the "hybrid trial" is now the default design, not the exception
- Pharma R&D productivity crisis: drug approvals cost ~$2.6B (DiMasi et al.); industry is actively funding tech that reduces cost
- AI is being integrated into trial operations *without* adequate regulatory architecture — the gap is real and growing

**Why this is not the obvious scribe-extension play**:
- Customer isn't the physician — it's the sponsor, CRO, or research site operations director
- Revenue model isn't per-physician SaaS — it's per-trial or per-site-per-trial, with deal sizes 10-100× larger
- Sales cycle isn't 3 weeks — it's 12-18 months
- Regulatory posture isn't "we're trying for SOC 2" — it's 21 CFR Part 11 validated and ICH GCP conformant from day one
- The buying decision isn't individual — it's committee-driven with clinical operations, QA/QC, data management, regulatory affairs, IT security, procurement, legal, and principal investigator all weighing in

None of this is necessarily bad. But it's different enough from the primary-care paths that the company shape, founder life, and go-to-market motion are fundamentally distinct. Decide with open eyes.

---

## 2. What a clinical trial actually is (from the site's perspective)

If you've never run a trial, the site workflow is invisible. Rough outline of the year of a typical Phase II/III interventional trial at a clinical research site:

**Trial initiation phase (months 1-6)**
- Feasibility assessment: does the site have the patient population? The equipment? The staff capacity?
- Site selection and contract negotiation (with the CRO or sponsor)
- Site Initiation Visit (SIV): sponsor/CRO visits, trains staff on the specific protocol
- Regulatory document assembly: IRB/REB submission, investigator CVs, FDA Form 1572, delegation logs, informed consent template review
- Technology setup: EDC system access (Medidata Rave, Veeva Vault, Clinical One), eCOA tablets provisioned, randomization system access
- Essential documents filed in the Trial Master File (TMF)

**Active enrollment phase (months 6-24)**
- Patient recruitment: pre-screening via chart review, outreach to eligible patients
- Informed consent process: 30-90 minute conversations, often multiple visits before enrollment
- Screening visit: eligibility confirmation, baseline labs, imaging, PROMs, vitals
- Enrollment / randomization
- Ongoing study visits per protocol (typically every 2-12 weeks depending on study): PROMs, vitals, adverse event assessment, drug dispensing / returns, protocol-specified assessments
- Unscheduled visits: AE follow-up, safety checks
- Source document creation: every data point entered into EDC must trace to a source document (chart note, lab report, physician-signed form)
- Protocol deviation documentation (always happens, always needs to be reported)

**Monitoring + data cleaning phase (ongoing)**
- Clinical Research Associate (CRA) visits from sponsor/CRO every 4-12 weeks
- Source Data Verification (SDV): CRA sits with source documents and EDC, verifies 100% (or risk-based sample) of entered data
- Query resolution: EDC flags data issues, site coordinators respond
- Essential document updates (regulatory amendments, staff changes)

**Close-out phase (months 24-30)**
- Last patient last visit (LPLV)
- Final data entry + query resolution
- Site close-out visit
- Database lock
- Final regulatory documents filed

**What consumes research coordinator time, roughly**:
- ~30% source data creation and chart review
- ~25% data entry into EDC from source documents (double-handling)
- ~15% participant interaction (consent, visits, AE follow-up)
- ~10% regulatory document management (paper and electronic files)
- ~10% query resolution
- ~5% study drug accountability
- ~5% other

Most of the 30% + 25% + 10% + 10% = **80% of research coordinator time is documentation and data-handling work** — not patient interaction, not clinical judgment. This is the wedge.

---

## 3. The five layers applied to clinical trials

Same architectural thinking as the CANS document. The mapping is surprisingly clean.

### Layer 0 — Peripheral sensing (continuous, always-on)
- **Audio** from consent rooms, assessment rooms, infusion areas
- **Presence + motion** from participant homes (for DCT continuous endpoints) and research site rooms
- **EMR telemetry** from source document system (what's being documented, by whom, when)
- **Study drug accountability** — weighing dispensed vs returned, temperature logging for refrigerated drug
- **Wearable + device integration** (participant wearables during trial: CGM, ActiGraph, Apple Watch, study-specific devices)
- **Scheduled-data timestamps** (visit start, visit end, protocol-assessment intervals)
- **External feeds**: central lab results via HL7, imaging results via DICOM, randomization system, eCOA tablets

### Layer 1 — Spinal reflexes (deterministic, instant)
- **Visit window compliance**: participant visit is scheduled day 28±3; reflex fires "out of window" alert if booked outside bounds
- **Protocol required assessments**: each protocol visit has a specified set of assessments; reflex detects "PROM-XYZ not completed, visit cannot close"
- **Informed consent version check**: participant signed v2.1; current protocol is v2.3 — reflex triggers reconsent workflow
- **Study drug temperature**: refrigerated drug detected at >8°C for >15 min → reflex alert + deviation log
- **Randomization interlock**: can't randomize until eligibility fields are all confirmed green
- **Electronic signature**: every signed record gets cryptographic signature with participant-binding + timestamp per 21 CFR 11.50/11.70
- **Source-to-CRF reconciliation**: discrepancy between source audio-extracted data and CRF field triggers query
- **AE severity grading**: specific phrases auto-grade per CTCAE (Common Terminology Criteria for Adverse Events)
- **Concomitant medication check**: new med mentioned during visit → reflex checks against exclusion medication list

### Layer 2 — Brainstem (autonomic orchestration)
- **Informed consent**: captured continuously, structured into "consent conversation" with understanding-check highlights, questions asked, participant answers, cryptographically signed version linking, audit log. Tomorrow's auditor can replay the consent conversation.
- **Pre-visit preparation**: for each upcoming visit, required assessments + supplies + participant briefing assembled automatically. Coordinator walks in with everything staged.
- **Source data generation**: visit captured ambiently → structured source document drafted → coordinator reviews + signs → source document linked to participant, visit, and all CRF entries derived from it
- **CRF auto-population**: from structured source data, CRF fields populated with AI-generated values → each field has confidence score, source reference, and requires human sign-off before submission
- **Protocol deviation detection**: pattern of source data + schedule + assessment completeness matched against protocol specifications → deviations flagged with category (major/minor), documented, routed to appropriate reporting
- **AE detection**: ambient audio + vitals + participant report cross-referenced → potential AEs surfaced for coordinator review → once confirmed, AE reports auto-drafted with severity, relationship, action taken
- **Participant diary collation**: continuous wearable + home sensor data + participant-reported events aggregated per visit, time-aligned with events of interest
- **Study drug accountability**: dispensing, return, and destruction logged with cryptographic chain of custody
- **Query response automation**: common queries (missing units, out-of-range values, date format) answered automatically with reference to source; unusual queries flagged for coordinator

### Layer 3 — Limbic (contextual + emotional awareness)
- **Participant engagement monitoring**: are they responding to check-ins? Opening the participant app? Attending visits? Risk-of-dropout model surfaces before loss.
- **Informed consent comprehension**: tone + question patterns + pause patterns during consent conversation → signal of comprehension issues. Coordinator alerted to revisit specific concepts.
- **Coordinator burnout signal**: pattern of extended hours, context-switching load, error frequency → gentle wellness nudge (self-only)
- **Participant distress during visit**: assessment task becoming too difficult, affect changes → coordinator prompted to check in
- **Site-level capacity awareness**: is the site over-booked? Running hot on specific protocol? Rate-limited on recruitment?

### Layer 4 — Cortex (strategic, slow)
- **Site performance analytics**: enrollment rate vs benchmark, protocol deviation rate vs peers (anonymized), query rate, database-lock readiness
- **Cohort analysis**: participant demographic representation vs target, retention curves, PROM trajectory patterns
- **Trial operational insights**: if we run another trial like this, what would we change?
- **Quality risk indicators**: per ICH E6(R3), continuously surfaced to QA/QC
- **Sponsor-facing dashboards**: sponsor sees their trial's real-time state across all sites running this platform

### Layer 5 — Peripheral motor (outputs, audited actions)
- **EDC writes** (direct integration with Medidata, Veeva, Oracle, REDCap, Castor)
- **eConsent signatures** (DocuSign-style but clinical-trial-specific, 21 CFR Part 11 compliant)
- **Regulatory submissions** (draft IRB amendments, drafted AE reports)
- **Participant communications** (visit reminders, consent updates, study results sharing)
- **Sponsor/CRO updates** (near-real-time status, not monthly PDF reports)

Every write at Layer 5 has cryptographic chain back to source; every AI-generated value has human approval; every signature is legally-binding-grade.

---

## 4. The specific product concept

Based on the architecture + market + codebase, the product shape I'd imagine:

### 4.1 Working name: **Veritas** (or equivalent)

Connotation: truth, verified. Suggests regulatory-grade evidence, not just efficiency.

### 4.2 What it *is*

A **clinical research site platform** that sits between the physical research site (or participant's home for DCTs) and the sponsor's EDC. It captures source data via ambient + multi-modal sensing, generates structured clinical trial records, enforces protocol compliance, and produces 21 CFR Part 11 validated outputs that feed existing EDC systems (Medidata, Veeva, Castor, REDCap).

**Not**:
- An EDC replacement (too entrenched, doesn't need to be displaced)
- A full DCT platform like Medable (different scope, simpler wedge)
- A generic "AI for clinical trials" product (too vague)

**Is**:
- The source data capture + protocol compliance + audit infrastructure layer, underneath existing EDC systems
- Deployed at sites and (for DCTs) at participant homes
- Partners with EDC vendors rather than competing with them

### 4.3 Initial therapeutic area: cognitive aging / dementia trials

Specific reasons:
- **Neurology is the fastest-growing DCT segment** (16.45% CAGR vs 13.67% overall)
- **Continuous ambient home observation is transformative** for capturing functional decline endpoints (activities of daily living, cognition, mood, sleep) — these are currently captured via 30-minute quarterly office assessments that miss 99.99% of the signal
- **Canadian research strength** in cognitive aging: CIHR (Canadian Institutes of Health Research) funding, CCNA (Canadian Consortium on Neurodegeneration in Aging), Baycrest's Rotman Research Institute, Sunnybrook, UBC, McGill's Douglas, University of Toronto
- **Underserved by current DCT vendors**: Medable and Thread focus on oncology and broad cardiovascular; dementia-specific workflows (proxy reporting by caregivers, longer consent processes, cognitive assessment complexity) aren't their specialty
- **Alignment with aging-in-place direction** (STRATEGY_UNBOUND § 3.3): same sensor deployment technology, different customer, different revenue model

Alternative initial therapeutic areas considered:
- Chronic pain (aligns with primary-care wedge) — but trial activity is lower-volume
- Psychiatric (great PROM fit) — but digital therapeutics companies are heavily funded here already
- Cardiovascular (wearables align) — but dominated by incumbents

### 4.4 Customer types (three, in order of acquisition sequence)

1. **Academic research groups** (year 1-2): CCNA sites, Rotman Research Institute, Sunnybrook Dementia Research, Douglas Institute, etc. Sell as research infrastructure. Longer sales cycle than commercial but relationships are dense and reference-rich.
2. **Clinical research sites** (year 2-3): Independent research sites ("SMOs" — Site Management Organizations) and academic medical centers running sponsored trials. Per-site license + per-trial fee.
3. **Pharma sponsors + CROs** (year 3+): Sell the platform at the sponsor level for deployment across all their sites. Enterprise SaaS. Higher ACV, longer cycle.

---

## 5. What the product does that Medable / Thread / Curebase don't

Everyone competing in this space has some version of eConsent, eCOA, patient app, participant engagement, and basic protocol compliance. The differentiation needs to be real, not marketing.

### 5.1 Ambient multi-modal source data capture

Medable/Thread/Curebase ask participants to *enter data* (into an app, into a tablet, into a PROM form). Veritas captures data *ambiently* from visits (audio → structured), sensors (mmWave + wearables → continuous), and environmental signals, with human review/approval gates before data goes to EDC. The participant burden and coordinator burden both drop dramatically.

### 5.2 Replay-verifiable AI output

If AI helped populate a CRF field, Veritas records: the raw source audio, the extraction prompt, the model output, the confidence score, the human approver, the final signed value — and can replay the full chain any time. For any AI-assisted data point. This is an architectural property the competitors don't have; they'd need to rebuild to retrofit.

### 5.3 Deterministic rules over LLM output for regulatory fields

Regulatory-sensitive fields (eligibility, AE severity, protocol deviations) are *never* direct LLM output. They're LLM-extracted features → deterministic rules → regulated output, with each stage independently auditable. This matches FDA's October 2024 guidance explicitly ("controls to review and approve AI-generated data").

### 5.4 Local-first at site + optional cloud for sponsor

Source data stays at the site (or participant's home) by default. Sponsor/CRO access via cryptographic anonymization + controlled API exposure. This satisfies data residency requirements (EU CTR, PIPEDA, forthcoming Canadian health data sovereignty) without the "cloud retrofit" work that every incumbent will need.

### 5.5 Multi-site fleet deployment + auto-update

The auto-deploy pattern already in the codebase (launchd + `~/transcriptionapp-deploy.sh`) generalizes to trial site deployment. Protocol amendments propagate to all sites automatically with cryptographically signed rollout. Site tech footprint is one compute appliance + sensor kit.

### 5.6 Continuous endpoint capture for decentralized participants

For the DCT portion: the same sensor stack used in exam rooms deploys to participant homes. Continuous ADL monitoring, sleep patterns, cognitive task engagement, mood proxy signals. Endpoints that currently require four 30-minute assessments per year become continuous time-series data.

---

## 6. Magic moments

What the experience feels like when it's working.

### 6.1 Informed consent ceremony, thirty minutes

A prospective participant, Mary, arrives for a consent discussion about a Phase III Alzheimer's prevention trial. The coordinator, Jen, walks her through the consent form over 35 minutes. Mary asks questions; Jen answers; Jen checks understanding ("Can you tell me in your own words what you'd be agreeing to?"). Mary's daughter sits in. Everything is captured ambiently.

Afterwards, Jen reviews the Veritas output: structured consent record showing each protocol element discussed, each question Mary asked, each understanding-check response, daughter's presence as witness, timestamps for everything, cryptographic signatures applied. The IRB's "documentation of adequate informed consent" requirement is met with evidence, not paperwork. Mary receives an audio file of her own consent discussion by email for her records. Jen spent 5 minutes reviewing and signing Veritas's drafted record instead of 30 minutes typing into the consent log.

Two years later, during an audit, the FDA inspector reviews Mary's consent. They can listen to the actual consent conversation, see the exact moment each understanding check happened, verify the signatures are cryptographically unchanged since that day. Audit takes 20 minutes instead of 2 hours.

### 6.2 Monday morning, 20 participants in the database

Coordinator Kevin walks in. Veritas shows: 3 participants have upcoming visits this week; 2 participants showed wearable signals suggesting potential AE over the weekend (increased fall frequency — flagged but not yet classified); 1 participant missed her virtual check-in yesterday (engagement risk); query queue has 4 items, 3 of which Veritas can answer automatically with source reference and 1 needs Kevin's judgment. Kevin clicks through 8 decisions in 12 minutes. In the old workflow this would have been 90 minutes of chart review plus query responses drafted manually.

### 6.3 Home visit, virtual assessment

Mary is at home, 6 months into the trial. Her scheduled virtual visit starts on the participant app. She speaks with Jen via video. Jen administers the MoCA (Montreal Cognitive Assessment) remotely. Mary's performance on each subtest is captured: her audio responses, her response times, her task engagement.

In parallel, Veritas has been observing Mary's home for six months: her sleep times, her ADL patterns (how long she's in the kitchen, how often she uses the bathroom, how much she moves), her TV-watching patterns, visitors detected (consented). Jen's dashboard at the visit shows not just "today's MoCA score" but "today's MoCA vs Mary's trajectory over 6 months, vs cohort averages," and flags that her morning activity has decreased 18% compared to month 3 — a potential early decline signal.

Mary would have gotten a single MoCA score per visit in the traditional trial. Veritas captures a continuous signal, with the MoCA as one point among hundreds.

### 6.4 Monitoring visit (CRA), 50% reduction

The sponsor's Clinical Research Associate (CRA) arrives for the scheduled monitoring visit. In the old workflow: 2 days of source data verification, sitting with a coordinator, checking every entered field against source documents.

In the Veritas workflow: the CRA arrives and asks Veritas for the auto-generated monitoring report. Every data point with its source (audio clip, PROM response, wearable reading), each AI-extracted field with its confidence and human approver, every discrepancy flagged and explained, protocol deviation documentation complete. The CRA reviews the edge cases (3% of records), spot-checks 10% of the automated reconciliations, and closes the visit in 4 hours instead of 16. Monitoring costs drop ~60% per visit.

### 6.5 Protocol amendment, site-wide in an afternoon

The sponsor amends the protocol to add a new PROM and change the visit schedule. In the old workflow: coordinators at each of 23 sites spend 2-4 hours updating their TMF, updating the EDC template, retraining on the new schedule, updating source document templates, updating participant-facing materials.

In the Veritas workflow: the sponsor publishes the amendment to the Veritas fleet. All 23 sites receive the cryptographically signed amendment within the hour. Site-specific document templates regenerate, the new PROM appears in participant apps, the revised visit schedule propagates to the scheduling system. Coordinators review the change log summary (~15 minutes each), acknowledge, and continue. Amendment implementation goes from 2-4 weeks to 1 day.

### 6.6 End of trial, database lock

The last patient completes the last visit. In the old workflow: 3-6 months of data cleaning, query resolution, source data verification completion, final statistical review, then database lock.

In the Veritas workflow: the final-visit structured data flows to EDC same-day. The reconciliation engine's done its work continuously; ~0.5% of records need final human attention instead of ~15%. Database lock happens in 4 weeks instead of 5 months. The sponsor's time-to-BLA (Biologics License Application) drops correspondingly.

These aren't science fiction. They're implementable on the current architecture. The gap is specialty domain knowledge (cognitive aging assessment protocols, pharmacoeconomics, CTMS integration standards) — not core capability.

---

## 7. User experiences

### 7.1 The research coordinator's experience

"I used to spend 80% of my time moving data around. Now I spend that time with participants."

- Start of day: dashboard with upcoming visits, quality concerns, engagement risks
- Participant visit: ambient capture, AI-drafted source data, coordinator reviews and signs
- Query queue: 90% auto-answered, coordinator handles exceptions
- Regulatory: TMF is always current because updates flow automatically
- End of day: source data is complete; CRFs are populated; nothing waiting for tomorrow

### 7.2 The principal investigator (PI)'s experience

"I can actually be the clinical lead again, not the data-administrator."

- Each visit briefing shows trajectory, cohort comparison, potential deviations
- AEs surface in real-time, not at month-end
- Protocol compliance is continuous, not quarterly
- Sponsor queries become rare events, not daily interruptions
- Publications: the trial's data is query-ready the day of LPLV

### 7.3 The sponsor's experience

"I see my trial in real-time for the first time."

- Cross-site dashboard showing every active trial, every site's enrollment status, every concerning signal
- Protocol deviation review happens weekly with fresh data, not monthly after manual aggregation
- SDV (source data verification) costs drop 60-70% because the reconciliation engine is continuous and spot-check–validated
- AE reporting timelines met by the system, not by coordinator heroics
- Time-to-database-lock is weeks, not months; time-to-submission compresses accordingly

### 7.4 The CRO's experience

"Our monitoring unit becomes risk-based monitoring in practice, not just in slides."

- ICH E6(R3)'s risk-based quality management is operationalized, not just documented
- CRAs handle exceptions and relationship-building, not line-by-line SDV
- Site support becomes proactive rather than reactive
- Quality management is continuous, aligned with the updated GCP standard

### 7.5 The participant's experience

"I'm in a trial but it doesn't feel like a job."

- Informed consent is a real conversation, recorded for my reference
- Home sensors are unobtrusive; I forget they're there after a week
- My wearable sends its data automatically; I don't log anything manually
- Virtual visits are respectful of my time
- I get a plain-language update after each visit with what was discussed and what happens next
- At trial end, I get a plain-language summary of what the trial found (mandated by EU CTR for all trials submitted post-2023)

### 7.6 The regulator's experience

"I can actually verify what happened, years after the fact."

- Every data point traces to source with cryptographic integrity
- AI-assisted fields are clearly labeled with human approver
- Protocol deviations are time-stamped from detection, not backdated
- Consent ceremonies are replay-able
- The validation package demonstrates compliance with 21 CFR Part 11, ICH E6(R3), and applicable regional regulations

---

## 8. What in the codebase already points here

Codebase-to-trials mapping. Everything that's a "nice-to-have" in primary care becomes "purchase-critical" in clinical trials.

| AMI Assist architectural property | Clinical trial requirement |
|------------------------------------|-----------------------------|
| Replay bundle schema v3, immutable on write | 21 CFR Part 11 electronic record integrity |
| Pipeline log with cryptographic timestamps | Part 11 audit trail |
| Deterministic rules engine over LLM output | FDA Oct 2024 guidance on AI-assisted data — human-approval-required pattern |
| Per-session version-locked config (replay_snapshot) | Protocol version binding for every captured record |
| Schema-versioned bundle format | TMF and EDC backward-compatibility |
| Multi-site fleet with signed auto-update | Multi-site trial deployment with protocol amendment propagation |
| Profile service (multi-user) with role-based patterns | Investigator/coordinator/monitor/sponsor role segmentation |
| Local-first storage with sync | Data residency (EU CTR, PIPEDA) + site data ownership |
| Sensor fusion (mmWave + thermal + CO2) + firmware | Decentralized home-based continuous endpoint capture |
| STT streaming + speaker diarization | Consent ceremony capture with participant/witness roles |
| Vision extraction with confidence tiers | Participant ID verification during remote visits |
| Longitudinal patient memory (extension) | Longitudinal participant tracking |
| Confidence-tiered policy for LLM output | Regulatory-grade human-in-the-loop pattern |
| Performance summary (post-hoc aggregation) | Monitoring report auto-generation |
| Merge/split encounter detection | Visit-vs-visit boundary detection, unscheduled visit detection |

Things the codebase **doesn't yet have** that are needed for clinical trials specifically:

- **Electronic signatures per 21 CFR 11.50/11.70 and 11.100-200**: cryptographic signature workflow, signature-record linking, reason-for-signature capture
- **EDC integration**: connectors for Medidata Rave, Veeva Vault CDMS, Oracle Clinical One, Castor, REDCap
- **CTMS integration**: Clinical Trial Management System connectors (Medidata CTMS, Veeva Vault CTMS, Advarra CTMS)
- **Protocol representation**: formal protocol specification that can be computationally enforced
- **Randomization integration**: IRT (Interactive Response Technology) vendor connectors
- **Central lab integration**: HL7 v2 + FHIR lab results ingestion
- **Validation documentation package**: IQ/OQ/PQ (Installation/Operational/Performance Qualification) templates, validation plans, traceability matrices
- **AE coding dictionaries**: MedDRA, CTCAE, WHO Drug Dictionary integrations
- **eConsent workflow**: specific consent-form rendering + electronic signature + version-tracking
- **Trial Master File (TMF) generation**: automated regulatory document production
- **Risk-based monitoring analytics**: ICH E6(R3) quality tolerance limits, key risk indicators
- **PROM instrument library**: validated PROs (HAM-D, MADRS, MoCA, MMSE, ADAS-Cog, CDR, etc.)

That's maybe 18-24 months of focused engineering on top of the current codebase — less than writing it from scratch by a factor of 3-5x.

---

## 9. Business model + unit economics

### 9.1 Revenue streams

**Per-trial license** (primary): $100K-$500K per trial depending on size, duration, complexity.
- Small trial (1 site, 50 participants, 1-year duration): $100K
- Mid trial (5-20 sites, 200-500 participants, 2-3 years): $250-500K
- Large trial (20+ sites, 1000+ participants, 3+ years): $500K-$2M

**Per-participant fee** (for DCT-heavy trials with home monitoring): $200-500/participant/month
- Reflects the additional sensor deployment + home compute + wearable integration costs

**Site annual subscription** (for research sites running multiple trials): $50-150K/site/year
- Site pays once per year, uses the platform across all their active trials

**Data services** (optional): custom integrations, bespoke analytics, validation documentation packages: $25-100K project fees

### 9.2 Unit economics example — a reference Phase III trial

Alzheimer's prevention trial:
- 15 sites
- 600 participants
- 3-year duration with monthly home monitoring + quarterly clinic visits
- $400K platform license + $300/participant/month DCT component × 600 × 36 = $6.48M
- Total trial revenue: ~$6.9M over 3 years, ~$2.3M annualized

For comparison, a Phase III Alzheimer's trial typically costs ~$200-500M total. Platform cost is <3% of total trial budget — well within reason.

### 9.3 Gross margin and COGS

- SaaS margins: 75-85% typical at scale
- Sensor hardware: low margin (~20-30%), treated as pass-through
- Professional services (validation, integration): 40-60% margin
- Overall blended: 60-70% gross margin at scale

### 9.4 5-year revenue trajectory (projected)

- **Year 1**: 1-2 academic pilots (CCNA network). ~$200K-400K revenue. Proof-of-concept.
- **Year 2**: 3-5 academic/community sites running 2-4 trials. ~$1.5M-2.5M.
- **Year 3**: First pharma sponsor deployment (e.g., Biogen, Roche, Eisai for AD). ~$5-8M.
- **Year 4**: 5-10 active trials across 2-3 therapeutic areas. ~$15-25M.
- **Year 5**: Established platform with 20+ trials, extend to second country. ~$40-60M.

Compare to DCT category leaders (Medable raised $300M+, projected ~$100M+ revenue; Thread, Curebase similar magnitude). A $50M ARR position in year 5 would be a meaningful company in this space.

### 9.5 Capital requirements

Higher than primary-care wedge, lower than the STRATEGY_2031 regulatory-moat plan because direct clinical trials (Phase III drug) isn't being funded here.

- Year 1: $1-2M seed (team of 4-6, 1 academic partnership)
- Year 2: $5-8M Series A (team of 10-15, validation package, 3-5 sites)
- Year 3: $15-25M Series B (team of 25-40, first pharma deployment, scale)
- Year 4-5: Growth capital or strategic — $30-60M (depending on expansion speed)
- Total through year 5: ~$50-90M

This is more capital than the 2026 primary-care plan but less than the STRATEGY_2031 regulatory moat ($25-40M) because there's no drug development, no Class II clearance pursuit, no 5-year delayed revenue.

---

## 10. Competitive landscape — honest read

### 10.1 Direct competitors (DCT platform)

- **Medable** (~$300M raised, private): Agentic AI platform for clinical trials. Recent pivot to AI-focused. Full-service DCT. Broad therapeutic area coverage. **Weakness**: cloud-first; not source-data-capture-oriented; generic AI claims without regulatory architecture.
- **Thread** (acquired by ICON): eCOA + consulting. Integrated with ICON's CRO. **Weakness**: mature but not AI-native; positioned for services pull-through.
- **Curebase** ($40M+ raised): eClinical platform with EDC + ePRO + eCOA + eConsent. More of a full EDC replacement play. **Weakness**: trying to replace EDCs; crowded positioning.
- **Science37** (public, struggling): Virtual trials pioneer; stock has been rough. **Weakness**: too much "platform" ambition, not enough specialty depth.
- **Lightship / ObvioHealth / Huma / Verana / Evidation**: Various DCT angles, different funding levels. None have AMI's specific replay/deterministic-rules/local-first architecture.

### 10.2 Site operations tools (overlapping but different)

- **Florence Healthcare** ($80M+ raised): eISF (electronic investigator site file) + eRegulatory + eSource. Strong in site-document-management. Not AI-native; not ambient capture.
- **Advarra / Complion**: site regulatory document management, IRB. Focused on compliance not data capture.
- **SiteVault (Veeva)**: Veeva's site-level product. Part of Veeva ecosystem.

### 10.3 EDC incumbents (partners, not competitors)

- **Medidata** (Dassault, $5B+ valuation in 2019): 90% of top pharma. Entrenched.
- **Veeva Vault**: Growing fast. Cloud-native.
- **Oracle Clinical One**: Legacy + new platform.
- **Castor, REDCap**: Academic / investigator-initiated.

**Strategic positioning**: don't try to replace Medidata/Veeva. Integrate with them. Position Veritas as "the AI-assisted source data capture layer underneath your EDC" — makes existing EDC investments more valuable, not obsolete.

### 10.4 Where Veritas wins

- **Replay-verifiable AI architecture**: no competitor has this; years of work to replicate
- **Local-first + data residency**: competitors are all cloud-first; EU CTR + Canadian data sovereignty matter more over time, not less
- **Multi-modal sensing with custom hardware**: competitors have software for wearable ingestion; none have the sensor-firmware-to-LLM stack
- **Specialty depth** (cognitive aging): deep outcome measurement, caregiver integration, ADL sensing — generic DCT platforms are shallow here
- **Post-Oct-2024 FDA guidance alignment**: designed for the specific AI-in-clinical-trials framework FDA published; competitors retrofit

### 10.5 Where Veritas loses

- **Enterprise relationships**: Medidata has 20-year sponsor relationships. Breaking into a pharma's tech vendor list is slow.
- **Validation history**: every CRO wants "how many trials has this system been used for, how many audits has it passed." First trial is the hardest.
- **Scale infrastructure**: cloud competitors scale easier than local-first; this is a trade-off not a weakness.
- **Team size**: competitors have 100-300 employees; starting at 4-6 is a multi-year disadvantage to overcome.
- **Capital**: competitors have raised $50-300M; matching that in Canada is harder than in US.

### 10.6 Acquisition endgame

Realistic 5-7 year acquirers:
- **Medidata (Dassault)**: acquire for AI + source capture layer they lack
- **Veeva**: acquire for the decentralized + specialty depth Veeva Vault doesn't cover
- **IQVIA**: strategic tech acquisition for their site-services business
- **Oracle**: plausible but slower decision-making
- **A pharma giant directly** (Roche, Novartis, Biogen): less likely but happens; they want to own AI infrastructure
- **IPO**: possible if revenue scales past $100M ARR with strong margins; public markets for clinical trial tech have been rough (Science37, etc.)

---

## 11. Regulatory architecture detail

This is the part where most startups hand-wave. Since it's the point of the business, I'll be specific.

### 11.1 21 CFR Part 11 — electronic records and electronic signatures (US)

Each sub-part, how Veritas satisfies it:

- **§11.10(a) Validation of systems to ensure accuracy, reliability, consistent intended performance**: replay testing framework + regression CLIs + validation test suite. 2-3 months of work to formalize what's already structurally present.
- **§11.10(b) Ability to generate accurate and complete copies of records**: archive + replay bundles are already this.
- **§11.10(c) Protection of records**: local-first + cryptographic integrity + controlled access + atomic writes. Structurally present.
- **§11.10(d) Limiting system access to authorized individuals**: profile service + role-based auth. Needs formalization for regulatory-grade.
- **§11.10(e) Use of secure, computer-generated, time-stamped audit trails**: pipeline_log is this pattern.
- **§11.10(f) Use of operational system checks**: Layer 1 reflexes.
- **§11.10(g) Authority checks**: role-based access + signature authority.
- **§11.10(h) Device checks**: sensor validation. Needs to be made explicit.
- **§11.10(i) Education, training**: standard ops doc (no code).
- **§11.10(j) Accountability**: audit trail + signature attribution. Present.
- **§11.30 Open system controls**: encryption + authentication.
- **§11.50 Signature manifestations**: NOT CURRENTLY BUILT. Signature record must include name, date/time, meaning of signature.
- **§11.70 Signature/record linking**: NOT CURRENTLY BUILT. Signatures must be cryptographically linked to records.
- **§11.100-11.200 Electronic signatures**: NOT CURRENTLY BUILT. Identity verification + signature process + signature-protection.

**Assessment**: ~70% of Part 11 requirements are structurally met by the existing codebase. ~30% (electronic signatures specifically) need to be built. Total work to reach validated status: 6-12 months of focused engineering + validation consulting + documentation.

### 11.2 ICH E6(R3) — Good Clinical Practice

The updated (2024) GCP standard emphasizes **risk-based quality management** — perfectly aligned with AMI's layered autonomy model. Layer 1 reflexes = risk-based monitoring triggers. Layer 2-3 autonomic + contextual = continuous quality assessment. Layer 4 cortex = risk-review reporting.

Veritas can be designed to operationalize ICH E6(R3), not just comply with it — which is a competitive advantage as sponsors and regulators increasingly prefer tools that demonstrate the principle, not just the letter.

### 11.3 Health Canada Division 5 (Drugs for Clinical Trials)

Similar to FDA requirements; Health Canada explicitly references ICH GCP; CTA (Clinical Trial Application) process similar in principle to IND. Canadian-first operation aligns well with HC Division 5. Expansion to US requires additional 21 CFR Part 11 validation + IND-compatible exports.

### 11.4 EU Clinical Trials Regulation (CTR 536/2014)

Fully in effect since January 2023. Centralized submission via CTIS (Clinical Trials Information System), mandatory lay summary at end of trial (aligns with Veritas's participant-result-sharing capability), data transparency provisions (Veritas's audit trail supports this). Europe is a natural second market; architecture is already compatible.

### 11.5 PIPEDA + forthcoming Canadian health data sovereignty

Local-first architecture aligns structurally. No data crosses border by default. Provincial health information legislation (Ontario PHIPA, BC PIPA, Alberta HIA) adds additional considerations but the local-first default handles most of them.

### 11.6 The regulatory package as a business asset

After 12-18 months of investment in validation documentation + actual audits on first trials, Veritas has a regulatory credential that:
- Takes every competitor 12-18 months to match from a standing start
- Is referenced by every future sponsor ("Veritas has been through X audits, Y FDA inspections, Z successful submissions")
- Is difficult to retrofit into a cloud-first or non-replay architecture
- Becomes the actual moat, not just table stakes

This is why clinical trials is the *only* direction where AMI's weird architecture is a selling point rather than overhead.

---

## 12. Five creative prompts worth weekend consideration

### 12.1 What if the platform was structured as a regulated data fiduciary?

Veritas doesn't just capture data for sponsors. It holds participant data as a fiduciary — participants opt in, retain access, can port to future trials. Creates a longitudinal research-participant graph that transcends any single trial. A participant who does an AD prevention trial at 65 + an arthritis trial at 68 + a cardiovascular trial at 72 has one Veritas identity, one consent management interface, one data rights portal. This is a very different company — but might be the real vision.

### 12.2 What if participant sensing data were treated as primary (not supplementary) endpoint?

Most trials use continuous sensing as supplementary data. A different framing: continuous home monitoring *is* the primary endpoint (e.g., "daily step count trajectory over 2 years" vs "6-minute walk test at 4 time points"). FDA is increasingly open to real-world digital endpoints. Veritas positions to enable this shift. Would require sponsor + regulatory champion-level alignment; transformative if it works.

### 12.3 What if the tool for site coordinators were voice-first?

Coordinator workflow today is keyboard-heavy EDC entry. A voice-first interface — coordinator narrates the visit, Veritas structures it in real-time, reviews at end — might feel 10× more humane and enable coordinators to be with participants instead of behind screens. Low-level feature, high-level workflow implication.

### 12.4 What if we made the consent process *better*, not just documented?

Informed consent is famously poorly understood by participants (multiple studies show ~30% comprehension). Veritas could be a consent-*improvement* tool, not just a capture tool — adaptive comprehension checks, plain-language alternatives, questions personalized to the individual. Could partner with bioethicists on evidence-based consent. Consent becomes a differentiator, not just a compliance artifact.

### 12.5 What if Veritas ran the site's entire regulatory operation, not just data?

Site coordinators spend 10-15% of time on TMF maintenance, CV updates, training log, delegation log, investigator brochure distribution. Expand Veritas to be the site's regulatory operations layer, not just data capture. Makes the case for annual site subscription rather than per-trial fees — higher ACV, stickier deployment.

---

## 13. Honest caveats

Things that could kill this direction:

### 13.1 Procurement velocity in pharma

Pharma procurement cycles are 18-24 months. CROs are oligopolistic. Entry is slow. First real commercial trial is likely 3 years from product launch. Runway needs to accommodate.

### 13.2 EDC integration politics

Medidata, Veeva, Oracle each have their own preferences. Partner-versus-compete dynamics are delicate. If the big three collectively decide Veritas is a threat, integration becomes hard; if they embrace, the business becomes easier. Founder needs to read this correctly.

### 13.3 Specialty domain knowledge

Cognitive aging assessment (MoCA, MMSE, ADAS-Cog, CDR, FAQ, NPI-Q, etc.) is not trivial. PROMs for this space, AE coding, protocol conventions — requires clinical research co-founder or deep adviser. Building specialty expertise from scratch adds 12-18 months.

### 13.4 Validation costs

21 CFR Part 11 validation is not cheap. Plan $500K-1M for specialized regulatory consulting + internal engineering time over 12-18 months. This is capital that doesn't produce features — purely compliance infrastructure.

### 13.5 Decentralized trials have underperformed hype

The 2021 DCT boom produced some burned pharma sponsors. The category is real and growing, but more cautiously than early projections. Don't assume the hockey-stick growth — plan for steadier adoption.

### 13.6 Canadian first-customer limits

Canadian clinical research market is ~10% the size of US. First-customer wedge is defensible; growth strategy must include US/UK/EU by year 3 to reach escape velocity.

### 13.7 Academic sites move slowly

IRB approvals, budget cycles, grant funding timelines. Academic partners are the right starting customer but not the fastest revenue customer. Plan for 6-12 month per-site deployment cycles.

### 13.8 Home sensor deployment is operationally heavy

Installing sensors in 600 participants' homes across 15 sites over a 3-year trial is a real operational undertaking. Requires either partner logistics (a field services partner) or in-house field operations team. Not pure software.

### 13.9 The participant side is undervalued

A lot of DCT startups have been sponsor-centric and neglected participant experience, causing dropout. Veritas must be genuinely excellent on the participant side or risk the same failure mode.

### 13.10 Competition from unexpected directions

A generic LLM platform (OpenAI, Anthropic) may release "AI for clinical trials" capability. An EMR (Oscar, Epic) may extend into research. A wearable company (Apple, Garmin) may enter. Category lines are blurring.

---

## 14. Decision framework

### 14.1 Three reasons to pursue this direction

1. **Verifiability is the product.** Nowhere else does AMI's weird architecture become a buying criterion instead of overhead.
2. **The codebase maps 70% of the requirements.** Remaining 30% is ~18 months of focused work, not a rebuild.
3. **The market is growing 13-16% CAGR, underserved by modern infrastructure, and explicitly open to AI-native entrants per recent FDA guidance.**

### 14.2 Three reasons to NOT pursue this direction

1. **Customer is not the founder's network.** Primary care physicians and research site operations directors are different humans; relationship-building starts from ~zero.
2. **Capital needs are higher than primary-care paths.** $50-90M through year 5 vs $300K-40M depending on which primary-care plan.
3. **First commercial revenue is 24-36 months out.** Primary-care paths hit revenue in 6-12 months.

### 14.3 The framing question

**"Do I want to build a regulatory-grade, enterprise-sales, team-of-30+ clinical research infrastructure company, or do I want to build a humane, physician-focused, team-of-5-10 Canadian primary-care product?"**

These are different companies. Same origin codebase, same architectural roots, very different founder lives.

Neither is better. The 2031 plan tries to bridge the two (use primary care as platform-validation, then expand to regulated territory); this plan picks the regulated territory as the target directly.

### 14.4 What would make this decision

Test over 6-8 weeks before committing:

1. **2-3 informational conversations with Canadian DCT/clinical research operations leaders** (CCNA, Baycrest, Rotman, Sunnybrook Research Institute). What do they actually need? Is the platform thesis resonant?
2. **1 conversation with a pharma R&D operations executive at AD-focused sponsor** (Biogen, Roche, Eisai Canada, Novo Nordisk). Would they pilot a Canadian-first validated platform?
3. **1 conversation with a regulatory consultant experienced in 21 CFR Part 11 validation for clinical trial software**. Realistic cost + timeline for validation package?
4. **1 conversation with a CRO operations leader** (IQVIA Canada, Parexel Canada, ICON). Partner-or-competitor dynamics?
5. **1 conversation with an academic who's run dementia prevention trials recently**. What did they wish they had?

If 3+ of 5 come back positive, the thesis is live. If 1-2, pivot or revert. Total cost of this validation: ~$5-10K including travel + consulting fees + ~80 founder hours.

---

## 15. Relation to the other strategy documents

Five strategy documents now exist in `docs/`. They span different horizons, different framings, and different commitment levels:

| Document | Horizon | Frame | Customer | Capital |
|----------|---------|-------|----------|---------|
| STRATEGY_2026 | 12 months | Scribe is the product | Individual physician | $300-500K |
| STRATEGY_2031 | 5 years | Scribe is the product | Clinic + validated specialty | $25-40M |
| STRATEGY_UNBOUND | N/A | Enumeration of platform directions | Varies | Varies |
| STRATEGY_CLINIC_OS | 3-5 years | Scribe is a reflex | Clinic owner / healthcare systems | $30-60M |
| **STRATEGY_CLINICAL_TRIALS (this)** | **3-5 years** | **Verifiability is the product** | **Sites → CROs → pharma sponsors** | **$50-90M** |

They're not mutually exclusive at the architectural level — the same underlying platform could be configured for primary care, clinic operations, or clinical trials. At the *company* level they are mutually exclusive — a 10-person team cannot credibly pursue three go-to-market motions simultaneously.

The choice among them is the founder's to make. This document exists so that the clinical trials direction is imagined at depth equal to the primary-care ones, not left as a bullet in an enumeration.

---

## Appendix A — What 2026 industry data supports

### Market size
- DCT market: $8.48B (2026) → $29.69B (2035), 13.67% CAGR
- Neurology DCT growing fastest at 16.45% CAGR — aligns with cognitive aging starting wedge
- Oncology DCT largest segment (46.34%) but crowded with specialists
- North America 48.65% of DCT market

### Regulatory momentum
- FDA October 2024 final guidance on electronic systems in clinical investigations — 29 Q&As covering AI, digital health technologies, e-signatures
- ICH E6(R3) Good Clinical Practice finalized 2024 — risk-based quality management
- EU CTR 536/2014 fully in effect since January 2023

### Competitive landscape
- Medable (~$300M raised): broad DCT platform, recent AI positioning
- Thread (acquired by ICON): eCOA + consulting
- Curebase ($40M+): integrated eClinical platform
- Florence Healthcare ($80M+): site operations and eRegulatory

## Appendix B — Sources

- [Decentralized Clinical Trials Market Report 2026 ($8.48B → $29.69B by 2035, 13.67% CAGR)](https://www.globenewswire.com/news-release/2026/03/19/3259193/0/en/Decentralized-Clinical-Trials-Market-to-Reach-18-8-Billion-by-2030-as-Digital-Health-Infrastructure-Reshapes-Drug-Development.html)
- [Mordor Intelligence — Decentralized Clinical Trials Market](https://www.mordorintelligence.com/industry-reports/decentralized-clinical-trials-market)
- [FDA October 2024 Finalized Guidance on Electronic Systems in Clinical Investigations — Cooley analysis](https://www.cooley.com/news/insight/2024/2024-10-24-fda-finalizes-guidance-on-use-of-part-11-electronic-systems-records-and-signatures-in-clinical-investigations)
- [Navigating the New 21 CFR 11 Guidelines — Applied Clinical Trials](https://www.appliedclinicaltrialsonline.com/view/navigating-new-21-cfr-11-guidelines)
- [21 CFR Part 11 — AI and GxP Compliance — IntuitionLabs](https://intuitionlabs.ai/articles/21-cfr-part-11-electronic-records-signatures-ai-gxp-compliance)
- [Medable Clinical Trial Platform (Agentic AI, eCOA, DCT)](https://www.medable.com/)
- [Thread Research (acquired by ICON)](https://www.threadresearch.com/)
- [Curebase eClinical Platform](https://www.curebase.com/)
- [Florence Healthcare — eISF Site Operations](https://florencehc.com/)
- [Medable Knowledge Center — Future of DCT](https://www.medable.com/knowledge-center/blog-the-future-of-decentralized-clinical-trials-opportunities-and-adaptations-for-medable)

# Unbounded Strategic Directions — If Scribe Is Just One Feature

*Companion to STRATEGY_2026.md and STRATEGY_2031.md. Both of those are "AMI Assist as a scribe business" — they differ only on horizon. This document is what happens when we refuse to accept that framing at all and look at the codebase + hardware as a capability set, not a product.*

---

## 1. What this codebase actually is (stripped of the "scribe" label)

If a stranger looked at this repository without knowing it was a medical scribe, they'd see a **local-first multi-modal ambient intelligence platform**. The capabilities, enumerated honestly:

**Sensing**
- Real-time audio capture with VAD, streaming STT, speaker diarization
- Screen-content understanding via vision LLM
- Presence detection via custom hardware (mmWave + thermal + CO2 on ESP32)
- Continuous operation with sleep cycles and session boundaries

**Reasoning**
- LLM orchestration with retry, timeout, circuit-breaker-ready structure
- Deterministic rules engines layered over non-deterministic ML output (the billing pattern is the canonical example — it's not a billing feature, it's an *architecture*)
- Decision replay and reproduction (the replay bundle schema is the canonical example — it's not a testing feature, it's a *verification substrate*)
- Multi-stage detection with confidence-tiered early termination
- Human-in-the-loop correction + feedback capture

**Coordination**
- Local-first state with server reconciliation
- Multi-machine deployment with URL failover
- Fleet-wide auto-update via signed manifests
- Sensor firmware that talks WiFi or USB-serial to a local compute node
- Per-user identity + config fan-out (the profile-service pattern)

**Persistence**
- Structured session archive with metadata, audio, transcripts, decisions
- Schema-versioned decision records with backward-compatible upgrades
- Audit trails cryptographically independent of the business logic
- Multi-tier logging (per-segment, per-encounter, per-day)

**The medical scribe uses maybe 30% of this.** The remaining 70% is dead weight for a scribe, but it's a functioning ambient-intelligence platform that happens to have a scribe as its first customer. The question the user is asking — correctly — is whether there's a different customer this platform serves better.

---

## 2. What kind of machine is this, actually

Three framings that keep suggesting themselves when I stop assuming "medical":

**Framing A — "Professional Space Intelligence"**: a system that observes a defined physical space where a specialist does a specific kind of work with other humans, and converts what happens there into structured, auditable records + actions. Medicine is one such space. Law, therapy, education, research interviewing, skilled trades, eldercare, coaching, and a dozen others are others.

**Framing B — "Regulated AI Runtime"**: a substrate for building AI products that have to be *defensible under scrutiny*. Not "ambient AI"; the *infrastructure for AI that needs replay, audit, drift monitoring, deterministic reasoning fences around LLMs*. Medical is one such domain. Financial services compliance, legal evidence, safety-critical industrial systems, insurance underwriting, government decisions are others.

**Framing C — "Continuous-Observation Platform"**: a system that watches over time — not moments, not visits, not sessions, but *continuous multi-year observation of people, spaces, or processes* — and builds longitudinal models from that observation. Medical monitoring is one such case. Aging-in-place, chronic disease management, clinical trials, behavioral research, workplace safety, animal welfare, equipment condition monitoring are others.

The current codebase expresses pieces of all three. The "scribe" product expresses none of them fully. The strategic question under this unbounded framing is: **which of these three framings does the founder want to commit to, because each one points at different markets, different customers, and different 5-year endgames.**

---

## 3. Non-medical verticals this platform could become

Ordered by how well the existing codebase assets compound — not by my guess at market size. Each of these is a thought experiment, not a recommendation.

### 3.1 Legal — depositions, client intake, case intelligence

**Customer**: law firms (especially litigation, immigration, family, personal injury).

**What the platform does**: captures multi-party sessions (depos, client meetings), structures them into transcripts + extracted facts + objection flags + exhibit references, tracks billable time automatically, preserves chain-of-custody for evidence.

**Why the codebase fits**: multi-speaker audio + conversational segmentation, billing code pattern → timekeeping code pattern, replay bundles → evidentiary integrity records, local-first → client confidentiality, deterministic rules over LLM → admissibility heuristics.

**Economic case**: legal billing rates are 5-20× higher than primary care billing rates. A scribe-equivalent for a partner billing $600/hr is worth 10× what a physician scribe is worth. Market is less regulated than medicine — moves faster. Fewer regulatory moats, though.

**What would be new**: chain-of-custody cryptographic signing; local dictionaries of legal terminology and jurisdiction-specific rules; exhibit tracking; court-case context (cases, parties, counsel); evidentiary export formats.

### 3.2 Psychology / psychotherapy — measurement-based care platform

**Customer**: psychologists, therapists, psychiatrists, counsellors.

**What the platform does**: captures sessions (with nuanced consent), tracks validated-instrument scores over time (PHQ-9, GAD-7, Y-BOCS, PCL-5), flags treatment-response patterns, surfaces critical moments in therapy for supervision.

**Why the codebase fits**: the entire longitudinal-patient-memory thread from STRATEGY_2031 applies with almost no modification. Affect + voice biomarkers (currently vestigial) become core. Multi-patient detection maps onto couples/family therapy. Clinical chat becomes therapist-side AI assist. Sleep mode + sessions map to clinic-hours operation.

**Economic case**: measurement-based care is a slow-moving but real trend. Payers in US + Canada are starting to require MBC for reimbursement. Single-practitioner clinics will pay $500/mo for the right product. ~100K English-speaking therapists in North America.

**What would be new**: validated-instrument scoring; mood/affect trajectory visualization; treatment-protocol adherence tracking (CBT / DBT / IFS / ACT protocol fidelity); supervisor review workflows; patient-facing check-ins between sessions.

**Regulatory**: lighter than medicine in North America. FDA is starting to notice (digital therapeutics clearances exist) but it's not the same bar.

### 3.3 Aging-in-place — home intelligence for independent elders

**Customer**: adult children of aging parents; senior living providers; Medicare Advantage plans; provincial home care programs.

**What the platform does**: deploys ambient sensors + local compute into an elder's home. Continuous (but privacy-respectful) monitoring of activity, sleep, bathroom visits, social interaction, mood indicators, medication adherence, fall risk. AI detects early decline. Coordinates with family + physician.

**Why the codebase fits**: this is where the hardware investment actually pays off most. The sensor package (mmWave + thermal + CO2) is *purpose-built* for unobtrusive ambient observation. Local-first matters critically (elder privacy). Multi-room → whole-home deployment. Longitudinal memory → decline-curve modeling. Sleep mode → correctly respect off-hours.

**Economic case**: aging is the largest demographic shift of this century. US: 65+ population projected to be 85M by 2050. Canada is similar. "Silver tsunami" market estimated $500B+ by 2030. CMS now reimburses remote patient monitoring. Home Care Ontario funds RPM pilots.

**What would be new**: sensor fleet for home deployment (adhesive mounts, door sensors, bed sensors, bathroom monitors) — requires hardware product development; cognitive decline biomarkers (vocal, temporal, activity-pattern); family-facing app; fall detection + emergency protocols; integration with HealthLink BC / Ontario TeleHomeCare / US RPM billing codes.

**Risks**: very different go-to-market (consumer + caregiver, not professional); trust-building takes years; false-alarm fatigue kills deployments; sensor cost + installation friction matters more than in a clinic.

### 3.4 Clinical trials — decentralized trial infrastructure

**Customer**: pharma sponsors, CROs, academic research centers running Phase II-IV trials.

**What the platform does**: deploys into trial participants' homes or at decentralized clinical trial sites. Captures study-visit audio + structured data + adverse event capture + PROMs + real-world continuous data. 21 CFR Part 11 compliant records. Multi-site fleet coordination.

**Why the codebase fits**: replay bundles → 21 CFR Part 11-compliant records are almost identical architectures. Multi-site fleet is already in the DNA. Local-first + audit trail + drift monitoring are exactly what trial operations auditors want. Sensors → continuous real-world endpoint capture, which is the hottest trend in trials.

**Economic case**: $52B clinical trial market growing 7% CAGR. Decentralized trials specifically are growing 25% CAGR. CROs (IQVIA, Parexel, Syneos, Medpace) spend $100s of millions on trial infrastructure. Per-trial SaaS revenue is $50K-$500K depending on scope.

**What would be new**: 21 CFR Part 11 validation documentation; IRB workflows; informed consent capture; protocol-specific visit templating; PROM instrument libraries; EDC (electronic data capture) integration with systems like Medidata Rave.

**Risks**: trial operations are enterprise-sales heavy; long deployment cycles; competitive landscape includes entrenched players (Medable, Thread, Curebase). Regulatory validation for GxP-cleared systems is itself an 18-month project.

### 3.5 Veterinary — direct port, smaller market

**Customer**: veterinarians, vet tech clinics, animal hospitals.

**What the platform does**: exam-room scribe + billing + longitudinal animal record + owner communication handouts. Scribenote already exists in this space.

**Why the codebase fits**: closest near-medical analogue; most of the existing product would just work with terminology changes.

**Economic case**: ~75K veterinarians in North America; high tech adoption; per-vet revenue comparable to physicians. Smaller than human medicine by ~5-10×.

**Risks**: Scribenote is the incumbent; competition on the same product shape, no differentiated wedge.

### 3.6 Research ethnography + qualitative research

**Customer**: university researchers, market research firms, UX research teams.

**What the platform does**: captures field research interviews + ambient observations, assists with inductive coding, structures qualitative data for analysis. Dovetail + Otter + Reduct own parts of this space.

**Why the codebase fits**: audio capture + speaker attribution + structured extraction + longitudinal subject tracking all apply. Lower regulatory friction than medical.

**Economic case**: middle tier — tens of thousands of academic researchers, thousands of commercial research teams. Per-seat SaaS. Competes with well-funded incumbents.

### 3.7 Skilled trades — HVAC/electrical/plumbing inspection intelligence

**Customer**: home inspection firms, HVAC service companies, electrical contractors.

**What the platform does**: technicians wear a capture device during inspections; AI structures findings + estimates + repair recommendations + photo annotations; generates customer-facing reports automatically.

**Why the codebase fits**: ambient audio capture + structured extraction + customer-facing document generation. Less is directly reusable than in the professional-specialist cases above, but the orchestration + deployment patterns apply.

**Economic case**: high-ticket services ($5K-$50K per job), high documentation burden, underserved by software. $100B+ US market.

**What would be new**: mobile-first (not clinic-first) deployment; specialty terminology; integration with service-software platforms (ServiceTitan, Jobber, Housecall Pro); photo analysis; cost estimation rules.

### 3.8 Expert-witness / specialized interviews

**Customer**: police departments, child protection services, immigration officials, journalism organizations, expert witnesses.

**What the platform does**: structured interview capture with AI-assisted pattern recognition (e.g., inconsistency detection, emotional-affect tracking, child-forensic-interview protocol compliance), chain-of-custody preservation, multi-language support.

**Why the codebase fits**: multi-party audio, protocol-driven structure, verifiable decisions, local-first for sensitive contexts.

**Economic case**: limited but high-value deployments. Government procurement cycles are brutal.

**Regulatory**: high. Depends on jurisdiction.

---

## 4. Hardware directions (beyond what exists today)

If the ESP32 sensor platform is the starting point, here are the plausible hardware extensions, each of which unlocks different verticals:

### 4.1 Near-term hardware extensions (6-18 months to prototype)

**Privacy-first visual sensor**: A camera with on-device processing that *only emits structured signals* — presence, posture, activity, not video. Important for any space-observation application where a camera is valuable but video cannot leave the device. Chip: Raspberry Pi Zero 2W or NVIDIA Jetson Nano with efficient on-device vision model (YOLOv8n, PoseNet). Used in: aging-in-place, clinic-operations intelligence, factory safety.

**Physician-wearable capture device**: Small chest-pendant or lanyard with high-quality mic + presence + small on-device LLM for low-latency, always-available capture. Think "Limitless pendant for professionals." Used in: house-call medicine, field trades, home visits.

**Wall-panel ambient display**: E-ink or small OLED mounted in the exam room showing current state (current patient, visit duration, next patient ETA, notes). Uses existing room-sensor inputs. Reduces cognitive load for physician and staff. Used in: clinic-operations intelligence.

**Elder-home sensor kit**: Pre-packaged adhesive-mount sensors for bedroom, bathroom, living room with provisioning simple enough for a non-technical adult child to install. Used in: aging-in-place.

**Local compute appliance**: Mac Mini or Asus NUC preconfigured with the full stack, shipped as a 1-box solution for clinics/practices. Zero-IT-support deployment. Used in: all verticals; reduces GTM friction enormously.

### 4.2 Medium-term hardware directions (18-60 months)

**Continuous biometric integration**: Read from Apple Watch, Oura, WHOOP, Garmin, medical wearables. Fuse with ambient signals. Used in: aging-in-place, clinical trials, chronic disease management.

**Ultrasound / thermal imaging accessory**: Specialty-specific sensor additions (POCUS ultrasound, high-resolution thermal). Used in: specialty clinics, point-of-care diagnostics.

**AR/smart-glasses integration**: Physician wears glasses, sees patient history + AI suggestions in peripheral vision, documents hands-free. Meta Ray-Ban / Apple Vision Pro / XReal. Used in: any field-work specialty.

**Voice-first satellite devices**: In-room dedicated microphones (not phone/laptop) with better audio quality and wake-word activation. Used in: multi-room clinic deployments.

**Patient-facing kiosk**: Waiting-room kiosk that captures pre-visit history, PROMs, consent forms. Uses existing sensor/audio/vision patterns. Used in: clinic-operations.

### 4.3 Hardware bets that would be transformative (5+ years)

**Custom silicon for local medical AI inference**: Apple Neural Engine-class accelerator optimized for medical LLM inference, packaged in a sealed device. Not a startup project usually, but partnerships with Apple/Qualcomm/DeepX could be structured. Enables regulated local AI at scale.

**Privacy-preserving wearable for continuous home observation**: Pendant + wristband + ambient room sensors with edge-AI fusion. Medical-device-grade. Think "whole-life health monitor with verified local-only processing." This is where the platform meets the "digital twin for humans" vision.

---

## 5. Genuinely weird combinations worth considering

These are low-probability but high-upside. The kind of ideas that sound stupid until they don't.

### 5.1 "The Clinic Autonomic Nervous System"

Not a scribe in a clinic; not a scribe plus sensors; the clinic *itself* running as a single integrated intelligent system. Every room instrumented. Every conversation captured. Every billing code automatic. Every patient flow optimized. The clinic operates with a minimum of human coordination because the platform runs the operational layer. Physicians see patients; nurses deliver care; the platform handles everything else. 

The economic end-state is that the platform *is* the clinic's operational infrastructure — as fundamental as electricity or HVAC. Probably rents at $5K-15K/mo per clinic regardless of physician count. TAM: ~10K Canadian primary care clinics × $10K/mo = $1.2B ARR potential.

The catch: this is a platform play that requires vertical integration. It's not a tool physicians adopt; it's a system clinic owners install. Different sales motion. Different customer. Much larger prize.

### 5.2 "Sovereign AI for professional spaces"

Positioning play rather than product pivot. The world in 2028-2030 will likely have a very clear split between **cloud AI** (convenient, cheap, non-sovereign) and **local AI** (slow, expensive, sovereign). The codebase is on the local-AI side of that split and is exceptionally well-built for it.

A horizontal "sovereign AI" brand could sell into law, medicine, therapy, government, finance — anywhere cloud AI is unacceptable. The product is the infrastructure (LLM runtime + replay + audit + deployment) plus a reference implementation in one vertical (medical). Customers buy the runtime and build their own vertical apps. Developer-platform positioning, not end-user positioning.

This is basically a combination of "HashiCorp for healthcare AI" and "Red Hat of regulated AI."

### 5.3 "Continuous attention fabric"

The codebase's sensor + audio + vision + event-processing architecture applied to a genuinely different domain: **attention research**. The ability to observe a person in an environment continuously over years and build a model of *how they use their time, attention, and relationships*. Potentially a consumer product ("your own LifeOS") or a research tool (cognitive science, productivity research, education research).

Extremely ambitious; would require years of consumer-brand building. But it's exactly the kind of platform-transcends-domain bet that Apple's early iPhone or Palantir's Gotham represented. Not for the faint of heart; not for anyone with a 3-year horizon.

### 5.4 "The animal-welfare platform"

Ambient sensing + local compute + continuous observation applied to farm animals, zoo animals, or research animals. Animal welfare is a real regulatory and ethical concern in agriculture and research. Continuous multi-modal observation with AI-structured welfare-indicator extraction could be sold to:
- Large-animal farms (dairy, poultry) for welfare compliance
- Research institutions for IACUC compliance
- Zoos for enrichment monitoring

Weird but genuinely underserved market. Animal welfare audits are expensive and manual today. Several VCs have started funding agriculture-tech specifically for this.

### 5.5 "Memory-as-a-service for institutions"

Not scribe, not clinic. The platform provides *institutional memory* for any organization that has recurring professional conversations. Every conversation captured, every decision replayable, every pattern searchable N years later. 

Customers: law firms (case history), architecture firms (project history), consultancies (engagement knowledge), family offices (relationship history), religious communities (pastoral care continuity). The product is not AI assistance — it's cryptographically-verified, searchable, privacy-respecting organizational memory. AI is how the interface works, not what the product is.

This is maybe the purest platform interpretation of the codebase — the scribe is one query pattern over an institutional memory store.

### 5.6 "Ambient audit for decisions"

Reverse the product: instead of capturing and structuring clinical work, capture and structure *decisions* being made in a space. Board meetings, management reviews, policy meetings, surgical briefings — any recorded session where "what did we decide, why, and who agreed" matters later.

This is a platform for organizational accountability. Corporate boards (who required that decision?), government committees (was this process followed?), safety review boards (did we consider X?). Probably B2B, probably enterprise, probably adjacent to compliance/legal budget.

---

## 6. Framework for narrowing — what I'd actually do

Given unbounded creativity and the honest constraint of "there's one person here currently" — the filters that matter most:

**Filter 1: Uses existing hardware investment.** The ESP32 firmware + sensor fusion + multi-room pattern is *real investment* that's unusual in the startup scribe landscape. A direction that doesn't use it is throwing that investment away. This eliminates directions like "pure legal software" or "pure research ethnography" where the hardware is decorative. It favors directions where ambient physical-space sensing is load-bearing.

**Filter 2: Compounds the replay/audit/verification architecture.** The other asset that's years of work to replicate. Favors regulated / regulated-adjacent domains. Eliminates consumer-grade applications where verification isn't a buying criterion.

**Filter 3: Plausible for current founder profile.** A single founder shifting from medical to eldercare is easier than medical to legal or medical to research. Domain knowledge transfers.

**Filter 4: Has defensible moats beyond year 5.** Favors directions with regulatory depth, longitudinal data networks, or specialty expertise. Eliminates commodity plays.

Applying these four filters to the list above, the directions that survive with all four green:

- **Aging-in-place / home intelligence for elders** (§ 3.3): uses hardware, compounds verification (medical-device territory), medical-adjacent, regulatory moat via RPM reimbursement + home-care certifications.
- **Clinical trial infrastructure** (§ 3.4): compounds verification strongly (21 CFR Part 11 is almost exactly the architecture), uses fleet management, regulatory moat, medical-adjacent for founder.
- **Psychology / measurement-based care** (§ 3.2): extends medical-clinic infrastructure to a clinical-adjacent specialty, compounds longitudinal memory, regulatory-adjacent.
- **"Clinic Autonomic Nervous System"** (§ 5.1): uses hardware + verification + compounds fleet management; requires bigger vision + capital but is the highest-ceiling play.

The directions that survive with 3-of-4 green:

- **Legal deposition platform**: doesn't use hardware well, but everything else fits.
- **Sovereign AI infrastructure**: great verification fit, but hardware contribution is weak.
- **"Institutional memory"** (§ 5.5): great platform frame, hardware optional.

Everything else has at least two filter-fails.

---

## 7. The uncomfortable meta-question

Reading my own list, the thing I notice is that the *best* non-medical directions still point toward medical-adjacent domains (eldercare, clinical trials, therapy). The existing codebase is deeply medical in its implicit assumptions — patient/encounter/clinical-context terminology, billing-code rules, HIPAA-style privacy posture, FDA-pathway architecture.

So the question is less "what else could this be" and more: **what's the right size of conceptual move?**

Small move: **chronic pain specialty** (STRATEGY_2026). Uses 90% of existing assets. Low risk, medium ceiling.

Medium move: **aging-in-place** or **clinical trials** or **therapy practice platform**. Uses 60-70% of existing assets. Medium risk, higher ceiling.

Large move: **clinic autonomic nervous system** or **sovereign AI infrastructure** or **institutional memory platform**. Uses 40-50% of existing assets but reframes the *product category entirely*. High risk, highest ceiling.

Reckless move: **animal welfare / pure legal / research ethnography / consumer LifeOS**. Uses 20-30% of existing assets. Gambles the investment.

Each size of move implies a different relationship to the existing product. Small move = evolve. Medium = re-architect one layer. Large = re-conceive the product but keep the platform. Reckless = start over.

**None of these sizes is wrong. The question is which matches the founder's appetite for risk, timeline, and capital.**

---

## 8. The three that most deserve a weekend of thinking

If I had to pick three directions from this document for the founder to genuinely think about over the coming weeks, not because they're *the answer* but because they'd most sharpen the strategy:

### 8.1 Aging-in-place home intelligence

**Why it's worth the thinking**: demographic inevitability of the aging population; hardware investment has highest leverage here; not currently well-served by a sovereign/local-first option; Medicare Advantage + provincial programs willing to pay for RPM; founder already has the *exact* architectural pattern (sensors + audio + local compute + longitudinal memory). The move from "sense clinic rooms" to "sense elder homes" is the smallest conceptual jump that unlocks the biggest TAM.

**Why it might be wrong**: consumer-grade deployment is fundamentally different from clinic deployment; trust-building with families takes years; elder tech has a cemetery of failed products.

### 8.2 Clinic Autonomic Nervous System

**Why it's worth the thinking**: fully uses every asset; largest per-customer revenue; largest ceiling; doesn't abandon medical focus; closest to what the codebase actually *is* architecturally; has natural progression from Phase 1 of either existing strategy document.

**Why it might be wrong**: requires team scale + capital + enterprise sales motion; 18-24 month deployment cycles per customer; needs an integration partner; is *still* ultimately Canadian primary care dependent.

### 8.3 Sovereign AI infrastructure (horizontal B2B platform)

**Why it's worth the thinking**: completely escapes the Canadian-healthcare-market ceiling; leverages the verification + replay architecture at highest value; has timing tailwind (regulatory scrutiny, data sovereignty); doesn't require specialty clinical knowledge.

**Why it might be wrong**: would abandon medical specialization; developer-platform sales is a different game; the horizontal positioning is historically hard for healthcare-origin companies (customers assume healthcare-only).

---

## 9. One paragraph to take away

The current codebase is not a scribe — it's a local-first multi-modal ambient-intelligence platform with a regulated-AI architecture that *has a scribe as its first customer*. At least five different products could be built from this platform in the next 5 years, ranging from small evolutions (specialty pain management) to complete category reinventions (aging-in-place home intelligence, clinic operating system, horizontal sovereign-AI infrastructure). The filter that matters most is which direction compounds the two most-unusual assets: the physical-world sensor + firmware investment (which eliminates pure-software directions) and the verification/replay/audit architecture (which eliminates commodity-AI directions). The directions that pass both filters converge on a small set: eldercare home intelligence, clinical trial infrastructure, psychology measurement-based-care, or "the clinic autonomic nervous system." None of these are scribes. All of them use the scribe as a feature — often a minor feature. The strategic question the founder actually faces is not "which scribe direction wins" but "what *size* of conceptual move does the founder want to make, given capital access, time horizon, and appetite for starting over on the market-facing half of the product while keeping the engineering half intact." That's a different question from either STRATEGY_2026 or STRATEGY_2031 answers — and it's the more honest question.

---

## Appendix: how these three strategy documents fit together

- **STRATEGY_2026** — 12-month plan, "scribe is the product," specialty-wedge execution, solo-founder-capable, $300-500K capital. Answers *how do I make revenue from the current codebase in 12 months*.
- **STRATEGY_2031** — 5-year plan, "scribe is the product," regulatory moat play, team-scaling required, $25-40M capital. Answers *how do I build a defensible clinical AI company over 5 years keeping the medical focus*.
- **STRATEGY_UNBOUND** — this document. "Scribe is one feature," platform-reconceived. Explores what the codebase + hardware could become without the medical frame. Not prescriptive; designed to stress-test the previous two by asking what's being left on the table.

A coherent decision uses all three:
1. Read UNBOUND first to genuinely open the space.
2. Decide the *size* of conceptual move (small / medium / large) based on appetite.
3. If small → execute STRATEGY_2026 with specialty wedge.
4. If medium → adapt STRATEGY_2031 to a different vertical (eldercare, therapy, trials).
5. If large → write STRATEGY_2032_CLINIC_OS (or equivalent) as a successor to STRATEGY_2031.

The worst outcome is attempting small + medium + large simultaneously. That's how a capable solo-founder project becomes a confused multi-product nothing.

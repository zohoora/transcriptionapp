# The Clinic Autonomic Nervous System — Creative Expansion

*Expansion on STRATEGY_UNBOUND § 5.1 and § 8.2. Not a roadmap. An exploration of what "a primary-care clinic that runs itself on the operational layer" would feel like, look like, and be architected like. Use this document to imagine; use the strategy documents to decide.*

---

## 1. The metaphor, taken seriously

The autonomic nervous system doesn't make you a better person. It makes you possible. You don't consciously breathe, digest, regulate your temperature, route blood to your organs based on metabolic demand, or trigger cortisol when threatened. Your cortex is free to think because everything else happens below it.

A primary-care clinic today is **consciously operated at every level**. The physician thinks about the patient *and* the room *and* the documentation *and* the billing *and* the next patient *and* the fridge temperature log *and* the referral letter and and and. The staff think about phones *and* scheduling *and* rooming *and* check-in *and* insurance *and* reminders *and* supplies *and* lab triage. Everyone is doing conscious cognitive work on operational questions that *should not require cognition.*

The Clinic Autonomic Nervous System (CANS) is not "AI tools for clinics." It's the hypothesis that **most of what a clinic's humans currently do consciously could be handled below the level of conscious attention, by a system that runs continuously, adapts to the specific clinic, and surfaces only what requires human judgment.** The scribe is one of its reflexes.

That's the prompt. The rest of this document is imagination constrained by the actual codebase and an honest read of primary care.

---

## 2. The five layers of a CANS

Mirroring the biological model, not because that's cute but because the layering is *load-bearing* — different decisions require different speeds, different autonomy levels, different override semantics.

### Layer 0 — The peripheral nervous system (always-on sensing)

Every source of signal the clinic already emits, captured continuously:

- **Audio** in each exam room, nurse's station, phones, front desk, waiting room
- **Presence** (mmWave) + **thermal** (who's there, are they moving, are they in distress) + **CO2** (is the room ventilating, is it crowded)
- **EMR telemetry** (which chart is open, how long, what's typed, which screens have been looked at)
- **Network activity** (lab feeds arriving, pharmacy messages, referral letters, portal messages, SMS inbound, fax)
- **Time** (appointment schedule state, actual arrival/rooming/departure timestamps, visit durations, gap durations)
- **Inventory** (what was used — gloves, swabs, vaccines, injectables — inferred from procedures done, not manually counted)
- **Environment** (temperature, humidity, air quality, vaccine fridge)
- **Staff identity** (who's on shift, where are they, what role)
- **Patient signals** (vitals from kiosk, PROMs from portal, wearables if consented)

Scale for a typical 4-physician primary-care clinic: ~8-12 rooms × 5 sensor modalities + EMR telemetry + external feeds = roughly 50 continuous signal streams, producing low-megabits-per-second locally, almost none leaving the clinic.

The critical design property at Layer 0: **everything is captured locally, nothing is mandatory-cloud, and every capture has a cryptographic audit trail from the moment it enters the system.**

### Layer 1 — Spinal reflexes (immediate, deterministic, never-wrong)

Sub-second responses to specific patterns. These are the "patellar tendon" of the clinic — hit the right stimulus, get the right response, no thinking required. Examples:

- Room state transitions: *patient checked in* + *room marked clean* → room assigned, nurse pinged
- Privacy protection: *EMR screen facing hallway* + *unrecognized face detected in hallway via waiting-room camera* → screen auto-dims
- Cold-chain monitoring: *vaccine fridge temp out of range for >2 min* → alert + timestamped log entry
- Documentation prompts: *chart closed* + *no billing submitted within 5 min* → gentle nudge to physician's tablet
- Critical lab arrival: *lab result flagged critical* + *physician currently rooming a patient* → note-on-tablet alert, not phone interrupt
- Hand hygiene: *room entered* + *no sink activation within 30s* → gentle chime (configurable, opt-in only)
- PHI-exposure prevention: *patient name visible on external-facing screen* → screen auto-rotate

These are rule-based, deterministic, fast. Reflexes never make hard calls. If the signal is ambiguous, they escalate up the layer stack. They are the "I know this is safe" layer. They must be *boringly reliable* or physicians will kill CANS in the first month.

### Layer 2 — Brainstem (patterned autonomic functions)

The constantly-running orchestration layer. Handles the rhythm of the clinic. Runs hundreds of times per hour without any human attention. Examples:

- **Room choreography**: the clinic is a dance. CANS knows who's in each room, how long they've been there, what's next, who's ready for what. Gently suggests: "Dr. Smith's Room 3 patient is close to wrap-up — Mrs. Thompson can be roomed in Room 1 in ~3 min." Not mandatory; just ambient awareness.
- **Pre-visit synthesis**: 90 seconds before the physician walks into a room, their tablet shows: last visit's key points, latest labs with deltas, open problems, overdue screens, active prescriptions, known allergies, patient-reported concerns from portal, recent communications. A briefing card, not a chart.
- **Ambient scribing** (the scribe feature): captures the visit, drafts the note, suggests billing. Just one function of Layer 2.
- **Referral orchestration**: specialist letters arrive → problem list updated, follow-up actions flagged, thank-you letter drafted, next appointment suggested if needed.
- **Refill triage**: pharmacy requests processed against criteria — stable chronic condition, labs in range, adherent patient → auto-approved with physician signature applied; anything else queued with context.
- **Lab auto-triage**: normal-stable → filed, patient notified per preference; abnormal-expected (known chronic) → updated in trend, flagged if trending wrong; abnormal-new → physician alert; critical → immediate physician alert regardless of context.
- **Inventory autopilot**: procedures done today → supplies depleted → reorder placed when below threshold.
- **Billing capture**: every billable moment (in-office time, counselling minutes, indirect care review, after-hours call) captured without physician entering timecards.
- **Scheduling optimization**: next-available slots weighted by historical visit-duration data for that physician + that patient + that visit reason.

Layer 2 is where the bulk of a CANS's *value* lives — not because it does surgical-precision AI, but because it does hundreds of small tasks that would otherwise consume conscious attention. It's the difference between the physician *doing 200 small operational things* and the physician *practicing medicine while 200 small operational things happen autonomously*.

### Layer 3 — The limbic system (contextual + emotional awareness)

The layer that makes CANS feel alive rather than bureaucratic. Slower, probabilistic, contextual. Examples:

- **Patient emotional state**: tone-of-voice analysis + word choice + silence patterns → "Mrs. Thompson sounds distressed today — probably needs more time than scheduled." Suggested, not acted on. Physician decides.
- **Physician fatigue**: shorter turns in conversation, slower typing, longer gaps between patients, tighter voice affect, fewer non-medical asides → ambient awareness for the physician themselves (opt-in), not for the boss.
- **Team friction**: staff conversations have measurable friction signals. Not a surveillance tool. Clinic manager might see a rolled-up signal: "team interactions have been terser this week. Last Thursday was a particularly hard day. Want to check in?"
- **Clinic mood**: is today a hectic day or a calm day? Should we push overflow to telehealth? Adjust walk-in capacity?
- **Patient crisis detection**: "I don't want to be here anymore" said in a specific context → gentle escalation to physician, not to 911. Calibrated for sensitivity.
- **Emergency detection**: chest pain discussed + heart rate elevated via kiosk vitals + duration of symptom > threshold → emergency protocol suggested.
- **Staff burnout surveillance** (consent-gated): longitudinal pattern detection across days/weeks/months. Gentle nudge toward wellness resources. Private to the individual, never reported up.

Layer 3 is the part most current "AI for clinics" gets very wrong — they either don't look at any of this, or they treat it as a surveillance dashboard for clinic owners. The CANS frame insists that emotional/contextual awareness exists for the benefit of the humans being sensed, not for the humans managing them. The architecture is built around that.

### Layer 4 — The cortex (strategic, slow, human-partnered)

Runs quarterly, not minute-by-minute. Outputs to humans who think about them. Examples:

- **Panel intelligence**: your panel is 2,200 patients, 40% over 65, chronic disease burden is increasing 3% year-over-year, visit frequency is up 8%, average visit complexity is up 4%, you're at 95% of capacity — likely need to cap panel or add capacity.
- **Quality improvement patterns**: 12% of your diabetic patients haven't had A1C in >9 months. Here are specific outreach lists. Here's what peer clinics' rates look like (anonymized).
- **Financial health**: your indirect-care capture (Q312) is running at 60% of the benchmark for similar-size FHO+ practices — here's where the missed time is.
- **Practice patterns**: your referral rate for musculoskeletal complaints is 3x the provincial average — some of this might be appropriate, some of it might be reducible with additional in-clinic procedures.
- **Clinical pathway compliance**: new hypertension diagnoses → only 65% have completed the recommended 3-month follow-up BP check. This is what we can do about it.
- **Physician wellness benchmarks** (self-only): you've averaged 11.2 hours of clinic time per day for the past two months. Peer physicians at similar panel size average 9.4.

Layer 4 is slow, synthesized, recommendations-not-actions. The human decides.

### Layer 5 — Peripheral motor (outputs, actions)

Writes to external systems. Every Layer 1-4 decision that escapes the brain must pass through Layer 5, which is the audit-and-execute point:

- EMR writes (notes, prescriptions, orders, billing codes, problem list updates)
- Patient communications (SMS, portal messages, email)
- Staff notifications (tablet, display, chime)
- Supply orders (to distributors)
- External integrations (pharmacy, lab, referral systems, insurance, OHIP submission)
- Physical-world actions, eventually (lights, HVAC, door locks, kiosk display content)

Every action here is logged, replay-able, and reversible where possible. Every action that touches a patient's record, billing, or communication has an associated *confidence* and *authorship* — was this reflex (L1), autonomic (L2), contextual (L3), or human? The record knows. Audit tomorrow. Audit in five years.

---

## 3. The operational categories CANS handles

Ten categories the current conscious-effort load of a clinic covers, reframed as autonomic functions:

### 3.1 Room flow & patient logistics
- Who goes where, when. Continuous optimization rather than fixed slots. Rooms assigned just-in-time. Patients SMS'd "Dr. Smith is running 8 minutes late — we'll text you when they're ready for you."

### 3.2 Clinical documentation
- Ambient scribing with specialty-appropriate templating. Drafted continuously, physician signs off. SOAP is one output; patient-facing handout is another; referral letter is another; all generated from the same captured encounter.

### 3.3 Revenue cycle
- Time-based billing captured automatically (Q310-313). Visit codes captured with diagnostic cross-validation (already built). Companion codes auto-added. Rejection prediction before submission. Recovered-missed-revenue reported weekly.

### 3.4 Inbound clinical traffic
- Lab results triaged by urgency and patient impact. Pharmacy refills handled per protocol. Referral letters parsed and problem lists updated. Portal messages classified and routed. Faxes digitized and filed.

### 3.5 Preventive care + population health
- Every walking-in patient evaluated against their care gaps. Screening reminders ambient during visits. Recall lists for chronic disease management generated continuously. Vaccination tracking with outreach automation.

### 3.6 Team coordination
- Handoff rules known. Who's on which patient. Who needs to know what. Standup meetings become shorter because the shared state is already known by the system.

### 3.7 Compliance & safety
- Cold-chain monitoring and reporting. Hand-hygiene reminders (opt-in). Documentation completeness audits. Controlled substance handling. Privacy-breach detection. Consent tracking.

### 3.8 Supply & equipment
- Consumables tracked by usage. Equipment maintenance schedules. Vaccine inventory with expiry. Medication stockouts predicted before they happen.

### 3.9 Patient engagement
- Pre-visit intake via kiosk or portal. Post-visit instructions delivered. Medication adherence check-ins between visits. Outcome surveys. Satisfaction tracking. Family communication (consent-gated).

### 3.10 Clinic business intelligence
- Daily operational dashboard. Weekly revenue/volume/quality snapshots. Quarterly panel intelligence. Annual benchmarking against peers.

---

## 4. Ten magic-moment scenarios (what it actually feels like)

Specific imagined moments that would make a physician, nurse, or clinic owner walking through a CANS-enabled clinic say *"wait, did that just—"*. These are the demos.

### Scenario 1 — Monday morning, 8:03 AM

Dr. Chen walks in. No computer login. The clinic knows her. Her tablet lights up with today's schedule + first patient's briefing. The briefing says: *Mrs. Lin, 74, here for BP follow-up. HbA1c improved from 8.2 → 7.4 over 3 months (you suggested metformin titration up; she's been doing it). Due for shingles vaccine. Reports on portal yesterday: "feeling better, sleeping more." One outstanding referral for ophthalmology accepted yesterday — appointment March 12.* Dr. Chen walks to Room 2. Mrs. Lin is already roomed. The tablet on the wall of Room 2 shows the same briefing, slightly abbreviated. Dr. Chen has made four operational decisions in under a minute and none of them felt like work.

### Scenario 2 — 10:47 AM, nurses' station

Nurse Sarah sees the ambient display: Room 3 has been waiting 18 minutes. Dr. Chen's patient in Room 5 was supposed to be done 12 minutes ago. Room 5's actual audio (structured, not verbatim) shows "complicated emotional conversation, 5 min past wrap-up signals." Sarah SMSes Room 3's patient: "Dr. Chen is finishing up with another patient, about 10 more minutes, thank you for your patience." No decision needed from Sarah about *whether* to message — CANS already decided that silent waiting >15 min triggers it. Sarah decided only to trust the system.

### Scenario 3 — 11:30 AM, Dr. Chen between patients

Dr. Chen glances at her tablet while walking to Room 4. Shows her: "Before you see Mr. Kumar — yesterday's cardiology letter arrived this morning. Key point: stress test negative, continue current meds, follow up in 6 months with us for BP management. Mr. Kumar's problem list has been updated. His question from the portal message last week ('do I still need to take the statin?') is still open — might be worth addressing today since you have the cardiology response now." She hadn't opened the letter. Hadn't seen the portal message. The thread was assembled for her.

### Scenario 4 — 1:15 PM, Dr. Chen's lunch

Lunch. Quiet waiting room. Her phone buzzes gently — not urgent. Tablet shows: *Pharmacy flagged: metformin refill for S. Kaur. Last A1C 9.8 (6 months ago, trending up). Missed her last diabetes follow-up. Auto-flag for physician because of pattern. Recommended: brief follow-up call rather than routine refill.* One decision: approve call-back. CANS drafts a brief "please book a follow-up" portal message. Dr. Chen reviews, sends. 30 seconds. Would have been 15 minutes of context-gathering without CANS.

### Scenario 5 — 2:30 PM, infection control

CANS notices Room 3 CO2 stayed elevated 20 minutes after the patient left. HVAC signal weak. Ventilation issue. Not an emergency — but the clinic manager's display gets an item: *Room 3 ventilation degraded this afternoon. Consider scheduling maintenance check. Details captured in today's report.* The manager didn't have to know there was a problem for the system to start tracking it.

### Scenario 6 — 3:45 PM, a difficult moment

A patient in Room 4 says something that registers at Layer 3: tone, silence pattern, a specific phrase about not wanting to continue. Nothing in Layer 1 (no explicit suicide ideation stated in protocol terms). But Layer 3 flags it. Dr. Chen's tablet gets a *gentle* note mid-visit: "Worth asking about mood/safety — some signals here." Not a diagnosis. Not a mandate. A peripheral nudge. Dr. Chen asks one more question she might not otherwise have asked. The conversation opens up. The visit takes 35 minutes instead of 15. Tomorrow's schedule gets rebalanced automatically.

### Scenario 7 — 4:30 PM, wrapping up

Dr. Chen's last patient wraps. Before she walks out, her tablet shows: *End of day summary. 22 patients seen (normal). Revenue on track. 3 referrals sent (2 auto-drafted, 1 pending your review — on your screen now). 14 portal messages triaged (4 need your reply — drafted and waiting). Tomorrow's schedule: one overbook, one new patient, one complex follow-up. No unsigned notes. Documentation complete.* She reviews and signs off on the 1 pending referral and 4 portal replies in 4 minutes. Leaves at 4:45 instead of 6:30.

### Scenario 8 — Saturday morning, the clinic owner at home

The clinic owner opens the weekly report. Not a spreadsheet. A one-page snapshot: *Revenue up 4% vs last week (typical). Q312 indirect care capture is at 82% of benchmark (up from 74% two weeks ago). No-show rate 6%, target 8%, doing well. Patient satisfaction proxy (post-visit surveys): 4.6/5 (stable). Physician wellness proxy: all green except Dr. Chen trending toward higher-than-usual hours (she mentioned renovations at home yesterday — probably why). Team function: normal. One flagged concern: Dr. Patel's diabetic A1C outreach completion is at 60% (target 80%) — here's what we can do.* The owner spent 6 minutes reading the report. The running of the clinic consumed no further mental energy from them this morning.

### Scenario 9 — Monday, 6 months in

Dr. Chen says to her clinical assistant chat: "How many of my patients with new hypertension diagnoses in the last year have had the 3-month follow-up?" CANS: "18 of 24. Of the 6 who haven't: 2 are booked for next month, 2 have moved, 1 declined in chart, 1 appears to have been missed — want me to outreach?" Dr. Chen: "Yes." CANS drafts outreach, Dr. Chen approves, it goes out. One decision, replacing ~45 minutes of chart-review and outreach drafting.

### Scenario 10 — Year 2

Dr. Chen has a patient in Room 2 who she's never seen before — a walk-in. The briefing says: *First visit to our clinic. Consented to record import via Oscar Pro's sharing feature — her previous clinic's records are available. Chronic: hypothyroidism (Levothyroxine 75 mcg, stable 3 years). Pregnancy: G2P1 (2nd pregnancy active, 18 weeks, prenatal care with Dr. Singh at Women's College). Mental health: GAD, last visit with psychology 3 weeks ago, doing well on sertraline. Current concern on portal: "I've had a cough for 3 weeks, not getting better, no fever." Differentials: post-viral, GERD, asthma exacerbation, ACE-inhibitor cough (N/A), pregnancy-related reflux.* Dr. Chen has never met this patient. She has 14 years of her medical history pre-processed in her pocket. Differential already surfaced. The visit is about the patient, not the paperwork.

These aren't futuristic. They're implementable on the current architecture. They require *integration and maturation*, not new core capabilities.

---

## 5. The three user experiences

CANS isn't one product — it's three interlocking experiences for three user types, each of whom would describe it differently.

### 5.1 The physician's experience

"I practice medicine. Everything else happens."

- Never logs in. Walks into the clinic, the system knows it's me.
- Never manually picks billing codes. They're captured, suggested, approved with voice or tap.
- Never writes a referral letter from scratch. Drafts appear; I edit.
- Never searches for a lab result. Relevant labs appear when relevant.
- Never asks "when did I last see this patient for X." The answer's in the briefing.
- Rarely works after hours. Everything I need to decide is on my screen; drafting is done.
- My panel is healthier because preventive care doesn't depend on me remembering.
- I know when I'm tired. So does the system — but only for me to see.

### 5.2 The staff member's experience

"The chaos I used to manage has become a rhythm I participate in."

- Arrive, know what the day looks like. Which patients are complex. Which rooms are prepped. Which nurses are on.
- Front desk: check-in is a kiosk for most patients. I handle the 10% who need a human. Phone calls are triaged — AI answers routine, I answer hard.
- Rooming: tablet tells me who needs rooming, where, and with what intake. Vitals are mostly entered before I walk in. I verify, observe, note anything the system might have missed.
- Handoffs: don't need standup meetings to know shared state. The display tells me.
- End of day: no chart-chasing. Documentation is complete. Inventory is ordered. Tomorrow is set up.

### 5.3 The clinic owner's experience

"I run a business. The operational layer manages itself."

- Weekly report. One page. Financial + quality + team health + flagged concerns.
- Hiring and capacity planning based on real trends, not gut feel.
- Billing compliance audited continuously. Missed revenue surfaced before it's too late to capture.
- Physician fatigue visible at the aggregate level (never individual unless they opt in). I can intervene before a physician burns out.
- Patient satisfaction proxy without running a survey.
- Annual benchmarking against peer clinics (anonymous) gives me real context for decisions.

### 5.4 The patient's experience

Worth considering even though they're not the buyer: what does CANS feel like to a patient in the clinic?

- Check-in is fast and respectful. Kiosk knows them. They can use staff if they prefer.
- Waiting is predictable. If there's a delay, they know and can leave and come back.
- Their physician walks in knowing them. Not "your last visit was about your knee, right?" — but "you finished the physiotherapy program, you told the portal you're running again, that's great; how's everything else?"
- Visit feels conversational. The physician is looking at them, not typing.
- After the visit, instructions arrive on their phone. Medications are ready at the pharmacy. Referrals are booked. Follow-up is on their calendar.
- Between visits: the system respects them. Gentle check-ins. No spam. They're in charge of their data.

---

## 6. What makes a CANS different from "a clinic with AI tools"

The distinction matters. Eight properties that separate a real CANS from the existing "scribe + billing + portal + ambient dashboard" aggregation:

### 6.1 Continuous, not episodic

The existing category: AI called per-session. Scribe for this visit. Billing for this claim. Portal for this message.

CANS: runs continuously, updating its model of the clinic every second. A patient walking in isn't "starting a scribe session" — they're *joining a flow the system has been aware of since they parked their car.*

### 6.2 Multi-modal, fused

Existing category: audio for scribing, separately EMR for documentation, separately sensors for occupancy, separately billing for claims. Each a silo.

CANS: audio + video + presence + EMR + external feeds all fused into a single model of "what's happening." Missing signal from one source is compensated by others.

### 6.3 Hierarchy of autonomy

Existing category: either fully manual (doctor writes everything) or fully automatic (AI writes, doctor signs). Binary.

CANS: five layers, each with different autonomy. Reflexes fire instantly. Autonomic functions run constantly. Contextual awareness surfaces gently. Strategic decisions stay with humans. The hierarchy is explicit and configurable — the clinic can dial autonomy up or down per function.

### 6.4 Transparent and auditable

Existing category: "the AI did it." Why? Unclear. Replay? Maybe.

CANS: every decision, at every layer, logged with inputs, outputs, confidence, and authorship. Replay any moment. Audit any decision. Regulatory-grade from day one.

### 6.5 Adaptive to this specific clinic

Existing category: same product for every clinic. Maybe per-user settings.

CANS: learns how *this clinic* operates. How Dr. Chen's Monday morning runs. How long Nurse Sarah's rooming takes. When the waiting room tends to back up. When this clinic tends to run over. The model is bespoke after 60 days; anonymized for cross-clinic benchmarking, never shared raw.

### 6.6 Interface-minimal

Existing category: another dashboard, another login, another app, another tab.

CANS: most of the time, invisible. The tablets staff carry are the surface; beyond that, the system is felt rather than seen. No dashboard for what doesn't need attention.

### 6.7 Local-first + sovereign

Existing category: cloud inference, cloud storage, vendor-controlled data.

CANS: runs on a local compute appliance in the clinic. Data stays in the clinic. Cloud is opt-in for specific features (e.g., cross-clinic benchmarks). Regulatory-grade sovereignty from day one. Competitors retrofitting cloud-first architecture for sovereignty will take 2-3 years.

### 6.8 Business-savvy

Existing category: the billing module is separate from the scribe which is separate from the analytics.

CANS: the billing reflex knows the scribe's output; the scheduling model knows the physician's stamina; the inventory predictor knows procedures done today; the quality-improvement analytics know the clinical pathways in use. Everything informs everything. The clinic is modeled as one organism, not ten systems.

---

## 7. What in the codebase points here already

Claim-by-claim, what the current architecture has that *already* points toward a CANS:

| CANS property | Existing codebase counterpart |
|---------------|--------------------------------|
| Continuous multi-modal sensing | `continuous_mode.rs` + `presence_sensor/` + `screenshot_task.rs` + STT streaming |
| Reflex layer | `encounter_detection` + hybrid-sensor trigger acceleration + `TRIGGER_*` constants |
| Autonomic layer | `encounter_pipeline.rs` orchestration of detection → SOAP → billing → sync |
| Contextual layer (nascent) | Patient name tracker + vision DOB extraction + retrospective multi-patient check |
| Strategic layer (nascent) | `performance_summary.json` + replay bundles as aggregate data substrate |
| Motor layer | Medplum client + `server_sync` + EMR writes + billing submission (future) |
| Transparent + auditable | Replay bundles schema v3 + pipeline_log + audit trail in archive |
| Adaptive to specific clinic | `server_config` for prompts/billing/thresholds; could extend to per-clinic learned params |
| Interface-minimal | Current minimalist overlay; existing continuous-mode UI is already "felt not seen" |
| Local-first + sovereign | Whole architecture; profile service + auto-deploy + sensor firmware |
| Hierarchy of autonomy | Confidence-tiered dx policy (v0.10.35) + server-configurable thresholds (v0.10.23+36) establish the pattern |

What *doesn't* exist yet:

- **Fusion of modalities into a unified state model.** Today each modality is processed in isolation (audio → detection → SOAP; vision → name; sensor → presence). They don't feed a common "clinic state" object.
- **Integrated external feeds.** Lab HL7, pharmacy, referral letters, portal messages. Medplum is the only ingress.
- **Scheduling/capacity model.** Not in the codebase.
- **Inventory + supply model.** Not there.
- **Staff model.** Profile service has physicians; nurses, reception, clinic manager roles don't exist.
- **Patient-facing interfaces.** Kiosk, portal, SMS, caregiver app — not built.
- **Strategic reporting layer.** `performance_summary` is a start; quality-improvement dashboards aren't.

A CANS is ~2 years of focused engineering from the current codebase. Not from zero. That's the point.

---

## 8. The R&D arc (if this became the bet)

Not a full roadmap — that's a different document. An arc, sketched:

**Year 1 — consolidate around ambient scribe + autonomic billing as the wedge**
- Chronic-pain specialty or generalist, but with complete autonomic billing (every billable moment captured, zero-effort for physician)
- Oscar Pro bidirectional integration
- Pre-visit synthesis (briefing card) for every patient
- Layer 1 reflexes formalized (the rule layer extracted from the pipeline)
- Revenue proof: 5-10 clinics paying

**Year 2 — Layer 2 expansion: autonomic referral/refill/lab triage**
- Referral-letter ingestion + problem list updates
- Pharmacy refill triage + physician-approved protocols
- Lab auto-triage
- Inventory model
- Scheduling optimization
- Fleet management across multiple clinics

**Year 3 — Layer 3 emergence: contextual awareness**
- Patient affect tracking (opt-in)
- Physician wellness monitoring (self-only, opt-in)
- Clinical pathway compliance tracking
- Panel intelligence at physician level
- Patient-facing kiosk + portal

**Year 4 — Layer 4 cortex: strategic analytics + cross-clinic benchmarks**
- Quality improvement dashboards
- Anonymous cross-clinic benchmarking
- Panel capacity planning
- Financial forecasting

**Year 5 — Layer 5 maturity: full peripheral motor integration**
- Outbound integrations: pharmacy (direct), lab, specialist networks, insurance
- Automated compliance reporting
- Physical-world integration (HVAC, lighting, kiosk hardware)
- Multi-jurisdictional expansion (BC, Alberta)

**Year 6+ — second vertical**
- Same CANS pattern applied to a second clinical domain (mental health practice, addiction medicine, long-term care facility)

---

## 9. What the business looks like (rough)

Unit economics for a typical primary-care clinic adopting CANS:

- 4-physician FHO+ clinic: ~$1.8-2.5M/year OHIP billing
- CANS per-clinic SaaS: $8-12K/month = $100-150K/year
- Billing-recovery share (optional tier): 1% of incremental capture = $15-30K/year typical
- Per-room sensor hardware: $2-3K one-time per room + $60/mo maintenance
- Initial installation: $15-25K (sensors + appliance + training)
- Total first-year revenue per clinic: ~$130-200K; steady-state ~$120-180K/year

TAM in Canada:
- ~10K primary-care clinics
- Realistic 5-year addressable: top 20% (~2K clinics) — multi-physician, FHO+, tech-adoption-positive
- At $150K/year ARR × 2K clinics: $300M ARR ceiling
- At 10% penetration: $30M ARR — a substantial business, not a unicorn

Strategic partnerships that matter:
- Oscar Pro / Accuro / PS Suite — integration partnerships, possibly investment
- Ontario Health Teams — aligned incentives for population-health outcomes
- Family-office healthcare funds + Canadian pension health-tech mandates
- Academic medical centers for clinical validation (reuse from STRATEGY_2031 pattern)
- Insurance/benefit providers — outcomes-based contracts

Competitive dynamics:
- Does not compete with Tali/Abridge for the per-physician scribe subscription. *Includes* scribing as one reflex among many.
- Does not compete with OpsMed for time billing. *Includes* time billing as one autonomic function among many.
- Competes with no one directly; creates a new category ("clinic operating system" rather than "AI tool for clinics").
- Acquisition endgame: an EMR acquires CANS to move up-stack, OR a large healthcare services company (TELUS Health, Loblaw Shoppers Drug Mart, LifeLabs) acquires for vertical integration.

---

## 10. Ten creative prompts that emerged from this exercise

Things worth sitting with for a weekend. Not product features — thought-provocations.

### 10.1 What if the scribe were the simplest reflex, not the flagship?
In this framing, the scribe is like the eye-blink reflex — useful, automatic, almost invisible. The flagship is the organism. What product story emerges when the scribe is downplayed rather than highlighted?

### 10.2 What if the clinic owner, not the physician, were the buyer?
Physicians choose scribes. Clinic owners would choose operating systems. The product shape, the demo, the sales motion all change. The roadmap changes — you'd build the clinic-owner dashboard before the physician's AI images.

### 10.3 What if CANS could be retrofitted onto an existing clinic without disruption?
Imagine a 2-week installation: sensors go up with adhesive mounts, local compute appliance is plugged in, kiosk arrives, physicians keep their existing Oscar workflow entirely. The CANS runs *alongside* existing systems, taking over functions one at a time. By month 6 it runs more than half the operational layer. What would that non-disruptive adoption path look like?

### 10.4 What if patients could opt into sharing their CANS-derived data with research?
Every patient's longitudinal trajectory — diseases, interventions, outcomes — is a research artifact. Consent-gated, anonymized, shared with academic medical centers for studies. The clinic becomes a research site without investing in research infrastructure. Revenue from contributed data.

### 10.5 What if the autonomic layer could operate below Layer 2 entirely via voice?
"Hey CANS, refill Mrs. Kaur's metformin." "Already queued, waiting for your approval — she's due for an A1C first. Want to add that?" "Yes, also add a follow-up in 2 weeks." Voice-first operational layer. Requires local speech understanding + clinical context, both of which the existing architecture supports.

### 10.6 What if CANS kept the clinic running when a physician was sick?
Coverage day: Dr. Chen is out, Dr. Patel is covering. CANS hands Dr. Patel Dr. Chen's patients with full context, pre-visit briefings tuned for a covering physician, and handoff summaries for Dr. Chen's return. Continuity-of-care problem solved by design.

### 10.7 What if CANS incorporated the clinic's physical environment as an input?
HVAC reading → infection control. Waiting-room CO2 → occupancy regulation. Lighting patterns → circadian-aware scheduling. The clinic's physical environment as a sensor input rather than a passive backdrop.

### 10.8 What if CANS supported "hospital at home" for post-discharge patients?
The same sensor + audio + local-compute stack, redeployed to a patient's home after discharge from hospital. Continuous monitoring. Coordination back to the home clinic. The CANS becomes the platform for multi-location, continuous care.

### 10.9 What if CANS kept a "clinic memory" that survives staff turnover?
Nurse Sarah retires. Her replacement has full context: how this clinic operates, which physicians prefer what, which patients need what, what the unspoken rules are. Tribal knowledge no longer walks out the door.

### 10.10 What if CANS were the pathway to capitation-readiness?
As Canadian primary care shifts toward capitation / value-based models, clinics need outcome measurement + population-health management + risk adjustment. Exactly what a CANS produces. The clinics running CANS would be the first to prosper under the new payment model.

---

## 11. Honest caveats

Writing an expansive creative piece is easy. Honest caveats:

- **Physicians are deeply skeptical of AI in clinic operations.** They've seen every iteration since the first EMR. CANS needs to earn trust by working quietly, not by promising grandly. The demo is "nothing felt different, and somehow my day was better."

- **Clinic owners are typically physician-owners, which means the sales cycle is a clinical-and-business sale simultaneously.** Long; requires champion physicians.

- **Regulatory exposure scales with CANS functionality.** Layer 1 reflexes are low-risk; Layer 2 billing/referral automation is Ontario-regulated; Layer 3 contextual/emotional sensing is privacy-regulated; Layer 4 analytics are de-identified fine. Each layer adds its own compliance overhead.

- **Privacy is load-bearing.** Every sentence of Layer 3 needs to be defensible under PIPEDA. Staff cannot be surveilled. Patients cannot be profiled without consent. The architecture reflects this, but it requires explicit policy work, not just technical work.

- **Change management inside clinics is the #1 reason clinic tech fails.** Installing sensors is easy; getting the front desk to trust the kiosk is hard. Product development needs heavy investment in onboarding + training + support — far more than "write the code."

- **The category doesn't exist yet in Canada.** That's an opportunity and a risk. No precedent means no reference customers, no analyst coverage, no investor familiarity.

- **Hardware + software + clinical + regulatory is a very wide product surface.** Larger team by year 2 is non-negotiable.

---

## 12. One paragraph that summarizes the creative case

The **Clinic Autonomic Nervous System** is what happens when you ask: *what if most of what a clinic's humans do consciously could happen below conscious attention?* The metaphor is literal — five layers mirroring peripheral nervous system → spinal reflex → brainstem → limbic → cortex, each with different speed, autonomy, and human-override semantics. The scribe is the eye-blink reflex, not the organism. A fully-realized CANS observes continuously across audio + video + sensor + EMR + external feeds; handles hundreds of small operational decisions per hour through reflexes and autonomic functions; surfaces contextual awareness gently without surveillance; reports strategic patterns slowly with human partnership; executes through a transparent motor layer that is auditable five years later. It transforms the physician's day from *"practice medicine while juggling 200 operational things"* into *"practice medicine"*; it transforms the staff member's day from managing chaos into participating in rhythm; it transforms the clinic owner's view from spreadsheet-based guess into calibrated confidence. The current codebase has the skeleton: continuous sensing, multi-modal ingestion, verifiable AI, local-first architecture, hierarchy of autonomy, sensor-firmware-to-LLM pipeline. What it lacks is fusion across modalities, external-feed integration, scheduling/capacity/inventory models, and the patient-facing surface. Those are ~2 years of focused engineering, not starting from zero. The business case is ~$150K per clinic per year, ~2,000 Canadian clinics addressable, ~$300M ARR ceiling, acquisition endgame via EMR or healthcare-services vertical. The honest risk is that this requires a larger team, longer timeline, bigger capital, and more clinical/regulatory discipline than a solo-founder-capable scribe — it's a different company. The honest upside is that it creates a category that doesn't currently exist, leveraging every unusual asset in the codebase (hardware, sovereignty, verification, multi-room) into a single coherent product story, and positions AMI Assist not as "a scribe competing with Tali" but as the infrastructure a clinic runs on — of which Tali and OpsMed each cover only one reflex.

---

## Appendix A — Naming

The working name "Clinic Autonomic Nervous System" is too scientific for customer-facing. Some alternatives tried:

- **Parasympath** — nods to the rest/digest branch of the autonomic system; suggests calm operation
- **Pulse** — heartbeat of the clinic; simple
- **Cadence** — rhythm of the day; implies the orchestration aspect
- **Keeper** — "what holds the clinic together"; softer
- **Stem** — brainstem; suggests fundamental-and-invisible
- **Meridian** — the underlying pattern of the clinic
- **Mesh** — fabric of sensors and intelligence
- **Chorale** — multiple voices singing together; implies coordination

For the company/product: something that doesn't signal "AI" or "scribe" — both are too narrow. Closer to infrastructure branding than feature branding.

## Appendix B — Relation to the other strategy documents

- **STRATEGY_2026.md**: the chronic-pain specialty wedge. Uses 90% of current codebase.
- **STRATEGY_2031.md**: the regulatory-moat + clinical validation play. Uses 70% of current codebase.
- **STRATEGY_UNBOUND.md**: the refusal to accept the scribe frame. Enumerates eight vertical directions and six weird combinations.
- **STRATEGY_CLINIC_OS.md** (this document): deep creative expansion of STRATEGY_UNBOUND § 5.1 and § 8.2. Explores one direction at real depth.

These documents together are *not* a roadmap. They're a decision surface. Reading them together, the question is not "which plan wins" but "what kind of company, at what risk, with what capital, with what founder life." Once that's decided, the appropriate strategy document becomes the working document.

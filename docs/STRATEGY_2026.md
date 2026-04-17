# AMI Assist — 12-Month Strategy (April 2026)

*This document is a working analysis. It's honest, not promotional. Facts, trade-offs, recommendations — in that order.*

## 0. Executive thesis

**The ambient-scribe category has commoditized faster than most incumbents expected, and the frontier has shifted from "generate a good SOAP note" to "run parts of the clinic autonomously." In Canada specifically, Tali is on track to become a default choice through the Canada Health Infoway program (10,000 fully funded primary-care licenses) and has already secured Ontario Ministry of Health billing-agent approval.** The window to compete head-on as a generic Canadian scribe is closing.

**AMI Assist has three assets that none of the winning competitors have combined**: (a) a mature OHIP/FHO+ billing engine with multi-patient and K013A-overflow logic, (b) sensor-based encounter detection with its own hardware firmware, and (c) a local-first multi-room clinic deployment architecture. Each on its own is a feature. Combined, they point toward a different product category: **clinic-operations intelligence rather than scribe**.

**The choice isn't "be a better scribe than Tali." It's to narrow the product to the shape only this codebase can fill**, then invest in the gap where Canadian FHO+ clinics are underserved by both US-style cloud scribes and Canadian-generic ones. The technical debt is manageable *if* we pick a direction within the next 60 days; if we keep adding features to all three plausible directions, the codebase will consume the roadmap.

---

## 1. Where the puck is going (12–24 months)

Five trajectories supported by 2026 reporting, not 2024 guesses:

### 1.1 Scribe → agent
Tali just launched **billing agents** — not code *suggestions*, actual claims submission. R1 + Heidi announced **prior-auth automation** at the point of care in March. Amazon launched **Connect Health** with agentic SDK. The category is converging on "the AI does the thing" rather than "the AI suggests the thing." Generic scribing is a loss leader; the revenue is downstream of the conversation.

### 1.2 Longitudinal memory becomes table stakes
Bi-directional EMR integration where the scribe *reads* labs, imaging, medications, and prior problem lists before writing — this is the dominant competitive claim in 2026 comparison guides. The future note is "grounded in the patient's longitudinal history," not just this visit. Pure single-visit SOAP generation is increasingly seen as the 2024 product.

### 1.3 Specialty-shaped notes
Generic SOAP is being replaced by specialty-specific reasoning templates. DeepScribe has gone deep on cardiology/oncology/orthopedics/urology. The market expects "this feels like a cardiologist wrote it," not "this is a competent generic note." Specialty documentation is the fastest-growing sub-segment.

### 1.4 Local-/edge-first for privacy
Heidi Remote launched 2026 — on-device STT explicitly as a privacy wedge. This is not a mainstream winner yet but it's a credible secondary axis for practices with regulatory sensitivity. PIPEDA + data residency are becoming real buying criteria in Canada, not checkboxes.

### 1.5 Clinic-owner-level tooling
Ambient AI is moving from individual physician tool to **fleet-scale clinical productivity platform**. Cleveland Clinic is running it across 80+ specialties as a system-level initiative. The buyer is shifting from the physician to the CMO/clinic owner, and the questions are about analytics dashboards, adoption metrics, coding accuracy per provider, and compliance monitoring.

**Implication**: the 2027 winners will be platforms that bundle (documentation + agentic billing/orders + longitudinal memory + clinic analytics). Pure scribes at $99-149 become the Freedia — commodity, low-margin, consumer-oriented.

---

## 2. Competitive landscape — honest read

### 2.1 Canadian market (where AMI Assist actually competes)

| Player | Moat | Weakness |
|--------|------|----------|
| **Tali AI** | Canada Health Infoway approval, Ontario billing-agent license, SOC 2 Type 2 + PIPEDA, all major Canadian EMR integrations (Oscar Pro, Accuro, PS Suite, Profile, CHR, Healthquest, Med Access), ~10K fully-funded licenses coming. Just launched billing *agents* (not just codes). | Cloud-first; not designed for multi-room clinic awareness; generic primary care; not sensor-aware. |
| **OpsMed** | **This is the most under-discussed direct competitor.** Ontario FHO+ specific. Already ships room sensors (adhesive-mounted) for Q310–Q313 time capture. Has OMH billing-agent approval. 1.5% fee model. Architecture eerily similar to AMI Assist's sensor work. | Appears focused on time capture + billing optimization; **does NOT ship ambient scribing/SOAP generation** based on public materials. Not generating notes. |
| **Scribeberry** | PIPEDA-compliant positioning, Canadian-owned, growing. | Small; no billing-agent approval; no sensor story; no EMR-native integration at Tali's depth. |
| **Mutuo Health (AutoScribe)** | Ontario-specific OHIP code prediction. | Narrow surface; small team; no multi-physician clinic story. |

### 2.2 US-origin competitors visible in Canada

| Player | Why they matter to Canadian practices | Why they're less dangerous to AMI |
|--------|---------------------------------------|--------|
| **Nuance DAX (Microsoft)** | 33% US market share, deepest Epic integration. | Epic-dependent; Canadian primary care runs on Oscar/Accuro/PS Suite. Not the competitor for the Canadian clinic. |
| **Abridge** | 30% US share; fast-growing; Epic mobile integration; 50+ specialties; patient summaries. | Cloud-only; US-billing focused; no OHIP model; enterprise health-system focus. |
| **Suki** | Ambient + voice commands + ICD-10 + order staging. | US billing model; no OHIP depth; no Canadian EMR integration. |
| **DeepScribe** | 98.8 KLAS score; specialty-tuned. | US-only focus; specialty-vertical play. Could cherry-pick a Canadian specialty someday. |
| **Freed** | $99-149 consumer pricing; simplicity; ~4-7% US share. | No clinic-operations layer; US-only focus. Competes mostly on price. |

### 2.3 What this means

The Canadian field is NOT crowded at what AMI Assist actually does well. Tali is the clear generic winner and will eat most price-sensitive physicians via Infoway. OpsMed is the sensor-time-billing specialist but **not a scribe**. Everyone else is either US-focused or smaller/narrower. The gap is:

> **Ambient scribe + OHIP billing agent + multi-room clinic awareness + local-first architecture, targeted at Ontario FHO+ clinics that want an operational platform, not a solo-physician tool.**

Nobody is filling that exact shape. That's the wedge.

---

## 3. AMI Assist today — honest assessment

### 3.1 Real strengths (earned, not claimed)

- **OHIP billing engine is depth of months of specialized work**: 235 SOB-verified codes, 145 in-basket / 90 out-of-basket, 562 diagnostic codes, K013A→K033A overflow, two-stage LLM+rules extraction, diagnostic cross-validation with confidence-tiered policy (just landed v0.10.35), multi-patient billing, time-based Q310–Q313 tracking with daily + 28-day caps. This is not something a competitor replicates in a quarter.
- **Multi-patient encounter handling is best-in-class.** Three-stage detection (pre-SOAP + retrospective safety net + split-point via LLM), size-gate protection, per-patient SOAP files, per-patient billing. This is real Ontario clinical reality (couples visits) that most scribes still fumble.
- **Sensor-based detection with hardware**: ESP32 Feather V2 + mmWave + thermal + CO2, custom firmware in `esp32-presence/` + `room6-xiao-sensor/`, hybrid fusion logic (sensor + LLM), sensor-continuity gate. OpsMed uses sensors too but for time capture. Nobody else has this architecture for documentation.
- **Replay logging architecture is ahead of the category**. Schema-v3 `replay_bundle.json`, 12 replay/regression CLIs, 192 replay bundles, 68 labeled bundles across 6 days, offline regression testing with forensic traceability. Most scribe companies run live-only tests. This is a dev-velocity moat if we exploit it.
- **Local-first, multi-machine architecture**: profile service on a MacBook, workstation apps with URL failover, auto-update via GitHub Releases, auto-deploy for the server. This is fit for clinics that don't trust US cloud.
- **Auditable decisions**: every LLM call is logged, every billing rule is deterministic, every merge/split decision has a replay bundle. For SOC 2 + PIPEDA audits, this is a head start.

### 3.2 Real weaknesses (strategic not cosmetic)

- **EMR integration is Medplum-only.** For Canadian primary care, the market is Oscar Pro, Accuro, PS Suite, Med Access, Profile, Healthquest, CHR. Tali integrates with all of these. AMI Assist integrates with none of them. **This is the single most strategic gap.**
- **Compliance posture is unfounded.** No SOC 2 Type 2, no PIPEDA certification, no HIPAA BAA, no signed Infoway approval, no DPIA documentation. Tali has all of these. For a clinic to adopt at scale, these aren't optional.
- **macOS-only.** Tauri can target Windows but screen capture, signing, bundling, CoreAudio, the deep-link flow are all Mac-specific today. Canadian primary care runs heavily on Windows. This limits the addressable market by roughly 2×.
- **No billing-agent approval.** Tali and OpsMed have OMH billing-agent approval — they can *submit* claims. AMI Assist suggests codes but doesn't submit. This is a licensure + operational process, not just software.
- **No Canada Health Infoway affiliation.** The 10K-license program routes primary-care adoption through Infoway-approved vendors. Being outside this becomes a paid-adoption headwind.
- **No longitudinal patient memory.** Every session is independent today. The 2026 comparison guides consistently name bidirectional EMR grounding as a dominant differentiator. AMI Assist doesn't read prior labs / imaging / problem lists into the note.
- **No agentic capabilities.** Every piece of functionality is "AI suggests, physician confirms." Tali's new billing agent *submits*. This gap will widen over the next 12 months.
- **Single-developer velocity is remarkable but not scalable.** The past 15 commits represent ~2 weeks of excellent solo output. Across a 12-month horizon with even modest strategic ambition, this must either grow to a team, be radically simplified, or open-source to recruit contributors.
- **macOS Screen Recording permission fragility.** Vision DOB extraction depends on a permission that invalidates on app rebuild. Not a bug, but a brittle production dependency.

### 3.3 Features that should probably be deprecated

Brutal honest list. These add surface area without pulling strategic weight:

- **AI image generation (Gemini, `gemini_client.rs`)** — default enabled, ~$0.04/image, 8/session cap. Clinically marginal; real physicians don't prescribe illustrations generated by an LLM. Remove or deprecate.
- **MIIS integration** — alt image source; almost certainly unused in practice.
- **Biomarkers (vitality / stability / cough — `biomarkers/`)** — research-grade feature without clinical validation. Interesting tech, not revenue.
- **Vision experiments CLI + commands** — useful as a dev tool, shouldn't be in the user-facing IPC surface.
- **Clinical assistant chat** — minor feature, not differentiated; Tali and every major competitor has it.
- **Patient handout** — not unique; not driving purchase decisions. Could stay but isn't core.
- **Differential diagnosis** — same; a minor feature, not a wedge.

Cutting these would shed roughly 8-12K LOC, simplify the mental model, and free attention for strategic direction. Not deprecating them is an active choice to accept maintenance cost on non-differentiated features.

---

## 4. The codebase — real technical debt, prioritized

Measured, not moralized. "Debt" means "will block strategic moves in the next 12 months."

### 4.1 P0 debt — blocks everything

#### 4.1.1 `continuous_mode.rs` is a god-procedure (3,577 LOC, 155 lock/Arc call-sites)
Contains the detector task, consumer task, flush-on-stop path, multi-patient orchestration, merge-back handling, sleep mode, recent-encounters, shadow mode wiring, screenshot task spawning. Four `tokio::spawn` sites share 155+ lock acquisitions across mostly-overlapping state. Every strategic feature (agentic flows, longitudinal memory, Oscar Pro integration) will want to hook into this file and the coupling means every change has global blast radius.

**Fix**: Extract into phase modules — `continuous_mode/detector.rs`, `continuous_mode/consumer.rs`, `continuous_mode/flush.rs`, `continuous_mode/merge_handler.rs`. Pass a `PipelineBus` (typed message passing) between them rather than shared `Arc<Mutex<_>>`. Target: no file > 1,000 LOC. This is a 2-3 week focused refactor; the tests already exist to catch regressions.

#### 4.1.2 136 Tauri IPC commands is an uncurated API surface
Many are one-off experiments from feature spikes that never got cleaned. Every one is a binding that needs types, error handling, and security review. For SOC 2 and for strategic velocity, this is the most visible attack + change surface.

**Fix**: 1-day audit → tag each as `production` / `experimental` / `deprecated`. Remove deprecated immediately. Move experimental behind a dev-build flag. Target <70 production commands.

#### 4.1.3 Windows/Linux support
Tauri supports them. The code doesn't. Screen capture (ScreenCaptureKit on mac), audio (CoreAudio), signing, bundling, deep-link — all have mac-only paths. Strategic: if you want to sell to a Windows-shop clinic, you're stuck.

**Fix**: Windows port is probably a 4-6 week project. Most Tauri plugins have Windows equivalents. Screen capture is the hardest part. This has to happen before the target-market conversation includes most of Ontario's clinics.

#### 4.1.4 No real EMR integration beyond Medplum
Strategic ceiling. Oscar Pro, Accuro, PS Suite each have HL7/FHIR facades but also quirky vendor-specific APIs. Pick one first.

**Recommendation**: **Oscar Pro integration is the single highest-ROI strategic engineering investment.** Oscar is the #1 Ontario primary-care EMR, has a documented API (OscarPro REST), and Tali prominently lists it first. Without this, AMI Assist is a demo for practices that happen to use Medplum.

### 4.2 P1 debt — blocks maturity / compliance

#### 4.2.1 No formal threat model or security architecture doc
Nothing in `docs/` covers threat modeling, data flow diagrams, encryption at rest specs, audit log policies, or retention. SOC 2 Type 2 audit requires these as baseline artifacts. **Do this before attempting compliance certification.** Moderate work: 1-2 weeks to write, validate with the actual code paths.

#### 4.2.2 Multi-tenancy doesn't exist
Profile service, archive paths, sensor configs, billing preferences — all assume one clinic. If AMI Assist ever serves a second clinic, the data model doesn't support it cleanly. Today a "clinic" is an implicit singleton on one MacBook.

**Fix**: Add a `clinic_id` field through the stack. Low-risk if done early; painful if done after paying customers exist.

#### 4.2.3 Frontend state management is ad-hoc
No store. Hooks coordinate via refs and custom `useSessionLifecycle`. It works for a primary-care workflow but as you add longitudinal patient memory, agentic flows, and a dashboard, you'll need a proper store (Zustand fits the Tauri model best). `App.tsx` at 994 LOC and `HistoryWindow.tsx` at 1,961 LOC are symptoms.

#### 4.2.4 No API versioning on profile-service
The v0.10.30 PATCH-semantics change was a breaking change made silently. Fine for a single-team, single-deployment world; unacceptable for anything multi-tenant or with third-party integrations. Tag all endpoints with `/v1/` now — cost is 1 day, benefit is 10 years.

#### 4.2.5 LLM Router is an unmitigated single point of failure
Detector / SOAP / billing / vision / clinical-content-check all fail together when the router is down. There's no graceful degradation plan, no circuit breaker, no fallback path for "router unreachable, buffer for retry later." A router restart during a clinic day = session disruption.

**Fix**: Add a `LLMCallDispatcher` layer between callers and the client with circuit-breaker logic, request queueing during outage, and explicit degraded-mode states. This also sets up for routing to multiple backends (cheap model for merge-check, expensive model for SOAP).

### 4.3 P2 debt — should address but not blocking

- `llm_client.rs` at 3,246 LOC — consolidation of many specialty methods (SOAP, vision, single-patient, multi-patient). Split per concern.
- `local_archive.rs` at 2,555 LOC — session storage + metadata + file allowlists. Split read-side from write-side.
- `HistoryWindow.tsx` at 1,961 LOC — needs component decomposition before adding longitudinal view.
- No performance regression suite — v0.10.36 laid the groundwork; build a "yesterday vs today p90" check.
- No chaos/integration testing across processes — kill STT mid-session, kill profile-service mid-merge, kill the LLM router between SOAP and billing. These paths aren't tested.
- `Cargo.toml` has 40+ direct dependencies — audit for abandonware and supply-chain risk.

### 4.4 Technical debt that is actually OK

- `billing/ohip_codes.rs` at 2,667 LOC — mostly data tables. Not a smell; it's a dataset.
- Rust test coverage (1,076 lib tests, 82 files with tests) — healthy.
- Auto-deploy via launchd — well-designed, canonical in repo.
- Replay logging architecture — solid, under-exploited.

---

## 5. Three strategic paths

Each path is internally coherent. Mixing them is how single-developer codebases collapse.

### Path A: **"The Clinic Operating System"** (18-24 months, highest ceiling)

**Positioning**: AMI Assist isn't a scribe — it's the operational intelligence platform for a Canadian primary-care clinic. Sensors + audio + EMR + billing + analytics, running on a local MacBook/Mac-Mini server. Sold to clinic *owners*, not individual physicians.

**Revenue model**: Per-room monthly SaaS + % of recovered billing (sensor-driven Q312 indirect-care recovery is a real dollar figure — OpsMed markets this openly).

**What to build in 12 months**:
1. Oscar Pro integration (bidirectional — reads labs/imaging/medication, writes SOAP + billing) — months 1–3
2. Clinic-owner dashboard: per-physician coding patterns, Q312 capture rate, merge/split quality, LLM cost per encounter — months 2–4
3. Billing agent license application (Ontario OMH) — months 3–8 in parallel (long regulatory timeline)
4. PIPEDA + SOC 2 Type 2 readiness — months 4–9
5. Fleet management: single dashboard for a multi-clinic operator — months 6–10
6. Windows port — months 7–10

**Defensibility**: Owns the clinic's operational data. Each clinic deployed makes the product better (sensor calibration, billing patterns, coding accuracy feedback). Moat widens over time.

**Risks**: Long sales cycle (clinic owner approval, IT, regulatory). Large scope. Requires second engineer by month 4 to be realistic.

### Path B: **"The Specialty Wedge"** (12 months, narrow but achievable solo)

**Positioning**: Pick one Canadian primary-care adjacent specialty underserved by Tali. Most plausible candidates: **addiction medicine / suboxone clinics**, **chronic pain / pain management**, **hormone replacement / longevity clinics**, **cannabis clinics**. All have weird billing + repetitive visit patterns + specific clinical-protocol documentation that generic scribes do poorly.

**Revenue model**: Per-physician premium SaaS ($299-499/mo) justified by specialty-specific billing capture and outcome tracking.

**What to build in 12 months**:
1. Interview 10 specialty physicians (month 1)
2. Specialty-specific SOAP templates + protocol-aware note generation — months 2-4
3. Specialty billing codes (e.g., addiction medicine K001A, K032A; chronic pain G nerve-block bundles, trigger points — partially done) — months 3-5
4. Longitudinal patient memory within the specialty (e.g., PHQ-9 / GAD-7 trajectory for psych, pain-scale trajectory for pain management) — months 4-7
5. Specialty-specific QI dashboard (e.g., buprenorphine dose trajectories, opioid tapering compliance, etc.) — months 7-10
6. Oscar Pro integration scoped to this specialty — months 8-12

**Defensibility**: Ontological depth in one specialty. Tali optimizes for the average, you optimize for the niche. Hard to compete with from the outside.

**Risks**: Picking the wrong specialty. Limited TAM per specialty (thousands of physicians, not tens of thousands).

### Path C: **"Open-Source Core + Proprietary Edge"** (12-18 months, disruptive)

**Positioning**: Open-source the Tauri app, detection pipeline, SOAP generation, replay tooling. Proprietary: OHIP billing engine + sensor firmware + cloud-hosted billing agent service. Sell enterprise support contracts and hosted billing.

**Revenue model**: Enterprise support subscriptions (clinics $5K-15K/yr) + hosted billing-agent service (% of submitted claims) + optional cloud sync tier.

**What to build in 12 months**:
1. License audit, extraction of proprietary components (OHIP rules) — month 1
2. Clean up the 136 Tauri commands and remove experimental features — month 2
3. Public docs, onboarding guide, contributor guide, CI for community PRs — months 2-3
4. v1.0 open-source release — month 3
5. Hosted billing-agent service (backend) — months 4-7
6. Community growth: Canadian open-source clinic association alignment, CBC/family-practice conference talks, 10K-dev hacker-news post — months 3-10
7. Oscar Pro integration (community-contributable) — months 4-9

**Defensibility**: The "Linux of medical scribes" positioning. Tali is closed; you're sovereign. Practices that distrust US-cloud scribes and want auditable code have a real choice. Contributors strengthen the codebase.

**Risks**: Revenue is slow to arrive. Requires strong founder marketing. Requires the billing engine to be fenced off cleanly. The community is small if you fail to ignite it.

### 5.1 Which path?

Candid opinion, based on what I can see of the codebase + single-developer constraint:

**Path B is the most executable solo**. Path A has the highest ceiling but needs a second engineer by month 4 and real operational capacity. Path C is intellectually interesting but Canadian primary-care adoption rarely flows through open-source communities.

Within Path B, the most interesting specialty given the existing code is **chronic pain management**. Reasons:
- Existing billing engine already handles K037A (fibromyalgia/ME care), G231A (nerve blocks somatic/peripheral), G223A (additional nerve sites), G228A (paravertebral), G384A/G385A (trigger points), G119A (epidural). This is already a pain-clinic-shaped billing surface.
- Multi-patient handling is less relevant (chronic pain is 1:1), reducing complexity.
- Continuous-mode + sensor detection fits "6-12 patient procedure day" workflows perfectly.
- PRP/prolotherapy tracking, pain-scale trajectories, opioid dose tracking are clean longitudinal features.
- Ontario has ~500 pain-management practices, most underserved by generic scribes.

**Path B with a chronic-pain-specialty wedge is the recommendation.**

---

## 6. Recommended 12-month roadmap (Path B — chronic pain wedge)

### Q2 2026 (next 90 days): Foundation + decision
- **Weeks 1-2**: Market validation — interview 10 pain-management physicians (FHO+ and OHIP billing focus). Goal: confirm specialty wedge; identify 3-5 killer features they'd pay for
- **Weeks 2-4**: Deprecate: AI images, MIIS, biomarkers, vision experiments (keep as dev CLI), clinical chat. Shed ~10K LOC. Ship as v0.11.0.
- **Weeks 3-6**: Refactor `continuous_mode.rs` into phase modules. Pipeline bus pattern. No behavior change. Ship as v0.11.1.
- **Weeks 4-8**: Oscar Pro integration (read-only first — labs, imaging, medication list, problem list). Ship as v0.12.0.
- **Weeks 6-10**: Pain-specific SOAP template + protocol awareness. Ship as v0.12.x.
- **Weeks 8-12**: Threat model + security architecture doc. Begin PIPEDA readiness plan.

### Q3 2026 (months 4-6): Specialty depth
- Pain-specific clinical decision support: prior procedure memory, opioid dose tracking, PHQ-9/GAD-7/pain-scale trajectories.
- Windows port (if any target practice is Windows-shop).
- Longitudinal view in HistoryWindow.
- Oscar Pro WRITE integration — upload SOAP + billing submission (without billing-agent license yet — physician submits via Oscar).
- Start SOC 2 Type 2 readiness prep.

### Q4 2026 (months 7-9): Productization
- Clinic-owner dashboard (simpler than Path A's version — for a solo-pain-practice owner).
- Billing-agent application in parallel (long regulatory lead time).
- Second physician beta cohort (5-10 pain clinics).
- Pricing + sales motion formalized.

### Q1 2027 (months 10-12): Scale readiness
- SOC 2 Type 2 audit completion.
- PIPEDA certification.
- First paid cohort (target: 20 pain clinics, 40-60 physicians).
- Metrics: demonstrable Q312 recovery per physician; coding accuracy; physician hours saved.

## 7. What to cut immediately (next 30 days)

Removing these is the cheapest high-value work on this list:

| Feature | Remove? | Rationale |
|---------|---------|-----------|
| `gemini_client.rs` + AI image generation | Yes | Clinically marginal; cost; not a wedge |
| `miis_client`-style integration paths | Yes | Alt to already-questionable feature |
| Vocal biomarkers (vitality/stability/cough) | Yes | Research-grade, unvalidated |
| Clinical assistant chat | Keep simple | Table stakes; don't grow it |
| Vision experiments CLI | Keep but hide | Dev tool only |
| Patient handout | Keep | Small maintenance; low clinical value but not harmful |
| Differential diagnosis | Keep | Same |
| Listening mode (auto-session detection) | Evaluate | If physicians don't use it, cut |

Target removal: ~10-15K LOC. Frontend components: `ImageSuggestions.tsx` (437), `ImageHistoryWindow`, `ImageViewerWindow`, related hooks.

## 8. Resource plan

- **Months 1-6**: Solo, with the refactor/cut/Oscar-integration/specialty-wedge focus. Maybe 15-20 hrs/week of clinical-physician input via interviews.
- **Month 4 recommendation**: contract frontend developer part-time to accelerate clinic-owner dashboard while founder focuses on Oscar Pro integration and specialty depth.
- **Month 6 recommendation**: part-time compliance consultant for SOC 2 prep.
- **Month 9 recommendation**: second full-time engineer (specialty preferred: Rust backend + Canadian healthcare regulatory familiarity).

## 9. Risks and unknowns

- **Tali moves into chronic pain specialty**. Probability: 20% within 12 months (they're generic-first; specialty is against their strategy). Mitigation: move fast; depth is defensible once built.
- **Canada Health Infoway expands program beyond primary care**. Would shift free-license market into specialty. Mitigation: apply for Infoway affiliation as specialty-focused vendor.
- **Apple changes screen-recording permission model**. Moderate risk; we already have workarounds. Mitigation: move vision-based DOB extraction to optional, don't make it required.
- **Oscar Pro API changes or gating tightens**. Not foreseeable. Mitigation: PS Suite or Accuro as backup integration target.
- **Single-developer bus factor**. Always the biggest risk. Mitigation: document, test, simplify, hire.

## 10. What I'd do in the next 2 weeks if this were my call

1. Validate the pain-specialty hypothesis by calling 5 pain clinicians (not surveys; calls). 2 days.
2. Announce internally: "we are cutting AI images, biomarkers, MIIS, vision experiments, clinical chat — ship v0.11.0 in 2 weeks". 5 days execution.
3. Ship the 136-commands audit + simplification. 2 days.
4. Start the `continuous_mode.rs` decomposition. 5 days for Phase 1 (extract detector task).

If the pain-clinician calls are lukewarm, pivot to another specialty or consider Path A seriously. But decide — don't add more scribe features to a generic-scribe surface in April 2026.

---

## Appendix A: Key references

**Canadian market**:
- Canada Health Infoway AI Scribe Program — 10,000 licenses
- Tali AI — Ontario billing-agent approval
- OpsMed — room sensor Q310–Q313 capture, 1.5% fee

**Trend reports**:
- McKinsey: "Ambient scribing at a crossroads" (April 2026)
- Beckers: "Ambient AI scribes, by market share" — 33% Nuance, 30% Abridge, 13% Ambience
- JMIR: "AI Scribes: Are We Measuring What Matters?" (2026)
- npj Digital Medicine: "Barriers and opportunities of scaling ambient AI scribes" (2026)

**Regulatory**:
- SOC 2 Type 2 prep: 3-6 months typical
- PIPEDA compliance: data residency, audit logs, breach protocols
- Ontario OMH billing-agent license: ~6 month application process

Sources consulted: Tali AI, OpsMed, Scribeberry, Mutuo Health AutoScribe, Heidi Remote, iatroX, Abridge, Nuance DAX, Suki, Freed, DeepScribe, Becker's Hospital Review, McKinsey, NIH PMC clinical-scribe narrative reviews.

# Codex Edge Case Detection

Date: 2026-03-08

Scope:
- Code-grounded review of continuous mode only
- Ground truth is the codebase, not the documentation
- Focus is robustness in messy real-world clinical workflows
- Security is intentionally out of scope for this memo

## Executive Summary

Continuous mode is already the most strategically important subsystem in this app. It is not a naive "record all day and ask an LLM to split" implementation. The current system already includes:

- a long-running audio pipeline
- silence-triggered checks
- manual `New Patient` override
- hybrid sensor + LLM mode
- shadow mode for alternate-split observation
- confidence gating
- force-split safety valves
- non-clinical filtering
- retrospective merge
- retrospective split after bad merge-backs
- screenshot-based patient-name extraction
- orphaned SOAP recovery during shutdown

That matters, because it means the problem is not "make continuous mode smarter in general." The real problem is narrower and harder:

`How do we increase confidence at encounter boundaries without turning the system into an overcomplicated, fragile mess?`

The strongest conclusion from reviewing the code is this:

- The next gains will not come primarily from better SOAP generation.
- The next gains will not come primarily from prompt tuning.
- The next gains will come from better boundary detection under uncertainty.
- The best way to improve boundary detection is not to add one giant new model. It is to add a small number of strong, low-ambiguity signals and a disciplined evaluation loop.

If I filter my own prior suggestions through the lens of real-world messiness and complexity cost, the solid build order is:

1. Build a boundary-scoring layer around the current detector.
2. Turn existing shadow mode and cleanup actions into a formal evaluation system.
3. Add better rescue controls and better operator visibility.
4. Add weak chart/schedule/banner priors, not hard chart-driven splitting.
5. Improve the audio front-end.
6. Add modest room-state sensing, starting with a door signal plus the current presence sensor path.
7. Only after that, consider richer sensing such as zone-aware presence or RTLS.

The main options I would reject for now are:

- making full-screen vision a first-class split trigger
- removing manual rescue controls
- relying on a single global threshold set across all rooms and specialties
- using a camera-first tracking approach as the main production boundary signal
- adding enterprise RTLS complexity unless the clinic already has that infrastructure or is already committed to it

## Ground Truth From The Code

The current architecture already reflects an unusually realistic understanding of continuous-mode failure modes.

### Core runtime shape

The app starts one continuous audio pipeline, then runs a separate detector loop that periodically analyzes the transcript buffer:

- pipeline startup: `tauri-app/src-tauri/src/continuous_mode.rs:294`
- transcript consumer and silence trigger: `tauri-app/src-tauri/src/continuous_mode.rs:356`
- detector loop start: `tauri-app/src-tauri/src/continuous_mode.rs:942`

This is important because the app is already structured as:

`audio capture -> buffered transcript -> boundary detection -> archive -> SOAP -> cleanup/repair`

That is the right architecture for a system that wants to become sessionless.

### Sensor and mode support

Continuous mode already supports:

- LLM-only detection
- sensor-only detection
- hybrid detection
- shadow mode

Relevant code:

- mode selection and sensor setup: `tauri-app/src-tauri/src/continuous_mode.rs:463`
- shadow mode observer: `tauri-app/src-tauri/src/continuous_mode.rs:596`
- hybrid trigger handling: `tauri-app/src-tauri/src/continuous_mode.rs:968`

Defaults in config:

- default detection mode is `hybrid`: `tauri-app/src-tauri/src/config.rs:210`
- default encounter check interval is `120s`: `tauri-app/src-tauri/src/config.rs:250`
- default silence trigger is `45s`: `tauri-app/src-tauri/src/config.rs:254`
- default presence absence threshold is `180s`: `tauri-app/src-tauri/src/config.rs:214`
- default presence debounce is `15s`: `tauri-app/src-tauri/src/config.rs:218`
- default hybrid confirm window is `180s`: `tauri-app/src-tauri/src/config.rs:194`
- default hybrid sensor timeout minimum words is `500`: `tauri-app/src-tauri/src/config.rs:198`

That is already a decent baseline.

### Detector context is still thin

The explicit encounter-detection context is currently only:

- `sensor_departed`
- `sensor_present`

See `tauri-app/src-tauri/src/encounter_detection.rs:27`.

That is the core limitation today. The system has a lot of repair logic, but the primary split decision is still operating with relatively few structured inputs.

### Vision is present but intentionally demoted

The screenshot path still exists and still tracks patient-name changes:

- screenshot task: `tauri-app/src-tauri/src/continuous_mode.rs:2422`
- stale vote suppression: `tauri-app/src-tauri/src/continuous_mode.rs:2553`
- name-change wake-up: `tauri-app/src-tauri/src/continuous_mode.rs:2590`

But the detector explicitly no longer trusts vision for split decisions:

- `tauri-app/src-tauri/src/continuous_mode.rs:912`
- `tauri-app/src-tauri/src/encounter_detection.rs:29`

That is a sensible production correction. Full-screen chart-name extraction is too noisy to be a hard trigger.

### The system already assumes split mistakes

The detector has multiple layers whose existence is direct evidence that encounter splitting remains the hardest problem:

- force-check and force-split thresholds: `tauri-app/src-tauri/src/encounter_detection.rs:8`
- confidence gate: `tauri-app/src-tauri/src/continuous_mode.rs:1424`
- retrospective merge: `tauri-app/src-tauri/src/continuous_mode.rs:1937`
- retrospective multi-patient split after merge-back: `tauri-app/src-tauri/src/continuous_mode.rs:2154`

This is not a criticism. It is the clearest signal of where the product risk really sits.

### Shutdown is still operationally brittle

On stop, the app:

- stops the pipeline
- aborts the detector task
- runs orphaned SOAP recovery
- flushes remaining buffer

Relevant lines:

- pipeline stop and detector abort: `tauri-app/src-tauri/src/continuous_mode.rs:2670`
- orphaned SOAP recovery: `tauri-app/src-tauri/src/continuous_mode.rs:2696`
- flush-on-stop: `tauri-app/src-tauri/src/continuous_mode.rs:2795`

This is a practical workaround, but it confirms that work is still being done in a task lifecycle that can be interrupted at the wrong moment.

### Logging and instrumentation are better than average

The pipeline logger is a strong asset:

- logger definition: `tauri-app/src-tauri/src/pipeline_log.rs:1`
- buffered pre-session logging: `tauri-app/src-tauri/src/pipeline_log.rs:16`

This gives you the raw material for a proper evaluation loop without rebuilding observability from scratch.

### The UI currently under-explains system behavior

The current continuous-mode UI shows:

- sensor status
- shadow indicator
- audio quality
- live transcript
- encounter count
- last encounter summary

See `tauri-app/src/components/modes/ContinuousMode.tsx:231`.

The frontend hook also polls stats every 5 seconds:

- `tauri-app/src/hooks/useContinuousMode.ts:128`

That is enough for a status dashboard, but not enough for a trust dashboard. Clinicians need "why" signals, not just state labels.

## What Makes Continuous Mode Hard In Reality

The following edge cases are the ones that matter. Any recommendation that does not survive these should not be built.

### Same patient, apparent boundary

Examples:

- doctor steps out to wash hands or get supplies
- nurse interrupts for a quick question
- phone rings, short pause, then exam continues
- clinician pauses dictation-style while reviewing a chart

Risk:

- false split

Why this matters:

- a pure silence rule will over-split
- a pure sensor departure rule will over-split
- a pure chart-change rule will over-split

### New patient, no clear physical departure

Examples:

- family member becomes the next patient in the same room
- physician pivots from child to parent
- doctor opens next chart before room turns over
- patient stays seated while the physician transitions to another person's issue

Risk:

- missed split

Why this matters:

- room occupancy alone is insufficient
- silence alone is insufficient
- simple greeting detection is insufficient

### Non-clinical content inside a real visit

Examples:

- scheduling talk
- social chat
- staff coordination
- billing or paperwork discussion

Risk:

- false split into a non-clinical artifact
- hallucinated SOAP for admin chatter

The current non-clinical pass helps here and should stay.

### Bad transcript, bad downstream everything

Examples:

- HVAC noise
- far-field speech
- cross-talk
- reverberant room
- partial clipping
- over-aggressive preprocessing

Risk:

- detector sees wrong boundary
- merge-back and retrospective split become more frequent
- SOAP quality degrades

This is why audio is not a secondary issue.

### Long tail operational failures

Examples:

- stop during SOAP generation
- LLM timeouts
- sensor disconnects
- screenshot permissions disappear
- chart context is stale

Risk:

- orphaned work
- inconsistent archives
- clinician trust collapses

The current code already contains repair logic for these scenarios. That is good, but the presence of repair logic is also evidence that the architecture should move toward more durable state transitions.

## Design Principles For What To Build Next

These principles are stricter than "make it smarter."

### 1. Every new layer must reduce a named failure class

If a new signal does not clearly reduce one of:

- false split
- missed split
- non-clinical artifact
- shutdown loss
- transcript quality uncertainty

then it should not be added.

### 2. Prefer deterministic weak signals before adding another model

Door events, occupancy transitions, speaker-count changes, and chart-banner changes are often more valuable than another large inference step.

### 3. Use weak priors, not hard triggers

This is especially true for:

- chart changes
- screen OCR
- schedule state
- patient-name extraction

The current code was correct to demote vision from hard split logic.

### 4. Improve evaluation before increasing complexity

Do not add new hardware or new model layers unless you can answer:

- what failure class it reduced
- by how much
- under which room and specialty conditions

### 5. Keep low-friction manual rescue forever, at least until the data proves it is almost never needed

Manual rescue is not product failure. It is the hedge that keeps autonomous workflows usable while the system matures.

### 6. Avoid global policies where room-specific policies are needed

Continuous mode will not behave the same in:

- family medicine
- psychiatry
- pediatrics
- procedure rooms
- shared urgent-care rooms

## Recommendation Matrix

| Option | Failure class reduced | Added complexity | Confidence | Build now |
|---|---|---:|---:|---|
| Boundary scorer with explicit features | False splits, missed splits | Medium | High | Yes |
| Formal evaluation loop from shadow + cleanup | All major classes | Low-Medium | High | Yes |
| Better rescue controls + trust UI | Workflow friction, trust loss | Low | High | Yes |
| Weak chart/schedule/banner priors | Missed splits, in-room pivots | Medium | High | Yes |
| Better audio front-end | Transcript uncertainty, all downstream quality | Medium | High | Yes |
| Door contact + current mmWave fusion | False splits, missed turnover | Low-Medium | High | Yes |
| Zone-aware presence sensing | Hard room-state ambiguity | Medium-High | Medium-High | Later |
| Dual-source audio (room + clinician mic) | Transcript quality, physician anchoring | Medium-High | Medium-High | Later |
| RTLS badges | Room/provider attribution | High | Medium | Only if infrastructure already exists |
| Camera-first person tracking | Some ambiguity classes | High | Low-Medium | No, not now |
| Full-screen vision as split trigger | False splits, stale context | High | Low | No |

## Solid Recommendation 1: Add A Boundary Scorer Around The Current Detector

### Recommendation

Build a deterministic `encounter boundary scorer` that aggregates signals and produces:

- boundary probability
- reasons
- confidence class
- recommended action

The LLM should become one contributor to the score, not the entire decision engine.

### Why this is solid

The current detector already behaves like a system that wants this. It already consumes:

- silence-triggered checks
- sensor state
- manual overrides
- confidence thresholds
- force-split fallbacks

But those pieces are still spread across procedural logic in `continuous_mode.rs`.

### Proposed signals

Structured signals to add:

- silence duration
- room vacancy duration
- room re-entry after vacancy
- door open then close within a short window
- current speaker count and recent speaker turnover
- dominant-speaker continuity
- whether an enrolled clinician voice is still the same anchor speaker
- speaker-turn distribution shift
- whether clinician voice remains present
- chart/banner patient change
- schedule proximity to a new appointment
- transcript uncertainty
- primary-vs-native STT divergence
- whether the last split was recently merged back
- whether the current content is clinically coherent or mostly admin/noise

### Counter-argument

"This just creates a hand-tuned rules engine that will become brittle."

### Response

That risk is real if the scorer becomes a giant rule forest. It is not real if the scorer is deliberately kept small and transparent.

A good version would:

- keep the feature set tight
- assign weights conservatively
- log every feature value
- expose reason codes
- still allow the LLM to arbitrate ambiguous cases

This reduces brittleness compared with the current situation, where many decisions are effectively implicit inside a single detector path and then repaired later.

### Complexity control

Do not build a full ML classifier first.

Start with:

- a Rust struct of feature values
- a weighted score
- reason codes
- per-room config overrides

Only after you have enough labeled data should you consider learning weights or replacing it with a trained classifier.

### Code touch points

- `tauri-app/src-tauri/src/continuous_mode.rs`
- `tauri-app/src-tauri/src/encounter_detection.rs`
- `tauri-app/src-tauri/src/pipeline_log.rs`

### Additional low-regret improvement: clinician voice anchoring

The codebase already has speaker-profile infrastructure and speaker-role ideas elsewhere in the app. That makes clinician voice anchoring one of the stronger low-complexity additions.

Why it helps:

- if the same enrolled clinician voice is continuously active, some apparent boundaries should be discounted
- if clinician speech patterns restart with a new greeting or new intake sequence, split confidence should increase
- in family-visit pivots, clinician anchoring can help distinguish "same clinician, new patient target" from random room noise

Counter-argument:

"The doctor is present in almost every encounter, so this does not identify the patient."

Response:

Correct. It should not be used to identify the patient. It should be used as a stability feature in the boundary scorer.

Decision:

- worth building
- but only as one feature among several, never as the split trigger by itself

## Solid Recommendation 2: Turn Shadow Mode And Cleanup Into A Formal Evaluation System

### Recommendation

Treat every retrospective merge, split, manual `New Patient`, and shadow disagreement as labeled evidence about detector quality.

### Why this is solid

This is already partially present in the code:

- shadow observer and decision logging: `tauri-app/src-tauri/src/continuous_mode.rs:596`
- merge-back path: `tauri-app/src-tauri/src/continuous_mode.rs:1937`
- retrospective split after merge-back: `tauri-app/src-tauri/src/continuous_mode.rs:2154`
- pipeline logging: `tauri-app/src-tauri/src/pipeline_log.rs:16`

You do not need to invent an observability system. You need to convert the existing artifacts into a measurable evaluation loop.

### Metrics that matter

Per room, provider, and specialty:

- false split proxy: `% of encounters later merged`
- missed split proxy: `% of merged encounters later retrospectively split`
- manual rescue rate: `New Patient` clicks per clinic day
- shadow disagreement rate
- detection latency from likely boundary to final archive
- orphaned SOAP recovery rate
- non-clinical false-positive rate
- note generation latency
- transcript uncertainty rate

### Counter-argument

"These are proxies, not truth."

### Response

Correct. But they are still valuable and much cheaper than trying to label everything manually. Use proxies first, then audit a sample of sessions manually to calibrate them.

The key is not to pretend the proxies are perfect. The key is to use them to rank failure classes and compare interventions.

### Complexity control

Start with offline reports from archive metadata and `pipeline_log.jsonl`. Do not build a big analytics backend yet.

### Code touch points

- `tauri-app/src-tauri/src/pipeline_log.rs`
- `tauri-app/src-tauri/src/local_archive.rs`
- cleanup commands in `tauri-app/src-tauri/src/commands/archive.rs`

## Solid Recommendation 3: Keep Chart Context, But Only As A Weak Prior

### Recommendation

Reintroduce chart context carefully through targeted banner OCR or direct app integration, but do not use full-screen name extraction as a hard split trigger.

### Why this is solid

The current code already documents why chart-driven splitting is dangerous:

- doctor may open family members
- doctor may review another chart during the same visit
- parsed names may be inconsistent

See:

- `tauri-app/src-tauri/src/encounter_detection.rs:29`
- `tauri-app/src-tauri/src/continuous_mode.rs:912`

This critique is correct. The right response is not "ignore chart context forever." The right response is "downgrade chart context to a prior."

### Stronger version of the idea

Use chart/schedule/banner as:

- `+small confidence` toward split
- `+small confidence` toward in-room pivot
- `+small confidence` toward same-patient continuation

Good targets:

- patient banner region only
- MRN or DOB changes
- appointment list highlight changes
- room assignment changes
- direct schedule feed if available

### Counter-argument

"Banner OCR and chart state are still noisy and will create more edge cases."

### Response

Yes, if they are made authoritative. No, if they are weak priors combined with transcript and room-state evidence.

The main mistake would be using chart change as:

- an immediate split
- a direct patient identity assignment
- a replacement for transcript evidence

### Complexity control

Do not keep a general full-screen vision pass as the strategic path. If you keep vision, narrow it:

- fixed crop
- OCR or lightweight extraction
- structured fields only

### Code touch points

- screenshot path: `tauri-app/src-tauri/src/continuous_mode.rs:2422`
- patient-name prompt and parsing: `tauri-app/src-tauri/src/patient_name_tracker.rs` and related prompt helpers

## Solid Recommendation 4: Improve Rescue Controls And Trust UX

### Recommendation

Keep `New Patient`, and add:

- `Undo last split`
- `Same patient / merge back`
- `Mark admin/non-clinical`
- visible reason codes for pending split confidence

### Why this is solid

The current UI shows status, but not enough reasoning:

- sensor status only: `tauri-app/src/components/modes/ContinuousMode.tsx:246`
- shadow summary only: `tauri-app/src/components/modes/ContinuousMode.tsx:274`
- audio quality summary only: `tauri-app/src/components/modes/ContinuousMode.tsx:310`

That is not enough for a clinician to quickly understand why the system is hesitating or why it split.

### Counter-argument

"More controls make the UI look less magical."

### Response

That is true if the controls are always visible and poorly framed. It is not true if:

- they are lightweight
- they appear only when useful
- they are phrased as recovery controls, not workflow obligations

The right product posture is:

- automation by default
- rescue when needed
- minimal interaction when things are going well

### Complexity control

Do not build a huge review dashboard in the main screen. Add:

- one reason string
- one confidence band
- one undo action
- one "same patient" action
- one "mark non-clinical" action

## Solid Recommendation 5: Improve The Audio Front-End Before Adding More Model Complexity

### Recommendation

Treat microphone and room acoustics as first-order determinants of continuous-mode quality.

### Why this is solid

Bad audio degrades:

- STT
- speaker attribution
- silence interpretation
- transcript coherence
- merge checks
- SOAP quality

The app already surfaces audio quality and biomarker outputs, which is a sign that audio quality is already recognized as important:

- audio quality event emission: `tauri-app/src-tauri/src/continuous_mode.rs:435`
- UI audio-quality panel: `tauri-app/src/components/modes/ContinuousMode.tsx:310`

### Recommended hardware tiers

#### Tier 1: Low-complexity portable improvement

Use a better full-duplex beamforming speakerphone as the room mic.

Good fit:

- single room
- low installation appetite
- quick experiment

Example:

- Jabra Speak2 75

Counter-argument:

"Speakerphones are still far-field and can still fail with reverberation."

Response:

Correct. This is why Tier 1 is an experiment or lightweight deployment path, not the end state for fixed-room production.

#### Tier 2: Fixed-room production improvement

Use a ceiling beamforming array with proper DSP.

Good fit:

- dedicated exam room
- willingness to install once and keep it stable
- desire to reduce desk-placement variability

Examples:

- Shure MXA902
- Shure MXA920

Counter-argument:

"Installation complexity is much higher."

Response:

Yes. But for fixed rooms, this is one of the few complexity increases that improves almost every downstream subsystem at once.

#### Tier 3: Dual-source audio

Use:

- room mic
- optional clinician-worn mic

This is the strongest audio architecture technically, but it adds:

- battery management
- device pairing
- workflow friction
- source synchronization complexity

Decision:

- promising, but not first

### Complexity control

Start with one better room mic before trying multi-source capture.

## Solid Recommendation 6: Add Modest Room-State Sensing Before Richer Hardware

### Recommendation

Add one more deterministic room-state signal before adding expensive sensing stacks.

The best next signal is:

- a door open/close event

paired with:

- the current presence sensor path

### Why this is solid

The current presence sensor module is built around a binary UART sensor:

- `tauri-app/src-tauri/src/presence_sensor.rs:1`

That is useful, but binary present/absent alone cannot reliably distinguish:

- patient left
- doctor left briefly
- someone stepped out and back
- next patient entered quickly

Door transitions sharply reduce ambiguity when fused with occupancy.

### Counter-argument

"Door sensors will not help in rooms where the door stays open."

### Response

Correct. That is why this is a room-dependent signal, not a universal one. In rooms where doors are irrelevant, do not force it.

### Complexity control

Add simple event fusion first:

- door opened
- room became absent
- room became present again

Only after that, consider richer occupancy or zone sensing.

### Good hardware sequence

1. Keep current DFRobot-style binary presence as baseline.
2. Add door contact sensor.
3. If still needed, move to zone-aware presence sensing.

### What about consumer zone sensors?

Products like Aqara FP2 are interesting for experimentation because they support zone ideas. They are not the first thing I would choose for production clinical deployment.

Reason:

- they increase integration complexity
- they are not obviously the most reliable clinical production path
- they are still weaker than simpler deterministic signals if your workflow is not yet instrumented

Decision:

- good experiment
- not first production recommendation

## Solid Recommendation 7: Use RTLS Or Badge Infrastructure Only If The Clinic Already Has It

### Recommendation

Do not greenfield RTLS just to improve continuous mode unless the deployment is already enterprise-grade and the clinic is already aligned with room-level infrastructure.

### Why this is solid

RTLS or badge infrastructure can improve:

- room attribution
- clinician presence
- staff movement understanding
- schedule/room correlation

But it adds major complexity:

- deployment
- calibration
- battery/device ops
- integration work
- ongoing support burden

### Counter-argument

"RTLS would solve many boundary problems."

### Response

It would solve some, not all. It does not solve:

- in-room family pivots
- transcript ambiguity
- non-clinical chatter
- poor audio

And it introduces substantial operational cost.

Decision:

- only build for environments that already have or clearly want this infrastructure
- not a core next step for the product as it exists today

## Solid Recommendation 8: Make Shutdown And Post-Detection Work Durable

### Recommendation

Move from task-lifecycle-based orchestration to explicit encounter job states.

### Why this is solid

Right now, stop handling still depends on:

- aborting the detector
- scanning for orphaned sessions
- regenerating missing SOAP afterward

See:

- detector abort: `tauri-app/src-tauri/src/continuous_mode.rs:2681`
- orphaned recovery: `tauri-app/src-tauri/src/continuous_mode.rs:2696`

This is effective as a fallback but not ideal as a core runtime model.

### Better model

Every detected encounter should move through durable states such as:

- `buffered`
- `boundary_confirmed`
- `archived`
- `clinical_check_done`
- `soap_pending`
- `soap_done`
- `merge_pending`
- `merge_done`

### Counter-argument

"This adds architectural complexity without directly improving split quality."

### Response

True, but it directly improves system robustness and operator trust. It also makes it much easier to:

- retry failed work
- inspect pipeline state
- recover after restarts

Decision:

- worth building after evaluation and boundary scoring
- not the first thing to do, but definitely on the core path

## Recommendation I Would Treat More Cautiously Than I Did Before

These ideas are not worthless. They are just not first-line solid investments.

### 1. Full-screen vision as ongoing context

Why I am more skeptical now:

- current code already documents why chart-derived name changes are unreliable
- the current screenshot path still consumes permission, timing, and model complexity
- the notify triggered by name change is no longer used as a meaningful split signal

Decision:

- narrow it to targeted OCR or explicit app integration
- do not expand the current full-screen approach

### 2. Camera-first room understanding

Why I am more skeptical now:

- very high complexity
- privacy and acceptability concerns
- easy to create another source of ambiguity
- room occupancy is simpler to solve with cheaper, lower-friction signals first

Decision:

- maybe experimental
- not a primary build recommendation

### 3. Greenfield RTLS for smaller deployments

Why I am more skeptical now:

- operational cost is disproportionate
- it does not solve enough of the hardest transcript-level problems by itself

Decision:

- only if the environment is already headed there

## Concrete Build Sequence

This is the order I would actually ship.

### Phase 1: Improve what is already there

1. Add offline metrics from archive metadata, cleanup actions, and `pipeline_log.jsonl`.
2. Add split reason codes and confidence bands to the UI.
3. Add `Undo last split`, `Same patient`, and `Mark non-clinical`.
4. Add transcript-uncertainty metrics from native-vs-primary STT disagreement.
5. Add per-room or per-profile configuration bundles.

Why first:

- low complexity
- very high learning value
- no hardware dependency

### Phase 2: Add better structured boundary signals

1. Build the boundary scorer.
2. Feed it:
   - silence duration
   - occupancy duration
   - recent occupancy transitions
   - clinician voice anchoring
   - transcript uncertainty
   - chart/banner weak priors
   - speaker-turnover signals
3. Keep the LLM as a contributor, not sole judge.

Why second:

- this is the highest software ROI
- it directly targets the main failure class

### Phase 3: Add modest hardware

1. Upgrade audio hardware in one or two rooms.
2. Add a door signal in those same rooms.
3. Re-measure split quality.
4. Only if ambiguity remains high, test zone-aware sensing.

Why third:

- better to instrument and score first, then add hardware with a measurable baseline

### Phase 4: Harden runtime durability

1. Move to durable encounter job states.
2. Make stop/drain graceful instead of abort-plus-repair.
3. Add retry semantics around SOAP and merge logic.

Why fourth:

- strengthens the operational core after the detector is less noisy

## The Strongest Low-Regret Hardware Suggestions

These are the suggestions I think survive real-world counter-arguments best.

### Audio

#### Portable / quick test

- Jabra Speak2 75

Why:

- lower deployment friction
- better than generic laptop or webcam audio
- useful to validate whether audio front-end quality is currently the hidden bottleneck

Limit:

- still a compromise in reverberant or large rooms

#### Fixed-room production

- Shure MXA902
- Shure MXA920

Why:

- beamforming
- more stable coverage
- less operator variability
- strongest chance of improving every downstream model

Limit:

- install complexity and cost

### Room-state

#### Production-first

- keep current mmWave presence path
- add a simple door contact sensor

Why:

- low ambiguity reduction per unit complexity is very good

#### Experimental

- richer mmWave or zone-aware presence sensors

Why:

- potentially useful
- but only after simple signal fusion is measured

#### Enterprise-only

- RTLS / smart badge infrastructure such as Kontakt.io-style systems

Why:

- powerful if already present
- too expensive and operationally heavy as an early build path

## What I Would Explicitly Not Build Yet

1. A camera-first production boundary system.
2. Full-screen vision as a high-confidence patient switch detector.
3. A greenfield RTLS deployment for small or medium practice settings.
4. More prompt engineering without a proper evaluation loop.
5. Removal of manual rescue paths.
6. One universal threshold set for all rooms and specialties.

## Final Position

If the goal is to make continuous mode robust enough that session mode eventually becomes unnecessary, the safest path is:

- fewer magical assumptions
- more structured weak signals
- better audio
- modest sensor fusion
- disciplined evaluation
- lightweight recovery controls

That path is slower than "just add another model," but it is much more likely to survive real clinics.

The single best next build is not a new model. It is:

`a boundary scorer plus an evaluation loop`

That gives you a place to plug in:

- better transcript uncertainty signals
- better room-state sensing
- better chart priors
- better room-specific tuning

without turning continuous mode into a pile of opaque heuristics and post-hoc repairs.

## External References

These are relevant product and hardware references that informed the hardware and workflow suggestions:

- Shure MXA902: https://www.shure.com/en-GB/products/microphones/mxa902
- Shure MXA920: https://www.shure.com/es-LATAM/productos/microfonos/mxa920
- Shure Microflex Advance overview: https://www.shure.com/en-US/solutions/microflex-advance
- Jabra Speak2 75: https://www.jabra.com/business/speakerphones/jabra-speak-series/%20jabra-speak2-75
- DFRobot SEN0395: https://wiki.dfrobot.com/mmwave_radar_human_presence_detection_sku_sen0395
- DFRobot C4001: https://wiki.dfrobot.com/SKU_SEN0610_Gravity_C4001_mmWave_Presence_Sensor_12m_I2C_UART
- DFRobot C4002: https://wiki.dfrobot.com/sen0691/
- Aqara Presence Sensor FP2: https://www.aqara.com/en/product/presence-sensor-fp2
- Kontakt.io Smart Badge 3: https://support.kontakt.io/hc/en-gb/articles/19676391233948-About-the-Smart-Badge-3
- Kontakt.io Epic RTLS integration: https://support.kontakt.io/hc/en-gb/articles/22641133977116-Epic-RTLS-integration-with-Kontakt-io
- Kontakt.io Access Agent: https://kontakt.io/ai-healthcare/access-agent/
- Nabla Epic workflow context: https://www.nabla.com/epic
- Oracle Health Clinical AI Agent: https://www.oracle.com/health/clinical-suite/clinical-ai-agent/

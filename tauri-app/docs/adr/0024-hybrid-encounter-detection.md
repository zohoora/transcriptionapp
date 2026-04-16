# ADR-0024: Hybrid Encounter Detection (Sensor + LLM)

## Status

Accepted (Apr 2026)

**Extracted from ADR-0019** (Continuous Charting Mode) which predated the hybrid mode. The original ADR documented LLM-only detection; this ADR covers the sensor-accelerated hybrid variant that is now the production default.

## Context

LLM-only detection has two weaknesses in a clinic setting:

1. **Latency on real departures** — the LLM runs on a timer (~2 minutes between checks). A patient who leaves early isn't detected as "encounter complete" until the next tick + the LLM's response time.
2. **False positives during couples/family visits** — topic shifts between a couple discussing different conditions trigger the LLM to split what should be one encounter.

We added a presence sensor (see ADR-0025) specifically to get an independent signal for "did the patient just leave?" But the sensor alone is also unreliable: hand-washing, injection prep, bathroom breaks, and cover-physician hand-offs all look like "departure" to a sensor.

Each signal alone is weak; the combination is strong.

## Decision

Run the LLM and the sensor concurrently. Neither one force-splits on its own. Instead:

1. **Sensor Present → Absent** accelerates the next LLM check from ~2 min to ~30s. The LLM gets a `sensor_departed=true` context signal and decides with transcript content.
2. **Sensor timeout force-split** — if the sensor remains Absent for `hybrid_confirm_window_secs` (default 180s) AND the encounter has at least `hybrid_min_words_for_sensor_split` (default 500) words, force-split even if the LLM disagreed. Sensor timeouts suggest genuine departure; the word-count floor prevents splitting empty encounters.
3. **Sensor-continuity gate** — while the sensor reports unbroken presence since the last split (`sensor_continuous_present=true`), raise the LLM confidence requirement for a split to `0.99`. This suppresses spurious LLM splits during couples/family visits.
4. **Graceful degradation** — if the sensor watch channel errors (unplugged, firmware crash), `sensor_available=false` is set and the mode continues as LLM-only until reconnect.

### Control flow

```
           ┌──────────────────────────────┐
           │  Continuous mode main loop   │
           └───────────────┬──────────────┘
                           │
                           ▼
     ┌─────────────────────────────────────────┐
     │  Every ~2 min (or 30s after sensor      │
     │  departure): run LLM encounter check    │
     └──────────────────┬──────────────────────┘
                        │
       ┌────────────────┴────────────────┐
       │                                 │
   LLM says SPLIT                    LLM says continue
       │                                 │
       ▼                                 │
  sensor_continuous_present?             │
       │                                 │
   YES → require conf ≥ 0.99 else CONTINUE
   NO  → require conf ≥ dynamic_gate     │
       │                                 │
   still above? → SPLIT             ┌────┴──────────────────┐
                                    │ sensor absent ≥       │
                                    │ hybrid_confirm_window │
                                    │ + words ≥ threshold?  │
                                    │                       │
                                    │ YES → FORCE_SPLIT     │
                                    │ NO  → CONTINUE        │
                                    └───────────────────────┘
```

### Config

| Field | Default | Clamp | Purpose |
|-------|---------|-------|---------|
| `encounter_detection_mode` | `"hybrid"` | `"hybrid"\|"llm_only"\|"shadow"` | Top-level mode selection |
| `hybrid_confirm_window_secs` | 180 | 30–600 | How long sensor must be absent before force-split |
| `hybrid_min_words_for_sensor_split` | 500 | 100–5000 | Word-count floor for sensor-timeout splits |

All auto-derived in the settings drawer: sensor configured → `hybrid`, else `llm_only`. Users don't pick mode explicitly.

### State on `DetectionEvalContext`

- `sensor_absent_since: Option<Instant>` — set when sensor first goes Absent; cleared on Present return or on successful split.
- `sensor_continuous_present: bool` — true iff sensor has been unbroken since last split. Toggled by sensor observer.
- `sensor_departed: bool` — current sensor state for prompt context.
- `sensor_available: bool` — false when the watch channel has errored.

### Prompt context

The detection prompt builder accepts `Option<&DetectionEvalContext>` and injects a sensor context section:

- `sensor_departed=true`: "The presence sensor detected possible movement away..." (+ guidance on timestamp reliability)
- `sensor_present=true && !sensor_departed`: "The presence sensor confirms someone is still in the room..." (+ guidance on couples/family visits)

Both texts are server-overridable via `PromptTemplates.encounter_detection_sensor_*` — see ADR-0023.

## Consequences

### Positive

- **Faster splits on genuine departures** — sensor acceleration catches real transitions within 30s instead of 2 min.
- **Fewer false splits during family visits** — sensor-continuity gate raises the bar when presence is steady.
- **Degrades cleanly without a sensor** — an unplugged or failed sensor silently falls back to LLM-only.

### Negative

- **Two systems to reason about** — debugging a wrong-split decision requires looking at both sensor state and LLM confidence.
- **Sensor timeout is still a hard force-split** — if a physician steps out for 5 minutes to check on another room, the encounter splits. The 500-word floor makes this rare (real encounters accumulate words fast), but it can happen during very quiet intervals.
- **Shadow mode complexity** — a third mode (`shadow`) runs both detectors and logs divergences without splitting. Useful for tuning but adds code paths (see `shadow_log.rs`, `shadow_observer.rs`).

## References

- `tauri-app/src-tauri/src/encounter_detection.rs::evaluate_detection()` — core decision logic
- `tauri-app/src-tauri/src/continuous_mode.rs` — sensor observer wiring
- `tauri-app/src-tauri/src/shadow_observer.rs` — shadow-mode dual detection
- ADR-0019: Continuous Charting Mode (predecessor — LLM-only)
- ADR-0025: Multi-Sensor Presence Suite
- ADR-0027: Retrospective Multi-Patient Check

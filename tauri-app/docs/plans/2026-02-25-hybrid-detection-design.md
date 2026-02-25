# Hybrid Encounter Detection — Design Document

**Date:** 2026-02-25
**Context:** Full-day clinic (5h, 12 encounters, ~36K words) revealed LLM detection is slow (8-45 min latency) and sometimes fails entirely (force-split at 11K words). Sensor detection is fast (~1 min) but misses back-to-back encounters and can't distinguish brief departures from encounter boundaries.

## Core Concept

**Sensor provides early warning, LLM confirms.** Neither acts alone (except as fallback).

The hybrid mode combines both detection methods to get the speed of the sensor with the content-awareness of the LLM.

## Two Detection Paths

### Path 1: Sensor-Accelerated (fast physical departures)
1. Sensor detects Present→Absent transition
2. Immediately wake the LLM check (bypass 2-min timer)
3. If LLM confirms split → split immediately (~30s instead of ~8 min)
4. If LLM says "no split" → person may have stepped out briefly, continue monitoring
5. If sensor timeout (3 min) + word_count >= 500 → force-split (sensor was right, LLM was slow/wrong)
6. If person returns (Absent→Present) before timeout → cancel pending, resume normal

### Path 2: LLM Content-Based (back-to-back encounters, phone calls)
1. Regular timer-based LLM checks continue firing (every 2 min)
2. LLM detects content transitions: new patient greeting, topic shift, wrap-up language
3. Works identically to current LLM mode — no sensor involvement needed
4. Handles: two patients in room, consecutive phone calls, any scenario without physical departure

### Sensor Unavailable Fallback
1. Sensor fails to start → `sensor_available = false` → pure LLM mode (transparent)
2. Sensor disconnects mid-operation (watch channel closes) → `sensor_available = false` → pure LLM mode
3. Warning logged, `sensor_status` event emitted to frontend
4. No automatic reconnection (USB device gone = port gone; user would replug and restart)

## State Machine

```
RECORDING ──sensor Present→Absent──► track absence_since, wake LLM check
    │                                      │
    │◄──sensor Absent→Present──────────────┘  (cancel tracking)
    │◄──LLM says "no split"────────────────┘  (continue monitoring)
    │
    ├──LLM says "split" (any trigger)──► SPLIT (method: hybrid_llm or hybrid_sensor_confirmed)
    ├──sensor timeout + words >= 500───► SPLIT (method: hybrid_sensor_timeout)
    ├──manual trigger──────────────────► SPLIT (method: hybrid_manual)
    ├──force-split thresholds──────────► SPLIT (method: hybrid_force)
    │
    └──timer fires──► run LLM check (standard path, handles back-to-back scenarios)
```

## Config Changes

```rust
pub enum EncounterDetectionMode {
    Llm,
    Sensor,
    Shadow,
    Hybrid,  // NEW
}

// New fields in Settings/Config:
hybrid_confirm_window_secs: u64,          // default 180 (3 min)
hybrid_min_words_for_sensor_split: usize, // default 500
```

## Detection Method Metadata

When archiving an encounter, `detection_method` records how it was triggered:

| Value | Meaning |
|-------|---------|
| `hybrid_sensor_confirmed` | Sensor triggered, LLM confirmed |
| `hybrid_llm` | Timer/silence triggered, LLM detected (back-to-back scenario) |
| `hybrid_sensor_timeout` | Sensor absence timeout, forced split |
| `hybrid_manual` | Manual "New Patient" button |
| `hybrid_force` | Word count force-split threshold |

## Implementation Strategy

Modify the existing `tokio::select!` in the detection loop to add a sensor watch channel arm (same approach as P0 shadow fix). Track `sensor_absent_since` as a local variable in the loop. The sensor arm is guarded by `if sensor_available` so it's disabled when sensor is absent.

Key behavioral difference from pure sensor mode: sensor transitions **wake** the LLM check rather than **force-split**. The `sensor_triggered` flag is handled differently:
- Sensor mode: `sensor_triggered = true` → force-split (bypass LLM)
- Hybrid mode: `sensor_triggered = true` → run LLM check immediately

## Simulated Impact on Today's Data

| Encounter | Issue | Hybrid Result |
|-----------|-------|---------------|
| 1→2 | LLM took 45 min | Sensor +1 min, LLM confirms +1.5 min. **43 min faster** |
| 3→4+5 | LLM failed, force-split merged 2 encs | Sensor triggers, LLM confirms. **Correctly split** |
| 5→6 | LLM took 46 min on empty buffer | Sensor triggers, 3 min timeout → split. **44 min faster** |
| Mid-enc 9 | 5.7 min departure | Sensor triggers, LLM says "no split" → cancelled. **False split prevented** |
| 7→8 | Back-to-back (chatter) | LLM timer detects. **No change** (P1 clinical check flags these) |

**Net: 4 improved, 1 false split prevented, 7 unchanged, 0 degraded.**

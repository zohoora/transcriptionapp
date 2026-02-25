# Hybrid Encounter Detection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a hybrid encounter detection mode that combines sensor early-warning with LLM content-based confirmation, gracefully falling back to LLM-only when the sensor is unavailable.

**Architecture:** The hybrid mode adds a 4th `EncounterDetectionMode::Hybrid` variant. In the detection loop, the `tokio::select!` gains a sensor watch channel arm alongside the existing timer/silence/manual arms. Sensor Present→Absent transitions immediately wake the LLM check (accelerating detection from ~8 min to ~30s). A sensor absence timeout (3 min) force-splits when LLM is slow. If the sensor fails or disconnects, the loop transparently degrades to pure LLM mode.

**Tech Stack:** Rust (Tauri v2), tokio async, watch channels, React TypeScript frontend

---

### Task 1: Add `Hybrid` variant to `EncounterDetectionMode` enum

**Files:**
- Modify: `src-tauri/src/config.rs:24-40` (enum + Display)

**Step 1: Add the Hybrid variant**

In `EncounterDetectionMode` enum (line 28), add `Hybrid` after `Shadow`:

```rust
pub enum EncounterDetectionMode {
    Llm,
    Sensor,
    Shadow,
    Hybrid,
}
```

In the `Display` impl (line 38), add:
```rust
EncounterDetectionMode::Hybrid => write!(f, "hybrid"),
```

**Step 2: Update default to Hybrid**

Change `default_encounter_detection_mode()` (line 190-192) to return `EncounterDetectionMode::Hybrid`.

**Step 3: Add validation for Hybrid mode**

In `Settings::validate()` (config.rs:547), update the sensor port validation to also apply to Hybrid:
```rust
// Sensor-only mode requires a sensor port; Hybrid gracefully falls back without it
if self.encounter_detection_mode == EncounterDetectionMode::Sensor && self.presence_sensor_port.is_empty() {
```
(No change needed — Hybrid doesn't require a sensor port, it falls back gracefully.)

**Step 4: Run test to verify compilation**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo check`
Expected: Should compile with 0 errors (existing tests may have warnings about unhandled variants)

**Step 5: Commit**

```bash
git add src-tauri/src/config.rs
git commit -m "feat: add Hybrid variant to EncounterDetectionMode enum"
```

---

### Task 2: Add hybrid config fields

**Files:**
- Modify: `src-tauri/src/config.rs` (Settings struct, defaults, clamp, test fixtures)

**Step 1: Add fields to Settings struct**

After `shadow_csv_log_enabled` (line 172), add:

```rust
// Hybrid detection settings (sensor accelerates LLM)
#[serde(default = "default_hybrid_confirm_window_secs")]
pub hybrid_confirm_window_secs: u64,
#[serde(default = "default_hybrid_min_words_for_sensor_split")]
pub hybrid_min_words_for_sensor_split: usize,
```

**Step 2: Add default functions**

After `default_shadow_csv_log_enabled()` (line 188), add:

```rust
fn default_hybrid_confirm_window_secs() -> u64 {
    180 // 3 minutes — sensor timeout before force-split
}

fn default_hybrid_min_words_for_sensor_split() -> usize {
    500 // Minimum words for sensor timeout to force-split
}
```

**Step 3: Add clamping**

In `clamp_values()` (after line 731), add:

```rust
// Hybrid: confirm window 30-600 seconds
self.hybrid_confirm_window_secs = self.hybrid_confirm_window_secs.clamp(30, 600);

// Hybrid: min words 100-5000
self.hybrid_min_words_for_sensor_split = self.hybrid_min_words_for_sensor_split.clamp(100, 5000);
```

**Step 4: Update Config::default() field list**

In `Config::default()` (around line 376), add the new fields to the Settings initializer.

**Step 5: Update ALL Settings struct literals in tests**

Add to every `Settings { ... }` literal in config.rs tests (~3 locations) and command_tests.rs (~1 location):
```rust
hybrid_confirm_window_secs: default_hybrid_confirm_window_secs(),
hybrid_min_words_for_sensor_split: default_hybrid_min_words_for_sensor_split(),
```

**Step 6: Run tests**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo test config`
Expected: All config tests pass

**Step 7: Commit**

```bash
git add src-tauri/src/config.rs src-tauri/src/command_tests.rs
git commit -m "feat: add hybrid detection config fields (confirm_window, min_words)"
```

---

### Task 3: Add hybrid config tests

**Files:**
- Modify: `src-tauri/src/config.rs` (tests module)

**Step 1: Write test for hybrid mode serialization**

```rust
#[test]
fn test_hybrid_detection_mode_round_trip() {
    let mut config = Config::default();
    config.encounter_detection_mode = EncounterDetectionMode::Hybrid;
    config.hybrid_confirm_window_secs = 120;
    config.hybrid_min_words_for_sensor_split = 300;

    let settings = config.to_settings();
    assert_eq!(settings.encounter_detection_mode, EncounterDetectionMode::Hybrid);
    assert_eq!(settings.hybrid_confirm_window_secs, 120);
    assert_eq!(settings.hybrid_min_words_for_sensor_split, 300);

    let json = serde_json::to_string(&settings).unwrap();
    let deserialized: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.encounter_detection_mode, EncounterDetectionMode::Hybrid);
}
```

**Step 2: Write test for hybrid field clamping**

```rust
#[test]
fn test_hybrid_config_clamping() {
    let mut config = Config::default();
    config.hybrid_confirm_window_secs = 5; // Below minimum (30)
    config.hybrid_min_words_for_sensor_split = 10; // Below minimum (100)
    config.clamp_values();
    assert_eq!(config.hybrid_confirm_window_secs, 30);
    assert_eq!(config.hybrid_min_words_for_sensor_split, 100);

    config.hybrid_confirm_window_secs = 9999; // Above maximum (600)
    config.hybrid_min_words_for_sensor_split = 99999; // Above maximum (5000)
    config.clamp_values();
    assert_eq!(config.hybrid_confirm_window_secs, 600);
    assert_eq!(config.hybrid_min_words_for_sensor_split, 5000);
}
```

**Step 3: Write test for hybrid mode does NOT require sensor port**

```rust
#[test]
fn test_hybrid_mode_no_sensor_port_is_valid() {
    let mut settings = Settings::default();
    settings.encounter_detection_mode = EncounterDetectionMode::Hybrid;
    settings.presence_sensor_port = String::new();
    // Hybrid mode should be valid without sensor port (graceful fallback)
    let errors: Vec<_> = settings.validate().into_iter()
        .filter(|e| e.field == "presence_sensor_port")
        .collect();
    assert!(errors.is_empty());
}
```

**Step 4: Run tests**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo test config`
Expected: All pass

**Step 5: Commit**

```bash
git add src-tauri/src/config.rs
git commit -m "test: add hybrid detection mode config tests"
```

---

### Task 4: Implement hybrid detection loop in continuous_mode.rs

**Files:**
- Modify: `src-tauri/src/continuous_mode.rs` — sensor init (~425-493), detection loop (~883-909), detection result (~970), force-split (~1039-1091), metadata (~1224-1228)

This is the core task. It modifies 5 sections of continuous_mode.rs.

**Step 1: Update sensor initialization to include Hybrid mode**

At line 426-430, update `use_sensor_mode` to include Hybrid:

```rust
let is_shadow_mode = config.encounter_detection_mode == EncounterDetectionMode::Shadow;
let is_hybrid_mode = config.encounter_detection_mode == EncounterDetectionMode::Hybrid;
let shadow_active_method = config.shadow_active_method.clone();
let use_sensor_mode = (config.encounter_detection_mode == EncounterDetectionMode::Sensor
    || (is_shadow_mode && !config.presence_sensor_port.is_empty())
    || (is_hybrid_mode && !config.presence_sensor_port.is_empty()))
    && !config.presence_sensor_port.is_empty();
```

**Step 2: Get a dedicated watch receiver for hybrid mode**

After `shadow_sensor_state_rx = Some(sensor.subscribe_state())` (line 469), add:

```rust
// Get dedicated state receiver for hybrid detection loop
let mut hybrid_sensor_state_rx: Option<tokio::sync::watch::Receiver<crate::presence_sensor::PresenceState>> = None;
```

In the `Ok(sensor)` arm (around line 469), add:
```rust
if is_hybrid_mode {
    hybrid_sensor_state_rx = Some(sensor.subscribe_state());
}
```

**Step 3: Compute effective_sensor_mode for hybrid**

Update effective_sensor_mode (line 489-493):
```rust
let effective_sensor_mode = if is_shadow_mode {
    shadow_active_method == ShadowActiveMethod::Sensor && sensor_handle.is_some()
} else if is_hybrid_mode {
    false // Hybrid doesn't use the pure sensor detection path
} else {
    sensor_handle.is_some()
};
```

**Step 4: Start sensor monitor task for hybrid mode too**

The sensor monitor task (line 496) only spawns when `effective_sensor_mode`. Change it to also spawn for hybrid when sensor is available:
```rust
let sensor_monitor_task: Option<tokio::task::JoinHandle<()>> = if effective_sensor_mode || (is_hybrid_mode && sensor_handle.is_some()) {
```

**Step 5: Pass hybrid config values into detection task**

Near lines 817-820 where detection config is prepared, add:
```rust
let hybrid_confirm_window_secs = config.hybrid_confirm_window_secs;
let hybrid_min_words_for_sensor_split = config.hybrid_min_words_for_sensor_split;
```

**Step 6: Add hybrid tracking state in the detection loop**

Inside the detector_task (after line 879), add:
```rust
// Hybrid mode: sensor absence tracking
let mut sensor_absent_since: Option<DateTime<Utc>> = None;
let mut prev_sensor_state = crate::presence_sensor::PresenceState::Unknown;
let mut sensor_available = hybrid_sensor_state_rx.is_some();
```

Move `hybrid_sensor_state_rx` into the detector task via clone/move.

**Step 7: Add hybrid select! branch to detection loop**

Replace the detection loop select! (lines 883-909) with a 3-way match:

```rust
let (manual_triggered, sensor_triggered) = if is_hybrid_mode {
    // Hybrid mode: timer + silence + manual + sensor state changes
    if sensor_available {
        let sensor_rx = hybrid_sensor_rx.as_mut().unwrap();
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => {
                // Regular timer check — handles back-to-back encounters without sensor
                (false, false)
            }
            _ = silence_trigger_for_detector.notified() => {
                info!("Hybrid: silence gap detected — triggering encounter check");
                (false, false)
            }
            _ = manual_trigger_rx.notified() => {
                info!("Manual new patient trigger received");
                (true, false)
            }
            result = sensor_rx.changed() => {
                match result {
                    Ok(()) => {
                        let new_state = *sensor_rx.borrow_and_update();
                        let old_state = prev_sensor_state;
                        prev_sensor_state = new_state;
                        match (old_state, new_state) {
                            (crate::presence_sensor::PresenceState::Present,
                             crate::presence_sensor::PresenceState::Absent) => {
                                sensor_absent_since = Some(Utc::now());
                                info!("Hybrid: sensor detected departure (Present→Absent), accelerating LLM check");
                                (false, true)  // sensor_triggered = true → accelerate LLM check
                            }
                            (_, crate::presence_sensor::PresenceState::Present) => {
                                if sensor_absent_since.is_some() {
                                    info!("Hybrid: person returned — cancelling sensor absence tracking");
                                    sensor_absent_since = None;
                                }
                                continue;  // Return to recording, no check needed
                            }
                            _ => continue,  // Other transitions, ignore
                        }
                    }
                    Err(_) => {
                        warn!("Hybrid: sensor watch channel closed — sensor disconnected. Falling back to LLM-only.");
                        sensor_available = false;
                        sensor_absent_since = None;
                        let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                            "type": "sensor_status",
                            "connected": false,
                            "state": "unknown"
                        }));
                        continue;  // Re-enter loop; next iteration uses sensor_available=false path
                    }
                }
            }
        }
    } else {
        // Hybrid without sensor (sensor failed or disconnected): pure LLM fallback
        let manual = tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => false,
            _ = silence_trigger_for_detector.notified() => {
                info!("Hybrid (LLM fallback): silence gap detected — triggering encounter check");
                false
            }
            _ = manual_trigger_rx.notified() => {
                info!("Manual new patient trigger received");
                true
            }
        };
        (manual, false)
    }
} else if effective_sensor_mode {
    // Pure sensor mode (existing)
    tokio::select! {
        _ = sensor_trigger_for_detector.notified() => {
            info!("Sensor: absence threshold reached — triggering encounter split");
            (false, true)
        }
        _ = manual_trigger_rx.notified() => {
            info!("Manual new patient trigger received");
            (true, false)
        }
    }
} else {
    // LLM / Shadow mode (existing)
    let manual = tokio::select! {
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => false,
        _ = silence_trigger_for_detector.notified() => {
            info!("Silence gap detected — triggering encounter check");
            false
        }
        _ = manual_trigger_rx.notified() => {
            info!("Manual new patient trigger received");
            true
        }
    };
    (manual, false)
};
```

**Step 8: Modify detection_result to NOT force-split for hybrid sensor triggers**

At line 970, change the condition to exclude hybrid sensor triggers:
```rust
let detection_result = if manual_triggered || (sensor_triggered && !is_hybrid_mode) {
    // Force split for manual trigger or pure sensor mode (NOT hybrid)
    ...
} else if let Some(ref client) = llm_client {
    // Run LLM check (including hybrid sensor-accelerated checks)
    ...
```

**Step 9: Add sensor timeout force-split check**

After the `force_split` logic block (around line 1091), add:

```rust
// Hybrid sensor timeout force-split
if is_hybrid_mode && !force_split && !manual_triggered {
    if let Some(absent_since) = sensor_absent_since {
        let elapsed = (Utc::now() - absent_since).num_seconds() as u64;
        if elapsed >= hybrid_confirm_window_secs
            && word_count >= hybrid_min_words_for_sensor_split
        {
            warn!(
                "Hybrid: sensor absence timeout ({}s ≥ {}s) with {} words ≥ {} — force-splitting",
                elapsed, hybrid_confirm_window_secs,
                word_count, hybrid_min_words_for_sensor_split
            );
            let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
            force_split = true;
            sensor_absent_since = None;
            detection_result = Some(EncounterDetectionResult {
                complete: true,
                end_segment_index: last_idx,
                confidence: Some(1.0),
            });
        }
    }
}
```

**Step 10: Clear sensor_absent_since on successful split**

After `consecutive_no_split = 0` (line 1116) where a successful split is processed, add:
```rust
// Clear hybrid sensor tracking on successful split
if is_hybrid_mode {
    sensor_absent_since = None;
}
```

**Step 11: Update detection_method metadata**

Replace lines 1224-1228 with:
```rust
metadata.detection_method = Some(
    if manual_triggered {
        "manual".to_string()
    } else if is_hybrid_mode {
        if sensor_triggered && !force_split {
            "hybrid_sensor_confirmed".to_string()
        } else if force_split && sensor_absent_since.is_none() {
            // sensor_absent_since was cleared by the timeout path
            "hybrid_sensor_timeout".to_string()
        } else if force_split {
            "hybrid_force".to_string()
        } else {
            "hybrid_llm".to_string()
        }
    } else if sensor_triggered {
        "sensor".to_string()
    } else {
        "llm".to_string()
    }
);
```

Note: The detection_method tracking for hybrid sensor timeout needs a tracking flag since `sensor_absent_since` gets cleared. Add a `let mut hybrid_sensor_timeout_triggered = false;` flag that's set in step 9 and checked here.

**Step 12: Run compilation check**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo check`
Expected: 0 errors

**Step 13: Commit**

```bash
git add src-tauri/src/continuous_mode.rs
git commit -m "feat: implement hybrid detection loop (sensor + LLM)"
```

---

### Task 5: Add unit tests for hybrid detection logic

**Files:**
- Modify: `src-tauri/src/continuous_mode.rs` (test module or new test file)

Since the detection loop is async and tightly coupled to tokio runtime + LLM client, test the component pieces:

**Step 1: Test sensor-triggered does not force-split in hybrid mode**

Add a test that verifies the `is_hybrid_mode` flag prevents sensor triggers from bypassing LLM:

```rust
#[test]
fn test_hybrid_sensor_trigger_does_not_force_split() {
    // Verify that the condition `sensor_triggered && !is_hybrid_mode`
    // correctly prevents force-split for hybrid mode
    let sensor_triggered = true;
    let manual_triggered = false;
    let is_hybrid_mode = true;

    let should_force = manual_triggered || (sensor_triggered && !is_hybrid_mode);
    assert!(!should_force, "Hybrid mode should NOT force-split on sensor trigger");

    let is_hybrid_mode = false;
    let should_force = manual_triggered || (sensor_triggered && !is_hybrid_mode);
    assert!(should_force, "Pure sensor mode SHOULD force-split on sensor trigger");
}
```

**Step 2: Test sensor timeout force-split logic**

```rust
#[test]
fn test_hybrid_sensor_timeout_logic() {
    use chrono::{Duration, Utc};

    let confirm_window_secs: u64 = 180;
    let min_words: usize = 500;

    // Case 1: Timeout exceeded with enough words → should force-split
    let absent_since = Utc::now() - Duration::seconds(200);
    let word_count = 600;
    let elapsed = (Utc::now() - absent_since).num_seconds() as u64;
    assert!(elapsed >= confirm_window_secs && word_count >= min_words);

    // Case 2: Timeout exceeded but not enough words → should NOT force-split
    let word_count = 100;
    assert!(!(elapsed >= confirm_window_secs && word_count >= min_words));

    // Case 3: Enough words but timeout not exceeded → should NOT force-split
    let absent_since = Utc::now() - Duration::seconds(60);
    let word_count = 600;
    let elapsed = (Utc::now() - absent_since).num_seconds() as u64;
    assert!(!(elapsed >= confirm_window_secs && word_count >= min_words));
}
```

**Step 3: Test detection method string assignment**

```rust
#[test]
fn test_hybrid_detection_method_strings() {
    // Verify all detection method strings are correct
    let cases = vec![
        (true, false, false, false, false, "manual"),
        (false, true, false, true, false, "hybrid_sensor_confirmed"),
        (false, false, false, true, true, "hybrid_sensor_timeout"),
        (false, false, true, true, false, "hybrid_force"),
        (false, false, false, true, false, "hybrid_llm"),
        (false, true, false, false, false, "sensor"),
        (false, false, false, false, false, "llm"),
    ];

    for (manual, sensor, force, hybrid, sensor_timeout, expected) in cases {
        let method = if manual {
            "manual"
        } else if hybrid {
            if sensor && !force {
                "hybrid_sensor_confirmed"
            } else if sensor_timeout {
                "hybrid_sensor_timeout"
            } else if force {
                "hybrid_force"
            } else {
                "hybrid_llm"
            }
        } else if sensor {
            "sensor"
        } else {
            "llm"
        };
        assert_eq!(method, expected, "Failed for manual={manual}, sensor={sensor}, force={force}, hybrid={hybrid}, sensor_timeout={sensor_timeout}");
    }
}
```

**Step 4: Test sensor state transition tracking**

```rust
#[test]
fn test_hybrid_sensor_state_transitions() {
    use crate::presence_sensor::PresenceState;

    // Present→Absent should trigger tracking
    let old = PresenceState::Present;
    let new = PresenceState::Absent;
    let triggers_check = matches!((old, new), (PresenceState::Present, PresenceState::Absent));
    assert!(triggers_check);

    // Absent→Present should cancel tracking
    let old = PresenceState::Absent;
    let new = PresenceState::Present;
    let cancels = matches!((_, new), (_, PresenceState::Present));
    assert!(cancels);

    // Unknown→Present should also cancel (if tracking was set)
    let old = PresenceState::Unknown;
    let new = PresenceState::Present;
    let cancels = matches!((old, new), (_, PresenceState::Present));
    assert!(cancels);

    // Absent→Absent should not trigger anything
    let old = PresenceState::Absent;
    let new = PresenceState::Absent;
    let triggers = matches!((old, new), (PresenceState::Present, PresenceState::Absent));
    let cancels = matches!((old, new), (_, PresenceState::Present));
    assert!(!triggers && !cancels);
}
```

**Step 5: Test hybrid fallback when sensor unavailable**

```rust
#[test]
fn test_hybrid_sensor_available_flag() {
    // When sensor_available is false, hybrid should behave like LLM mode
    let sensor_available = false;
    let is_hybrid = true;

    // The select! branch would use the "sensor_available" path
    // Verify the logic: hybrid + no sensor = LLM fallback
    let uses_sensor_arm = is_hybrid && sensor_available;
    assert!(!uses_sensor_arm);

    let uses_llm_fallback = is_hybrid && !sensor_available;
    assert!(uses_llm_fallback);
}
```

**Step 6: Run tests**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo test hybrid`
Expected: All new tests pass

**Step 7: Commit**

```bash
git add src-tauri/src/continuous_mode.rs
git commit -m "test: add hybrid detection unit tests"
```

---

### Task 6: Update frontend types

**Files:**
- Modify: `src/types/index.ts` (~line 680)
- Modify: `src/components/SettingsDrawer.tsx` (~lines 166-219)

**Step 1: Update EncounterDetectionMode type**

In `types/index.ts`, change:
```typescript
export type EncounterDetectionMode = 'llm' | 'sensor' | 'shadow' | 'hybrid';
```

**Step 2: Add hybrid config fields to Settings interface**

After `shadow_csv_log_enabled`:
```typescript
// Hybrid detection settings
hybrid_confirm_window_secs: number;
hybrid_min_words_for_sensor_split: number;
```

**Step 3: Add Hybrid button to SettingsDrawer**

After the Shadow button (line 185), add:
```tsx
<button
  className={`charting-mode-btn ${pendingSettings.encounter_detection_mode === 'hybrid' ? 'active' : ''}`}
  onClick={() => onSettingsChange({ ...pendingSettings, encounter_detection_mode: 'hybrid' })}
>
  Hybrid
</button>
```

**Step 4: Show sensor port input for Hybrid mode too**

Update the condition on line 219 to include hybrid:
```tsx
{(pendingSettings.encounter_detection_mode === 'sensor' || pendingSettings.encounter_detection_mode === 'shadow' || pendingSettings.encounter_detection_mode === 'hybrid') && (
```

**Step 5: Run TypeScript check**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app && npx tsc --noEmit`
Expected: 0 errors

**Step 6: Commit**

```bash
git add src/types/index.ts src/components/SettingsDrawer.tsx
git commit -m "feat: add hybrid detection mode to frontend types and settings UI"
```

---

### Task 7: Update frontend tests

**Files:**
- Check/modify any test files that construct Settings or use EncounterDetectionMode

**Step 1: Search for test fixtures that need updating**

Search for `encounter_detection_mode` in test files and `hybrid_confirm_window` patterns.

**Step 2: Update any test fixtures with new fields**

Add `hybrid_confirm_window_secs` and `hybrid_min_words_for_sensor_split` with defaults to any mocked Settings objects.

**Step 3: Run frontend tests**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app && pnpm test:run`
Expected: All 414 tests pass

**Step 4: Commit if changes needed**

```bash
git add src/
git commit -m "test: update frontend test fixtures for hybrid detection fields"
```

---

### Task 8: Run full verification suite

**Step 1: Cargo check (0 warnings)**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo check 2>&1`
Expected: 0 warnings, 0 errors

**Step 2: TypeScript check**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app && npx tsc --noEmit`
Expected: 0 errors

**Step 3: Full Rust test suite**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo test 2>&1`
Expected: All pass (previously 524), 0 failures

**Step 4: Frontend test suite**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app && pnpm test:run`
Expected: All 414 tests pass

**Step 5: E2E test suite (layers 1-6)**

Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo test e2e_layer1 -- --ignored --nocapture`
Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo test e2e_layer2 -- --ignored --nocapture`
Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo test e2e_layer3 -- --ignored --nocapture`
Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo test e2e_layer4 -- --ignored --nocapture`
Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo test e2e_layer5 -- --ignored --nocapture`
Run: `cd /Users/backoffice/transcriptionapp/tauri-app/src-tauri && cargo test e2e_layer6 -- --ignored --nocapture`
Expected: All 14 E2E tests pass (32 total including ignored)

---

### Task 9: Update documentation

**Files:**
- Modify: `CLAUDE.md` (Settings Schema, Features table)
- Modify: `memory/MEMORY.md`

**Step 1: Update CLAUDE.md Settings Schema**

Add hybrid settings to the Settings Schema section. Update the encounter_detection_mode default to "hybrid".

**Step 2: Update CLAUDE.md Features table**

Update the Continuous Mode and Presence Sensor feature rows to mention hybrid mode.

**Step 3: Update MEMORY.md**

Add a section about hybrid detection mode with key details: sensor acceleration, LLM confirmation, timeout force-split, graceful fallback.

**Step 4: Commit**

```bash
git add tauri-app/CLAUDE.md memory/MEMORY.md
git commit -m "docs: document hybrid detection mode in CLAUDE.md and MEMORY.md"
```

---

## Implementation Order Summary

1. **Task 1**: Add `Hybrid` enum variant (config.rs)
2. **Task 2**: Add hybrid config fields + defaults + clamping (config.rs)
3. **Task 3**: Add config unit tests (config.rs)
4. **Task 4**: Implement hybrid detection loop (continuous_mode.rs) — **core task**
5. **Task 5**: Add detection logic unit tests (continuous_mode.rs)
6. **Task 6**: Update frontend types + settings UI
7. **Task 7**: Update frontend tests
8. **Task 8**: Full verification suite
9. **Task 9**: Update documentation

## Key Design Decisions

- **Sensor acceleration, not sensor force-split**: In hybrid mode, `sensor_triggered=true` runs an LLM check, NOT a force-split (unlike pure sensor mode)
- **Graceful degradation**: If sensor fails/disconnects, `sensor_available=false` and the loop becomes identical to LLM mode
- **No automatic reconnection**: If sensor disconnects mid-session (USB unplugged), fall back to LLM. User would need to replug and restart.
- **Back-to-back encounters**: Handled entirely by the LLM timer path — no sensor involvement needed
- **sensor_absent_since tracking**: Local variable in detection loop; cleared on: person returns, successful split, sensor timeout, sensor disconnect

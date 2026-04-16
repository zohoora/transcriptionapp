# ADR-0025: Multi-Sensor Presence Suite

## Status

Accepted (Apr 2026)

## Context

ADR-0024 (Hybrid Encounter Detection) needs an independent "is a patient in the room" signal. We evaluated options:

| Option | Reason rejected |
|--------|-----------------|
| Audio-only (VAD gap detection) | Unreliable — physicians work in silence (chart reading), patients wait quietly. |
| Motion sensor (PIR) | False positives from physician movement; no "room empty" signal. |
| Camera + CV | Privacy concerns in a clinical setting. |
| Pressure pad | Intrusive, unreliable with office chairs. |

We settled on **mmWave presence + thermal + CO2 fusion**, packaged as an ESP32 peripheral on the clinic LAN. mmWave is the primary signal (micro-motion-aware, works through clothing and light obstacles). Thermal (AMG8833 / MLX90640-style) and CO2 (MH-Z19C) are additional fusion inputs — currently informational, Phase 2 will weight them.

## Decision

Hardware: an Adafruit ESP32 Feather V2 with 3 sensors (mmWave, thermal, CO2) exposes an HTTP endpoint on the room's LAN. Firmware lives in `room6-xiao-sensor/` (Arduino) and `esp32-presence/` (PlatformIO alternative).

Software: a modular `presence_sensor/` directory with a `SensorSource` trait so real-world and mock sources are interchangeable in tests.

### Module layout

```
presence_sensor/
├── mod.rs               # PresenceSensorSuite (public type, alias PresenceSensor)
├── types.rs             # PresenceState, SensorData, SensorHealth
├── sensor_source.rs     # SensorSource trait
├── sources/
│   ├── esp32_http.rs    # Production: polls ESP32 over HTTP at configured URL
│   ├── serial.rs        # Alternative: XIAO ESP32-C3 over USB serial
│   └── mock.rs          # Test: scripted state transitions
├── debounce.rs          # DebounceFsm — N consecutive reads before state change
├── thermal.rs           # Hot-pixel count from 8×8 or 32×24 grid
├── co2.rs               # CO2 delta vs rolling baseline
├── fusion.rs            # Combines signals into PresenceState
├── absence_monitor.rs   # "Sustained absence" tracker for hybrid mode
└── csv_logger.rs        # Per-room sensor reading log (shadow analysis)
```

### Fusion policy (current)

**mmWave-only passthrough.** Thermal and CO2 readings are logged to CSV for offline analysis but don't influence `PresenceState`. This is a deliberate conservative choice — each sensor needs per-room calibration (ambient temperature, airflow, occupancy baseline) before fusion is trustworthy. See `presence_sensor/co2_calibration.rs` for the baseline-learning routine.

Phase 2 work: weighted fusion with per-room calibration profiles. Tracker in `project_room_calibration.md`.

### State machine

```
      Present ─ debounce N reads ─▶ Present
         ▲                              │
         │                         sensor goes Absent
         │                              │
         │                              ▼
     Absent  ◀─ debounce M reads ─  Absent (pending)
```

Debounce is asymmetric (typical: 2 reads to confirm Absent, 1 read to confirm Present) because missing a patient arrival is worse than missing a patient departure.

### Output channel

`PresenceSensorSuite` exposes a `tokio::sync::watch::Receiver<PresenceState>`. Continuous mode and shadow mode each subscribe; new subscribers get the current value immediately. `watch::changed().err()` → `sensor_available=false` for graceful degradation.

### ESP32 endpoints

| Endpoint | Payload |
|----------|---------|
| `GET /` | JSON with all three sensors — `{"mmwave": bool, "thermal": [float×pixels], "co2_ppm": int, "timestamp": int}` |
| `GET /thermal` | Raw thermal array (debugging) |

Firmware is a minimal Arduino sketch on WiFi — no TLS, LAN-only. Room config stores the URL in `room_config.json`.

### Calibration

CO2 baseline drifts with HVAC, seasonal airflow, and office occupancy. `CO2Calibration` tool learns the baseline over a ≥30-minute quiet period and writes `~/.transcriptionapp/co2_baseline.json`. The calibration is surfaced in the admin panel.

Thermal hot-pixel threshold (default 28.0°C, clamp 20–40°C) is room-local — sunny rooms need a higher threshold.

## Consequences

### Positive

- **Independent signal** — sensor presence is not correlated with transcript content, so LLM + sensor combined reduce the same errors neither can catch alone.
- **Privacy-preserving** — no video, no audio beyond what the app already captures.
- **Modular** — `SensorSource` trait lets tests inject scripted transitions; a second hardware variant (USB serial XIAO) dropped in without touching the main pipeline.
- **Degrades gracefully** — sensor failure reverts to LLM-only without any mode switch in continuous mode.

### Negative

- **Requires hardware deployment** — a physical sensor per room with network reachability. Not all rooms have one yet.
- **Calibration burden** — CO2 baseline and thermal threshold need tuning per room. The app works without fusion (mmWave-only) until calibration exists.
- **LAN dependency** — if WiFi is down the sensor appears "unavailable" and hybrid mode degrades to LLM-only. Acceptable because the profile service and STT/LLM routers have the same dependency.

## References

- `tauri-app/src-tauri/src/presence_sensor/` — module directory
- `room6-xiao-sensor/` — XIAO ESP32-C3 firmware (Arduino, USB serial)
- `esp32-presence/` — ESP32 Feather V2 firmware (PlatformIO, WiFi HTTP)
- `tauri-app/src-tauri/src/co2_calibration.rs` — baseline learner
- `tauri-app/src-tauri/src/commands/calibration.rs` — Tauri commands for the admin panel
- ADR-0024: Hybrid Encounter Detection — consumer of this signal

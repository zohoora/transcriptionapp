---
name: preflight
description: Run daily clinic preflight checks to verify STT Router, LLM Router, archive, and full pipeline health
user-invocable: true
disable-model-invocation: true
arguments:
  - name: level
    description: "Check level: quick (layers 1-3, ~10s) or full (all 5 layers, ~30s)"
    default: quick
---

# Daily Preflight Check

Verify all services are healthy before clinic starts. This wraps the E2E test layers.

## Quick Check (default)

Runs layers 1-3: STT Router, LLM Router, Local Archive. No audio pipeline needed.

```bash
cd tauri-app/src-tauri

# Layer 1: STT Router health + WebSocket streaming
cargo test e2e_layer1 -- --ignored --nocapture 2>&1

# Layer 2: LLM Router SOAP + encounter detection
cargo test e2e_layer2 -- --ignored --nocapture 2>&1

# Layer 3: Local archive save/retrieve
cargo test e2e_layer3 -- --ignored --nocapture 2>&1
```

## Full Check (level=full)

Also runs layers 4-5: full session and continuous mode pipelines.

```bash
cd tauri-app/src-tauri

# Layer 4: Session mode full pipeline (Audio -> STT -> SOAP -> Archive)
cargo test e2e_layer4 -- --ignored --nocapture 2>&1

# Layer 5: Continuous mode full pipeline (Audio -> STT -> Detection -> SOAP -> Archive)
cargo test e2e_layer5 -- --ignored --nocapture 2>&1
```

## Run each layer sequentially

Run layers one at a time — concurrent WebSocket streams can overload the STT Router.

## Interpreting Results

| Layer | Failure Meaning | Fix |
|-------|----------------|-----|
| 1 - STT Router | STT Router down or unreachable | Check `http://10.241.15.154:8001/health` |
| 1 - Streaming | WebSocket connection failed | Check STT Router logs, restart if needed |
| 2 - SOAP empty | LLM Router down or model not loaded | Check `http://10.241.15.154:8080/health` |
| 2 - Detection | Encounter detection model regression | Run `cargo run --bin encounter_experiment_cli` to investigate |
| 3 - Archive | Disk permissions or path issue | Check `~/.transcriptionapp/archive/` is writable |
| 4 - Session | Full pipeline failure | Diagnose from Layer 1-3 results first |
| 5 - Continuous | Continuous mode pipeline failure | Check encounter detection + SOAP generation separately |

## Report Format

After running, summarize:
1. Which layers passed/failed
2. For failures: the specific error and suggested fix
3. Overall verdict: READY or NOT READY for clinic

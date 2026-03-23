---
name: run-experiment
description: Run encounter detection replay, encounter experiments, or vision experiments against archived sessions
user-invocable: true
disable-model-invocation: true
arguments:
  - name: tool
    description: "Which experiment CLI: replay, encounter, or vision"
    required: true
  - name: date
    description: "Archive date (YYYY-MM-DD) or 'all' for full archive"
    default: all
  - name: flags
    description: "Additional flags (e.g. --mismatches, --override hybrid_confirm_window_secs=120, --model fast-model)"
    default: ""
---

# Run Experiment CLI

Wrapper for the three offline experiment/replay CLIs. These tools replay archived decisions for what-if analysis and regression testing.

## Tools

### replay (detection_replay_cli)

Replays archived encounter detection decisions through `evaluate_detection()` and compares to actual outcomes. Supports parameter overrides for what-if tuning.

```bash
cd tauri-app/src-tauri

# Replay specific date
cargo run --bin detection_replay_cli -- ~/.transcriptionapp/archive/2026/03/17/

# Replay all archived decisions
cargo run --bin detection_replay_cli -- --all

# Show only mismatches (where replay differs from actual)
cargo run --bin detection_replay_cli -- --all --mismatches

# What-if: test different parameters
cargo run --bin detection_replay_cli -- --all --override hybrid_confirm_window_secs=120
cargo run --bin detection_replay_cli -- --all --override min_sensor_hybrid_words=300
cargo run --bin detection_replay_cli -- --all --override merge_back_count=0
```

**Override parameters:**
- `hybrid_confirm_window_secs` — sensor timeout before force-split (default 180)
- `hybrid_min_words_for_sensor_split` — minimum words for sensor timeout force-split (default 500)
- `merge_back_count` — override merge-back count (affects confidence threshold)
- `min_sensor_hybrid_words` — minimum words for sensor-triggered LLM check in hybrid mode (default 500)

### encounter (encounter_experiment_cli)

Replays archived transcripts through different detection prompts for accuracy comparison.

```bash
cd tauri-app/src-tauri

# Run all experiments
cargo run --bin encounter_experiment_cli

# Use specific model
cargo run --bin encounter_experiment_cli -- --model fast-model

# Run specific prompt variants only
cargo run --bin encounter_experiment_cli -- --detect-only p0 p3
```

### vision (vision_experiment_cli)

Compares vision-based SOAP generation strategies across archived sessions.

```bash
cd tauri-app/src-tauri
cargo run --bin vision_experiment_cli
```

## Interpreting Results

### Replay output
- `Match` = replay agrees with actual decision
- `Mismatch` = replay would have decided differently
- `Skipped(sensor_precheck)` = check would now be skipped by the 500-word sensor guard
- Agreement % = overall replay accuracy (higher = production code matches archived decisions)

### What to look for
1. **High mismatch rate** → parameter change would affect many decisions (risky)
2. **Mismatches only on micro-splits** → parameter change correctly prevents bad splits (good)
3. **Mismatches on legitimate splits** → parameter too aggressive (back off)

## Report Format

After running, summarize:
1. Total bundles/checks and agreement percentage
2. Notable mismatches with context (word count, trigger type, actual vs replayed)
3. For what-if overrides: how the parameter change affects overall accuracy
4. Recommendation: whether the parameter change is safe to deploy

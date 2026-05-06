#!/bin/bash
# Daily Preflight Check — run before starting a clinic day
#
# Verifies all external services and the full transcription pipeline are working.
# Runs layered E2E tests so failures are easy to diagnose.
#
# Usage:
#   ./scripts/preflight.sh             # Quick check (layers 1-3, ~10s)
#   ./scripts/preflight.sh --full      # Full pipeline (all layers, ~30s)
#   ./scripts/preflight.sh --regression  # Offline regression corpus only
#                                        # (layers 6-9, no STT/LLM required).
#                                        # Used by the PR-side CI ratchet.
#   ./scripts/preflight.sh --layer 2   # Specific layer only
#
# Exit codes:
#   0 = All checks passed
#   1 = One or more checks failed (see output for details)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")/src-tauri"

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Parse arguments
MODE="quick"
LAYER=""
while [[ $# -gt 0 ]]; do
    case $1 in
        --full)        MODE="full"; shift ;;
        --regression)  MODE="regression"; shift ;;
        --layer)       LAYER="$2"; shift 2 ;;
        --help|-h)
            echo "Daily Preflight Check"
            echo ""
            echo "Usage: $0 [--full | --regression] [--layer N]"
            echo ""
            echo "Layers:"
            echo "  1  STT Router            — health, alias, WebSocket streaming"
            echo "  2  LLM Router            — SOAP generation, encounter detection, hybrid model"
            echo "  3  Local Archive         — save/retrieve, continuous mode metadata"
            echo "  4  Session Mode          — full Audio → STT → SOAP → Archive → History"
            echo "  5  Continuous            — full Audio → STT → Detection → SOAP → Archive"
            echo "  6  Detection Replay      — offline evaluate_detection replay"
            echo "  7  Golden Day            — labeled clinic days vs production archive"
            echo "  8  Harness               — orchestrator equivalence (run_continuous_mode snapshot)"
            echo "  9  Labeled Regression    — per-check label vs production with expected_failures baseline"
            echo ""
            echo "Modes:"
            echo "  (default)     Quick — layers 1, 2, 3 (connectivity) + 6, 7, 8, 9 (offline)"
            echo "  --full        All 9 layers (adds 4, 5 full pipeline)"
            echo "  --regression  Code-regression corpus only — layers 6, 8, 9"
            echo "                (no live STT/LLM required; used by PR-side CI ratchet)"
            echo "                Layer 7 (golden day) runs in --full because corpus drift"
            echo "                is clinical maintenance, not code regression."
            echo "  --layer N     Run only layer N"
            exit 0
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "========================================"
echo "  Preflight Check — $(date '+%Y-%m-%d %H:%M')"
echo "========================================"
echo ""

cd "$PROJECT_DIR"

# Build the lib-test binary upfront. Layers 1-5 invoke `cargo test --ignored`,
# which reuses this artifact. Skipped in --regression mode (layers 6, 8, 9
# build their own binaries via `cargo run`/`cargo test --test`).
if [[ "$MODE" != "regression" ]]; then
    echo -e "${YELLOW}Building tests...${NC}"
    cargo test --no-run --lib 2>&1 | grep -E "Compiling|Finished" || true
    echo ""
fi

FAILED=0
PASSED=0

# Returns 0 if the layer should run given $MODE and $LAYER.
# Gating values:
#   connectivity — runs in quick + full + explicit-N. Skipped in --regression.
#   full         — runs in --full + explicit-N only.
#   offline      — always runs (quick + full + --regression + explicit-N).
should_run() {
    local layer="$1" gating="$2"
    if [[ -n "$LAYER" ]]; then
        [[ "$LAYER" == "$layer" ]]
        return $?
    fi
    case "$gating" in
        connectivity) [[ "$MODE" != "regression" ]] ;;
        full)         [[ "$MODE" == "full" ]] ;;
        offline)      true ;;
        *)            echo "should_run: unknown gating '$gating'" >&2; return 1 ;;
    esac
}

run_test() {
    local test_name="$1"
    local description="$2"

    echo -n "  $description... "

    if OUTPUT=$(cargo test "$test_name" -- --ignored --nocapture 2>&1); then
        echo -e "${GREEN}PASS${NC}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}FAIL${NC}"
        echo "$OUTPUT" | grep -E "panicked|FAILED|Error|error" | head -5 | sed 's/^/    /'
        FAILED=$((FAILED + 1))
    fi
}

# Run a CLI-based layer: invokes a command, redirects to a log, prints PASS
# with `success_grep` lines on success, or FAIL with `fail_grep` lines (or
# `tail -10` if fail_grep is empty) on failure. Optional fail_hint appears
# below the failure tail.
#
# Usage: run_cli_layer LAYER NAME DESCRIPTION LOG_FILE SUCCESS_GREP FAIL_GREP FAIL_HINT -- CMD...
run_cli_layer() {
    local layer_num="$1" layer_name="$2" description="$3" log_file="$4"
    local success_grep="$5" fail_grep="$6" fail_hint="$7"
    shift 7
    [[ "$1" == "--" ]] && shift

    echo -e "${YELLOW}Layer ${layer_num}: ${layer_name}${NC}"
    echo -n "  ${description}... "
    if "$@" > "$log_file" 2>&1; then
        echo -e "${GREEN}PASS${NC}"
        grep -E "$success_grep" "$log_file" | sed 's/^/    /'
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}FAIL${NC}"
        if [[ -n "$fail_grep" ]]; then
            grep -E "$fail_grep" "$log_file" | head -10 | sed 's/^/    /'
        else
            tail -10 "$log_file" | sed 's/^/    /'
        fi
        [[ -n "$fail_hint" ]] && echo "    $fail_hint"
        FAILED=$((FAILED + 1))
    fi
    echo ""
}

if should_run 1 connectivity; then
    echo -e "${YELLOW}Layer 1: STT Router${NC}"
    run_test "e2e_layer1_stt_health_check" "Health check"
    run_test "e2e_layer1_stt_alias_available" "Alias 'medical-streaming'"
    run_test "e2e_layer1_stt_streaming_protocol" "WebSocket streaming"
    echo ""
fi

if should_run 2 connectivity; then
    echo -e "${YELLOW}Layer 2: LLM Router${NC}"
    run_test "e2e_layer2_llm_soap_generation" "SOAP generation (soap-model-fast)"
    run_test "e2e_layer2_llm_encounter_detection" "Encounter detection (fast-model)"
    run_test "e2e_layer2_hybrid_detection_and_merge" "Hybrid model (detect + merge + filter)"
    echo ""
fi

if should_run 3 connectivity; then
    echo -e "${YELLOW}Layer 3: Local Archive${NC}"
    run_test "e2e_layer3_archive_save_and_retrieve" "Save and retrieve"
    run_test "e2e_layer3_archive_continuous_mode_metadata" "Continuous mode metadata"
    echo ""
fi

if should_run 4 full; then
    echo -e "${YELLOW}Layer 4: Session Mode (full pipeline)${NC}"
    run_test "e2e_layer4_session_mode_full" "Audio → STT → SOAP → Archive → History"
    echo ""
fi

if should_run 5 full; then
    echo -e "${YELLOW}Layer 5: Continuous Mode (full pipeline)${NC}"
    run_test "e2e_layer5_continuous_mode_full" "Audio → Detection → SOAP → Archive → History"
    echo ""
fi

if should_run 6 offline; then
    run_cli_layer 6 "Detection Replay Regression" \
        "Replaying detection decisions against archive (target ≥ 99.0%)" \
        /tmp/preflight_replay.log \
        "Bundles:|Agreement:" "" "" \
        -- cargo run --quiet --bin detection_replay_cli -- \
            --all --fail-on-mismatch --threshold 99.0
fi

# Layer 7: Golden Day Regression (offline, labeled fixtures vs production archive)
# This is a CLINICAL-MAINTENANCE check — flags days where the archive and the
# label corpus have drifted (extra sessions, sessions only on profile service,
# placeholder labels for known-missing sessions, etc). Not gated on PRs because
# corpus state changes independently of code changes (cross-room sync, test
# artifacts, late labelling) and would block merges for reasons unrelated to
# the PR. Runs in --full for the daily preflight and on explicit --layer 7.
if should_run 7 connectivity; then
    run_cli_layer 7 "Golden Day Regression" \
        "Verifying labeled clinic days match production" \
        /tmp/preflight_golden.log \
        "Total:|Golden Day:" "" "" \
        -- cargo run --quiet --bin golden_day_cli -- --all-days --fail-on-regression
fi

# Layer 8 spec: docs/superpowers/specs/2026-04-18-continuous-mode-test-harness-design.md
if should_run 8 offline; then
    run_cli_layer 8 "Orchestrator Equivalence Harness" \
        "Verifying run_continuous_mode behavior against snapshot baselines" \
        /tmp/preflight_harness.log \
        "test result:" \
        "FAILED|panicked|harness detected" \
        "Full reports: target/harness-report/*.json" \
        -- cargo test --test harness_per_encounter --quiet
fi

# Layer 9 spec: docs/superpowers/specs/2026-05-05-regression-ci-design.md
if should_run 9 offline; then
    run_cli_layer 9 "Labeled Regression Corpus" \
        "Comparing production output to per-check labels" \
        /tmp/preflight_labeled.log \
        "^Labels:" \
        "^REGRESSION |^Labels:" \
        "Full report: /tmp/preflight_labeled.log" \
        -- cargo run --quiet --bin labeled_regression_cli -- --all --fail-on-regression
fi

# Summary
echo "========================================"
if [[ $FAILED -eq 0 ]]; then
    echo -e "  ${GREEN}All $PASSED checks passed${NC}"
    echo "  Ready for clinic day!"
else
    echo -e "  ${RED}$FAILED failed${NC}, $PASSED passed"
    echo "  Fix failures before starting clinic."
fi
echo "========================================"

exit $FAILED

#!/bin/bash
# Daily Preflight Check — run before starting a clinic day
#
# Verifies all external services and the full transcription pipeline are working.
# Runs layered E2E tests so failures are easy to diagnose.
#
# Usage:
#   ./scripts/preflight.sh           # Quick check (layers 1-3, ~10s)
#   ./scripts/preflight.sh --full    # Full pipeline (all layers, ~30s)
#   ./scripts/preflight.sh --layer 2 # Specific layer only
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
        --full)   MODE="full"; shift ;;
        --layer)  LAYER="$2"; shift 2 ;;
        --help|-h)
            echo "Daily Preflight Check"
            echo ""
            echo "Usage: $0 [--full] [--layer N]"
            echo ""
            echo "Layers:"
            echo "  1  STT Router    — health, alias, WebSocket streaming"
            echo "  2  LLM Router    — SOAP generation, encounter detection, hybrid model"
            echo "  3  Local Archive — save/retrieve, continuous mode metadata"
            echo "  4  Session Mode  — full Audio → STT → SOAP → Archive → History"
            echo "  5  Continuous    — full Audio → STT → Detection → SOAP → Archive"
            echo ""
            echo "Options:"
            echo "  --full      Run all 5 layers (default: layers 1-3)"
            echo "  --layer N   Run only layer N"
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

# Build test binary (fast if already compiled)
echo -e "${YELLOW}Building tests...${NC}"
cargo test --no-run --lib 2>&1 | grep -E "Compiling|Finished" || true
echo ""

FAILED=0
PASSED=0

run_test() {
    local test_name="$1"
    local description="$2"

    echo -n "  $description... "

    if OUTPUT=$(cargo test "$test_name" -- --ignored --nocapture 2>&1); then
        echo -e "${GREEN}PASS${NC}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}FAIL${NC}"
        # Print relevant error lines (skip cargo noise)
        echo "$OUTPUT" | grep -E "panicked|FAILED|Error|error" | head -5 | sed 's/^/    /'
        FAILED=$((FAILED + 1))
    fi
}

# Layer 1: STT Router
if [[ -z "$LAYER" || "$LAYER" == "1" ]]; then
    echo -e "${YELLOW}Layer 1: STT Router${NC}"
    run_test "e2e_layer1_stt_health_check" "Health check"
    run_test "e2e_layer1_stt_alias_available" "Alias 'medical-streaming'"
    run_test "e2e_layer1_stt_streaming_protocol" "WebSocket streaming"
    echo ""
fi

# Layer 2: LLM Router
if [[ -z "$LAYER" || "$LAYER" == "2" ]]; then
    echo -e "${YELLOW}Layer 2: LLM Router${NC}"
    run_test "e2e_layer2_llm_soap_generation" "SOAP generation (soap-model-fast)"
    run_test "e2e_layer2_llm_encounter_detection" "Encounter detection (faster + /nothink)"
    run_test "e2e_layer2_hybrid_detection_and_merge" "Hybrid model (detect + merge + filter)"
    echo ""
fi

# Layer 3: Local Archive
if [[ -z "$LAYER" || "$LAYER" == "3" ]]; then
    echo -e "${YELLOW}Layer 3: Local Archive${NC}"
    run_test "e2e_layer3_archive_save_and_retrieve" "Save and retrieve"
    run_test "e2e_layer3_archive_continuous_mode_metadata" "Continuous mode metadata"
    echo ""
fi

# Layers 4-5: Full pipeline (only in --full mode or explicit --layer)
if [[ "$MODE" == "full" || "$LAYER" == "4" ]]; then
    echo -e "${YELLOW}Layer 4: Session Mode (full pipeline)${NC}"
    run_test "e2e_layer4_session_mode_full" "Audio → STT → SOAP → Archive → History"
    echo ""
fi

if [[ "$MODE" == "full" || "$LAYER" == "5" ]]; then
    echo -e "${YELLOW}Layer 5: Continuous Mode (full pipeline)${NC}"
    run_test "e2e_layer5_continuous_mode_full" "Audio → Detection → SOAP → Archive → History"
    echo ""
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

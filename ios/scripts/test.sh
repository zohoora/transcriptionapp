#!/bin/bash
# iOS Test Runner for AMI Assist
#
# Usage:
#   ./ios/scripts/test.sh              # Run all tests
#   ./ios/scripts/test.sh --build-only # Build only, skip tests
#   ./ios/scripts/test.sh --verbose    # Show full xcodebuild output
#
# Requires: Xcode 16+, xcodegen, iOS Simulator runtime

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

# Parse arguments
BUILD_ONLY=false
VERBOSE=false
for arg in "$@"; do
    case $arg in
        --build-only) BUILD_ONLY=true ;;
        --verbose) VERBOSE=true ;;
        --help|-h)
            echo "Usage: $0 [--build-only] [--verbose]"
            echo "  --build-only  Build without running tests"
            echo "  --verbose     Show full xcodebuild output"
            exit 0
            ;;
    esac
done

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "─────────────────────────────────────────"
echo "  AMI Assist iOS — Build & Test"
echo "─────────────────────────────────────────"

# Step 1: Generate project
echo -e "\n${YELLOW}[1/4]${NC} Generating Xcode project..."
xcodegen generate 2>&1 | grep -v "^$"

# Step 2: Find a simulator
SIMULATOR_NAME="iPhone 16 Pro"
SIMULATOR_ID=$(xcrun simctl list devices available -j 2>/dev/null | \
    python3 -c "
import json, sys
data = json.load(sys.stdin)
for runtime, devices in data.get('devices', {}).items():
    for d in devices:
        if d.get('name') == '$SIMULATOR_NAME' and d.get('isAvailable', False):
            print(d['udid'])
            sys.exit(0)
# Fallback: any available iPhone
for runtime, devices in data.get('devices', {}).items():
    for d in devices:
        if 'iPhone' in d.get('name', '') and d.get('isAvailable', False):
            print(d['udid'])
            sys.exit(0)
" 2>/dev/null || true)

if [ -z "$SIMULATOR_ID" ]; then
    echo -e "${RED}ERROR: No iOS simulator found. Install one via Xcode > Settings > Platforms.${NC}"
    exit 1
fi
echo "  Using simulator: $SIMULATOR_NAME ($SIMULATOR_ID)"

# Step 3: Build
echo -e "\n${YELLOW}[2/4]${NC} Building app + tests..."
BUILD_CMD=(
    xcodebuild build-for-testing
    -project "AMI Assist.xcodeproj"
    -scheme "AMI Assist"
    -sdk iphonesimulator
    -destination "id=$SIMULATOR_ID"
)

if $VERBOSE; then
    "${BUILD_CMD[@]}" 2>&1
else
    if ! "${BUILD_CMD[@]}" 2>&1 | tail -5; then
        echo -e "${RED}BUILD FAILED${NC}"
        echo "Re-run with --verbose to see full output."
        exit 1
    fi
fi

echo -e "${GREEN}  Build succeeded${NC}"

if $BUILD_ONLY; then
    echo -e "\n${GREEN}Build-only mode — skipping tests.${NC}"
    exit 0
fi

# Step 4: Run tests
echo -e "\n${YELLOW}[3/4]${NC} Running tests..."
TEST_CMD=(
    xcodebuild test-without-building
    -project "AMI Assist.xcodeproj"
    -scheme "AMI Assist"
    -sdk iphonesimulator
    -destination "id=$SIMULATOR_ID"
    -only-testing:"AMI Assist Tests"
)

if $VERBOSE; then
    "${TEST_CMD[@]}" 2>&1
    TEST_EXIT=$?
else
    TEST_OUTPUT=$("${TEST_CMD[@]}" 2>&1)
    TEST_EXIT=$?

    # Show test results
    echo "$TEST_OUTPUT" | grep -E "Test case .* passed|Test case .* failed" | \
        sed 's/Test case /  /; s/ passed.*/ ✓/; s/ failed.*/ ✗/'
fi

# Step 5: Summary
echo -e "\n${YELLOW}[4/4]${NC} Summary"
PASS_COUNT=$(echo "${TEST_OUTPUT:-}" | grep -c "passed" 2>/dev/null || echo "?")
FAIL_COUNT=$(echo "${TEST_OUTPUT:-}" | grep -c "failed" 2>/dev/null || echo "0")

if [ $TEST_EXIT -eq 0 ]; then
    echo -e "  ${GREEN}ALL TESTS PASSED${NC} ($PASS_COUNT tests)"
    echo "─────────────────────────────────────────"
    exit 0
else
    echo -e "  ${RED}TESTS FAILED${NC} ($FAIL_COUNT failures)"
    if ! $VERBOSE; then
        echo "  Re-run with --verbose to see details."
    fi
    echo "─────────────────────────────────────────"
    exit 1
fi

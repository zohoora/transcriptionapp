#!/bin/bash
#
# Soak Test Script for Transcription App
#
# Runs the application under sustained load for extended periods
# to detect memory leaks, performance degradation, and stability issues.
#
# Usage:
#   ./soak_test.sh [duration_hours] [interval_seconds]
#
# Examples:
#   ./soak_test.sh 1      # Run for 1 hour, default 30s interval
#   ./soak_test.sh 4 60   # Run for 4 hours, 60s interval
#   ./soak_test.sh 0.5    # Run for 30 minutes
#

set -e

# Configuration
DURATION_HOURS=${1:-1}
INTERVAL_SECONDS=${2:-30}
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
RUST_DIR="$PROJECT_DIR/src-tauri"
LOG_DIR="$SCRIPT_DIR/logs"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="$LOG_DIR/soak_test_$TIMESTAMP.log"
METRICS_FILE="$LOG_DIR/metrics_$TIMESTAMP.csv"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create log directory
mkdir -p "$LOG_DIR"

echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║         Transcription App Soak Test                        ║${NC}"
echo -e "${BLUE}╠════════════════════════════════════════════════════════════╣${NC}"
echo -e "${BLUE}║${NC} Duration:     ${GREEN}$DURATION_HOURS hours${NC}"
echo -e "${BLUE}║${NC} Interval:     ${GREEN}$INTERVAL_SECONDS seconds${NC}"
echo -e "${BLUE}║${NC} Log file:     ${GREEN}$LOG_FILE${NC}"
echo -e "${BLUE}║${NC} Metrics file: ${GREEN}$METRICS_FILE${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

log() {
    local level=$1
    shift
    local message="$*"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo "[$timestamp] [$level] $message" | tee -a "$LOG_FILE"
}

log_metric() {
    echo "$*" >> "$METRICS_FILE"
}

get_memory_mb() {
    local pid=$1
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        ps -o rss= -p "$pid" 2>/dev/null | awk '{print $1/1024}' || echo "0"
    else
        # Linux
        ps -o rss= -p "$pid" 2>/dev/null | awk '{print $1/1024}' || echo "0"
    fi
}

get_cpu_percent() {
    local pid=$1
    if [[ "$OSTYPE" == "darwin"* ]]; then
        ps -o %cpu= -p "$pid" 2>/dev/null | awk '{print $1}' || echo "0"
    else
        ps -o %cpu= -p "$pid" 2>/dev/null | awk '{print $1}' || echo "0"
    fi
}

check_dependencies() {
    log "INFO" "Checking dependencies..."

    if ! command -v cargo &> /dev/null; then
        log "ERROR" "cargo not found. Please install Rust."
        exit 1
    fi

    log "INFO" "Dependencies OK"
}

build_soak_test() {
    log "INFO" "Building soak test binary..."
    cd "$RUST_DIR"

    # Build the soak test
    cargo build --release --bin soak_test 2>&1 | tee -a "$LOG_FILE"

    if [ $? -ne 0 ]; then
        log "ERROR" "Build failed"
        exit 1
    fi

    log "INFO" "Build complete"
}

run_backend_soak_test() {
    log "INFO" "Starting backend soak test..."

    cd "$RUST_DIR"

    # Initialize metrics CSV
    log_metric "timestamp,elapsed_seconds,memory_mb,cpu_percent,segments_processed,utterances_processed,errors"

    local start_time=$(date +%s)
    local duration_seconds=$(echo "$DURATION_HOURS * 3600" | bc | cut -d. -f1)
    local end_time=$((start_time + duration_seconds))

    # Start the soak test binary in background
    ./target/release/soak_test &
    local pid=$!

    log "INFO" "Soak test started with PID: $pid"

    local initial_memory=$(get_memory_mb $pid)
    local max_memory=0
    local samples=0
    local total_memory=0

    # Monitor loop
    while [ $(date +%s) -lt $end_time ]; do
        if ! kill -0 $pid 2>/dev/null; then
            log "ERROR" "Soak test process died unexpectedly!"
            break
        fi

        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))
        local memory=$(get_memory_mb $pid)
        local cpu=$(get_cpu_percent $pid)

        # Track max memory
        if (( $(echo "$memory > $max_memory" | bc -l) )); then
            max_memory=$memory
        fi

        # Track average
        total_memory=$(echo "$total_memory + $memory" | bc)
        samples=$((samples + 1))

        # Read stats from soak test (if available)
        local segments=0
        local utterances=0
        local errors=0
        if [ -f "/tmp/soak_stats.txt" ]; then
            segments=$(grep "segments:" /tmp/soak_stats.txt 2>/dev/null | cut -d: -f2 || echo "0")
            utterances=$(grep "utterances:" /tmp/soak_stats.txt 2>/dev/null | cut -d: -f2 || echo "0")
            errors=$(grep "errors:" /tmp/soak_stats.txt 2>/dev/null | cut -d: -f2 || echo "0")
        fi

        log_metric "$(date -Iseconds),$elapsed,$memory,$cpu,$segments,$utterances,$errors"

        # Progress report
        local progress=$(echo "scale=1; $elapsed * 100 / $duration_seconds" | bc)
        local remaining=$((duration_seconds - elapsed))
        local remaining_min=$((remaining / 60))

        echo -ne "\r${BLUE}Progress:${NC} ${GREEN}${progress}%${NC} | "
        echo -ne "${BLUE}Memory:${NC} ${memory}MB (max: ${max_memory}MB) | "
        echo -ne "${BLUE}CPU:${NC} ${cpu}% | "
        echo -ne "${BLUE}Remaining:${NC} ${remaining_min}min    "

        sleep $INTERVAL_SECONDS
    done

    echo ""

    # Stop the soak test
    if kill -0 $pid 2>/dev/null; then
        log "INFO" "Stopping soak test..."
        kill -TERM $pid 2>/dev/null
        wait $pid 2>/dev/null
    fi

    # Calculate statistics
    local avg_memory=$(echo "scale=2; $total_memory / $samples" | bc)
    local memory_growth=$(echo "scale=2; $max_memory - $initial_memory" | bc)

    echo ""
    echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║                    Soak Test Results                       ║${NC}"
    echo -e "${BLUE}╠════════════════════════════════════════════════════════════╣${NC}"
    echo -e "${BLUE}║${NC} Duration:        ${GREEN}$(echo "scale=2; $elapsed / 3600" | bc) hours${NC}"
    echo -e "${BLUE}║${NC} Initial Memory:  ${GREEN}${initial_memory} MB${NC}"
    echo -e "${BLUE}║${NC} Final Memory:    ${GREEN}$(get_memory_mb $pid 2>/dev/null || echo 'N/A') MB${NC}"
    echo -e "${BLUE}║${NC} Max Memory:      ${GREEN}${max_memory} MB${NC}"
    echo -e "${BLUE}║${NC} Avg Memory:      ${GREEN}${avg_memory} MB${NC}"
    echo -e "${BLUE}║${NC} Memory Growth:   ${GREEN}${memory_growth} MB${NC}"

    # Check for memory leak
    if (( $(echo "$memory_growth > 100" | bc -l) )); then
        echo -e "${BLUE}║${NC} Memory Leak:     ${RED}POSSIBLE (growth > 100MB)${NC}"
    else
        echo -e "${BLUE}║${NC} Memory Leak:     ${GREEN}NONE DETECTED${NC}"
    fi

    echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"

    log "INFO" "Soak test completed"
    log "INFO" "Results: initial=$initial_memory MB, max=$max_memory MB, avg=$avg_memory MB, growth=$memory_growth MB"
}

# Run Rust-only soak test (exercises core pipeline without GUI)
run_rust_soak_test() {
    log "INFO" "Running Rust pipeline soak test..."

    cd "$RUST_DIR"

    # Run the Rust soak test with timeout
    local duration_seconds=$(echo "$DURATION_HOURS * 3600" | bc | cut -d. -f1)

    SOAK_DURATION_SECS=$duration_seconds cargo test --release soak_test_extended -- --ignored --nocapture 2>&1 | tee -a "$LOG_FILE"

    log "INFO" "Rust soak test completed"
}

cleanup() {
    log "INFO" "Cleaning up..."
    rm -f /tmp/soak_stats.txt
    # Kill any remaining soak test processes
    pkill -f "soak_test" 2>/dev/null || true
}

trap cleanup EXIT

# Main
check_dependencies

echo -e "${YELLOW}Select soak test mode:${NC}"
echo "  1) Rust pipeline soak test (no GUI, faster)"
echo "  2) Full app soak test (requires built app)"
echo ""
read -p "Enter choice [1]: " choice
choice=${choice:-1}

case $choice in
    1)
        run_rust_soak_test
        ;;
    2)
        build_soak_test
        run_backend_soak_test
        ;;
    *)
        echo "Invalid choice"
        exit 1
        ;;
esac

echo ""
echo -e "${GREEN}Soak test complete!${NC}"
echo -e "Logs: ${BLUE}$LOG_FILE${NC}"
echo -e "Metrics: ${BLUE}$METRICS_FILE${NC}"

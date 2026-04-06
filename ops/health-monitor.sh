#!/bin/bash
# AMI Assist — Health Monitor
#
# Checks all 4 clinic services every run (scheduled via launchd every 5 min).
# Alerts via macOS notification only on state transitions (healthy→down or down→healthy).
# Optionally sends to a webhook if AMI_ALERT_WEBHOOK is set.
#
# Usage:
#   bash ops/health-monitor.sh
#   launchctl load ~/Library/LaunchAgents/com.ami-assist.health-monitor.plist

set -uo pipefail

# ── Configuration ─────────────────────────────────────────────────────
LOG_DIR="${AMI_BACKUP_ROOT:-$HOME/backups/ami-assist}"
LOG="$LOG_DIR/health.log"
STATE_DIR="/tmp/ami-health-state"
TIMEOUT=5

PROFILE_URL="http://localhost:8090/health"
STT_URL="http://localhost:8001/health"
LLM_URL="http://localhost:8080/v1/models"
MEDPLUM_URL="http://localhost:8103/.well-known/openid-configuration"

mkdir -p "$LOG_DIR" "$STATE_DIR"
trap 'rm -f /tmp/ami-health-body-*' EXIT

log() {
    echo "[$(date +%Y-%m-%dT%H:%M:%S)] $1" | tee -a "$LOG"
}

# ── Alert function ────────────────────────────────────────────────────

alert() {
    local service="$1"
    local status="$2"  # "DOWN" or "RECOVERED"
    local detail="$3"

    if [ "$status" = "DOWN" ]; then
        local msg="$service is DOWN: $detail"
        log "ALERT: $msg"
        osascript -e "display notification \"$msg\" with title \"AMI Health Alert\" sound name \"Basso\"" 2>/dev/null || true
    else
        local msg="$service has RECOVERED"
        log "RECOVERED: $msg"
        osascript -e "display notification \"$msg\" with title \"AMI Health OK\"" 2>/dev/null || true
    fi

    if [ -n "${AMI_ALERT_WEBHOOK:-}" ]; then
        curl -s -X POST "$AMI_ALERT_WEBHOOK" \
            -H "Content-Type: application/json" \
            -d "{\"text\":\"[$status] $service: $detail\",\"timestamp\":\"$(date +%Y-%m-%dT%H:%M:%S)\"}" \
            --max-time 5 >/dev/null 2>&1 || true
    fi
}

# ── Check a single service ────────────────────────────────────────────
# health_field: JSON field to check, or "http_only" for status-code-only
# expected: value the field must equal for healthy (default: "true")

check_service() {
    local name="$1"
    local url="$2"
    local health_field="$3"
    local expected="${4:-true}"

    local state_file="$STATE_DIR/${name}.status"
    local body_file="/tmp/ami-health-body-${name}"
    local prev_status="unknown"
    [ -f "$state_file" ] && prev_status=$(cat "$state_file")

    local http_code body
    http_code=$(curl -s -o "$body_file" -w "%{http_code}" \
        --max-time "$TIMEOUT" "$url" 2>/dev/null) || http_code="000"
    body=$(cat "$body_file" 2>/dev/null || echo "")

    local current_status="healthy"
    local detail=""

    if [ "$http_code" = "000" ]; then
        current_status="down"
        detail="connection refused or timeout"
    elif [ "${http_code:0:1}" != "2" ]; then
        current_status="down"
        detail="HTTP $http_code"
    elif [ "$health_field" != "http_only" ] && [ -n "$body" ]; then
        local field_value
        if command -v jq &>/dev/null; then
            field_value=$(echo "$body" | jq -r ".$health_field" 2>/dev/null || echo "")
        else
            field_value=$(echo "$body" | grep -o "\"$health_field\"[[:space:]]*:[[:space:]]*[a-z\"]*" | head -1 | sed 's/.*://;s/[" ]//g')
        fi

        if [ "$field_value" != "$expected" ]; then
            current_status="down"
            detail="$health_field=$field_value (expected $expected)"
        fi
    fi

    log "  $name: $current_status (HTTP $http_code)"

    if [ "$prev_status" = "unknown" ]; then
        :
    elif [ "$prev_status" = "healthy" ] && [ "$current_status" = "down" ]; then
        alert "$name" "DOWN" "$detail"
    elif [ "$prev_status" = "down" ] && [ "$current_status" = "healthy" ]; then
        alert "$name" "RECOVERED" ""
    fi

    echo "$current_status" > "$state_file"
}

# ── Run checks ────────────────────────────────────────────────────────

log "Health check starting"

check_service "Profile-Service" "$PROFILE_URL" "healthy" "true"
check_service "STT-Router"      "$STT_URL"     "status"  "healthy"
check_service "LLM-Router"      "$LLM_URL"     "http_only"
check_service "Medplum"         "$MEDPLUM_URL"  "http_only"

log "Health check complete"

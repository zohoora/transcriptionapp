#!/bin/bash
# Auto-deploy profile service: git pull + rebuild + restart on changes.
# Runs periodically via launchd (every 5 minutes) — see
# com.fabricscribe.profile-service-updater.plist.
#
# This file is the canonical artifact — it was authored on the MacBook
# (~/transcriptionapp-deploy.sh) and adopted into the repo verbatim so the
# setup is reproducible. When changing the script, edit this file, commit,
# and the updater agent will auto-pull the new version on its next tick (the
# `git pull --ff-only` below advances the working tree, and subsequent ticks
# then execute the freshly pulled script).
#
# Restart strategy: `kill $PID` + rely on launchd `KeepAlive=true` to respawn.
# The service plist (com.fabricscribe.profile-service.plist) must have KeepAlive
# set for this to work. Health is verified via curl /health after restart.

REPO_DIR="$HOME/transcriptionapp"
PROFILE_SERVICE_DIR="$REPO_DIR/profile-service"
LOG_FILE="$HOME/.fabricscribe/deploy.log"
LOCK_DIR="$HOME/.fabricscribe/deploy.lock.d"

mkdir -p "$(dirname "$LOG_FILE")"

log() {
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" >> "$LOG_FILE"
}

# mkdir is atomic on macOS — use it as a lock
if ! mkdir "$LOCK_DIR" 2>/dev/null; then
    # Stale lock check: if older than 10 min, assume crashed and reclaim
    if [ -d "$LOCK_DIR" ]; then
        LOCK_AGE=$(( $(date +%s) - $(stat -f %m "$LOCK_DIR") ))
        if [ "$LOCK_AGE" -gt 600 ]; then
            log "Reclaiming stale lock (age ${LOCK_AGE}s)"
            rmdir "$LOCK_DIR"
            mkdir "$LOCK_DIR" || exit 0
        else
            exit 0
        fi
    fi
fi
trap "rmdir '$LOCK_DIR' 2>/dev/null" EXIT

cd "$REPO_DIR" 2>/dev/null || { log "ERROR: cannot cd to $REPO_DIR"; exit 1; }

# Fetch latest from origin (silent if no network)
if ! git fetch origin main 2>>"$LOG_FILE"; then
    log "WARN: git fetch failed (network down?), skipping"
    exit 0
fi

LOCAL_HEAD=$(git rev-parse HEAD)
REMOTE_HEAD=$(git rev-parse origin/main)

if [ "$LOCAL_HEAD" = "$REMOTE_HEAD" ]; then
    # Up to date, no work needed
    exit 0
fi

# Check if profile-service files changed
CHANGED=$(git diff --name-only "$LOCAL_HEAD" "$REMOTE_HEAD" -- profile-service/ 2>>"$LOG_FILE")

if [ -z "$CHANGED" ]; then
    log "No profile-service changes — pulling but skipping rebuild"
    git pull origin main --ff-only 2>>"$LOG_FILE"
    exit 0
fi

log "Profile-service changes detected, deploying..."
log "Files changed: $(echo "$CHANGED" | tr '\n' ' ')"

# Pull
if ! git pull origin main --ff-only 2>>"$LOG_FILE"; then
    log "ERROR: git pull failed"
    exit 1
fi

# Build
cd "$PROFILE_SERVICE_DIR"
log "Building..."
if ! cargo build --release >>"$LOG_FILE" 2>&1; then
    log "ERROR: cargo build failed"
    exit 1
fi
log "Build complete"

# Restart by killing the process — launchd will auto-respawn
PID=$(pgrep -f "profile-service --port" | head -1)
if [ -n "$PID" ]; then
    log "Killing PID $PID for restart..."
    kill "$PID"
    sleep 3
fi

# Wait up to 10s for service to come back up
for i in 1 2 3 4 5 6 7 8 9 10; do
    if curl -sf http://localhost:8090/health >/dev/null 2>&1; then
        NEW_PID=$(pgrep -f "profile-service --port" | head -1)
        log "Deploy SUCCESS — new PID $NEW_PID"
        exit 0
    fi
    sleep 1
done

log "ERROR: health check failed after restart"
exit 1

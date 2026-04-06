#!/bin/bash
# AMI Assist — Nightly Backup Script
#
# Backs up clinical session data and configuration from both the profile
# service (server) and local Tauri app data directories.
#
# Usage:
#   bash ops/backup.sh
#   launchctl load ~/Library/LaunchAgents/com.ami-assist.backup.plist
#
# NOT backed up (recoverable or ephemeral):
#   models/, logs/, debug/, mmwave/, shadow/

set -euo pipefail

BACKUP_ROOT="${AMI_BACKUP_ROOT:-$HOME/backups/ami-assist}"
RETENTION_DAYS=30
DATE=$(date +%Y-%m-%d)
DEST="$BACKUP_ROOT/$DATE"
LOG="$BACKUP_ROOT/backup.log"

mkdir -p "$BACKUP_ROOT"

log() {
    echo "[$(date +%Y-%m-%dT%H:%M:%S)] $1" | tee -a "$LOG"
}

# Rsync a directory if it exists, logging size
backup_dir() {
    local src="$1" dest_name="$2" flags="${3:---delete}"
    if [ -d "$src" ]; then
        mkdir -p "$DEST/$dest_name"
        rsync -a $flags "$src/" "$DEST/$dest_name/"
        log "  $dest_name: $(du -sh "$DEST/$dest_name" | cut -f1)"
    else
        log "  $dest_name: SKIPPED (not found)"
    fi
}

log "Backup starting → $DEST"
chmod 700 "$DEST" 2>/dev/null || true

backup_dir "$HOME/.fabricscribe"              "fabricscribe" "--delete"
backup_dir "$HOME/.transcriptionapp/archive"  "archive"      "--delete"
backup_dir "$HOME/.transcriptionapp/cache"    "cache"

# Config files
mkdir -p "$DEST/config"
for f in config.json speaker_profiles.json room_config.json medplum_auth.json; do
    [ -f "$HOME/.transcriptionapp/$f" ] && cp "$HOME/.transcriptionapp/$f" "$DEST/config/$f"
done
log "  config: copied"

# Prune old backups
PRUNED=0
while IFS= read -r old_dir; do
    if [ -d "$old_dir" ] && [ "$old_dir" != "$DEST" ]; then
        rm -rf "$old_dir"
        PRUNED=$((PRUNED + 1))
    fi
done < <(find "$BACKUP_ROOT" -maxdepth 1 -type d -name "20*" -mtime "+$RETENTION_DAYS" 2>/dev/null)

log "Backup complete: $(du -sh "$DEST" | cut -f1) total, $PRUNED old backups pruned"

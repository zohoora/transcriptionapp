# AMI Assist — Operations Scripts

Scripts for backup and health monitoring. Designed for the MacBook server (100.119.83.76, user: arash).

## Automated Nightly Backup

Backs up profile service data (`~/.fabricscribe/`) and local archive (`~/.transcriptionapp/archive/`) with 30-day retention.

### Install

```bash
# Copy plist to LaunchAgents (adjust path in plist if repo location differs)
cp ops/launchd/com.ami-assist.backup.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.ami-assist.backup.plist
```

### Manual run

```bash
bash ops/backup.sh
```

### Check logs

```bash
cat ~/backups/ami-assist/backup.log | tail -20
```

### What's backed up

| Source | Destination | Notes |
|--------|------------|-------|
| `~/.fabricscribe/` | `~/backups/ami-assist/{date}/fabricscribe/` | Sessions, physicians, rooms, speakers |
| `~/.transcriptionapp/archive/` | `~/backups/ami-assist/{date}/archive/` | Local session transcripts, SOAP, audio |
| `~/.transcriptionapp/config.json` | `~/backups/ami-assist/{date}/config/` | App settings |
| `~/.transcriptionapp/speaker_profiles.json` | `~/backups/ami-assist/{date}/config/` | Voice enrollment data |
| `~/.transcriptionapp/room_config.json` | `~/backups/ami-assist/{date}/config/` | Room setup |

**Not backed up** (recoverable): ONNX models, activity logs, debug storage, sensor CSV logs, mobile audio uploads (`mobile_uploads/` — re-uploadable from iOS app).

### Configuration

| Env Var | Default | Purpose |
|---------|---------|---------|
| `AMI_BACKUP_ROOT` | `~/backups/ami-assist` | Backup destination directory |

---

## Health Monitoring

Checks 4 services every 5 minutes. Alerts via macOS notification on state transitions only (no alert spam).

### Install

```bash
cp ops/launchd/com.ami-assist.health-monitor.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.ami-assist.health-monitor.plist
```

### Manual run

```bash
bash ops/health-monitor.sh
```

### Check logs

```bash
cat ~/backups/ami-assist/health.log | tail -20
```

### Services monitored

| Service | URL | Health Check |
|---------|-----|-------------|
| Profile Service | `http://localhost:8090/health` | JSON `.healthy` field |
| STT Router | `http://localhost:8001/health` | JSON `.status` field |
| LLM Router | `http://localhost:8080/v1/models` | HTTP 200 |
| Medplum | `http://localhost:8103/.well-known/openid-configuration` | HTTP 200 |

### Alert behavior

- First run after install: establishes baseline (no alerts)
- Subsequent runs: alerts only on transitions (healthy→down, down→healthy)
- macOS notification with sound on service down
- Optional webhook: set `AMI_ALERT_WEBHOOK` env var

### Configuration

| Env Var | Default | Purpose |
|---------|---------|---------|
| `AMI_BACKUP_ROOT` | `~/backups/ami-assist` | Log directory |
| `AMI_ALERT_WEBHOOK` | (unset) | Optional webhook URL for alerts |

---

## Uninstall

```bash
launchctl unload ~/Library/LaunchAgents/com.ami-assist.backup.plist
launchctl unload ~/Library/LaunchAgents/com.ami-assist.health-monitor.plist
rm ~/Library/LaunchAgents/com.ami-assist.backup.plist
rm ~/Library/LaunchAgents/com.ami-assist.health-monitor.plist
```

## Launchd plist paths

The plist files reference `/Users/arash/transcriptionapp/ops/` as the script location. If the repo is cloned to a different path, edit the `ProgramArguments` in both plists before installing.

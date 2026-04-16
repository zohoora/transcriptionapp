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

---

## Profile Service Auto-Deploy

The profile service on the MacBook auto-rebuilds and restarts whenever the repo's `main` branch advances. This avoids manual SSH + `cargo build` after every commit.

### How it works

A launchd agent (`com.fabricscribe.profile-service-updater`) runs `~/transcriptionapp-deploy.sh` on a 5-minute schedule. The script:

1. `git fetch origin main` in `~/transcriptionapp/`
2. If no new commits: exit. If `profile-service/` unchanged: fast-forward pull and exit
3. Otherwise: `git pull --ff-only`, `cd profile-service && cargo build --release`
4. Kill the running profile-service process; launchd `KeepAlive=true` auto-respawns the new binary
5. Verify via `curl /health` (up to 10s) before declaring success
6. Atomic `mkdir` lock at `~/.fabricscribe/deploy.lock.d` (stale locks reclaimed after 10 min)
7. Log to `~/.fabricscribe/deploy.log`

### Logs

```bash
ssh arash@100.119.83.76 'tail -50 ~/.fabricscribe/deploy.log'
```

### Manual trigger

```bash
ssh arash@100.119.83.76 'bash ~/transcriptionapp-deploy.sh'
```

### Reproducible from repo

The deploy script and launchd plist are checked into `ops/auto-deploy/`:

- `ops/auto-deploy/transcriptionapp-deploy.sh`
- `ops/auto-deploy/com.fabricscribe.profile-service-updater.plist`
- `ops/auto-deploy/README.md` — install + operations guide

If the server is rebuilt, follow the install steps in that README. The existing `~/transcriptionapp-deploy.sh` on the MacBook should be reviewed against the repo version and the repo made authoritative (any server-local tweaks should be folded back into the checked-in script or captured as environment variables).

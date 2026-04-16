# Auto-Deploy (Profile Service)

Keeps the MacBook clinic server's profile service in sync with `main`. Every 5 minutes a launchd agent pulls the latest commits, rebuilds `profile-service/` **only if that directory changed**, restarts the running service via `kill` + launchd `KeepAlive=true` auto-respawn, and verifies health via `curl /health` before exiting.

The files in this directory are the **canonical artifacts** — they were authored on the MacBook and adopted into the repo so the setup is reproducible. When changing deploy behavior, edit the files here and commit; the updater agent self-updates on its next tick (the `git pull --ff-only` in the deploy script advances the working tree, and subsequent ticks execute the freshly pulled script).

## Components

| File | Purpose |
|------|---------|
| `transcriptionapp-deploy.sh` | Deploy logic: fetch → fast-forward → conditional build → kill+respawn → health verify. Uses an atomic `mkdir` lock with 10-min stale reclamation. Logs to `~/.fabricscribe/deploy.log`. |
| `com.fabricscribe.profile-service-updater.plist` | launchd agent definition for the updater. `StartInterval=300`, `RunAtLoad=true`. |
| `com.fabricscribe.profile-service.plist` | launchd agent definition for the actual profile service. **`KeepAlive=true` is load-bearing** — the deploy script's kill-to-restart strategy depends on it. |

## Install (on the MacBook server, one-time)

```bash
# 1. Clone the repo to the expected location
cd ~ && git clone https://github.com/zohoora/transcriptionapp.git

# 2. Build the profile service
cd ~/transcriptionapp/profile-service && cargo build --release

# 3. Install the service plist and start the profile service
cp ~/transcriptionapp/ops/auto-deploy/com.fabricscribe.profile-service.plist \
   ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.fabricscribe.profile-service.plist

# 4. Install the deploy script as a home-directory symlink (the updater plist
#    references ~/transcriptionapp-deploy.sh, not the in-repo path)
ln -sf ~/transcriptionapp/ops/auto-deploy/transcriptionapp-deploy.sh \
       ~/transcriptionapp-deploy.sh

# 5. Install the updater plist and start the updater
cp ~/transcriptionapp/ops/auto-deploy/com.fabricscribe.profile-service-updater.plist \
   ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.fabricscribe.profile-service-updater.plist
```

Step 4 uses a symlink rather than a copy so repo updates to the deploy script automatically propagate without re-copying — the updater self-bootstraps.

## Verify

```bash
# Watch logs as new commits land
tail -f ~/.fabricscribe/deploy.log

# Force a manual run (respects the lockfile):
bash ~/transcriptionapp-deploy.sh

# Confirm both agents are registered
launchctl list | grep fabricscribe
```

Expected output:
```
-    0    com.fabricscribe.profile-service-updater
NNNN 0    com.fabricscribe.profile-service
```

## Log files

| Path | What |
|------|------|
| `~/.fabricscribe/deploy.log` | Timestamped events from the deploy script (lock acquisitions, git fetch results, build success/failure, health check outcome) |
| `~/.fabricscribe/deploy.stdout.log`, `~/.fabricscribe/deploy.stderr.log` | launchd's raw stdout/stderr — emergency-only, normal logs go to deploy.log |
| `~/.fabricscribe/profile-service.log` | The profile service's own log stream (axum access logs, business events) |

## Failure modes

| Situation | Behavior |
|-----------|----------|
| No new commits | Exits quickly — no rebuild, no restart. |
| `profile-service/` unchanged | Fast-forwards git and exits — no rebuild, no restart. |
| Diverged branch (local commits) | `git pull --ff-only` refuses the merge. Manual intervention required; service keeps running on previous binary. |
| Build fails | Exits 1, service keeps running on previous binary. Fix the root cause, push a new commit — next tick retries. |
| `kill` restart succeeds but health check fails within 10s | Exits 1 with "ERROR: health check failed after restart". Manual investigation needed — usually indicates a new startup failure (missing config, port conflict). |
| Concurrent tick | Second run sees the lock directory, exits 0 silently. Stale locks (>10 min old) are reclaimed automatically. |

## Configuration knobs

All paths and the branch are hardcoded in the script — edit `transcriptionapp-deploy.sh` to change them. Key variables:

| Variable | Value | Notes |
|----------|-------|-------|
| `REPO_DIR` | `$HOME/transcriptionapp` | Working tree location |
| `LOG_FILE` | `$HOME/.fabricscribe/deploy.log` | Event log |
| `LOCK_DIR` | `$HOME/.fabricscribe/deploy.lock.d` | Atomic mkdir-based lock |
| Branch | `origin/main` (hardcoded) | Change the two `git` lines if you want a different branch |
| Health endpoint | `http://localhost:8090/health` | Must match the service's port |

## Restart strategy — known trade-off

`kill` + launchd respawn is a **hard restart**: in-flight requests are terminated mid-response. Profile service handlers finish in <1s and clients retry automatically, so this is acceptable in practice.

If future traffic patterns change (longer-running handlers, streaming responses, etc.), upgrade to a graceful restart:

1. Add a SIGTERM handler in `profile-service` that stops accepting new connections, drains the active set, then exits. launchd will auto-restart per `KeepAlive`.
2. Change the `kill "$PID"` line to `kill -TERM "$PID"` and bump the `sleep 3` to allow drain time.

## Related

- `ops/README.md` — backup + health-monitor agents on the same server
- Root `CLAUDE.md` → "Auto-Deploy (Profile Service)" — high-level mention

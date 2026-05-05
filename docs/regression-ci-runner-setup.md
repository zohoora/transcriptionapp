# Regression CI Runner Setup

One-time setup to register a workstation as a self-hosted GitHub Actions runner so the `regression-corpus` CI job can read the production session archive at `~/.transcriptionapp/archive/`.

**The runner host is a workstation, not the MacBook server.** The replay corpus (`replay_bundle.json` files) is written by the desktop AMI Assist app, which only runs on workstations (Room 2 iMac, Room 6). The MacBook runs the profile service and the LLM/STT routers but has no local archive of replay bundles. We tried registering the runner on the MacBook first; the regression-corpus job hit `No replay_bundle.json files found under /Users/arash/.transcriptionapp/archive` and exited 1.

Current host: **Room 6 (`backoffice` user, MacBook Pro arm64)**. Runner name `ami-ci-room6`, labels `[self-hosted, macOS, ami-ci]`.

## Why a self-hosted runner

The regression corpus (`labeled_regression_cli`, `detection_replay_cli`) compares production output against archived `replay_bundle.json` and `billing.json` files. Those live under `~/.transcriptionapp/archive/` (PHI). They cannot be uploaded to GitHub-hosted runners — the archive is only on the workstations that run the desktop app.

`labeled_regression_cli` already supports the profile-service fallback via `ArchiveFetcher::from_env()`, but `detection_replay_cli --all` walks the local filesystem only. The simplest path is to host the runner on a workstation that has the local archive natively.

## Preconditions

- Workstation is reachable from GitHub (outbound HTTPS only, runner connects out — no inbound port).
- Repo is checked out somewhere readable; runner workspace is a separate clone under `~/actions-runner-ami-ci/_work/`.
- `cargo` (stable Rust toolchain), `node` (≥ 20), and `pnpm` (≥ 10) installed and on the runner user's PATH at login. Verify:
  ```bash
  which cargo node pnpm
  ```
- `~/.transcriptionapp/archive/` exists and contains `replay_bundle.json` files (the desktop app populates this during continuous-mode use). The CI job only reads. Verify before bring-up:
  ```bash
  find ~/.transcriptionapp/archive -name 'replay_bundle.json' | head -5
  ```

## Bring-up

### 1. Generate a registration token

From a workstation logged into GitHub with admin access to the repo:

```bash
gh api -X POST repos/zohoora/transcriptionapp/actions/runners/registration-token \
  --jq '.token'
```

This token is short-lived (~1 hour). Don't commit it.

### 2. Install the runner agent on the workstation

Run directly on the workstation (no SSH needed if you're already on it):

```bash
mkdir -p ~/actions-runner-ami-ci && cd ~/actions-runner-ami-ci

# Latest macOS arm64 runner — verify version + checksum at
# https://github.com/actions/runner/releases/latest
curl -O -L https://github.com/actions/runner/releases/download/v2.334.0/actions-runner-osx-arm64-2.334.0.tar.gz
echo "760899b29fd4e942076bcd1160a662bf83c15d9ce8a8cc466763aec7e582b21b  actions-runner-osx-arm64-2.334.0.tar.gz" | shasum -a 256 -c
tar xzf ./actions-runner-osx-arm64-2.334.0.tar.gz

# Configure with the token from step 1 and the AMI Assist labels.
# Pick a unique --name per workstation (ami-ci-room6, ami-ci-room2, etc.)
./config.sh \
  --url https://github.com/zohoora/transcriptionapp \
  --token <REGISTRATION_TOKEN_FROM_STEP_1> \
  --name ami-ci-room6 \
  --labels self-hosted,macOS,ami-ci \
  --work _work \
  --unattended
```

### 3. Run as a launchd service (so it survives reboots)

```bash
./svc.sh install
./svc.sh start
./svc.sh status
```

The service is installed under `~/Library/LaunchAgents/actions.runner.zohoora-transcriptionapp.ami-ci-<name>.plist`. It auto-starts on login. If the workstation reboots and no one logs in, the service does NOT start until first login — relevant for unattended workstations. For a server-style always-on runner, install via `sudo ./svc.sh install <user>` so it loads at boot.

### 4. Smoke test

From a workstation:

1. Push a no-op branch with a trivial change.
2. Open a PR against `main`.
3. Watch the `Regression Corpus` job in the PR check list. Expected duration: ~1–2 minutes (most of which is `cargo` warm cache + `cargo run` for the two CLIs + `cargo test --test harness_per_encounter`).

The job runs `./scripts/preflight.sh --regression`, which executes Layers 6 (detection_replay), 8 (harness), and 9 (labeled_regression). Layer 9 should report `Regressions: 0` — that's the load-bearing assertion.

## Maintenance

### Bumping the gate

To "tighten the ratchet" — tell CI that a previously-known failure has been fixed and is no longer expected:

1. Open `tauri-app/src-tauri/tests/fixtures/labels/<file>.json`.
2. Remove the relevant entry from `labels.expected_failures`.
3. Run locally:
   ```bash
   cd tauri-app && ./scripts/preflight.sh --regression
   ```
   It must exit 0. If the regression returns, the fix didn't stick — investigate before landing.
4. Commit + push. CI will reproduce the local run.

### Re-bootstrapping after corpus growth

When new label files are added (e.g. after a forensic review day) and they have known failures:

```bash
cd tauri-app/src-tauri
cargo run --bin labeled_regression_cli -- --all --bootstrap-expected-failures
git diff tests/fixtures/labels/  # review what got pinned
```

The bootstrap is idempotent — re-running on already-bootstrapped files only changes `expected_failures` if the actual set of failing checks differs from the recorded one.

### Disabling the runner

On the workstation hosting the runner:

```bash
cd ~/actions-runner-ami-ci
./svc.sh stop
./svc.sh uninstall
./config.sh remove --token <REMOVAL_TOKEN>
```

Get the removal token via:
```bash
gh api -X POST repos/zohoora/transcriptionapp/actions/runners/remove-token --jq '.token'
```

## Operating notes

- **CPU contention.** The runner shares the workstation with whoever's working on it. PR-side `--regression` mode is short (~1-2 min, mostly cargo cache hits). If you're running a parallel cargo build, expect transient slowdowns during CI runs. To avoid contention entirely, dedicate a less-used workstation (Room 2 iMac is a candidate) — the labels stay the same.
- **Disk usage.** Each runner workflow checks the repo into `_work/`. Old checkouts persist until cleared. Periodic cleanup:
  ```bash
  cd ~/actions-runner-ami-ci/_work && du -sh */
  ```
- **macOS sleep / lid close.** macOS sleep cancels in-flight runs. For a workstation that's mostly active during the day, this is fine. If the workstation closes its lid overnight, in-flight jobs at that moment will fail; restart the runner to recover. Configure: System Settings → Battery → "Prevent automatic sleeping when display is off."
- **Archive currency.** The runner reads the workstation's local `~/.transcriptionapp/archive/`. As long as the desktop app stays running on this workstation through normal clinic days, the archive grows organically. If you switch the runner to a workstation that hasn't been used recently, expect Layer 9 to surface drift until the new host's archive catches up.
- **What to do if the runner is offline.** The PR `Regression Corpus` check stays in "queued" state until the runner reappears. To force a merge in an emergency, repo admin can disable the required-check rule for that PR or temporarily comment out the `regression-corpus` job in `.github/workflows/ci.yml`. Re-enable as soon as the runner is back.

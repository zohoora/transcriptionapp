# Regression CI Runner Setup

One-time setup to register the MacBook (100.119.83.76, user `arash`) as a self-hosted GitHub Actions runner so the `regression-corpus` CI job can read the production session archive at `~/.transcriptionapp/archive/`.

The runner is paired with the `[self-hosted, macOS, ami-ci]` label set in `.github/workflows/ci.yml`. The `ami-ci` tag exists to disambiguate from any other future MacBook-hosted runners.

## Why a self-hosted runner

The regression corpus (`labeled_regression_cli`, `detection_replay_cli`) compares production output against archived `replay_bundle.json` and `billing.json` files. Those live under `~/.transcriptionapp/archive/` (PHI). They cannot be uploaded to GitHub-hosted runners — the archive is only on the MacBook and on the workstations that sync from it. The MacBook is the canonical copy because it also runs the profile service that all rooms sync to.

## Preconditions

- MacBook is reachable on Tailscale (`100.119.83.76`) AND LAN (`10.241.15.154`).
- Repo is checked out somewhere readable; runner workspace can be a separate clone (recommended: `~/actions-runner-ami-ci/`).
- `cargo` (stable Rust toolchain), `node` (≥ 20), and `pnpm` (≥ 10) installed and on `arash`'s PATH at login. Verify:
  ```bash
  which cargo node pnpm
  ```
- The MacBook's `~/.transcriptionapp/archive/` exists and is writable by `arash`. The CI job only reads, but the binaries it invokes (`detection_replay_cli`, `labeled_regression_cli`) check the path for existence at startup.

## Bring-up

### 1. Generate a registration token

From a workstation logged into GitHub with admin access to the repo:

```bash
gh api -X POST repos/zohoora/transcriptionapp/actions/runners/registration-token \
  --jq '.token'
```

This token is short-lived (~1 hour). Don't commit it.

### 2. Install the runner agent on the MacBook

SSH to the MacBook:

```bash
ssh arash@100.119.83.76
```

Then on the MacBook:

```bash
mkdir -p ~/actions-runner-ami-ci && cd ~/actions-runner-ami-ci

# Latest macOS arm64 runner — bump versions if newer is published
curl -o actions-runner-osx-arm64.tar.gz -L \
  https://github.com/actions/runner/releases/download/v2.330.0/actions-runner-osx-arm64-2.330.0.tar.gz
echo "<sha256 from release notes>  actions-runner-osx-arm64.tar.gz" | shasum -a 256 -c
tar xzf ./actions-runner-osx-arm64.tar.gz

# Configure with the token from step 1 and the AMI Assist labels
./config.sh \
  --url https://github.com/zohoora/transcriptionapp \
  --token <REGISTRATION_TOKEN_FROM_STEP_1> \
  --name ami-ci-macbook \
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

The service is installed under `~/Library/LaunchAgents/actions.runner.zohoora-transcriptionapp.ami-ci-macbook.plist`. It auto-starts on login.

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

```bash
ssh arash@100.119.83.76
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

- **The MacBook also runs the production STT/LLM/profile services.** A misbehaving CI job cannot use those services — `--regression` mode only runs offline replay (no network, no LLM). If a future PR adds a CI step that needs live LLM, scope it carefully so it doesn't compete with clinic traffic.
- **Disk usage.** Each runner workflow checks the repo into `_work/`. Old checkouts persist until cleared. Periodic cleanup:
  ```bash
  ssh arash@100.119.83.76
  cd ~/actions-runner-ami-ci/_work && du -sh */
  ```
- **MacBook sleep / lid close.** macOS sleep cancels in-flight runs. Configure: System Settings → Battery → "Prevent automatic sleeping when display is off." Or use `caffeinate` in a launchd plist if the user keeps closing the lid.
- **What to do if the runner is offline.** The PR `Regression Corpus` check stays in "queued" state until the runner reappears. To force a merge in an emergency, repo admin can disable the required-check rule for that PR or temporarily comment out the `regression-corpus` job in `.github/workflows/ci.yml`. Re-enable as soon as the runner is back.

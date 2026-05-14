---
name: bump-version
description: Bump the AMI Assist version across all three files (tauri.conf.json, package.json, src-tauri/Cargo.toml), commit, tag, and push to trigger the auto-update release workflow
user-invocable: true
disable-model-invocation: true
arguments:
  - name: bump
    description: "Semver bump kind: patch | minor | major (defaults to patch)"
    default: patch
---

# Bump Version & Release

The release ritual for AMI Assist. Tags published to the `zohoora/transcriptionapp` GitHub repo trigger `.github/workflows/release.yml`, which builds + signs the macOS app and publishes a GitHub Release with `latest.json`. Running clinic apps pick up the update on next launch via the Tauri updater plugin.

## Step 1: Resolve current and next version

```bash
grep -E '"version"' /Users/backoffice/transcriptionapp/tauri-app/package.json
grep -E '"version"' /Users/backoffice/transcriptionapp/tauri-app/src-tauri/tauri.conf.json
grep -E '^version' /Users/backoffice/transcriptionapp/tauri-app/src-tauri/Cargo.toml
```

All three MUST report the same version. If they're out of sync, fix that first — out-of-sync versions are how previous releases shipped without ONNX bundling, etc.

Compute the next version per the `bump` argument (default patch). Confirm with the user before proceeding ONLY if the bump is minor or major; for a patch, proceed.

## Step 2: Apply the bump in all three files

```
tauri-app/package.json:           "version": "X.Y.Z"
tauri-app/src-tauri/tauri.conf.json: "version": "X.Y.Z"
tauri-app/src-tauri/Cargo.toml:    version = "X.Y.Z"
```

## Step 3: Verify build is still green

```bash
cd tauri-app/src-tauri && cargo check
```

Plain `cargo check` (no `--lib`) — the release workflow builds the binary targets too (`ort_smoke`, `process_mobile`, the 15 replay/regression CLIs), so a binary-only compile error would slip past `--lib` and break the release. Don't run the full test suite here — assume the user already validated. If `cargo check` fails, abort the release and report.

## Step 4: Stage + commit

Stage ONLY the version files plus the work files for this release. Never use `git add .` or `git add -A` — the user often has WIP in `.claude/` (settings, lock files, in-flight agents/skills) that shouldn't ship. List files explicitly:

```bash
git add tauri-app/package.json tauri-app/src-tauri/Cargo.toml tauri-app/src-tauri/tauri.conf.json \
        <other-files-from-this-session>
```

`tauri-app/src-tauri/Cargo.lock` is gitignored (see top-level `.gitignore`) — don't try to stage it.

The commit message must follow the existing pattern:

```
vX.Y.Z — <one-line summary>

<3-15 bullet body explaining what shipped: defect classes addressed,
features added, key behavior changes, test results>

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

If the user already authored a commit message earlier in the session (for the underlying work), reuse / extend it — don't fabricate a new narrative.

## Step 5: Tag and push

```bash
git tag vX.Y.Z
git push origin main --tags
```

## Step 6: Confirm the release workflow started

```bash
gh run list --workflow=release.yml --limit 1
```

The new tag should appear with status `queued` or `in_progress`. Capture the run ID from the third column and report it to the user — they can stream it with `gh run watch <run-id>` or open `https://github.com/zohoora/transcriptionapp/actions/runs/<run-id>`. Recent releases take ~25 min to build, sign, run `ort_smoke`, and publish.

## Notes

- This skill assumes the user has already finished, type-checked, and tested the work being shipped. It does NOT run the full test suite — that's `/run-checks`.
- If `cargo check` fails OR the three version files diverge OR `git push` fails, abort and report. Never force-push.
- Auto-update relies on Ed25519 signing keys stored as GitHub secrets (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). If a release builds but doesn't sign, the workflow run will show signing errors — surface those to the user.

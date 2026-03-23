---
name: prepare-pr
description: Prepare and create a pull request with automated checks, change categorization, and security review
user-invocable: true
disable-model-invocation: true
arguments:
  - name: base
    description: "Base branch for the PR"
    default: main
---

# Prepare Pull Request

Automated PR preparation workflow: run checks, categorize changes, flag security concerns, create PR.

## Steps

### 1. Pre-flight validation

Run the check suite before creating the PR. Stop if anything fails.

```bash
cd tauri-app

# TypeScript type check
npx tsc --noEmit

# ESLint
npx eslint src/ --max-warnings 0

# Vitest
pnpm test:run

# Rust checks
cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test
```

### 2. Analyze changes

```bash
# What changed since base branch
git diff --stat origin/main...HEAD
git log origin/main..HEAD --oneline

# Categorize files
git diff --name-only origin/main...HEAD
```

**Categories to detect:**
- **Backend**: `src-tauri/src/*.rs`, `src-tauri/src/commands/*.rs`
- **Frontend**: `src/components/*.tsx`, `src/hooks/*.ts`
- **Types**: `src/types/index.ts`
- **Config**: `src-tauri/src/config.rs`
- **Tests**: `*.test.ts`, `*.test.tsx`, `*_tests.rs`
- **Docs**: `CLAUDE.md`, `README.md`, `docs/`
- **CI**: `.github/workflows/`
- **Skills/Agents**: `.claude/skills/`, `.claude/agents/`

### 3. Security scan

Check if any security-sensitive files were modified:

```bash
git diff --name-only origin/main...HEAD | grep -E "(medplum|auth|credentials|permissions|oauth|token|secret|llm_client|continuous_mode|local_archive|config\.rs)"
```

If security-sensitive files changed, note this in the PR description and recommend running the security-reviewer agent.

### 4. Check for common issues

- [ ] No `.env` files staged
- [ ] No `config.json` with real credentials staged
- [ ] `CLAUDE.md` updated if architecture changed
- [ ] New Tauri commands registered in `lib.rs`
- [ ] New windows added to `capabilities/default.json`
- [ ] Frontend types match Rust structs (`#[serde(rename_all = "camelCase")]`)

### 5. Create PR

```bash
# Push branch if not already pushed
git push -u origin HEAD

# Create PR using gh CLI
gh pr create --title "<short title>" --body "$(cat <<'EOF'
## Summary
<1-3 bullet points describing the changes>

## Categories
<list of affected areas: Backend, Frontend, Tests, etc.>

## Security Impact
<none / list of security-sensitive files changed>

## Test plan
- [ ] `pnpm test:run` passes
- [ ] `cargo test` passes
- [ ] Manual testing: <specific scenarios>

Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

## On Failure

If checks fail:
1. Report which check failed
2. Offer to fix the issue
3. Re-run only the failed check
4. Continue once passing

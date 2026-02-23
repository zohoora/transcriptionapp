---
name: commit
description: Create a well-structured git commit with PHI safety checks
user-invocable: true
disable-model-invocation: true
arguments:
  - name: message
    description: Optional commit message (auto-generated if omitted)
---

# Structured Commit

Create a git commit following project conventions with PHI safety checks.

## Pre-Commit Safety

Before staging, scan for PHI risks:

1. **Check staged diff for sensitive patterns**:
   - Files matching `*.env`, `*auth.json`, `*credentials*`, `*secrets*`
   - Hardcoded IP addresses or server URLs that differ from defaults in CLAUDE.md
   - Large binary blobs (audio files, model weights)

2. **If any PHI-risk files are found**: warn the user and do NOT stage them unless explicitly confirmed.

## Commit Message Format

Follow the existing project style (check recent `git log --oneline -10`):
- Imperative mood, present tense ("Fix", "Add", "Update", not "Fixed", "Added")
- First line: concise summary under 72 characters
- Focus on the "why" not the "what"
- No conventional-commits prefix needed (project doesn't use them)

## Process

1. Run `git status` and `git diff --staged` to understand changes
2. Check for PHI-risk files in the diff
3. Run `git log --oneline -10` to match existing message style
4. Stage relevant files by name (never `git add -A` or `git add .`)
5. Draft commit message and show to user for approval
6. Create the commit

## After Commit

Run a quick verification:
```bash
cd tauri-app && npx tsc --noEmit
cd tauri-app/src-tauri && cargo check
```

If either fails, inform the user but do NOT amend — let them decide.

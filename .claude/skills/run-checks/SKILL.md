---
name: run-checks
description: Run all validation checks before committing
user-invocable: true
disable-model-invocation: true
---

# Run Pre-Commit Checks

Run the full validation suite for the transcription app.

## Checklist

Run these checks in order, stopping on first failure:

### 1. TypeScript Type Check
```bash
cd tauri-app && npm run typecheck
```

### 2. ESLint
```bash
cd tauri-app && npm run lint
```

### 3. Vitest Unit Tests
```bash
cd tauri-app && npm run test:run
```

### 4. Rust Clippy (Lints)
```bash
cd tauri-app/src-tauri && cargo clippy --all-targets -- -D warnings
```

### 5. Rust Tests
```bash
cd tauri-app/src-tauri && cargo test
```

## Optional Extended Checks

Only run these if specifically requested:

### Visual Regression Tests
```bash
cd tauri-app && npm run visual:test
```

### E2E Tests
```bash
cd tauri-app && npm run e2e:headless
```

### Mutation Testing
```bash
cd tauri-app && npm run mutation:test
```

## On Failure

If any check fails:
1. Report the specific failure with error output
2. Offer to fix the issue
3. Re-run only the failed check after fixing
4. Continue with remaining checks once fixed

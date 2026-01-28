---
name: gen-test
description: Generate unit/integration tests following project conventions
user-invocable: true
disable-model-invocation: true
arguments:
  - name: file
    description: File to generate tests for
    required: true
  - name: type
    description: Test type (unit, visual, e2e)
    default: unit
---

# Generate Tests

Generate tests for the specified file using project conventions.

## Test Types

### Unit Tests (Vitest) - Default
- **Location**: Same directory as source (`*.test.ts`, `*.test.tsx`)
- **Framework**: Vitest + @testing-library/react
- **A11y**: Include vitest-axe checks for React components

**Reference patterns:**
- React components: `tauri-app/src/App.test.tsx`
- Hooks: `tauri-app/src/hooks/useSoapNote.test.ts`
- Utilities: `tauri-app/src/utils.test.ts`
- Contracts: `tauri-app/src/contracts.test.ts`

**Test structure:**
```typescript
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { axe, toHaveNoViolations } from 'vitest-axe';

expect.extend(toHaveNoViolations);

describe('ComponentName', () => {
  it('should render correctly', () => {
    // test implementation
  });

  it('should have no accessibility violations', async () => {
    const { container } = render(<Component />);
    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });
});
```

### Visual Tests (Playwright)
- **Location**: `tauri-app/tests/visual/`
- **Config**: `tauri-app/tests/visual/playwright.config.ts`
- **Run**: `npm run visual:test`

### E2E Tests (WebdriverIO)
- **Location**: `tauri-app/e2e/specs/`
- **Config**: `tauri-app/e2e/wdio.conf.ts`
- **Run**: `npm run e2e`

### Rust Tests
- **Location**: Same file as source (inline `#[cfg(test)]` module)
- **Reference**: `tauri-app/src-tauri/src/command_tests.rs`
- **Run**: `cargo test` in `tauri-app/src-tauri/`

## Instructions

1. Read the source file to understand its functionality
2. Check existing test files for project patterns
3. Generate comprehensive tests covering:
   - Happy path scenarios
   - Edge cases
   - Error handling
   - For React: accessibility (a11y) checks
4. Place test file in the correct location based on type
5. Run the tests to verify they pass

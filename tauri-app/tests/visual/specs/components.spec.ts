import { test, expect } from '@playwright/test';

/**
 * Component-level Visual Regression Tests
 *
 * Tests individual UI components in isolation for visual consistency.
 */

test.describe('Button Components', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('primary button (Start)', async ({ page }) => {
    const button = page.locator('button:has-text("Start")');
    await button.waitFor();

    // Normal state
    await expect(button).toHaveScreenshot('btn-primary-normal.png');

    // Hover state
    await button.hover();
    await expect(button).toHaveScreenshot('btn-primary-hover.png');

    // Focus state
    await button.focus();
    await expect(button).toHaveScreenshot('btn-primary-focus.png');
  });

  test('disabled button state', async ({ page }) => {
    // Look for any disabled button (might appear in different states)
    const disabledBtn = page.locator('button:disabled').first();

    // Only test if a disabled button exists
    const count = await disabledBtn.count();
    if (count > 0) {
      await expect(disabledBtn).toHaveScreenshot('btn-disabled.png');
    }
  });
});

test.describe('Select Component', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('device select normal state', async ({ page }) => {
    const select = page.locator('select');
    await select.waitFor();

    await expect(select).toHaveScreenshot('select-normal.png');
  });

  test('device select focused state', async ({ page }) => {
    const select = page.locator('select');
    await select.waitFor();
    await select.focus();

    await expect(select).toHaveScreenshot('select-focused.png');
  });
});

test.describe('Status Indicators', () => {
  test('provider badge', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');

    const badge = page.locator('.provider-badge');
    await badge.waitFor();

    await expect(badge).toHaveScreenshot('provider-badge.png');
  });
});

test.describe('Typography', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('placeholder text style', async ({ page }) => {
    const placeholder = page.locator('.transcript-placeholder');
    const count = await placeholder.count();

    if (count > 0) {
      await expect(placeholder).toHaveScreenshot('placeholder-text.png');
    }
  });
});

test.describe('Layout Consistency', () => {
  test('controls alignment', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');

    const controls = page.locator('.controls');
    await controls.waitFor();

    await expect(controls).toHaveScreenshot('controls-alignment.png');
  });

  test('header alignment', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');

    const header = page.locator('.header');
    await header.waitFor();

    await expect(header).toHaveScreenshot('header-alignment.png');
  });
});

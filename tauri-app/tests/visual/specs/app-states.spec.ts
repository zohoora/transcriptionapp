import { test, expect } from '@playwright/test';

/**
 * Visual Regression Tests
 *
 * These tests capture screenshots of the app in various states
 * and compare them against baseline images. Run with:
 *
 *   pnpm visual:test
 *
 * To update baselines after intentional changes:
 *
 *   pnpm visual:update
 */

test.describe('App Visual States', () => {
  test.beforeEach(async ({ page }) => {
    // Wait for app to fully load
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('idle state appearance', async ({ page }) => {
    // Wait for the Start button to be visible
    await page.waitForSelector('button:has-text("Start")');

    // Take full page screenshot
    await expect(page).toHaveScreenshot('idle-state.png', {
      fullPage: true,
    });
  });

  test('device selector dropdown', async ({ page }) => {
    await page.waitForSelector('select');

    // Focus the select to show it's interactive
    await page.focus('select');

    await expect(page).toHaveScreenshot('device-selector-focused.png', {
      fullPage: true,
    });
  });

  test('start button hover state', async ({ page }) => {
    const startButton = page.locator('button:has-text("Start")');
    await startButton.waitFor();

    // Hover over the button
    await startButton.hover();

    await expect(page).toHaveScreenshot('start-button-hover.png', {
      fullPage: true,
    });
  });

  test('header layout', async ({ page }) => {
    const header = page.locator('header');
    await header.waitFor();

    await expect(header).toHaveScreenshot('header-layout.png');
  });

  test('footer controls layout', async ({ page }) => {
    const footer = page.locator('footer');
    await footer.waitFor();

    await expect(footer).toHaveScreenshot('footer-controls.png');
  });

  test('transcript area empty state', async ({ page }) => {
    const main = page.locator('main');
    await main.waitFor();

    await expect(main).toHaveScreenshot('transcript-empty.png');
  });
});

test.describe('Responsive Layouts', () => {
  test('mobile viewport (375px)', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.waitForSelector('button:has-text("Start")');

    await expect(page).toHaveScreenshot('mobile-375.png', {
      fullPage: true,
    });
  });

  test('tablet viewport (768px)', async ({ page }) => {
    await page.setViewportSize({ width: 768, height: 1024 });
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.waitForSelector('button:has-text("Start")');

    await expect(page).toHaveScreenshot('tablet-768.png', {
      fullPage: true,
    });
  });

  test('desktop viewport (1280px)', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.waitForSelector('button:has-text("Start")');

    await expect(page).toHaveScreenshot('desktop-1280.png', {
      fullPage: true,
    });
  });
});

test.describe('Dark Mode', () => {
  test('dark color scheme', async ({ page }) => {
    // Emulate dark mode preference
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.waitForSelector('button:has-text("Start")');

    await expect(page).toHaveScreenshot('dark-mode.png', {
      fullPage: true,
    });
  });

  test('light color scheme', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.waitForSelector('button:has-text("Start")');

    await expect(page).toHaveScreenshot('light-mode.png', {
      fullPage: true,
    });
  });
});

test.describe('Accessibility Visual Tests', () => {
  test('high contrast mode', async ({ page }) => {
    await page.emulateMedia({ forcedColors: 'active' });
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.waitForSelector('button:has-text("Start")');

    await expect(page).toHaveScreenshot('high-contrast.png', {
      fullPage: true,
    });
  });

  test('reduced motion', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.waitForSelector('button:has-text("Start")');

    await expect(page).toHaveScreenshot('reduced-motion.png', {
      fullPage: true,
    });
  });

  test('focus indicators visible', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');

    // Tab through elements to show focus
    await page.keyboard.press('Tab');
    await page.keyboard.press('Tab');

    await expect(page).toHaveScreenshot('focus-indicators.png', {
      fullPage: true,
    });
  });
});

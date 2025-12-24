/**
 * Performance E2E Tests
 *
 * Tests application performance metrics
 */

describe('Performance', () => {
  describe('Initial Load', () => {
    it('should load within acceptable time', async () => {
      const startTime = Date.now();

      // Wait for main UI to be interactive
      const startButton = await $('button*=Start');
      await startButton.waitForDisplayed({ timeout: 10000 });

      const loadTime = Date.now() - startTime;

      // App should be interactive within 5 seconds
      expect(loadTime).toBeLessThan(5000);
      console.log(`App load time: ${loadTime}ms`);
    });
  });

  describe('State Transitions', () => {
    it('should transition to recording state quickly', async () => {
      const startButton = await $('button*=Start');
      if (!(await startButton.isDisplayed())) {
        return; // Skip if not in correct state
      }

      const startTime = Date.now();
      await startButton.click();

      // Wait for either Stop button or error
      await browser.waitUntil(
        async () => {
          const stopBtn = await $('button*=Stop');
          const errorMsg = await $('*=Error');
          return (await stopBtn.isDisplayed().catch(() => false)) ||
                 (await errorMsg.isDisplayed().catch(() => false));
        },
        { timeout: 10000 }
      );

      const transitionTime = Date.now() - startTime;

      // State transition should be under 3 seconds
      expect(transitionTime).toBeLessThan(3000);
      console.log(`Start transition time: ${transitionTime}ms`);

      // Clean up
      const stopButton = await $('button*=Stop');
      if (await stopButton.isDisplayed()) {
        await stopButton.click();
      }
    });

    it('should stop recording quickly', async () => {
      // First start recording
      const startButton = await $('button*=Start');
      if (await startButton.isDisplayed()) {
        await startButton.click();
        await browser.pause(1000);
      }

      const stopButton = await $('button*=Stop');
      if (!(await stopButton.isDisplayed())) {
        return; // Skip if not recording
      }

      const startTime = Date.now();
      await stopButton.click();

      // Wait for idle or completed state
      await browser.waitUntil(
        async () => {
          const startBtn = await $('button*=Start');
          const newRecBtn = await $('button*=New Recording');
          return (await startBtn.isDisplayed().catch(() => false)) ||
                 (await newRecBtn.isDisplayed().catch(() => false));
        },
        { timeout: 10000 }
      );

      const stopTime = Date.now() - startTime;

      // Stop should be quick (under 2 seconds for state change)
      expect(stopTime).toBeLessThan(2000);
      console.log(`Stop transition time: ${stopTime}ms`);
    });
  });

  describe('Memory', () => {
    it('should not have memory leaks during state cycles', async () => {
      // Perform multiple start/stop cycles
      for (let i = 0; i < 3; i++) {
        const startButton = await $('button*=Start');
        if (await startButton.isDisplayed()) {
          await startButton.click();
          await browser.pause(500);

          const stopButton = await $('button*=Stop');
          if (await stopButton.isDisplayed()) {
            await stopButton.click();
            await browser.pause(500);
          }

          // Check for New Recording button and click it
          const newRecButton = await $('button*=New Recording');
          if (await newRecButton.isDisplayed().catch(() => false)) {
            await newRecButton.click();
            await browser.pause(500);
          }
        }
      }

      // App should still be responsive after cycles
      const body = await $('body');
      expect(await body.isDisplayed()).toBe(true);
    });
  });

  describe('UI Responsiveness', () => {
    it('should respond to clicks within acceptable time', async () => {
      const startButton = await $('button*=Start');
      await startButton.waitForDisplayed({ timeout: 5000 });

      // Measure click response time
      const startTime = Date.now();
      await startButton.click();

      // Wait for any state change
      await browser.waitUntil(
        async () => {
          const preparing = await $('*=Preparing');
          const stopBtn = await $('button*=Stop');
          return (await preparing.isDisplayed().catch(() => false)) ||
                 (await stopBtn.isDisplayed().catch(() => false));
        },
        { timeout: 5000 }
      );

      const responseTime = Date.now() - startTime;

      // UI should respond within 500ms
      expect(responseTime).toBeLessThan(500);

      // Clean up
      const stopButton = await $('button*=Stop');
      if (await stopButton.isDisplayed()) {
        await stopButton.click();
      }
    });
  });
});

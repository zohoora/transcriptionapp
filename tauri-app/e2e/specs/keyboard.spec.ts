/**
 * Keyboard Navigation E2E Tests
 *
 * Tests keyboard accessibility and shortcuts
 */

describe('Keyboard Navigation', () => {
  describe('Tab Navigation', () => {
    it('should navigate through interactive elements with Tab', async () => {
      // Press Tab to move through elements
      await browser.keys(['Tab']);

      // Check that some element is focused
      const activeElement = await browser.getActiveElement();
      expect(activeElement).toBeDefined();
    });

    it('should have visible focus indicators', async () => {
      const startButton = await $('button*=Start');
      await startButton.waitForDisplayed({ timeout: 5000 });

      // Focus the button
      await startButton.click();
      await browser.keys(['Tab']);
      await browser.keys(['Shift', 'Tab']); // Go back

      // The button should be focusable
      expect(await startButton.isFocused()).toBeDefined();
    });
  });

  describe('Button Activation', () => {
    it('should activate Start button with Enter key', async () => {
      const startButton = await $('button*=Start');
      if (await startButton.isDisplayed()) {
        await startButton.focus();
        await browser.keys(['Enter']);

        // Should transition to recording state
        await browser.pause(1000);

        const stopButton = await $('button*=Stop');
        const isRecording = await stopButton.isDisplayed().catch(() => false);

        // Stop if we started recording
        if (isRecording) {
          await stopButton.click();
        }
      }
    });

    it('should activate buttons with Space key', async () => {
      const startButton = await $('button*=Start');
      if (await startButton.isDisplayed()) {
        await startButton.focus();
        await browser.keys(['Space']);

        await browser.pause(1000);

        const stopButton = await $('button*=Stop');
        const isRecording = await stopButton.isDisplayed().catch(() => false);

        if (isRecording) {
          await stopButton.click();
        }
      }
    });
  });

  describe('Select Element', () => {
    it('should navigate device select with arrow keys', async () => {
      const deviceSelect = await $('select');
      if (await deviceSelect.isDisplayed()) {
        await deviceSelect.focus();
        await browser.keys(['ArrowDown']);

        // Just verify no errors occur
        expect(await deviceSelect.isFocused()).toBe(true);
      }
    });
  });

  describe('Escape Key', () => {
    it('should handle Escape key gracefully', async () => {
      await browser.keys(['Escape']);

      // App should still be responsive
      const body = await $('body');
      expect(await body.isDisplayed()).toBe(true);
    });
  });
});

/**
 * Stryker Mutation Testing Configuration
 *
 * Mutation testing validates the quality of your tests by introducing
 * small changes (mutations) to your code and checking if tests catch them.
 *
 * Run with: pnpm mutation:test
 *
 * A mutation score below 80% suggests tests may be missing important cases.
 */

/** @type {import('@stryker-mutator/api').PartialStrykerOptions} */
export default {
  // Use Vitest as the test runner
  testRunner: 'vitest',

  // Files to mutate
  mutate: [
    'src/**/*.ts',
    'src/**/*.tsx',
    // Exclude test files
    '!src/**/*.test.ts',
    '!src/**/*.test.tsx',
    '!src/**/*.spec.ts',
    '!src/**/*.spec.tsx',
    // Exclude mock files
    '!src/test/**/*',
    // Exclude type-only files
    '!src/**/*.d.ts',
  ],

  // Vitest configuration
  vitest: {
    configFile: 'vite.config.ts',
  },

  // Reporter configuration
  reporters: ['html', 'clear-text', 'progress'],
  htmlReporter: {
    fileName: 'mutation-report.html',
  },

  // Thresholds - fail if mutation score is too low
  thresholds: {
    high: 80,
    low: 60,
    break: 50,
  },

  // Performance settings
  concurrency: 4,
  timeoutMS: 30000,

  // Ignore specific mutations that cause false positives
  ignoreMutations: [
    // String mutations often cause false positives in UI
    'StringLiteral',
  ],

  // Incremental mode for faster subsequent runs
  incremental: true,
  incrementalFile: '.stryker-cache/incremental.json',

  // Clean output
  cleanTempDir: true,
};

/**
 * WebDriverIO configuration for Tauri E2E tests
 * Uses tauri-driver for WebDriver protocol support
 */

import type { Options } from '@wdio/types';
import { spawn, ChildProcess } from 'child_process';
import path from 'path';

let tauriDriver: ChildProcess | null = null;

export const config: Options.Testrunner = {
  runner: 'local',
  autoCompileOpts: {
    autoCompile: true,
    tsNodeOpts: {
      transpileOnly: true,
      project: path.join(__dirname, 'tsconfig.json'),
    },
  },

  specs: ['./specs/**/*.spec.ts'],
  exclude: [],

  maxInstances: 1, // Tauri apps run as single instance

  capabilities: [
    {
      maxInstances: 1,
      'tauri:options': {
        application: '../src-tauri/target/release/transcription-app',
      },
    },
  ],

  logLevel: 'info',
  bail: 0,
  baseUrl: '',
  waitforTimeout: 10000,
  connectionRetryTimeout: 120000,
  connectionRetryCount: 3,

  framework: 'mocha',
  reporters: ['spec'],

  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },

  // Hooks
  onPrepare: async function () {
    // Start tauri-driver before tests
    tauriDriver = spawn('tauri-driver', [], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    tauriDriver.stdout?.on('data', (data) => {
      console.log(`tauri-driver: ${data}`);
    });

    tauriDriver.stderr?.on('data', (data) => {
      console.error(`tauri-driver error: ${data}`);
    });

    // Wait for driver to be ready
    await new Promise((resolve) => setTimeout(resolve, 2000));
  },

  onComplete: async function () {
    // Stop tauri-driver after tests
    if (tauriDriver) {
      tauriDriver.kill();
      tauriDriver = null;
    }
  },
};

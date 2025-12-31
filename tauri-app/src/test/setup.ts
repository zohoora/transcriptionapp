import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock @tauri-apps/api/core
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

// Mock @tauri-apps/api/event
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

// Mock @tauri-apps/plugin-clipboard-manager
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: vi.fn(() => Promise.resolve()),
  readText: vi.fn(() => Promise.resolve('')),
}));

// Mock @tauri-apps/api/webviewWindow for history window
const mockWebviewWindow = vi.fn().mockImplementation(() => ({
  once: vi.fn(),
  show: vi.fn(() => Promise.resolve()),
  setFocus: vi.fn(() => Promise.resolve()),
}));
// Add static method
(mockWebviewWindow as { getByLabel?: ReturnType<typeof vi.fn> }).getByLabel = vi.fn(() => Promise.resolve(null));

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: mockWebviewWindow,
}));

// Mock AuthProvider's useAuth hook
// Path is relative to the module being tested (App.tsx is in src/)
vi.mock('../components/AuthProvider', () => ({
  useAuth: vi.fn(() => ({
    authState: {
      is_authenticated: false,
      access_token: null,
      refresh_token: null,
      token_expiry: null,
      practitioner_id: null,
      practitioner_name: null,
    },
    isLoading: false,
    error: null,
    login: vi.fn(() => Promise.resolve()),
    logout: vi.fn(() => Promise.resolve()),
    refreshAuth: vi.fn(() => Promise.resolve()),
  })),
  AuthProvider: ({ children }: { children: React.ReactNode }) => children,
}));

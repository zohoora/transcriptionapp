/**
 * AuthProvider - Medplum Authentication Context
 *
 * Provides authentication state and actions throughout the app:
 * - OAuth login/logout flow
 * - Token refresh handling
 * - Auth state persistence
 */

import React, { createContext, useContext, useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-shell';
import { onOpenUrl, getCurrent } from '@tauri-apps/plugin-deep-link';
import type { AuthState, AuthUrl } from '../types';
import { formatErrorMessage } from '../utils';

interface AuthContextType {
  authState: AuthState;
  isLoading: boolean;
  error: string | null;
  login: () => Promise<void>;
  logout: () => Promise<void>;
  refreshAuth: () => Promise<void>;
  cancelLogin: () => void;
}

const defaultAuthState: AuthState = {
  is_authenticated: false,
  access_token: null,
  refresh_token: null,
  token_expiry: null,
  practitioner_id: null,
  practitioner_name: null,
};

const AuthContext = createContext<AuthContextType | null>(null);

export function useAuth(): AuthContextType {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}

interface AuthProviderProps {
  children: React.ReactNode;
}

export function AuthProvider({ children }: AuthProviderProps) {
  const [authState, setAuthState] = useState<AuthState>(defaultAuthState);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Check initial auth state on mount
  useEffect(() => {
    checkAuthState();
  }, []);

  // Handle OAuth callback from deep link URL
  const processDeepLink = useCallback((url: string) => {
    // Log path only, not query params (may contain OAuth code/state)
    console.log('Processing deep link:', url.split('?')[0]);
    if (url.startsWith('fabricscribe://oauth/callback')) {
      handleOAuthCallback(url);
    }
  }, []);

  // Check for deep links at startup (if app was launched via deep link)
  useEffect(() => {
    getCurrent().then((urls) => {
      if (urls && urls.length > 0) {
        // Log count only, not URLs (may contain OAuth code/state)
        console.log('Deep links at startup:', urls.length);
        for (const url of urls) {
          processDeepLink(url);
        }
      }
    }).catch((err) => {
      console.error('Failed to get current deep links:', err);
    });
  }, [processDeepLink]);

  // Set up deep link listener for OAuth callback
  useEffect(() => {
    // Listen for deep links from the plugin (when app is already running)
    const unlistenPlugin = onOpenUrl((urls) => {
      // Log count only, not URLs (may contain OAuth code/state)
      console.log('Deep link received via plugin:', urls.length);
      for (const url of urls) {
        processDeepLink(url);
      }
    });

    // Also listen for deep links from single-instance callback (when second instance tries to launch)
    const unlistenEvent = listen<string>('deep-link', (event) => {
      // Log path only, not query params (may contain OAuth code/state)
      console.log('Deep link received via event:', event.payload.split('?')[0]);
      processDeepLink(event.payload);
    });

    return () => {
      unlistenPlugin.then(fn => fn());
      unlistenEvent.then(fn => fn());
    };
  }, [processDeepLink]);

  // Set up token refresh timer
  useEffect(() => {
    if (!authState.is_authenticated || !authState.token_expiry) {
      return;
    }

    const expiryTime = authState.token_expiry * 1000; // Convert to ms
    const now = Date.now();
    const refreshBuffer = 5 * 60 * 1000; // 5 minutes before expiry
    const timeUntilRefresh = expiryTime - now - refreshBuffer;

    if (timeUntilRefresh <= 0) {
      // Token is about to expire or already expired, refresh now
      refreshAuth();
      return;
    }

    const timer = setTimeout(() => {
      refreshAuth();
    }, timeUntilRefresh);

    return () => clearTimeout(timer);
  }, [authState.token_expiry, authState.is_authenticated]);

  const checkAuthState = async () => {
    try {
      setIsLoading(true);
      // Try to restore session from saved file (auto-refreshes if expired)
      const state = await invoke<AuthState>('medplum_try_restore_session');
      setAuthState(state);
      setError(null);
    } catch (e) {
      console.error('Failed to restore auth state:', e);
      setAuthState(defaultAuthState);
    } finally {
      setIsLoading(false);
    }
  };

  const handleOAuthCallback = async (url: string) => {
    try {
      setIsLoading(true);
      setError(null);

      // Parse the callback URL
      const urlObj = new URL(url);
      const code = urlObj.searchParams.get('code');
      const state = urlObj.searchParams.get('state');
      const errorParam = urlObj.searchParams.get('error');

      if (errorParam) {
        throw new Error(`OAuth error: ${errorParam}`);
      }

      if (!code || !state) {
        throw new Error('Missing code or state in OAuth callback');
      }

      // Exchange code for tokens
      const newState = await invoke<AuthState>('medplum_handle_callback', { code, state });
      setAuthState(newState);
    } catch (e) {
      const errorMsg = formatErrorMessage(e);
      setError(errorMsg);
      console.error('OAuth callback error:', e);
    } finally {
      setIsLoading(false);
    }
  };

  const login = useCallback(async () => {
    try {
      setIsLoading(true);
      setError(null);

      // Start OAuth flow and get authorization URL
      const authUrl = await invoke<AuthUrl>('medplum_start_auth');

      // Open the authorization URL in the default browser
      await open(authUrl.url);

      // The callback will be handled by the deep link listener
    } catch (e) {
      const errorMsg = formatErrorMessage(e);
      setError(errorMsg);
      console.error('Login error:', e);
      setIsLoading(false);
    }
  }, []);

  const logout = useCallback(async () => {
    try {
      setIsLoading(true);
      await invoke('medplum_logout');
      setAuthState(defaultAuthState);
      setError(null);
    } catch (e) {
      const errorMsg = formatErrorMessage(e);
      setError(errorMsg);
      console.error('Logout error:', e);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const refreshAuth = useCallback(async () => {
    try {
      const newState = await invoke<AuthState>('medplum_refresh_token');
      setAuthState(newState);
      setError(null);
    } catch (e) {
      const errorMsg = formatErrorMessage(e);
      console.error('Token refresh error:', e);
      // If refresh fails, clear auth state
      setAuthState(defaultAuthState);
      setError(errorMsg);
    }
  }, []);

  // Cancel an in-progress login (e.g., if user closes OAuth browser)
  const cancelLogin = useCallback(() => {
    setIsLoading(false);
    setError(null);
  }, []);

  const value: AuthContextType = {
    authState,
    isLoading,
    error,
    login,
    logout,
    refreshAuth,
    cancelLogin,
  };

  return (
    <AuthContext.Provider value={value}>
      {children}
    </AuthContext.Provider>
  );
}

export default AuthProvider;

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
import { open } from '@tauri-apps/plugin-shell';
import { onOpenUrl } from '@tauri-apps/plugin-deep-link';
import type { AuthState, AuthUrl } from '../types';

interface AuthContextType {
  authState: AuthState;
  isLoading: boolean;
  error: string | null;
  login: () => Promise<void>;
  logout: () => Promise<void>;
  refreshAuth: () => Promise<void>;
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

  // Set up deep link listener for OAuth callback
  useEffect(() => {
    const unlisten = onOpenUrl((urls) => {
      for (const url of urls) {
        if (url.startsWith('fabricscribe://oauth/callback')) {
          handleOAuthCallback(url);
          break;
        }
      }
    });

    return () => {
      unlisten.then(fn => fn());
    };
  }, []);

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
      const errorMsg = e instanceof Error ? e.message : String(e);
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
      const errorMsg = e instanceof Error ? e.message : String(e);
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
      const errorMsg = e instanceof Error ? e.message : String(e);
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
      const errorMsg = e instanceof Error ? e.message : String(e);
      console.error('Token refresh error:', e);
      // If refresh fails, clear auth state
      setAuthState(defaultAuthState);
      setError(errorMsg);
    }
  }, []);

  const value: AuthContextType = {
    authState,
    isLoading,
    error,
    login,
    logout,
    refreshAuth,
  };

  return (
    <AuthContext.Provider value={value}>
      {children}
    </AuthContext.Provider>
  );
}

export default AuthProvider;

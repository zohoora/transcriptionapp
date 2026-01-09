import { memo } from 'react';
import type { AuthState, SyncedEncounter } from '../types';

interface SyncStatusBarProps {
  authState: AuthState;
  authLoading: boolean;
  onLogin: () => void;
  onCancelLogin: () => void;

  // Sync state
  isSyncing: boolean;
  syncSuccess: boolean;
  syncError: string | null;
  syncedEncounter: SyncedEncounter | null;
  isAddingSoap: boolean;
  onClearSyncError: () => void;

  // Auto-sync config
  autoSyncEnabled: boolean;
}

/**
 * Unified sync status bar shown at the bottom of ReviewMode.
 * - When not authenticated: shows login prompt
 * - When authenticated: shows sync status (auto-sync handles the rest)
 */
export const SyncStatusBar = memo(function SyncStatusBar({
  authState,
  authLoading,
  onLogin,
  onCancelLogin,
  isSyncing,
  syncSuccess,
  syncError,
  syncedEncounter,
  isAddingSoap,
  onClearSyncError,
  autoSyncEnabled,
}: SyncStatusBarProps) {
  // Not authenticated - show login prompt
  if (!authState.is_authenticated) {
    if (!autoSyncEnabled) {
      return null; // Auto-sync disabled and not logged in - nothing to show
    }

    return (
      <div className="sync-status-bar login">
        <span className="sync-status-message">Sign in to sync to Medplum</span>
        <div className="sync-status-actions">
          <button
            className="btn-signin-small"
            onClick={onLogin}
            disabled={authLoading}
          >
            {authLoading ? 'Signing in...' : 'Sign In'}
          </button>
          {authLoading && (
            <button className="btn-cancel-small" onClick={onCancelLogin}>
              Cancel
            </button>
          )}
        </div>
      </div>
    );
  }

  // Error state
  if (syncError) {
    return (
      <div className="sync-status-bar error">
        <span className="sync-status-icon">!</span>
        <span className="sync-status-message">{syncError}</span>
        <button className="btn-dismiss" onClick={onClearSyncError}>
          Dismiss
        </button>
      </div>
    );
  }

  // Syncing states
  if (isSyncing) {
    return (
      <div className="sync-status-bar syncing">
        <div className="spinner-tiny" />
        <span className="sync-status-message">Syncing to Medplum...</span>
      </div>
    );
  }

  if (isAddingSoap) {
    return (
      <div className="sync-status-bar syncing">
        <div className="spinner-tiny" />
        <span className="sync-status-message">Adding SOAP note...</span>
      </div>
    );
  }

  // Synced states
  if (syncedEncounter?.hasSoap) {
    return (
      <div className="sync-status-bar synced">
        <span className="sync-status-icon">✓</span>
        <span className="sync-status-message">Synced with SOAP note</span>
      </div>
    );
  }

  if (syncedEncounter || syncSuccess) {
    return (
      <div className="sync-status-bar synced">
        <span className="sync-status-icon">✓</span>
        <span className="sync-status-message">Synced to Medplum</span>
      </div>
    );
  }

  // Waiting for auto-sync (authenticated but not yet synced)
  if (autoSyncEnabled) {
    return (
      <div className="sync-status-bar pending">
        <span className="sync-status-message">Will sync automatically</span>
      </div>
    );
  }

  // Auto-sync disabled but authenticated - no status to show
  return null;
});

export default SyncStatusBar;

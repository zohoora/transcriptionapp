import { memo, useState, useEffect } from 'react';
import { getVersion } from '@tauri-apps/api/app';

/** Sync status for the header indicator */
export type SyncStatus = 'idle' | 'syncing' | 'success' | 'error';

interface HeaderProps {
  statusDotClass: string;
  showSettings: boolean;
  disabled: boolean;
  onHistoryClick: () => void;
  onSettingsClick: () => void;
  /** Current sync status */
  syncStatus?: SyncStatus;
  /** Error message if sync failed */
  syncError?: string | null;
  /** Callback to dismiss sync status */
  onDismissSync?: () => void;
}

/**
 * App header with status indicator, title, and action buttons.
 * Displays recording status and provides access to history and settings.
 */
export const Header = memo(function Header({
  statusDotClass,
  showSettings,
  disabled,
  onHistoryClick,
  onSettingsClick,
  syncStatus = 'idle',
  syncError,
  onDismissSync,
}: HeaderProps) {
  const [version, setVersion] = useState('');
  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  return (
    <header className="header">
      <div className="header-left">
        <span className={`status-dot ${statusDotClass}`} />
        <span className="app-title">AMI Assist</span>
        {version && <span className="app-version">v{version}</span>}
      </div>

      {/* Sync Status Indicator */}
      {syncStatus !== 'idle' && (
        <div className={`sync-indicator sync-${syncStatus}`} title={syncError || undefined}>
          {syncStatus === 'syncing' && (
            <>
              <span className="sync-spinner" />
              <span className="sync-text">Syncing...</span>
            </>
          )}
          {syncStatus === 'success' && (
            <>
              <span className="sync-icon">✓</span>
              <span className="sync-text">Synced</span>
              {onDismissSync && (
                <button className="sync-dismiss" onClick={onDismissSync} aria-label="Dismiss">×</button>
              )}
            </>
          )}
          {syncStatus === 'error' && (
            <>
              <span className="sync-icon">!</span>
              <span className="sync-text">Sync failed</span>
              {onDismissSync && (
                <button className="sync-dismiss" onClick={onDismissSync} aria-label="Dismiss">×</button>
              )}
            </>
          )}
        </div>
      )}

      <div className="header-buttons">
        <button
          className="history-btn"
          onClick={onHistoryClick}
          aria-label="Session History"
          disabled={disabled}
          title={disabled ? 'History disabled during recording' : 'Session History'}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
            <line x1="16" y1="2" x2="16" y2="6" />
            <line x1="8" y1="2" x2="8" y2="6" />
            <line x1="3" y1="10" x2="21" y2="10" />
          </svg>
        </button>
        <button
          className={`settings-btn ${showSettings ? 'active' : ''}`}
          onClick={onSettingsClick}
          aria-label="Settings"
          disabled={disabled}
          title={disabled ? 'Settings disabled during recording' : 'Settings'}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
          </svg>
        </button>
      </div>
    </header>
  );
});

export default Header;

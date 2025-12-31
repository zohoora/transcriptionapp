import { memo } from 'react';

interface HeaderProps {
  statusDotClass: string;
  showSettings: boolean;
  disabled: boolean;
  onHistoryClick: () => void;
  onSettingsClick: () => void;
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
}: HeaderProps) {
  return (
    <header className="header">
      <div className="header-left">
        <span className={`status-dot ${statusDotClass}`} />
        <span className="app-title">Scribe</span>
      </div>
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
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="3" />
            <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
          </svg>
        </button>
      </div>
    </header>
  );
});

export default Header;

import { memo } from 'react';
import type { Device, OllamaStatus, AuthState } from '../types';

// Whisper models available for transcription
const WHISPER_MODELS = [
  { value: 'tiny', label: 'Tiny (fastest)' },
  { value: 'base', label: 'Base' },
  { value: 'small', label: 'Small (recommended)' },
  { value: 'medium', label: 'Medium' },
  { value: 'large', label: 'Large (best quality)' },
];

// Supported languages
const LANGUAGES = [
  { value: 'en', label: 'English' },
  { value: 'fa', label: 'Persian (Farsi)' },
  { value: 'es', label: 'Spanish' },
  { value: 'fr', label: 'French' },
  { value: 'de', label: 'German' },
  { value: 'zh', label: 'Chinese' },
  { value: 'ja', label: 'Japanese' },
  { value: 'auto', label: 'Auto-detect' },
];

export interface PendingSettings {
  model: string;
  language: string;
  device: string;
  diarization_enabled: boolean;
  max_speakers: number;
  ollama_server_url: string;
  ollama_model: string;
  medplum_server_url: string;
  medplum_client_id: string;
  medplum_auto_sync: boolean;
}

interface SettingsDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  pendingSettings: PendingSettings | null;
  onSettingsChange: (settings: PendingSettings) => void;
  onSave: () => void;
  devices: Device[];

  // Biomarkers toggle
  showBiomarkers: boolean;
  onShowBiomarkersChange: (show: boolean) => void;

  // Ollama settings
  ollamaStatus: OllamaStatus | null;
  ollamaModels: string[];
  onTestOllama: () => void;

  // Medplum settings
  medplumConnected: boolean;
  medplumError: string | null;
  onTestMedplum: () => void;

  // Auth
  authState: AuthState;
  authLoading: boolean;
  onLogin: () => void;
  onLogout: () => void;
}

/**
 * Settings drawer for configuring transcription, SOAP generation, and EMR sync.
 */
export const SettingsDrawer = memo(function SettingsDrawer({
  isOpen,
  onClose,
  pendingSettings,
  onSettingsChange,
  onSave,
  devices,
  showBiomarkers,
  onShowBiomarkersChange,
  ollamaStatus,
  ollamaModels,
  onTestOllama,
  medplumConnected,
  medplumError,
  onTestMedplum,
  authState,
  authLoading,
  onLogin,
  onLogout,
}: SettingsDrawerProps) {
  if (!isOpen) return null;

  return (
    <>
      <div className="settings-overlay" onClick={onClose} />
      <div className="settings-drawer">
        <div className="settings-drawer-header">
          <span className="settings-drawer-title">Settings</span>
          <button className="close-btn" onClick={onClose}>
            &times;
          </button>
        </div>
        <div className="settings-drawer-content">
          {pendingSettings && (
            <>
              <div className="settings-group">
                <label className="settings-label" htmlFor="model-select">Model</label>
                <select
                  id="model-select"
                  className="settings-select"
                  value={pendingSettings.model}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, model: e.target.value })}
                >
                  {WHISPER_MODELS.map((m) => (
                    <option key={m.value} value={m.value}>
                      {m.label}
                    </option>
                  ))}
                </select>
              </div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="language-select">Language</label>
                <select
                  id="language-select"
                  className="settings-select"
                  value={pendingSettings.language}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, language: e.target.value })}
                >
                  {LANGUAGES.map((l) => (
                    <option key={l.value} value={l.value}>
                      {l.label}
                    </option>
                  ))}
                </select>
              </div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="microphone-select">Microphone</label>
                <select
                  id="microphone-select"
                  className="settings-select"
                  value={pendingSettings.device}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, device: e.target.value })}
                >
                  <option value="default">Default</option>
                  {devices.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.name}
                    </option>
                  ))}
                </select>
              </div>

              <div className="settings-group">
                <div className="settings-toggle">
                  <span className="settings-label" style={{ marginBottom: 0 }}>Speaker Detection</span>
                  <label className="toggle-switch">
                    <input
                      type="checkbox"
                      checked={pendingSettings.diarization_enabled}
                      onChange={(e) =>
                        onSettingsChange({ ...pendingSettings, diarization_enabled: e.target.checked })
                      }
                      aria-label="Enable speaker detection"
                    />
                    <span className="toggle-slider"></span>
                  </label>
                </div>
              </div>

              {pendingSettings.diarization_enabled && (
                <div className="settings-group">
                  <label className="settings-label" htmlFor="max-speakers-slider">Max Speakers</label>
                  <div className="settings-slider">
                    <input
                      id="max-speakers-slider"
                      type="range"
                      min="2"
                      max="10"
                      value={pendingSettings.max_speakers}
                      onChange={(e) =>
                        onSettingsChange({ ...pendingSettings, max_speakers: parseInt(e.target.value) })
                      }
                    />
                    <span className="slider-value">{pendingSettings.max_speakers}</span>
                  </div>
                </div>
              )}

              <div className="settings-group">
                <div className="settings-toggle">
                  <span className="settings-label" style={{ marginBottom: 0 }}>Show Biomarkers</span>
                  <label className="toggle-switch">
                    <input
                      type="checkbox"
                      checked={showBiomarkers}
                      onChange={(e) => onShowBiomarkersChange(e.target.checked)}
                      aria-label="Show biomarkers panel"
                    />
                    <span className="toggle-slider"></span>
                  </label>
                </div>
              </div>

              {/* Ollama / SOAP Note Settings */}
              <div className="settings-divider" />
              <div className="settings-section-title">SOAP Note Generation</div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="ollama-url">Ollama Server</label>
                <input
                  id="ollama-url"
                  type="text"
                  className="settings-input"
                  value={pendingSettings.ollama_server_url}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, ollama_server_url: e.target.value })}
                  placeholder="http://localhost:11434"
                />
              </div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="ollama-model">Model</label>
                <select
                  id="ollama-model"
                  className="settings-select"
                  value={pendingSettings.ollama_model}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, ollama_model: e.target.value })}
                >
                  <option value={pendingSettings.ollama_model}>{pendingSettings.ollama_model}</option>
                  {ollamaModels
                    .filter((m) => m !== pendingSettings.ollama_model)
                    .map((m) => (
                      <option key={m} value={m}>{m}</option>
                    ))}
                </select>
              </div>

              <div className="settings-group ollama-status-group">
                <div className="ollama-status">
                  <span className={`status-indicator ${ollamaStatus?.connected ? 'connected' : 'disconnected'}`} />
                  <span className="status-text">
                    {ollamaStatus?.connected
                      ? `Connected (${ollamaModels.length} models)`
                      : ollamaStatus?.error || 'Not connected'}
                  </span>
                </div>
                <button className="btn-test" onClick={onTestOllama}>
                  Test
                </button>
              </div>

              {/* Medplum EMR Settings */}
              <div className="settings-divider" />
              <div className="settings-section-title">Medplum EMR</div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="medplum-url">Server URL</label>
                <input
                  id="medplum-url"
                  type="text"
                  className="settings-input"
                  value={pendingSettings.medplum_server_url}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, medplum_server_url: e.target.value })}
                  placeholder="http://localhost:8103"
                  disabled={authState.is_authenticated}
                />
              </div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="medplum-client-id">Client ID</label>
                <input
                  id="medplum-client-id"
                  type="text"
                  className="settings-input"
                  value={pendingSettings.medplum_client_id}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, medplum_client_id: e.target.value })}
                  placeholder="Enter client ID from Medplum"
                  disabled={authState.is_authenticated}
                />
              </div>

              <div className="settings-group">
                <div className="settings-toggle">
                  <span className="settings-label" style={{ marginBottom: 0 }}>Auto-sync encounters</span>
                  <label className="toggle-switch">
                    <input
                      type="checkbox"
                      checked={pendingSettings.medplum_auto_sync}
                      onChange={(e) =>
                        onSettingsChange({ ...pendingSettings, medplum_auto_sync: e.target.checked })
                      }
                      aria-label="Auto-sync encounters to Medplum"
                    />
                    <span className="toggle-slider"></span>
                  </label>
                </div>
              </div>

              <div className="settings-group ollama-status-group">
                <div className="ollama-status">
                  <span className={`status-indicator ${medplumConnected ? 'connected' : 'disconnected'}`} />
                  <span className="status-text">
                    {medplumConnected
                      ? 'Connected'
                      : medplumError || 'Not connected'}
                  </span>
                </div>
                <button className="btn-test" onClick={onTestMedplum}>
                  Test
                </button>
              </div>

              {/* Medplum Authentication */}
              <div className="settings-group medplum-auth-group">
                {authState.is_authenticated ? (
                  <div className="medplum-auth-status">
                    <div className="auth-user-info">
                      <span className="status-indicator connected" />
                      <span className="auth-user-name">
                        {authState.practitioner_name || 'Signed in'}
                      </span>
                    </div>
                    <button
                      className="btn-signout"
                      onClick={onLogout}
                      disabled={authLoading}
                    >
                      {authLoading ? 'Signing out...' : 'Sign Out'}
                    </button>
                  </div>
                ) : (
                  <button
                    className="btn-signin"
                    onClick={onLogin}
                    disabled={authLoading || !medplumConnected}
                    title={!medplumConnected ? 'Connect to server first' : ''}
                  >
                    {authLoading ? 'Signing in...' : 'Sign In with Medplum'}
                  </button>
                )}
              </div>
            </>
          )}
        </div>
        <div className="settings-drawer-footer">
          <p className="settings-note">Changes apply on next recording</p>
          <button className="btn-save" onClick={onSave}>
            Save Settings
          </button>
        </div>
      </div>
    </>
  );
});

export default SettingsDrawer;

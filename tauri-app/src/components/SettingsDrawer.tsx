import { memo } from 'react';
import type { Device, LLMStatus, AuthState, WhisperServerStatus } from '../types';
import { SpeakerEnrollment } from './SpeakerEnrollment';

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
  // LLM Router settings (OpenAI-compatible API)
  llm_router_url: string;
  llm_api_key: string;
  llm_client_id: string;
  soap_model: string;
  fast_model: string;
  // Medplum EMR settings
  medplum_server_url: string;
  medplum_client_id: string;
  medplum_auto_sync: boolean;
  // Whisper server settings (remote only - local mode removed)
  whisper_mode: 'remote';  // Always 'remote'
  whisper_server_url: string;
  whisper_server_model: string;
  // Auto-session detection settings
  auto_start_enabled: boolean;
  // Auto-end settings
  auto_end_enabled: boolean;
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

  // Whisper server settings
  whisperServerStatus: WhisperServerStatus | null;
  whisperServerModels: string[];
  onTestWhisperServer: () => void;

  // LLM Router settings
  llmStatus: LLMStatus | null;
  llmModels: string[];
  onTestLLM: () => void;

  // Medplum settings
  medplumConnected: boolean;
  medplumError: string | null;
  onTestMedplum: () => void;

  // Auth
  authState: AuthState;
  authLoading: boolean;
  onLogin: () => void;
  onLogout: () => void;
  onCancelLogin: () => void;
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
  whisperServerStatus,
  whisperServerModels,
  onTestWhisperServer,
  llmStatus,
  llmModels,
  onTestLLM,
  medplumConnected,
  medplumError,
  onTestMedplum,
  authState,
  authLoading,
  onLogin,
  onLogout,
  onCancelLogin,
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
              {/* Whisper Server Settings (remote only mode) */}
              <div className="settings-group">
                <label className="settings-label" htmlFor="whisper-server-url">Whisper Server URL</label>
                <input
                  id="whisper-server-url"
                  type="text"
                  className="settings-input"
                  value={pendingSettings.whisper_server_url}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, whisper_server_url: e.target.value })}
                  placeholder="http://172.16.100.45:8001"
                />
              </div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="whisper-server-model">Server Model</label>
                <select
                  id="whisper-server-model"
                  className="settings-select"
                  value={pendingSettings.whisper_server_model}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, whisper_server_model: e.target.value })}
                >
                  <option value={pendingSettings.whisper_server_model}>{pendingSettings.whisper_server_model}</option>
                  {whisperServerModels
                    .filter((m) => m !== pendingSettings.whisper_server_model)
                    .map((m) => (
                      <option key={m} value={m}>{m}</option>
                    ))}
                </select>
              </div>

              <div className="settings-group ollama-status-group">
                <div className="ollama-status">
                  <span className={`status-indicator ${whisperServerStatus?.connected ? 'connected' : 'disconnected'}`} />
                  <span className="status-text">
                    {whisperServerStatus?.connected
                      ? `Connected (${whisperServerModels.length} models)`
                      : whisperServerStatus?.error || 'Not connected'}
                  </span>
                </div>
                <button className="btn-test" onClick={onTestWhisperServer}>
                  Test
                </button>
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

              {/* Speaker Profiles (Enrollment) */}
              <div className="settings-divider" />
              <div className="settings-section-title">Speaker Profiles</div>
              <div className="settings-group">
                <SpeakerEnrollment />
              </div>

              <div className="settings-divider" />
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

              <div className="settings-group">
                <div className="settings-toggle">
                  <span className="settings-label" style={{ marginBottom: 0 }}>Auto-start on Greeting</span>
                  <label className="toggle-switch">
                    <input
                      type="checkbox"
                      checked={pendingSettings.auto_start_enabled}
                      onChange={(e) => onSettingsChange({ ...pendingSettings, auto_start_enabled: e.target.checked })}
                      aria-label="Auto-start recording when greeting detected"
                    />
                    <span className="toggle-slider"></span>
                  </label>
                </div>
                <span className="settings-hint">Start recording automatically when speech with a greeting is detected</span>
              </div>

              <div className="settings-group">
                <div className="settings-toggle">
                  <span className="settings-label" style={{ marginBottom: 0 }}>Auto-end on Silence</span>
                  <label className="toggle-switch">
                    <input
                      type="checkbox"
                      checked={pendingSettings.auto_end_enabled}
                      onChange={(e) => onSettingsChange({ ...pendingSettings, auto_end_enabled: e.target.checked })}
                      aria-label="Auto-end recording after prolonged silence"
                    />
                    <span className="toggle-slider"></span>
                  </label>
                </div>
                <span className="settings-hint">End recording automatically after 2 minutes of silence (with countdown warning at 1 minute)</span>
              </div>

              {/* LLM Router / SOAP Note Settings */}
              <div className="settings-divider" />
              <div className="settings-section-title">SOAP Note Generation</div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="llm-router-url">LLM Router URL</label>
                <input
                  id="llm-router-url"
                  type="text"
                  className="settings-input"
                  value={pendingSettings.llm_router_url}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, llm_router_url: e.target.value })}
                  placeholder="http://localhost:8080"
                />
              </div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="llm-api-key">API Key</label>
                <input
                  id="llm-api-key"
                  type="password"
                  className="settings-input"
                  value={pendingSettings.llm_api_key}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, llm_api_key: e.target.value })}
                  placeholder="Enter API key"
                />
              </div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="llm-client-id">LLM Client ID</label>
                <input
                  id="llm-client-id"
                  type="text"
                  className="settings-input"
                  value={pendingSettings.llm_client_id}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, llm_client_id: e.target.value })}
                  placeholder="clinic-001"
                />
              </div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="soap-model">SOAP Model</label>
                <select
                  id="soap-model"
                  className="settings-select"
                  value={pendingSettings.soap_model}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, soap_model: e.target.value })}
                >
                  <option value={pendingSettings.soap_model}>{pendingSettings.soap_model}</option>
                  {llmModels
                    .filter((m) => m !== pendingSettings.soap_model)
                    .map((m) => (
                      <option key={m} value={m}>{m}</option>
                    ))}
                </select>
              </div>

              <div className="settings-group">
                <label className="settings-label" htmlFor="fast-model">Fast Model (for greeting detection)</label>
                <select
                  id="fast-model"
                  className="settings-select"
                  value={pendingSettings.fast_model}
                  onChange={(e) => onSettingsChange({ ...pendingSettings, fast_model: e.target.value })}
                >
                  <option value={pendingSettings.fast_model}>{pendingSettings.fast_model}</option>
                  {llmModels
                    .filter((m) => m !== pendingSettings.fast_model)
                    .map((m) => (
                      <option key={m} value={m}>{m}</option>
                    ))}
                </select>
              </div>

              <div className="settings-group ollama-status-group">
                <div className="ollama-status">
                  <span className={`status-indicator ${llmStatus?.connected ? 'connected' : 'disconnected'}`} />
                  <span className="status-text">
                    {llmStatus?.connected
                      ? `Connected (${llmModels.length} models)`
                      : llmStatus?.error || 'Not connected'}
                  </span>
                </div>
                <button className="btn-test" onClick={onTestLLM}>
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
                  <div className="medplum-login-actions">
                    <button
                      className="btn-signin"
                      onClick={onLogin}
                      disabled={authLoading || !medplumConnected}
                      title={!medplumConnected ? 'Connect to server first' : ''}
                    >
                      {authLoading ? 'Signing in...' : 'Sign In with Medplum'}
                    </button>
                    {authLoading && (
                      <button
                        className="btn-cancel-login"
                        onClick={onCancelLogin}
                      >
                        Cancel
                      </button>
                    )}
                  </div>
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

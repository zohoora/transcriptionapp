import { memo, useState } from 'react';
import type { Device, LLMStatus, AuthState, WhisperServerStatus, SpeakerRole } from '../types';
import { SPEAKER_ROLE_LABELS } from '../types';
import type { PendingSettings } from '../hooks/useSettings';
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

interface SettingsDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  pendingSettings: PendingSettings | null;
  onSettingsChange: (settings: PendingSettings) => void;
  onSave: () => void;
  devices: Device[];

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
 * Settings drawer with 4-zone layout:
 * Zone 1: Clinical Workflow (5 controls, always visible)
 * Zone 2: Connection Status (3 dots)
 * Zone 3: Advanced (collapsed by default)
 * Zone 4: Speaker Profiles (sub-view)
 */
export const SettingsDrawer = memo(function SettingsDrawer({
  isOpen,
  onClose,
  pendingSettings,
  onSettingsChange,
  onSave,
  devices,
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
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [showSpeakerProfiles, setShowSpeakerProfiles] = useState(false);

  if (!isOpen) return null;

  // Speaker Profiles sub-view replaces drawer content
  if (showSpeakerProfiles) {
    return (
      <>
        <div className="settings-overlay" onClick={onClose} />
        <div className="settings-drawer">
          <div className="settings-drawer-header">
            <button className="settings-back-button" onClick={() => setShowSpeakerProfiles(false)}>
              &larr; Back to Settings
            </button>
            <button className="close-btn" onClick={onClose}>
              &times;
            </button>
          </div>
          <div className="settings-drawer-content">
            <SpeakerEnrollment />
          </div>
        </div>
      </>
    );
  }

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
              {/* ── Zone 1: Clinical Workflow ── */}
              <div className="settings-section">
                <div className="settings-row">
                  <div className="charting-mode-toggle">
                    <button
                      className={`charting-mode-btn ${pendingSettings.charting_mode !== 'continuous' ? 'active' : ''}`}
                      onClick={() => onSettingsChange({ ...pendingSettings, charting_mode: 'session' })}
                    >
                      After Every Session
                    </button>
                    <button
                      className={`charting-mode-btn ${pendingSettings.charting_mode === 'continuous' ? 'active' : ''}`}
                      onClick={() => onSettingsChange({ ...pendingSettings, charting_mode: 'continuous' })}
                    >
                      End of Day
                    </button>
                  </div>
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
                  <label className="settings-label" htmlFor="image-source">Medical Illustrations</label>
                  <select
                    id="image-source"
                    className="settings-select"
                    value={pendingSettings.image_source}
                    onChange={(e) => onSettingsChange({ ...pendingSettings, image_source: e.target.value })}
                  >
                    <option value="off">Off</option>
                    <option value="ai">AI Generated</option>
                  </select>
                </div>

                <div className="settings-group">
                  <div className="settings-toggle">
                    <span className="settings-label" style={{ marginBottom: 0 }}>Screen Capture</span>
                    <label className="toggle-switch">
                      <input
                        type="checkbox"
                        checked={pendingSettings.screen_capture_enabled}
                        onChange={(e) => onSettingsChange({ ...pendingSettings, screen_capture_enabled: e.target.checked })}
                        aria-label="Capture screen during recording"
                      />
                      <span className="toggle-slider"></span>
                    </label>
                  </div>
                  <span className="settings-hint">Capture screen periodically during recording</span>
                </div>

                {/* ─ SOAP Preferences ─ */}
                <div className="settings-divider" />
                <h4 className="settings-sub-header">SOAP Preferences</h4>
                <div className="settings-group">
                  <label className="settings-label" htmlFor="soap-custom-instructions">Personal Instructions</label>
                  <textarea
                    id="soap-custom-instructions"
                    className="settings-textarea"
                    rows={4}
                    value={pendingSettings.soap_custom_instructions}
                    onChange={(e) => onSettingsChange({ ...pendingSettings, soap_custom_instructions: e.target.value })}
                    placeholder="e.g., I am a family medicine physician. Use concise bullet points. Always include ICD-10 codes..."
                  />
                  <span className="settings-hint">Added to every SOAP note prompt. Describe your preferences, specialty context, or formatting rules.</span>
                </div>
              </div>

              {/* ── Zone 2: Connection Status ── */}
              <div className="connection-status-bar">
                <div className="status-dot-item">
                  <span className={`status-indicator ${whisperServerStatus?.connected ? 'connected' : 'disconnected'}`} />
                  <span className="status-dot-label">STT</span>
                </div>
                <div className="status-dot-item">
                  <span className={`status-indicator ${llmStatus?.connected ? 'connected' : 'disconnected'}`} />
                  <span className="status-dot-label">LLM</span>
                </div>
                <div className="status-dot-item">
                  <span className={`status-indicator ${medplumConnected ? 'connected' : 'disconnected'}`} />
                  <span className="status-dot-label">EMR</span>
                </div>
              </div>

              {/* ── Zone 3: Advanced ── */}
              <div className="advanced-section">
                <button
                  className="advanced-section-header"
                  onClick={() => setShowAdvanced(!showAdvanced)}
                  aria-expanded={showAdvanced}
                >
                  <span>Advanced</span>
                  <span className="advanced-chevron">{showAdvanced ? '\u25BE' : '\u25B8'}</span>
                </button>

                {showAdvanced && (
                  <div className="advanced-section-content">
                    {/* ─ STT Router ─ */}
                    <h4 className="advanced-sub-header">STT Router</h4>
                    <div className="settings-group">
                      <label className="settings-label" htmlFor="whisper-server-url">STT Server URL</label>
                      <input
                        id="whisper-server-url"
                        type="text"
                        className="settings-input"
                        value={pendingSettings.whisper_server_url}
                        onChange={(e) => onSettingsChange({ ...pendingSettings, whisper_server_url: e.target.value })}
                        placeholder="http://172.16.100.45:8001"
                      />
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

                    {/* ─ LLM Router ─ */}
                    <h4 className="advanced-sub-header">LLM Router</h4>
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
                      <label className="settings-label" htmlFor="fast-model">Fast Model</label>
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

                    {/* ─ Medplum EMR ─ */}
                    <h4 className="advanced-sub-header">Medplum EMR</h4>
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

                    {/* ─ Continuous Mode ─ */}
                    {pendingSettings.charting_mode === 'continuous' && (
                      <>
                        <h4 className="advanced-sub-header">Continuous Mode</h4>
                        <div className="settings-row">
                          <label className="settings-label" style={{ marginBottom: 4 }}>Detection Mode</label>
                          <div className="charting-mode-toggle">
                            <button
                              className={`charting-mode-btn ${pendingSettings.encounter_detection_mode === 'llm' ? 'active' : ''}`}
                              onClick={() => onSettingsChange({ ...pendingSettings, encounter_detection_mode: 'llm' })}
                            >
                              LLM
                            </button>
                            <button
                              className={`charting-mode-btn ${pendingSettings.encounter_detection_mode === 'hybrid' ? 'active' : ''}`}
                              onClick={() => onSettingsChange({ ...pendingSettings, encounter_detection_mode: 'hybrid' })}
                            >
                              Hybrid
                            </button>
                          </div>
                        </div>
                        {pendingSettings.encounter_detection_mode === 'hybrid' && (
                          <>
                            <div className="settings-group">
                              <label className="settings-label">Sensor URL (WiFi)</label>
                              <input
                                type="text"
                                className="settings-input"
                                value={pendingSettings.presence_sensor_url}
                                onChange={(e) => onSettingsChange({ ...pendingSettings, presence_sensor_url: e.target.value })}
                                placeholder="http://172.16.100.37"
                              />
                            </div>
                            <div className="settings-group">
                              <label className="settings-label">Serial Port (fallback)</label>
                              <input
                                type="text"
                                className="settings-input"
                                value={pendingSettings.presence_sensor_port}
                                onChange={(e) => onSettingsChange({ ...pendingSettings, presence_sensor_port: e.target.value })}
                                placeholder="/dev/cu.usbserial-2110"
                              />
                            </div>
                            <div className="settings-group">
                              <label className="settings-label">Absence Threshold</label>
                              <div className="settings-slider">
                                <input
                                  type="range"
                                  min="10"
                                  max="600"
                                  step="10"
                                  value={pendingSettings.presence_absence_threshold_secs}
                                  onChange={(e) => onSettingsChange({ ...pendingSettings, presence_absence_threshold_secs: Number(e.target.value) })}
                                />
                                <span className="slider-value">{pendingSettings.presence_absence_threshold_secs}s</span>
                              </div>
                            </div>
                          </>
                        )}
                        <div className="settings-group">
                          <div className="settings-toggle">
                            <span className="settings-label" style={{ marginBottom: 0 }}>Auto-merge encounters</span>
                            <label className="toggle-switch">
                              <input
                                type="checkbox"
                                checked={pendingSettings.encounter_merge_enabled}
                                onChange={(e) => onSettingsChange({ ...pendingSettings, encounter_merge_enabled: e.target.checked })}
                                aria-label="Auto-merge split encounters"
                              />
                              <span className="toggle-slider"></span>
                            </label>
                          </div>
                        </div>
                      </>
                    )}

                    {/* ─ Session Automation ─ */}
                    <h4 className="advanced-sub-header">Session Automation</h4>
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
                    {pendingSettings.auto_start_enabled && (
                      <>
                        <div className="settings-group settings-subgroup">
                          <div className="settings-toggle">
                            <span className="settings-label" style={{ marginBottom: 0 }}>Require Enrolled Speaker</span>
                            <label className="toggle-switch">
                              <input
                                type="checkbox"
                                checked={pendingSettings.auto_start_require_enrolled}
                                onChange={(e) => onSettingsChange({ ...pendingSettings, auto_start_require_enrolled: e.target.checked })}
                                aria-label="Only auto-start when speaker is enrolled"
                              />
                              <span className="toggle-slider"></span>
                            </label>
                          </div>
                          <span className="settings-hint">Only auto-start when the speaker's voice matches an enrolled profile</span>
                        </div>
                        {pendingSettings.auto_start_require_enrolled && (
                          <div className="settings-group settings-subgroup">
                            <label className="settings-label" htmlFor="required-role">Required Role (optional)</label>
                            <select
                              id="required-role"
                              className="settings-select"
                              value={pendingSettings.auto_start_required_role || ''}
                              onChange={(e) => onSettingsChange({
                                ...pendingSettings,
                                auto_start_required_role: e.target.value ? e.target.value as SpeakerRole : null
                              })}
                            >
                              <option value="">Any enrolled speaker</option>
                              {Object.entries(SPEAKER_ROLE_LABELS).map(([role, label]) => (
                                <option key={role} value={role}>{label}</option>
                              ))}
                            </select>
                          </div>
                        )}
                      </>
                    )}
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
                      <span className="settings-hint">End recording after 3 minutes of silence</span>
                    </div>

                    {/* ─ AI Images ─ */}
                    {pendingSettings.image_source === 'ai' && (
                      <>
                        <h4 className="advanced-sub-header">AI Images</h4>
                        <div className="settings-group">
                          <label className="settings-label" htmlFor="gemini-api-key">Gemini API Key</label>
                          <input
                            id="gemini-api-key"
                            type="password"
                            className="settings-input"
                            value={pendingSettings.gemini_api_key}
                            onChange={(e) => onSettingsChange({ ...pendingSettings, gemini_api_key: e.target.value })}
                            placeholder="Enter your Gemini API key"
                          />
                          <span className="settings-hint">Required for AI-generated medical illustrations</span>
                        </div>
                      </>
                    )}

                    <p className="settings-hint" style={{ marginTop: 16, opacity: 0.5, fontSize: 10 }}>
                      Additional options available in config.json
                    </p>
                  </div>
                )}
              </div>

              {/* ── Zone 4: Speaker Profiles ── */}
              <div className="settings-divider" />
              <button
                className="speaker-profiles-button"
                onClick={() => setShowSpeakerProfiles(true)}
              >
                Manage Speaker Profiles
              </button>
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

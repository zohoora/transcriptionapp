import { memo } from 'react';
import type { Device, OllamaStatus, AuthState, WhisperModelInfo, WhisperServerStatus } from '../types';
import type { DownloadProgress } from '../hooks/useWhisperModels';

// Category display order
const CATEGORY_ORDER = ['Standard', 'Large', 'Quantized', 'Distil-Whisper'];

// Helper to format file size
function formatBytes(bytes: number): string {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(0) + ' KB';
  return (bytes / (1024 * 1024)).toFixed(0) + ' MB';
}

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
  // Whisper server settings
  whisper_mode: 'local' | 'remote';
  whisper_server_url: string;
  whisper_server_model: string;
}

interface SettingsDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  pendingSettings: PendingSettings | null;
  onSettingsChange: (settings: PendingSettings) => void;
  onSave: () => void;
  devices: Device[];

  // Whisper models
  whisperModels: WhisperModelInfo[];
  whisperModelsByCategory: Record<string, WhisperModelInfo[]>;
  onDownloadModel: (modelId: string) => Promise<boolean>;
  downloadProgress: DownloadProgress | null;

  // Biomarkers toggle
  showBiomarkers: boolean;
  onShowBiomarkersChange: (show: boolean) => void;

  // Whisper server settings
  whisperServerStatus: WhisperServerStatus | null;
  whisperServerModels: string[];
  onTestWhisperServer: () => void;

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
  whisperModels,
  whisperModelsByCategory,
  onDownloadModel,
  downloadProgress,
  showBiomarkers,
  onShowBiomarkersChange,
  whisperServerStatus,
  whisperServerModels,
  onTestWhisperServer,
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
              <div className="settings-group">
                <label className="settings-label" htmlFor="model-select">Model</label>
                <select
                  id="model-select"
                  className="settings-select"
                  value={pendingSettings.model}
                  onChange={(e) => {
                    const newModelId = e.target.value;
                    const newModel = whisperModels.find((m) => m.id === newModelId);
                    // Auto-set language based on model type
                    const newLanguage = newModel?.english_only ? 'en' : 'auto';
                    onSettingsChange({ ...pendingSettings, model: newModelId, language: newLanguage });
                  }}
                >
                  {CATEGORY_ORDER.filter(cat => whisperModelsByCategory[cat]?.length > 0).map((category) => (
                    <optgroup key={category} label={category}>
                      {whisperModelsByCategory[category].map((m) => (
                        <option key={m.id} value={m.id}>
                          {m.label} ({formatBytes(m.size_bytes)}) {m.downloaded ? '' : ''}
                        </option>
                      ))}
                    </optgroup>
                  ))}
                </select>
                {(() => {
                  const selectedModel = whisperModels.find((m) => m.id === pendingSettings.model);
                  const isDownloading = downloadProgress?.modelId === pendingSettings.model;

                  if (!selectedModel) return null;

                  return (
                    <div className="model-status-row">
                      {selectedModel.downloaded ? (
                        <span className="model-status downloaded">Downloaded</span>
                      ) : isDownloading ? (
                        <span className="model-status downloading">
                          {downloadProgress?.status === 'downloading' && 'Downloading...'}
                          {downloadProgress?.status === 'testing' && 'Testing...'}
                          {downloadProgress?.status === 'completed' && 'Complete!'}
                          {downloadProgress?.status === 'failed' && `Failed: ${downloadProgress.error}`}
                        </span>
                      ) : (
                        <button
                          className="btn-download-model"
                          onClick={() => onDownloadModel(pendingSettings.model)}
                          disabled={!!downloadProgress}
                        >
                          Download ({formatBytes(selectedModel.size_bytes)})
                        </button>
                      )}
                      {selectedModel.recommended && (
                        <span className="model-badge recommended">Recommended</span>
                      )}
                      {selectedModel.english_only && (
                        <span className="model-badge english-only">English only</span>
                      )}
                    </div>
                  );
                })()}
                {(() => {
                  const selectedModel = whisperModels.find((m) => m.id === pendingSettings.model);
                  return selectedModel?.description && (
                    <p className="model-description">{selectedModel.description}</p>
                  );
                })()}
              </div>

              {/* Transcription Mode Toggle */}
              <div className="settings-group">
                <label className="settings-label">Transcription Mode</label>
                <div className="settings-toggle-buttons">
                  <button
                    className={`toggle-btn ${pendingSettings.whisper_mode === 'local' ? 'active' : ''}`}
                    onClick={() => onSettingsChange({ ...pendingSettings, whisper_mode: 'local' })}
                  >
                    Local
                  </button>
                  <button
                    className={`toggle-btn ${pendingSettings.whisper_mode === 'remote' ? 'active' : ''}`}
                    onClick={() => onSettingsChange({ ...pendingSettings, whisper_mode: 'remote' })}
                  >
                    Remote Server
                  </button>
                </div>
                <p className="model-description">
                  {pendingSettings.whisper_mode === 'local'
                    ? 'Run Whisper locally on this device'
                    : 'Use a remote Whisper server (faster-whisper)'}
                </p>
              </div>

              {/* Remote Whisper Server Settings (shown only when remote mode) */}
              {pendingSettings.whisper_mode === 'remote' && (
                <>
                  <div className="settings-group">
                    <label className="settings-label" htmlFor="whisper-server-url">Whisper Server URL</label>
                    <input
                      id="whisper-server-url"
                      type="text"
                      className="settings-input"
                      value={pendingSettings.whisper_server_url}
                      onChange={(e) => onSettingsChange({ ...pendingSettings, whisper_server_url: e.target.value })}
                      placeholder="http://192.168.50.149:8000"
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
                </>
              )}

              {(() => {
                const selectedModel = whisperModels.find((m) => m.id === pendingSettings.model);
                const isEnglishOnly = selectedModel?.english_only ?? false;

                return (
                  <div className="settings-group">
                    <label className="settings-label" htmlFor="language-select">Language</label>
                    <select
                      id="language-select"
                      className="settings-select"
                      value={pendingSettings.language}
                      onChange={(e) => onSettingsChange({ ...pendingSettings, language: e.target.value })}
                      disabled={isEnglishOnly}
                    >
                      {isEnglishOnly ? (
                        <option value="en">English</option>
                      ) : (
                        LANGUAGES.map((l) => (
                          <option key={l.value} value={l.value}>
                            {l.label}
                          </option>
                        ))
                      )}
                    </select>
                    {isEnglishOnly && (
                      <p className="model-description">English-only model selected</p>
                    )}
                  </div>
                );
              })()}

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

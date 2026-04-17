import { memo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Device, AuthState, SpeakerRole, OperationalDefaults, Settings } from '../types';
import { SPEAKER_ROLE_LABELS, DETAIL_LEVEL_LABELS, VISIT_SETTING_OPTIONS } from '../types';
import type { PendingSettings } from '../hooks/useSettings';
import { SpeakerEnrollment } from './SpeakerEnrollment';

// Phase 3 note: the operational-defaults UI below (clinic-default line +
// "Reset to clinic default" link) is wired only for the Cat B fields that are
// actually rendered as inputs in this drawer — currently `soap_model` and
// `fast_model`. The remaining Cat B fields (`sleep_*`, `encounter_*`,
// `soap_model_fast`, `encounter_detection_model`, `thermal_hot_pixel_threshold_c`,
// `co2_baseline_ppm`) stay server-controllable via `PUT /config/defaults` on
// the profile service, but they have no UI surface in this drawer until a
// future phase (e.g. the admin panel gets the same treatment for sensor
// baselines, or Advanced grows more fields).

interface SettingsDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  pendingSettings: PendingSettings | null;
  onSettingsChange: (settings: PendingSettings) => void;
  onSave: () => void;
  devices: Device[];

  // Auth
  authState: AuthState;
  authLoading: boolean;
  onLogin: () => void;
  onLogout: () => void;
  onCancelLogin: () => void;

  // Room config
  roomName?: string | null;
  profileServerUrl?: string | null;
  onChangeRoom?: () => void;

  // Server-pushed operational defaults (Phase 3)
  operationalDefaults?: OperationalDefaults | null;
  // Settings (for reading `user_edited_fields` tracking)
  settings?: Settings | null;
  // Called after a successful `clear_user_edited_field` so the parent can
  // re-sync settings + operational defaults state.
  onClearUserEditedField?: (fieldName: string) => void;
}

export const SettingsDrawer = memo(function SettingsDrawer({
  isOpen,
  onClose,
  pendingSettings,
  onSettingsChange,
  onSave,
  devices,
  authState: _authState,
  authLoading: _authLoading,
  onLogin: _onLogin,
  onLogout: _onLogout,
  onCancelLogin: _onCancelLogin,
  roomName,
  profileServerUrl,
  onChangeRoom,
  operationalDefaults,
  settings,
  onClearUserEditedField,
}: SettingsDrawerProps) {
  const [showSpeakerProfiles, setShowSpeakerProfiles] = useState(false);

  // Helper: given a Cat B field with a string value, is the pending value
  // different from the server's clinic default? Empty/undefined default means
  // "no server value known" — we treat that as "no hint to show".
  const hasClinicDefault = (defaultValue: string | undefined): defaultValue is string =>
    typeof defaultValue === 'string' && defaultValue.length > 0;

  const isUserEdited = (fieldName: string): boolean =>
    Array.isArray(settings?.user_edited_fields) &&
    settings!.user_edited_fields!.includes(fieldName);

  // Calls the backend to clear the field from `user_edited_fields` and resets
  // the local pending value to the server default. Fail-open: errors are
  // surfaced to the console but don't block — the user can re-type the value.
  const resetCatBField = async (
    fieldName: 'soap_model' | 'fast_model',
    serverValue: string,
  ) => {
    if (!pendingSettings) return;
    try {
      await invoke('clear_user_edited_field', { fieldName });
      onSettingsChange({ ...pendingSettings, [fieldName]: serverValue });
      onClearUserEditedField?.(fieldName);
    } catch (e) {
      console.error(`Failed to reset ${fieldName} to clinic default:`, e);
    }
  };

  if (!isOpen) return null;

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
              <div className="settings-section">
                {/* Continuous Mode */}
                <div className="settings-group">
                  <div className="settings-toggle">
                    <span className="settings-label" style={{ marginBottom: 0 }}>Continuous Mode</span>
                    <label className="toggle-switch">
                      <input
                        type="checkbox"
                        checked={pendingSettings.charting_mode === 'continuous'}
                        onChange={(e) => onSettingsChange({ ...pendingSettings, charting_mode: e.target.checked ? 'continuous' : 'session' })}
                        aria-label="Enable continuous mode"
                      />
                      <span className="toggle-slider"></span>
                    </label>
                  </div>
                  <span className="settings-hint">Record all day and auto-detect patient encounters</span>
                </div>

                {/* Microphone */}
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

                {/* SOAP Preferences */}
                <div className="settings-divider" />
                <h4 className="settings-sub-header">SOAP Preferences</h4>

                {/* Format Toggle */}
                <div className="settings-group">
                  <label className="settings-label">Format</label>
                  <div className="soap-format-toggle">
                    <button
                      className={`format-btn ${pendingSettings.soap_format === 'problem_based' ? 'active' : ''}`}
                      onClick={() => onSettingsChange({ ...pendingSettings, soap_format: 'problem_based' })}
                    >
                      Problem
                    </button>
                    <button
                      className={`format-btn ${pendingSettings.soap_format === 'comprehensive' ? 'active' : ''}`}
                      onClick={() => onSettingsChange({ ...pendingSettings, soap_format: 'comprehensive' })}
                    >
                      Comprehensive
                    </button>
                  </div>
                </div>

                {/* Detail Level Slider */}
                <div className="settings-group">
                  <label className="settings-label">
                    Detail: {DETAIL_LEVEL_LABELS[pendingSettings.soap_detail_level]?.name || 'Standard'}
                  </label>
                  <div className="soap-detail-slider">
                    <input
                      type="range"
                      min="1"
                      max="10"
                      value={pendingSettings.soap_detail_level}
                      onChange={(e) => onSettingsChange({ ...pendingSettings, soap_detail_level: parseInt(e.target.value) })}
                      className="detail-slider"
                      aria-label="SOAP note detail level"
                    />
                    <span className="detail-value">{pendingSettings.soap_detail_level}</span>
                  </div>
                </div>

                {/* Personal Instructions */}
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

                {/* Billing Preferences */}
                <div className="settings-divider" />
                <h4 className="settings-sub-header">Billing Preferences</h4>

                <div className="settings-group">
                  <label className="settings-label" htmlFor="billing-visit-setting">Default Visit Setting</label>
                  <select
                    id="billing-visit-setting"
                    className="settings-select"
                    value={pendingSettings.billing_default_visit_setting}
                    onChange={(e) => onSettingsChange({ ...pendingSettings, billing_default_visit_setting: e.target.value })}
                  >
                    {VISIT_SETTING_OPTIONS.map(opt => (
                      <option key={opt.value} value={opt.value}>{opt.label}</option>
                    ))}
                  </select>
                </div>

                <div className="settings-group">
                  <div className="settings-toggle">
                    <span className="settings-label" style={{ marginBottom: 0 }}>K013 Exhausted This Year</span>
                    <label className="toggle-switch">
                      <input
                        type="checkbox"
                        checked={pendingSettings.billing_counselling_exhausted}
                        onChange={(e) => onSettingsChange({ ...pendingSettings, billing_counselling_exhausted: e.target.checked })}
                        aria-label="K013 counselling units exhausted"
                      />
                      <span className="toggle-slider"></span>
                    </label>
                  </div>
                  <span className="settings-hint">Use K033 instead of K013 for counselling (3+ units used this year)</span>
                </div>

                <div className="settings-group">
                  <div className="settings-toggle">
                    <span className="settings-label" style={{ marginBottom: 0 }}>Hospital-Based Practice</span>
                    <label className="toggle-switch">
                      <input
                        type="checkbox"
                        checked={pendingSettings.billing_is_hospital}
                        onChange={(e) => onSettingsChange({ ...pendingSettings, billing_is_hospital: e.target.checked })}
                        aria-label="Hospital-based practice"
                      />
                      <span className="toggle-slider"></span>
                    </label>
                  </div>
                  <span className="settings-hint">Suppresses tray fees (E542A, E430A) — hospitals cover supplies via global budget</span>
                </div>

                {/* Advanced — LLM model aliases (Cat B operational defaults).
                    Collapsed by default: these are IT/developer inputs, so keep
                    the drawer's "5 visible controls at first open" pattern. */}
                <div className="settings-divider" />
                <details className="settings-advanced-models">
                  <summary className="settings-sub-header" style={{ cursor: 'pointer' }}>
                    Advanced (LLM Models)
                  </summary>

                  {/* SOAP Model */}
                  <div className="settings-group">
                    <label className="settings-label" htmlFor="soap-model-input">SOAP Model</label>
                    <input
                      id="soap-model-input"
                      type="text"
                      className="settings-input"
                      value={pendingSettings.soap_model}
                      onChange={(e) => onSettingsChange({ ...pendingSettings, soap_model: e.target.value })}
                      placeholder="soap-model-fast"
                    />
                    {hasClinicDefault(operationalDefaults?.soap_model)
                      && operationalDefaults!.soap_model !== pendingSettings.soap_model && (
                      <span className="settings-hint" style={{ opacity: 0.6 }}>
                        Clinic default: {operationalDefaults!.soap_model}
                        {isUserEdited('soap_model') && (
                          <>
                            {' — '}
                            <button
                              type="button"
                              className="btn-link"
                              onClick={() => {
                                void resetCatBField('soap_model', operationalDefaults!.soap_model);
                              }}
                            >
                              Reset to clinic default
                            </button>
                          </>
                        )}
                      </span>
                    )}
                  </div>

                  {/* Fast Model */}
                  <div className="settings-group">
                    <label className="settings-label" htmlFor="fast-model-input">Fast Model</label>
                    <input
                      id="fast-model-input"
                      type="text"
                      className="settings-input"
                      value={pendingSettings.fast_model}
                      onChange={(e) => onSettingsChange({ ...pendingSettings, fast_model: e.target.value })}
                      placeholder="fast-model"
                    />
                    {hasClinicDefault(operationalDefaults?.fast_model)
                      && operationalDefaults!.fast_model !== pendingSettings.fast_model && (
                      <span className="settings-hint" style={{ opacity: 0.6 }}>
                        Clinic default: {operationalDefaults!.fast_model}
                        {isUserEdited('fast_model') && (
                          <>
                            {' — '}
                            <button
                              type="button"
                              className="btn-link"
                              onClick={() => {
                                void resetCatBField('fast_model', operationalDefaults!.fast_model);
                              }}
                            >
                              Reset to clinic default
                            </button>
                          </>
                        )}
                      </span>
                    )}
                  </div>
                </details>

                {/* Session Automation (session mode only) */}
                {pendingSettings.charting_mode !== 'continuous' && (<>
                <div className="settings-divider" />
                <h4 className="settings-sub-header">Session Automation</h4>
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
                </>)}

                {/* EMR Auth (hidden for now)
                <div className="settings-divider" />
                <h4 className="settings-sub-header">EMR</h4>
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
                        disabled={authLoading}
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
                */}

                {/* Room */}
                <div className="settings-divider" />
                <h4 className="settings-sub-header">Room</h4>
                <div className="settings-group">
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <div>
                      <div style={{ fontSize: 13, fontWeight: 500 }}>{roomName || 'Not configured'}</div>
                      <div style={{ fontSize: 11, opacity: 0.5 }}>{profileServerUrl || ''}</div>
                    </div>
                    {onChangeRoom && (
                      <button
                        className="btn-outline"
                        style={{ fontSize: 11, padding: '4px 10px' }}
                        onClick={onChangeRoom}
                      >
                        Change
                      </button>
                    )}
                  </div>
                </div>
              </div>

              {/* Speaker Profiles */}
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

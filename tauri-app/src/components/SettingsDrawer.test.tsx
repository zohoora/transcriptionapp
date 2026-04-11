import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SettingsDrawer } from './SettingsDrawer';
import type { PendingSettings } from '../hooks/useSettings';
import type { Device, AuthState } from '../types';

const mockDevices: Device[] = [
  { id: 'device-1', name: 'Built-in Microphone', is_default: true },
  { id: 'device-2', name: 'External USB Microphone', is_default: false },
];

const defaultPendingSettings: PendingSettings = {
  device: 'default',
  llm_router_url: 'http://localhost:8080',
  llm_api_key: 'test-api-key',
  llm_client_id: 'clinic-001',
  soap_model: 'gpt-4',
  fast_model: 'gpt-3.5-turbo',
  medplum_server_url: 'http://localhost:8103',
  medplum_client_id: 'test-client-id',
  medplum_auto_sync: false,
  whisper_server_url: 'http://localhost:8001',
  auto_start_enabled: false,
  auto_start_require_enrolled: false,
  auto_start_required_role: null,
  auto_end_enabled: false,
  image_source: 'off',
  gemini_api_key: '',
  screen_capture_enabled: false,
  charting_mode: 'session',
  encounter_detection_mode: 'llm',
  sensor_connection_type: 'none',
  presence_sensor_port: '',
  presence_sensor_url: '',
  presence_absence_threshold_secs: 180,
  presence_debounce_secs: 15,
  hybrid_confirm_window_secs: 180,
  hybrid_min_words_for_sensor_split: 500,
  thermal_hot_pixel_threshold_c: 28.0,
  co2_baseline_ppm: 420.0,
  presence_csv_log_enabled: true,
  encounter_merge_enabled: false,
  soap_custom_instructions: '',
};

const defaultAuthState: AuthState = {
  is_authenticated: false,
  access_token: null,
  refresh_token: null,
  token_expiry: null,
  practitioner_id: null,
  practitioner_name: null,
};

const authenticatedAuthState: AuthState = {
  is_authenticated: true,
  access_token: 'test-token',
  refresh_token: 'test-refresh',
  token_expiry: 1234567890,
  practitioner_id: 'prac-123',
  practitioner_name: 'Dr. Test',
};

const defaultProps = {
  isOpen: true,
  onClose: vi.fn(),
  pendingSettings: defaultPendingSettings,
  onSettingsChange: vi.fn(),
  onSave: vi.fn(),
  devices: mockDevices,
  authState: defaultAuthState,
  authLoading: false,
  onLogin: vi.fn(),
  onLogout: vi.fn(),
  onCancelLogin: vi.fn(),
};

describe('SettingsDrawer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('rendering', () => {
    it('renders when open', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByText('Settings')).toBeInTheDocument();
      expect(screen.getByLabelText('Enable continuous mode')).toBeInTheDocument();
      expect(screen.getByLabelText(/Microphone/)).toBeInTheDocument();
    });

    it('does not render when closed', () => {
      render(<SettingsDrawer {...defaultProps} isOpen={false} />);
      expect(screen.queryByText('Settings')).not.toBeInTheDocument();
    });

    it('renders close and save buttons', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByRole('button', { name: '×' })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Save Settings' })).toBeInTheDocument();
    });

    it('shows all sections without needing to expand', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByText('SOAP Preferences')).toBeInTheDocument();
      expect(screen.getByText('Room')).toBeInTheDocument();
    });

    it('shows session automation only when not in continuous mode', () => {
      render(<SettingsDrawer {...defaultProps} pendingSettings={{ ...defaultPendingSettings, charting_mode: 'session' }} />);
      expect(screen.getByText('Session Automation')).toBeInTheDocument();
    });

    it('hides session automation in continuous mode', () => {
      render(<SettingsDrawer {...defaultProps} pendingSettings={{ ...defaultPendingSettings, charting_mode: 'continuous' }} />);
      expect(screen.queryByText('Session Automation')).not.toBeInTheDocument();
    });
  });

  describe('clinical workflow', () => {
    it('displays current microphone', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText(/Microphone/)).toHaveValue('default');
    });

    it('calls onSettingsChange when continuous mode toggled', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);
      await user.click(screen.getByLabelText('Enable continuous mode'));
      expect(defaultProps.onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({ charting_mode: 'continuous' })
      );
    });

    it('renders SOAP preferences', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText(/Personal Instructions/)).toBeInTheDocument();
    });
  });

  describe('session automation', () => {
    it('shows auto-start and auto-end toggles in session mode', () => {
      render(<SettingsDrawer {...defaultProps} pendingSettings={{ ...defaultPendingSettings, charting_mode: 'session' }} />);
      expect(screen.getByLabelText('Auto-start recording when greeting detected')).toBeInTheDocument();
      expect(screen.getByLabelText('Auto-end recording after prolonged silence')).toBeInTheDocument();
    });
  });

  // EMR auth section is currently hidden

  describe('room', () => {
    it('shows room name', () => {
      render(<SettingsDrawer {...defaultProps} roomName="Room 6" profileServerUrl="http://server:8090" />);
      expect(screen.getByText('Room 6')).toBeInTheDocument();
    });
  });

  describe('speaker profiles', () => {
    it('renders Manage Speaker Profiles button', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByRole('button', { name: 'Manage Speaker Profiles' })).toBeInTheDocument();
    });

    it('shows speaker enrollment view when button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);
      await user.click(screen.getByRole('button', { name: 'Manage Speaker Profiles' }));
      expect(screen.getByText(/Back to Settings/)).toBeInTheDocument();
    });
  });

  describe('close and save', () => {
    it('calls onClose when close button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);
      await user.click(screen.getByRole('button', { name: '×' }));
      expect(defaultProps.onClose).toHaveBeenCalled();
    });

    it('calls onSave when save button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);
      await user.click(screen.getByRole('button', { name: 'Save Settings' }));
      expect(defaultProps.onSave).toHaveBeenCalled();
    });
  });
});

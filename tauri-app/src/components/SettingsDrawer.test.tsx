import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SettingsDrawer } from './SettingsDrawer';
import type { PendingSettings } from '../hooks/useSettings';
import type { Device, LLMStatus, AuthState, WhisperServerStatus } from '../types';

const mockDevices: Device[] = [
  { id: 'device-1', name: 'Built-in Microphone', is_default: true },
  { id: 'device-2', name: 'External USB Microphone', is_default: false },
];

const defaultPendingSettings: PendingSettings = {
  language: 'en',
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
  presence_sensor_port: '',
  presence_sensor_url: '',
  presence_absence_threshold_secs: 180,
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

const defaultWhisperServerStatus: WhisperServerStatus = {
  connected: true,
  available_models: ['large-v3', 'large-v3-turbo', 'base'],
  error: null,
};

const defaultLLMStatus: LLMStatus = {
  connected: true,
  available_models: ['gpt-4', 'gpt-3.5-turbo'],
  error: null,
};

const defaultProps = {
  isOpen: true,
  onClose: vi.fn(),
  pendingSettings: defaultPendingSettings,
  onSettingsChange: vi.fn(),
  onSave: vi.fn(),
  devices: mockDevices,
  whisperServerStatus: defaultWhisperServerStatus,
  whisperServerModels: ['large-v3', 'large-v3-turbo', 'base'],
  onTestWhisperServer: vi.fn(),
  llmStatus: defaultLLMStatus,
  llmModels: ['gpt-4', 'gpt-3.5-turbo'],
  onTestLLM: vi.fn(),
  medplumConnected: true,
  medplumError: null,
  onTestMedplum: vi.fn(),
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
    it('renders nothing when closed', () => {
      render(<SettingsDrawer {...defaultProps} isOpen={false} />);
      expect(screen.queryByText('Settings')).not.toBeInTheDocument();
    });

    it('renders drawer when open', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByText('Settings')).toBeInTheDocument();
    });

    it('renders Zone 1 clinical workflow controls', () => {
      render(<SettingsDrawer {...defaultProps} />);

      // Charting mode buttons
      expect(screen.getByText('After Every Session')).toBeInTheDocument();
      expect(screen.getByText('End of Day')).toBeInTheDocument();

      // Language, Microphone, Medical Illustrations, Screen Capture
      expect(screen.getByLabelText(/Language/)).toBeInTheDocument();
      expect(screen.getByLabelText(/Microphone/)).toBeInTheDocument();
      expect(screen.getByLabelText(/Medical Illustrations/)).toBeInTheDocument();
      expect(screen.getByLabelText('Capture screen during recording')).toBeInTheDocument();
    });

    it('renders close button', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByRole('button', { name: '×' })).toBeInTheDocument();
    });

    it('renders save button', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByRole('button', { name: 'Save Settings' })).toBeInTheDocument();
    });

    it('renders overlay', () => {
      const { container } = render(<SettingsDrawer {...defaultProps} />);
      expect(container.querySelector('.settings-overlay')).toBeInTheDocument();
    });

    it('does not render advanced controls by default', () => {
      render(<SettingsDrawer {...defaultProps} />);

      // Advanced section header should be visible but content hidden
      expect(screen.getByText('Advanced')).toBeInTheDocument();
      expect(screen.queryByLabelText('Server URL')).not.toBeInTheDocument();
      expect(screen.queryByLabelText('LLM Router URL')).not.toBeInTheDocument();
    });
  });

  describe('connection status bar', () => {
    it('renders three status labels', () => {
      render(<SettingsDrawer {...defaultProps} />);

      expect(screen.getByText('STT')).toBeInTheDocument();
      expect(screen.getByText('LLM')).toBeInTheDocument();
      expect(screen.getByText('EMR')).toBeInTheDocument();
    });

    it('shows connected indicators when services are connected', () => {
      const { container } = render(<SettingsDrawer {...defaultProps} />);

      const statusBar = container.querySelector('.connection-status-bar');
      expect(statusBar).toBeInTheDocument();

      const connectedDots = statusBar!.querySelectorAll('.status-indicator.connected');
      expect(connectedDots).toHaveLength(3);
    });

    it('shows disconnected indicators when services are down', () => {
      const { container } = render(
        <SettingsDrawer
          {...defaultProps}
          whisperServerStatus={{ connected: false, available_models: [], error: 'fail' }}
          llmStatus={{ connected: false, available_models: [], error: 'fail' }}
          medplumConnected={false}
        />
      );

      const statusBar = container.querySelector('.connection-status-bar');
      const disconnectedDots = statusBar!.querySelectorAll('.status-indicator.disconnected');
      expect(disconnectedDots).toHaveLength(3);
    });
  });

  describe('Zone 1 clinical workflow', () => {
    it('displays current language', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText(/Language/)).toHaveValue('en');
    });

    it('displays current microphone', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText(/Microphone/)).toHaveValue('default');
    });

    it('calls onSettingsChange when language changes', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.selectOptions(screen.getByLabelText(/Language/), 'es');

      expect(defaultProps.onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({ language: 'es' })
      );
    });

    it('calls onSettingsChange when microphone changes', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.selectOptions(screen.getByLabelText(/Microphone/), 'device-1');

      expect(defaultProps.onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({ device: 'device-1' })
      );
    });

    it('calls onSettingsChange when charting mode changes', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('End of Day'));

      expect(defaultProps.onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({ charting_mode: 'continuous' })
      );
    });

    it('calls onSettingsChange when screen capture toggled', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByLabelText('Capture screen during recording'));

      expect(defaultProps.onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({ screen_capture_enabled: true })
      );
    });

    it('renders all language options', () => {
      render(<SettingsDrawer {...defaultProps} />);

      const languageSelect = screen.getByLabelText(/Language/);
      const options = languageSelect.querySelectorAll('option');

      expect(options).toHaveLength(8);
      expect(options[0]).toHaveValue('en');
      expect(options[7]).toHaveValue('auto');
    });

    it('renders default option plus devices', () => {
      render(<SettingsDrawer {...defaultProps} />);

      const deviceSelect = screen.getByLabelText(/Microphone/);
      const options = deviceSelect.querySelectorAll('option');

      expect(options).toHaveLength(3); // default + 2 devices
      expect(options[0]).toHaveValue('default');
    });

    it('renders personal instructions textarea in Zone 1', () => {
      render(<SettingsDrawer {...defaultProps} />);

      expect(screen.getByText('SOAP Preferences')).toBeInTheDocument();
      expect(screen.getByLabelText(/Personal Instructions/)).toBeInTheDocument();
      expect(screen.getByText(/Added to every SOAP note prompt/)).toBeInTheDocument();
    });

    it('updates soap_custom_instructions when textarea changes', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      const textarea = screen.getByLabelText(/Personal Instructions/);
      await user.type(textarea, 'Use bullet points');

      expect(defaultProps.onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({ soap_custom_instructions: expect.any(String) })
      );
    });

    it('displays existing soap_custom_instructions value', () => {
      render(
        <SettingsDrawer
          {...defaultProps}
          pendingSettings={{ ...defaultPendingSettings, soap_custom_instructions: 'I am a cardiologist' }}
        />
      );

      expect(screen.getByLabelText(/Personal Instructions/)).toHaveValue('I am a cardiologist');
    });
  });

  describe('advanced section', () => {
    it('expands when Advanced header is clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      // Click to expand
      await user.click(screen.getByText('Advanced'));

      // Advanced content should now be visible
      expect(screen.getByLabelText('STT Server URL')).toBeInTheDocument();
      expect(screen.getByText('LLM Router')).toBeInTheDocument();
      expect(screen.getByText('Medplum EMR')).toBeInTheDocument();
    });

    it('collapses when Advanced header is clicked again', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      // Expand
      await user.click(screen.getByText('Advanced'));
      expect(screen.getByLabelText('STT Server URL')).toBeInTheDocument();

      // Collapse
      await user.click(screen.getByText('Advanced'));
      expect(screen.queryByLabelText('STT Server URL')).not.toBeInTheDocument();
    });

    it('shows STT Router settings when expanded', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByText('STT Router')).toBeInTheDocument();
      expect(screen.getByLabelText('STT Server URL')).toHaveValue('http://localhost:8001');
    });

    it('shows LLM Router settings when expanded', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByLabelText('LLM Router URL')).toHaveValue('http://localhost:8080');
      expect(screen.getByLabelText('API Key')).toBeInTheDocument();
      expect(screen.getByLabelText('LLM Client ID')).toHaveValue('clinic-001');
      expect(screen.getByLabelText('SOAP Model')).toHaveValue('gpt-4');
      expect(screen.getByLabelText('Fast Model')).toHaveValue('gpt-3.5-turbo');
    });

    it('shows Medplum EMR settings when expanded', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByText('Medplum EMR')).toBeInTheDocument();
      expect(screen.getByLabelText('Auto-sync encounters to Medplum')).toBeInTheDocument();
    });

    it('calls onSettingsChange when STT Server URL changes', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      const input = screen.getByLabelText('STT Server URL');
      await user.clear(input);
      await user.type(input, 'http://stt:8001');

      expect(defaultProps.onSettingsChange).toHaveBeenCalled();
    });

    it('shows continuous mode settings when charting mode is continuous', async () => {
      const user = userEvent.setup();
      render(
        <SettingsDrawer
          {...defaultProps}
          pendingSettings={{ ...defaultPendingSettings, charting_mode: 'continuous' }}
        />
      );

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByText('Continuous Mode')).toBeInTheDocument();
      // Detection mode buttons (LLM also appears in status bar, so check for Hybrid which is unique)
      expect(screen.getByText('Hybrid')).toBeInTheDocument();
      expect(screen.getByLabelText('Auto-merge split encounters')).toBeInTheDocument();
    });

    it('hides continuous mode settings when charting mode is session', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.queryByText('Continuous Mode')).not.toBeInTheDocument();
    });

    it('shows sensor settings when hybrid detection mode is selected', async () => {
      const user = userEvent.setup();
      render(
        <SettingsDrawer
          {...defaultProps}
          pendingSettings={{
            ...defaultPendingSettings,
            charting_mode: 'continuous',
            encounter_detection_mode: 'hybrid',
          }}
        />
      );

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByText('Sensor URL (WiFi)')).toBeInTheDocument();
      expect(screen.getByText('Serial Port (fallback)')).toBeInTheDocument();
      expect(screen.getByText('Absence Threshold')).toBeInTheDocument();
    });

    it('shows session automation settings when expanded', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByText('Session Automation')).toBeInTheDocument();
      expect(screen.getByLabelText('Auto-start recording when greeting detected')).toBeInTheDocument();
      expect(screen.getByLabelText('Auto-end recording after prolonged silence')).toBeInTheDocument();
    });

    it('shows AI Images section when image_source is ai', async () => {
      const user = userEvent.setup();
      render(
        <SettingsDrawer
          {...defaultProps}
          pendingSettings={{ ...defaultPendingSettings, image_source: 'ai' }}
        />
      );

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByText('AI Images')).toBeInTheDocument();
      expect(screen.getByLabelText('Gemini API Key')).toBeInTheDocument();
    });

    it('hides AI Images section when image_source is off', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.queryByText('AI Images')).not.toBeInTheDocument();
    });

    it('shows config.json note at bottom of advanced section', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByText('Additional options available in config.json')).toBeInTheDocument();
    });
  });

  describe('test buttons (in advanced)', () => {
    it('calls onTestWhisperServer when test button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      const testButtons = screen.getAllByRole('button', { name: 'Test' });
      await user.click(testButtons[0]); // First test button is for STT

      expect(defaultProps.onTestWhisperServer).toHaveBeenCalled();
    });

    it('calls onTestLLM when test button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      const testButtons = screen.getAllByRole('button', { name: 'Test' });
      await user.click(testButtons[1]); // Second test button is for LLM

      expect(defaultProps.onTestLLM).toHaveBeenCalled();
    });

    it('calls onTestMedplum when test button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      const testButtons = screen.getAllByRole('button', { name: 'Test' });
      await user.click(testButtons[2]); // Third test button is for Medplum

      expect(defaultProps.onTestMedplum).toHaveBeenCalled();
    });
  });

  describe('authentication (in advanced)', () => {
    it('shows sign in button when not authenticated', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByRole('button', { name: 'Sign In with Medplum' })).toBeInTheDocument();
    });

    it('disables sign in button when not connected', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} medplumConnected={false} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByRole('button', { name: 'Sign In with Medplum' })).toBeDisabled();
    });

    it('shows signing in state with cancel button', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} authLoading={true} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByRole('button', { name: 'Signing in...' })).toBeDisabled();
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    });

    it('calls onLogin when sign in clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByText('Advanced'));
      await user.click(screen.getByRole('button', { name: 'Sign In with Medplum' }));

      expect(defaultProps.onLogin).toHaveBeenCalled();
    });

    it('shows user info when authenticated', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} authState={authenticatedAuthState} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByText('Dr. Test')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Sign Out' })).toBeInTheDocument();
    });

    it('calls onLogout when sign out clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} authState={authenticatedAuthState} />);

      await user.click(screen.getByText('Advanced'));
      await user.click(screen.getByRole('button', { name: 'Sign Out' }));

      expect(defaultProps.onLogout).toHaveBeenCalled();
    });

    it('disables medplum URL when authenticated', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} authState={authenticatedAuthState} />);

      await user.click(screen.getByText('Advanced'));

      expect(screen.getByLabelText('Server URL')).toBeDisabled();
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

      // Should show back button and hide settings title
      expect(screen.getByText(/Back to Settings/)).toBeInTheDocument();
      expect(screen.queryByText('Settings')).not.toBeInTheDocument();
    });

    it('returns to settings when back button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      // Go to speaker profiles
      await user.click(screen.getByRole('button', { name: 'Manage Speaker Profiles' }));
      expect(screen.getByText(/Back to Settings/)).toBeInTheDocument();

      // Go back
      await user.click(screen.getByText(/Back to Settings/));
      expect(screen.getByText('Settings')).toBeInTheDocument();
    });
  });

  describe('close and save', () => {
    it('calls onClose when close button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByRole('button', { name: '×' }));

      expect(defaultProps.onClose).toHaveBeenCalled();
    });

    it('calls onClose when overlay clicked', async () => {
      const user = userEvent.setup();
      const { container } = render(<SettingsDrawer {...defaultProps} />);

      await user.click(container.querySelector('.settings-overlay')!);

      expect(defaultProps.onClose).toHaveBeenCalled();
    });

    it('calls onSave when save button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByRole('button', { name: 'Save Settings' }));

      expect(defaultProps.onSave).toHaveBeenCalled();
    });
  });

  describe('null pendingSettings', () => {
    it('renders empty drawer when pendingSettings is null', () => {
      render(<SettingsDrawer {...defaultProps} pendingSettings={null} />);

      expect(screen.getByText('Settings')).toBeInTheDocument();
      expect(screen.queryByLabelText('Language')).not.toBeInTheDocument();
    });
  });
});

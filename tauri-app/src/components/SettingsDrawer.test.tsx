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
  model: 'small',
  language: 'en',
  device: 'default',
  diarization_enabled: true,
  max_speakers: 4,
  llm_router_url: 'http://localhost:8080',
  llm_api_key: 'test-api-key',
  llm_client_id: 'clinic-001',
  soap_model: 'gpt-4',
  fast_model: 'gpt-3.5-turbo',
  medplum_server_url: 'http://localhost:8103',
  medplum_client_id: 'test-client-id',
  medplum_auto_sync: false,
  whisper_mode: 'remote',
  whisper_server_url: 'http://localhost:8001',
  whisper_server_model: 'large-v3-turbo',
  auto_start_enabled: false,
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
  showBiomarkers: true,
  onShowBiomarkersChange: vi.fn(),
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

    it('renders all settings sections', () => {
      render(<SettingsDrawer {...defaultProps} />);

      // Transcription settings
      expect(screen.getByLabelText('Whisper Server URL')).toBeInTheDocument();
      expect(screen.getByLabelText('Server Model')).toBeInTheDocument();
      expect(screen.getByLabelText('Language')).toBeInTheDocument();
      expect(screen.getByLabelText('Microphone')).toBeInTheDocument();
      expect(screen.getByLabelText('Max Speakers')).toBeInTheDocument();

      // SOAP Note settings
      expect(screen.getByText('SOAP Note Generation')).toBeInTheDocument();
      expect(screen.getByLabelText('LLM Router URL')).toBeInTheDocument();
      expect(screen.getByLabelText('API Key')).toBeInTheDocument();
      expect(screen.getByLabelText('LLM Client ID')).toBeInTheDocument();

      // Medplum settings
      expect(screen.getByText('Medplum EMR')).toBeInTheDocument();
      expect(screen.getByLabelText('Server URL')).toBeInTheDocument();
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
  });

  describe('settings values display', () => {
    it('displays current whisper server URL', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText('Whisper Server URL')).toHaveValue('http://localhost:8001');
    });

    it('displays current whisper server model', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText('Server Model')).toHaveValue('large-v3-turbo');
    });

    it('displays current language', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText('Language')).toHaveValue('en');
    });

    it('displays current microphone', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText('Microphone')).toHaveValue('default');
    });

    it('displays max speakers slider value', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText('Max Speakers')).toHaveValue('4');
      expect(screen.getByText('4')).toBeInTheDocument();
    });

    it('displays LLM router URL', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText('LLM Router URL')).toHaveValue('http://localhost:8080');
    });

    it('displays SOAP model', () => {
      render(<SettingsDrawer {...defaultProps} />);
      const modelSelect = screen.getByLabelText('SOAP Model');
      expect(modelSelect).toHaveValue('gpt-4');
    });

    it('displays fast model', () => {
      render(<SettingsDrawer {...defaultProps} />);
      const modelSelect = screen.getByLabelText('Fast Model (for greeting detection)');
      expect(modelSelect).toHaveValue('gpt-3.5-turbo');
    });

    it('displays medplum server URL', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText('Server URL')).toHaveValue('http://localhost:8103');
    });

    it('displays LLM client ID', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByLabelText('LLM Client ID')).toHaveValue('clinic-001');
    });
  });

  describe('settings changes', () => {
    it('calls onSettingsChange when whisper server URL changes', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      const input = screen.getByLabelText('Whisper Server URL');
      await user.clear(input);
      await user.type(input, 'http://newserver:8002');

      expect(defaultProps.onSettingsChange).toHaveBeenCalled();
    });

    it('calls onSettingsChange when language changes', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.selectOptions(screen.getByLabelText('Language'), 'es');

      expect(defaultProps.onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({ language: 'es' })
      );
    });

    it('calls onSettingsChange when microphone changes', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.selectOptions(screen.getByLabelText('Microphone'), 'device-1');

      expect(defaultProps.onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({ device: 'device-1' })
      );
    });

    it('calls onSettingsChange when max speakers changes', () => {
      render(<SettingsDrawer {...defaultProps} />);

      const slider = screen.getByLabelText('Max Speakers');
      fireEvent.change(slider, { target: { value: '6' } });

      expect(defaultProps.onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({ max_speakers: 6 })
      );
    });

    it('calls onSettingsChange when LLM router URL changes', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      const input = screen.getByLabelText('LLM Router URL');
      await user.clear(input);
      await user.type(input, 'http://llm:8080');

      expect(defaultProps.onSettingsChange).toHaveBeenCalled();
    });

    it('calls onSettingsChange when medplum auto-sync changes', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      const toggle = screen.getByLabelText('Auto-sync encounters to Medplum');
      await user.click(toggle);

      expect(defaultProps.onSettingsChange).toHaveBeenCalledWith(
        expect.objectContaining({ medplum_auto_sync: true })
      );
    });
  });

  describe('biomarkers toggle', () => {
    it('displays biomarkers toggle with current state', () => {
      render(<SettingsDrawer {...defaultProps} showBiomarkers={true} />);
      expect(screen.getByLabelText('Show biomarkers panel')).toBeChecked();
    });

    it('calls onShowBiomarkersChange when toggled', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} showBiomarkers={true} />);

      await user.click(screen.getByLabelText('Show biomarkers panel'));

      expect(defaultProps.onShowBiomarkersChange).toHaveBeenCalledWith(false);
    });
  });

  describe('connection status indicators', () => {
    it('shows whisper server connected status', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByText('Connected (3 models)')).toBeInTheDocument();
    });

    it('shows whisper server disconnected status', () => {
      render(
        <SettingsDrawer
          {...defaultProps}
          whisperServerStatus={{ connected: false, available_models: [], error: 'Connection refused' }}
          whisperServerModels={[]}
        />
      );
      expect(screen.getByText('Connection refused')).toBeInTheDocument();
    });

    it('shows LLM connected status', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByText('Connected (2 models)')).toBeInTheDocument();
    });

    it('shows LLM disconnected status with error', () => {
      render(
        <SettingsDrawer
          {...defaultProps}
          llmStatus={{ connected: false, available_models: [], error: 'Timeout' }}
          llmModels={[]}
        />
      );
      expect(screen.getByText('Timeout')).toBeInTheDocument();
    });

    it('shows medplum connected status', () => {
      render(<SettingsDrawer {...defaultProps} medplumConnected={true} />);
      expect(screen.getByText('Connected')).toBeInTheDocument();
    });

    it('shows medplum disconnected status with error', () => {
      render(
        <SettingsDrawer {...defaultProps} medplumConnected={false} medplumError="Auth failed" />
      );
      expect(screen.getByText('Auth failed')).toBeInTheDocument();
    });
  });

  describe('test buttons', () => {
    it('calls onTestWhisperServer when test button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      const testButtons = screen.getAllByRole('button', { name: 'Test' });
      await user.click(testButtons[0]); // First test button is for Whisper server

      expect(defaultProps.onTestWhisperServer).toHaveBeenCalled();
    });

    it('calls onTestLLM when test button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      const testButtons = screen.getAllByRole('button', { name: 'Test' });
      await user.click(testButtons[1]); // Second test button is for LLM Router

      expect(defaultProps.onTestLLM).toHaveBeenCalled();
    });

    it('calls onTestMedplum when test button clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      const testButtons = screen.getAllByRole('button', { name: 'Test' });
      await user.click(testButtons[2]); // Third test button is for Medplum

      expect(defaultProps.onTestMedplum).toHaveBeenCalled();
    });
  });

  describe('authentication', () => {
    it('shows sign in button when not authenticated', () => {
      render(<SettingsDrawer {...defaultProps} />);
      expect(screen.getByRole('button', { name: 'Sign In with Medplum' })).toBeInTheDocument();
    });

    it('disables sign in button when not connected', () => {
      render(<SettingsDrawer {...defaultProps} medplumConnected={false} />);
      expect(screen.getByRole('button', { name: 'Sign In with Medplum' })).toBeDisabled();
    });

    it('shows signing in state with cancel button', () => {
      render(<SettingsDrawer {...defaultProps} authLoading={true} />);
      expect(screen.getByRole('button', { name: 'Signing in...' })).toBeDisabled();
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    });

    it('calls onLogin when sign in clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} />);

      await user.click(screen.getByRole('button', { name: 'Sign In with Medplum' }));

      expect(defaultProps.onLogin).toHaveBeenCalled();
    });

    it('calls onCancelLogin when cancel clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} authLoading={true} />);

      await user.click(screen.getByRole('button', { name: 'Cancel' }));

      expect(defaultProps.onCancelLogin).toHaveBeenCalled();
    });

    it('shows user info when authenticated', () => {
      render(<SettingsDrawer {...defaultProps} authState={authenticatedAuthState} />);
      expect(screen.getByText('Dr. Test')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Sign Out' })).toBeInTheDocument();
    });

    it('shows fallback text when authenticated without practitioner name', () => {
      const authWithoutName = { ...authenticatedAuthState, practitioner_name: null };
      render(<SettingsDrawer {...defaultProps} authState={authWithoutName} />);
      expect(screen.getByText('Signed in')).toBeInTheDocument();
    });

    it('calls onLogout when sign out clicked', async () => {
      const user = userEvent.setup();
      render(<SettingsDrawer {...defaultProps} authState={authenticatedAuthState} />);

      await user.click(screen.getByRole('button', { name: 'Sign Out' }));

      expect(defaultProps.onLogout).toHaveBeenCalled();
    });

    it('shows signing out state', () => {
      render(
        <SettingsDrawer
          {...defaultProps}
          authState={authenticatedAuthState}
          authLoading={true}
        />
      );
      expect(screen.getByRole('button', { name: 'Signing out...' })).toBeDisabled();
    });

    it('disables medplum URL and client ID when authenticated', () => {
      render(<SettingsDrawer {...defaultProps} authState={authenticatedAuthState} />);
      expect(screen.getByLabelText('Server URL')).toBeDisabled();
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

  describe('language options', () => {
    it('renders all language options', () => {
      render(<SettingsDrawer {...defaultProps} />);

      const languageSelect = screen.getByLabelText('Language');
      const options = languageSelect.querySelectorAll('option');

      expect(options).toHaveLength(8);
      expect(options[0]).toHaveValue('en');
      expect(options[0]).toHaveTextContent('English');
      expect(options[7]).toHaveValue('auto');
      expect(options[7]).toHaveTextContent('Auto-detect');
    });
  });

  describe('device options', () => {
    it('renders default option plus devices', () => {
      render(<SettingsDrawer {...defaultProps} />);

      const deviceSelect = screen.getByLabelText('Microphone');
      const options = deviceSelect.querySelectorAll('option');

      expect(options).toHaveLength(3); // default + 2 devices
      expect(options[0]).toHaveValue('default');
      expect(options[1]).toHaveValue('device-1');
      expect(options[2]).toHaveValue('device-2');
    });
  });

  describe('max speakers slider', () => {
    it('has correct min and max values', () => {
      render(<SettingsDrawer {...defaultProps} />);

      const slider = screen.getByLabelText('Max Speakers');
      expect(slider).toHaveAttribute('min', '2');
      expect(slider).toHaveAttribute('max', '10');
    });
  });

  describe('null pendingSettings', () => {
    it('renders empty drawer when pendingSettings is null', () => {
      render(<SettingsDrawer {...defaultProps} pendingSettings={null} />);

      expect(screen.getByText('Settings')).toBeInTheDocument();
      expect(screen.queryByLabelText('Whisper Server URL')).not.toBeInTheDocument();
    });
  });
});

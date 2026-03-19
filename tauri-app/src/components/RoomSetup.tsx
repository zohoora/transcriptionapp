import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RoomSelect } from './RoomSelect';
import type { Room } from '../types';

interface RoomSetupProps {
  onComplete: () => void;
  /** If provided, skip server URL step and go straight to room selection */
  existingServerUrl?: string | null;
}

/**
 * Full-screen setup for room configuration.
 * Step 1 (first-run only): Enter profile server URL and test connection.
 * Step 2: Select or create a room from the server.
 */
export function RoomSetup({ onComplete, existingServerUrl }: RoomSetupProps) {
  const [serverUrl, setServerUrl] = useState(existingServerUrl || 'http://100.119.83.76:8090');
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<boolean | null>(null);
  // Skip step 1 if we already have a server URL (changing rooms, not first run)
  const [step, setStep] = useState<'server' | 'room'>(existingServerUrl ? 'room' : 'server');

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const result = await invoke<boolean>('test_profile_server', { url: serverUrl });
      setTestResult(result);
    } catch {
      setTestResult(false);
    } finally {
      setTesting(false);
    }
  };

  const handleContinueToRooms = useCallback(async () => {
    // Save the server URL first so the profile client gets initialized
    try {
      await invoke('save_room_config', {
        roomName: 'Unconfigured',
        profileServerUrl: serverUrl,
        roomId: null,
      });
    } catch (e) {
      console.error('Failed to save initial room config:', e);
    }
    setStep('room');
  }, [serverUrl]);

  const handleRoomSelected = useCallback(async (room: Room) => {
    try {
      await invoke('save_room_config', {
        roomName: room.name,
        profileServerUrl: serverUrl,
        roomId: room.id,
      });
      onComplete();
    } catch (e) {
      console.error('Failed to save room config:', e);
    }
  }, [serverUrl, onComplete]);

  if (step === 'room') {
    return (
      <RoomSelect
        profileServerUrl={serverUrl}
        onSelect={handleRoomSelected}
      />
    );
  }

  // Step 1: Server URL
  return (
    <div className="room-setup-overlay">
      <div className="room-setup-card">
        <h2 className="room-setup-title">Welcome</h2>
        <p className="room-setup-subtitle">
          Connect to your clinic's profile server
        </p>

        <div className="room-setup-field">
          <label className="room-setup-label" htmlFor="server-url">
            Profile Server URL
          </label>
          <div className="room-setup-url-row">
            <input
              id="server-url"
              type="text"
              className="room-setup-input"
              value={serverUrl}
              onChange={(e) => {
                setServerUrl(e.target.value);
                setTestResult(null);
              }}
              placeholder="http://..."
              autoFocus
            />
            <button
              className="room-setup-test-btn"
              onClick={handleTest}
              disabled={testing || !serverUrl.trim()}
            >
              {testing ? 'Testing...' : 'Test'}
            </button>
          </div>
          {testResult !== null && (
            <span className={`room-setup-test-result ${testResult ? 'success' : 'failure'}`}>
              {testResult ? 'Connected successfully' : 'Connection failed'}
            </span>
          )}
        </div>

        <button
          className="room-setup-save-btn"
          onClick={handleContinueToRooms}
          disabled={!testResult || !serverUrl.trim()}
        >
          Continue
        </button>
      </div>
    </div>
  );
}

export default RoomSetup;

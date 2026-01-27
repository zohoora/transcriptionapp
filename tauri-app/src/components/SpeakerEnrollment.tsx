import { useState, useRef, useCallback, useEffect } from 'react';
import type { SpeakerProfileInfo, SpeakerRole } from '../types';
import { SPEAKER_ROLE_LABELS } from '../types';
import { useSpeakerProfiles } from '../hooks/useSpeakerProfiles';

type EnrollmentStep = 'list' | 'form' | 'recording' | 'saving';

interface EnrollmentFormData {
  name: string;
  role: SpeakerRole;
  description: string;
}

const SAMPLE_RATE = 16000;
const MIN_RECORDING_SECONDS = 5;
const MAX_RECORDING_SECONDS = 15;

/**
 * Speaker Enrollment component for managing speaker profiles.
 *
 * Allows users to:
 * - View existing speaker profiles
 * - Create new profiles with audio enrollment
 * - Edit profile metadata
 * - Re-enroll with new audio samples
 * - Delete profiles
 */
export function SpeakerEnrollment() {
  const {
    profiles,
    isLoading,
    isSaving,
    error,
    createProfile,
    updateProfile,
    reenrollProfile,
    deleteProfile,
  } = useSpeakerProfiles();

  // UI state
  const [step, setStep] = useState<EnrollmentStep>('list');
  const [editingProfile, setEditingProfile] = useState<SpeakerProfileInfo | null>(null);
  const [formData, setFormData] = useState<EnrollmentFormData>({
    name: '',
    role: 'physician',
    description: '',
  });
  const [localError, setLocalError] = useState<string | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);

  // Recording state
  const [recordingSeconds, setRecordingSeconds] = useState(0);
  const [audioSamples, setAudioSamples] = useState<Float32Array | null>(null);

  // Audio refs
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const processorRef = useRef<ScriptProcessorNode | null>(null);
  const samplesRef = useRef<number[]>([]);
  const recordingTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      stopRecording();
    };
  }, []);

  const resetForm = useCallback(() => {
    setFormData({ name: '', role: 'physician', description: '' });
    setEditingProfile(null);
    setAudioSamples(null);
    setRecordingSeconds(0);
    setLocalError(null);
    samplesRef.current = [];
  }, []);

  const startNewEnrollment = useCallback(() => {
    resetForm();
    setStep('form');
  }, [resetForm]);

  const startEditProfile = useCallback((profile: SpeakerProfileInfo) => {
    setEditingProfile(profile);
    setFormData({
      name: profile.name,
      role: profile.role,
      description: profile.description,
    });
    setAudioSamples(null);
    setStep('form');
  }, []);

  const cancelEnrollment = useCallback(() => {
    resetForm();
    setStep('list');
  }, [resetForm]);

  const startRecording = useCallback(async () => {
    setLocalError(null);
    samplesRef.current = [];

    try {
      // Request microphone access
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          sampleRate: SAMPLE_RATE,
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
        },
      });

      mediaStreamRef.current = stream;

      // Create audio context for processing
      const audioContext = new AudioContext({ sampleRate: SAMPLE_RATE });
      audioContextRef.current = audioContext;

      const source = audioContext.createMediaStreamSource(stream);

      // Use ScriptProcessorNode to capture raw audio samples
      // Note: This is deprecated but more compatible than AudioWorklet
      const processor = audioContext.createScriptProcessor(4096, 1, 1);
      processorRef.current = processor;

      processor.onaudioprocess = (e) => {
        const inputData = e.inputBuffer.getChannelData(0);
        // Copy samples to our buffer
        for (let i = 0; i < inputData.length; i++) {
          samplesRef.current.push(inputData[i]);
        }
      };

      source.connect(processor);
      processor.connect(audioContext.destination);

      setRecordingSeconds(0);
      setStep('recording');

      // Start timer
      recordingTimerRef.current = setInterval(() => {
        setRecordingSeconds((prev) => {
          const next = prev + 1;
          // Auto-stop at max duration
          if (next >= MAX_RECORDING_SECONDS) {
            stopRecording();
          }
          return next;
        });
      }, 1000);
    } catch (err) {
      console.error('Failed to start recording:', err);
      setLocalError('Failed to access microphone. Please check permissions.');
    }
  }, []);

  const stopRecording = useCallback(() => {
    // Stop timer
    if (recordingTimerRef.current) {
      clearInterval(recordingTimerRef.current);
      recordingTimerRef.current = null;
    }

    // Stop audio processing
    if (processorRef.current) {
      processorRef.current.disconnect();
      processorRef.current = null;
    }

    if (audioContextRef.current) {
      audioContextRef.current.close();
      audioContextRef.current = null;
    }

    // Stop media stream
    if (mediaStreamRef.current) {
      mediaStreamRef.current.getTracks().forEach((track) => track.stop());
      mediaStreamRef.current = null;
    }

    // Convert samples to Float32Array
    if (samplesRef.current.length > 0) {
      const samples = new Float32Array(samplesRef.current);
      setAudioSamples(samples);
      console.log(`Recorded ${samples.length} samples (${(samples.length / SAMPLE_RATE).toFixed(1)}s)`);
    }

    setStep('form');
  }, []);

  const handleSave = useCallback(async () => {
    setLocalError(null);

    // Validate form
    if (!formData.name.trim()) {
      setLocalError('Please enter a name');
      return;
    }

    // For new profiles, require audio
    if (!editingProfile && !audioSamples) {
      setLocalError('Please record an audio sample');
      return;
    }

    // Check minimum recording duration
    if (audioSamples && audioSamples.length < SAMPLE_RATE * MIN_RECORDING_SECONDS) {
      setLocalError(`Please record at least ${MIN_RECORDING_SECONDS} seconds of audio`);
      return;
    }

    setStep('saving');

    try {
      if (editingProfile) {
        // Update existing profile
        if (audioSamples) {
          // Re-enroll with new audio
          await reenrollProfile(editingProfile.id, audioSamples);
        }
        // Update metadata
        await updateProfile(
          editingProfile.id,
          formData.name.trim(),
          formData.role,
          formData.description.trim()
        );
      } else {
        // Create new profile
        await createProfile(
          formData.name.trim(),
          formData.role,
          formData.description.trim(),
          audioSamples!
        );
      }

      resetForm();
      setStep('list');
    } catch (err) {
      console.error('Failed to save profile:', err);
      setLocalError(String(err));
      setStep('form');
    }
  }, [formData, audioSamples, editingProfile, createProfile, updateProfile, reenrollProfile, resetForm]);

  const handleDelete = useCallback(async (profileId: string) => {
    try {
      await deleteProfile(profileId);
      setDeleteConfirm(null);
    } catch (err) {
      console.error('Failed to delete profile:', err);
      setLocalError(String(err));
    }
  }, [deleteProfile]);

  // Role options for dropdown
  const roleOptions: SpeakerRole[] = ['physician', 'pa', 'rn', 'ma', 'patient', 'other'];

  // Render profile list
  if (step === 'list') {
    return (
      <div className="speaker-enrollment">
        <div className="speaker-enrollment-header">
          <span className="speaker-enrollment-title">Speaker Profiles</span>
          <button
            className="btn-add-speaker"
            onClick={startNewEnrollment}
            disabled={isLoading}
          >
            + Add
          </button>
        </div>

        {error && <div className="enrollment-error">{error}</div>}
        {localError && <div className="enrollment-error">{localError}</div>}

        {isLoading ? (
          <div className="enrollment-loading">Loading profiles...</div>
        ) : profiles.length === 0 ? (
          <div className="enrollment-empty">
            <p>No speaker profiles yet.</p>
            <p className="enrollment-hint">
              Add profiles to help the system recognize speakers by name.
            </p>
          </div>
        ) : (
          <div className="speaker-profiles-list">
            {profiles.map((profile) => (
              <div key={profile.id} className="speaker-profile-item">
                <div className="profile-info">
                  <span className="profile-name">{profile.name}</span>
                  <span className="profile-role">{SPEAKER_ROLE_LABELS[profile.role]}</span>
                  {profile.description && (
                    <span className="profile-description">{profile.description}</span>
                  )}
                </div>
                <div className="profile-actions">
                  {deleteConfirm === profile.id ? (
                    <>
                      <button
                        className="btn-confirm-delete"
                        onClick={() => handleDelete(profile.id)}
                        disabled={isSaving}
                      >
                        Confirm
                      </button>
                      <button
                        className="btn-cancel-delete"
                        onClick={() => setDeleteConfirm(null)}
                      >
                        Cancel
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        className="btn-edit-profile"
                        onClick={() => startEditProfile(profile)}
                        disabled={isSaving}
                      >
                        Edit
                      </button>
                      <button
                        className="btn-delete-profile"
                        onClick={() => setDeleteConfirm(profile.id)}
                        disabled={isSaving}
                      >
                        Delete
                      </button>
                    </>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  // Render recording step
  if (step === 'recording') {
    const canStop = recordingSeconds >= MIN_RECORDING_SECONDS;
    const progress = Math.min(100, (recordingSeconds / MAX_RECORDING_SECONDS) * 100);

    return (
      <div className="speaker-enrollment">
        <div className="speaker-enrollment-header">
          <span className="speaker-enrollment-title">Recording Voice Sample</span>
        </div>

        <div className="recording-container">
          <div className="recording-indicator">
            <div className="recording-pulse" />
            <span className="recording-time">{recordingSeconds}s</span>
          </div>

          <div className="recording-progress">
            <div
              className="recording-progress-bar"
              style={{ width: `${progress}%` }}
            />
          </div>

          <p className="recording-instructions">
            {recordingSeconds < MIN_RECORDING_SECONDS
              ? `Keep speaking... (${MIN_RECORDING_SECONDS - recordingSeconds}s more needed)`
              : 'You can stop now, or continue for better accuracy'}
          </p>

          <div className="recording-actions">
            <button
              className="btn-stop-recording"
              onClick={stopRecording}
              disabled={!canStop}
            >
              {canStop ? 'Stop Recording' : `Wait ${MIN_RECORDING_SECONDS - recordingSeconds}s`}
            </button>
            <button
              className="btn-cancel-recording"
              onClick={() => {
                stopRecording();
                cancelEnrollment();
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Render form step (and saving step with disabled form)
  return (
    <div className="speaker-enrollment">
      <div className="speaker-enrollment-header">
        <span className="speaker-enrollment-title">
          {editingProfile ? 'Edit Speaker' : 'New Speaker'}
        </span>
      </div>

      {(error || localError) && (
        <div className="enrollment-error">{error || localError}</div>
      )}

      <div className="enrollment-form">
        <div className="form-group">
          <label htmlFor="speaker-name">Name</label>
          <input
            id="speaker-name"
            type="text"
            className="settings-input"
            value={formData.name}
            onChange={(e) => setFormData({ ...formData, name: e.target.value })}
            placeholder="Dr. Smith"
            disabled={step === 'saving'}
          />
        </div>

        <div className="form-group">
          <label htmlFor="speaker-role">Role</label>
          <select
            id="speaker-role"
            className="settings-select"
            value={formData.role}
            onChange={(e) => setFormData({ ...formData, role: e.target.value as SpeakerRole })}
            disabled={step === 'saving'}
          >
            {roleOptions.map((role) => (
              <option key={role} value={role}>
                {SPEAKER_ROLE_LABELS[role]}
              </option>
            ))}
          </select>
        </div>

        <div className="form-group">
          <label htmlFor="speaker-description">Description (optional)</label>
          <input
            id="speaker-description"
            type="text"
            className="settings-input"
            value={formData.description}
            onChange={(e) => setFormData({ ...formData, description: e.target.value })}
            placeholder="Attending physician, internal medicine"
            disabled={step === 'saving'}
          />
        </div>

        <div className="form-group">
          <label>Voice Sample</label>
          {audioSamples ? (
            <div className="audio-sample-status">
              <span className="audio-sample-indicator">✓</span>
              <span>
                {(audioSamples.length / SAMPLE_RATE).toFixed(1)}s recorded
              </span>
              <button
                className="btn-rerecord"
                onClick={startRecording}
                disabled={step === 'saving'}
              >
                Re-record
              </button>
            </div>
          ) : editingProfile ? (
            <div className="audio-sample-status">
              <span className="audio-sample-existing">Using existing voice sample</span>
              <button
                className="btn-rerecord"
                onClick={startRecording}
                disabled={step === 'saving'}
              >
                Re-record
              </button>
            </div>
          ) : (
            <button
              className="btn-record"
              onClick={startRecording}
              disabled={step === 'saving'}
            >
              Record Voice Sample
            </button>
          )}
          <p className="form-hint">
            Speak naturally for {MIN_RECORDING_SECONDS}-{MAX_RECORDING_SECONDS} seconds.
            The system will learn to recognize this voice.
          </p>
        </div>

        <div className="form-actions">
          <button
            className="btn-cancel"
            onClick={cancelEnrollment}
            disabled={step === 'saving'}
          >
            Cancel
          </button>
          <button
            className="btn-save-profile"
            onClick={handleSave}
            disabled={step === 'saving' || isSaving}
          >
            {step === 'saving' || isSaving
              ? 'Saving...'
              : editingProfile
                ? 'Save Changes'
                : 'Create Profile'}
          </button>
        </div>
      </div>
    </div>
  );
}

export default SpeakerEnrollment;

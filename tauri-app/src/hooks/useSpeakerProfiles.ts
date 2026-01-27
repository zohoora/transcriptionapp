import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { SpeakerProfileInfo, SpeakerRole } from '../types';

export interface UseSpeakerProfilesResult {
  // State
  profiles: SpeakerProfileInfo[];
  isLoading: boolean;
  isSaving: boolean;
  error: string | null;

  // Actions
  reloadProfiles: () => Promise<void>;
  createProfile: (
    name: string,
    role: SpeakerRole,
    description: string,
    audioSamples: Float32Array
  ) => Promise<SpeakerProfileInfo>;
  updateProfile: (
    profileId: string,
    name: string,
    role: SpeakerRole,
    description: string
  ) => Promise<SpeakerProfileInfo>;
  reenrollProfile: (
    profileId: string,
    audioSamples: Float32Array
  ) => Promise<SpeakerProfileInfo>;
  deleteProfile: (profileId: string) => Promise<void>;

  // Helpers
  getProfile: (profileId: string) => SpeakerProfileInfo | undefined;
  hasProfiles: boolean;
}

/**
 * Hook for managing speaker profiles for enrollment-based speaker recognition.
 *
 * Speaker profiles allow the diarization system to recognize known speakers
 * (physician, PA, RN, etc.) by name instead of generic "Speaker N" labels.
 */
export function useSpeakerProfiles(): UseSpeakerProfilesResult {
  const [profiles, setProfiles] = useState<SpeakerProfileInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const initialLoadRef = useRef(false);

  // Load profiles from backend
  const loadProfiles = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<SpeakerProfileInfo[]>('list_speaker_profiles');
      // Ensure result is an array (defensive against mocks returning undefined)
      setProfiles(Array.isArray(result) ? result : []);
    } catch (e) {
      console.error('Failed to load speaker profiles:', e);
      setError(String(e));
      setProfiles([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Load profiles on mount (only once)
  useEffect(() => {
    if (initialLoadRef.current) return;
    initialLoadRef.current = true;
    loadProfiles();
  }, [loadProfiles]);

  // Create a new profile from audio samples
  const createProfile = useCallback(async (
    name: string,
    role: SpeakerRole,
    description: string,
    audioSamples: Float32Array
  ): Promise<SpeakerProfileInfo> => {
    setIsSaving(true);
    setError(null);
    try {
      // Convert Float32Array to regular array for Tauri serialization
      const samples = Array.from(audioSamples);

      const result = await invoke<SpeakerProfileInfo>('create_speaker_profile', {
        name,
        role,
        description,
        audioSamples: samples,
      });

      // Update local state
      setProfiles(prev => [...prev, result]);
      return result;
    } catch (e) {
      console.error('Failed to create speaker profile:', e);
      const errorMsg = String(e);
      setError(errorMsg);
      throw new Error(errorMsg);
    } finally {
      setIsSaving(false);
    }
  }, []);

  // Update profile metadata (not embedding)
  const updateProfile = useCallback(async (
    profileId: string,
    name: string,
    role: SpeakerRole,
    description: string
  ): Promise<SpeakerProfileInfo> => {
    setIsSaving(true);
    setError(null);
    try {
      const result = await invoke<SpeakerProfileInfo>('update_speaker_profile', {
        profileId,
        name,
        role,
        description,
      });

      // Update local state
      setProfiles(prev => prev.map(p => p.id === profileId ? result : p));
      return result;
    } catch (e) {
      console.error('Failed to update speaker profile:', e);
      const errorMsg = String(e);
      setError(errorMsg);
      throw new Error(errorMsg);
    } finally {
      setIsSaving(false);
    }
  }, []);

  // Re-enroll with new audio samples
  const reenrollProfile = useCallback(async (
    profileId: string,
    audioSamples: Float32Array
  ): Promise<SpeakerProfileInfo> => {
    setIsSaving(true);
    setError(null);
    try {
      const samples = Array.from(audioSamples);

      const result = await invoke<SpeakerProfileInfo>('reenroll_speaker_profile', {
        profileId,
        audioSamples: samples,
      });

      // Update local state
      setProfiles(prev => prev.map(p => p.id === profileId ? result : p));
      return result;
    } catch (e) {
      console.error('Failed to re-enroll speaker profile:', e);
      const errorMsg = String(e);
      setError(errorMsg);
      throw new Error(errorMsg);
    } finally {
      setIsSaving(false);
    }
  }, []);

  // Delete a profile
  const deleteProfile = useCallback(async (profileId: string): Promise<void> => {
    setIsSaving(true);
    setError(null);
    try {
      await invoke('delete_speaker_profile', { profileId });

      // Update local state
      setProfiles(prev => prev.filter(p => p.id !== profileId));
    } catch (e) {
      console.error('Failed to delete speaker profile:', e);
      const errorMsg = String(e);
      setError(errorMsg);
      throw new Error(errorMsg);
    } finally {
      setIsSaving(false);
    }
  }, []);

  // Get a profile by ID
  const getProfile = useCallback((profileId: string): SpeakerProfileInfo | undefined => {
    return profiles.find(p => p.id === profileId);
  }, [profiles]);

  return {
    profiles,
    isLoading,
    isSaving,
    error,
    reloadProfiles: loadProfiles,
    createProfile,
    updateProfile,
    reenrollProfile,
    deleteProfile,
    getProfile,
    hasProfiles: profiles.length > 0,
  };
}

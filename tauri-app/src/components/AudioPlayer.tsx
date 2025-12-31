import React, { useState, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface AudioPlayerProps {
  audioUrl: string; // "Binary/abc123" format from Medplum
}

const AudioPlayer: React.FC<AudioPlayerProps> = ({ audioUrl }) => {
  const [audioBlob, setAudioBlob] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const audioRef = useRef<HTMLAudioElement>(null);

  // Clean up blob URL on unmount
  useEffect(() => {
    return () => {
      if (audioBlob) {
        URL.revokeObjectURL(audioBlob);
      }
    };
  }, [audioBlob]);

  const loadAudio = async () => {
    if (audioBlob) return; // Already loaded

    setIsLoading(true);
    setError(null);

    try {
      // Extract binary ID from URL
      // Handles both formats:
      // - "Binary/abc123"
      // - "http://localhost:8103/fhir/R4/Binary/abc123"
      let binaryId = audioUrl;
      if (audioUrl.includes('/Binary/')) {
        const match = audioUrl.match(/\/Binary\/([^/]+)$/);
        binaryId = match ? match[1] : audioUrl;
      } else if (audioUrl.startsWith('Binary/')) {
        binaryId = audioUrl.replace('Binary/', '');
      }

      // Fetch audio data from Medplum
      const data = await invoke<number[]>('medplum_get_audio_data', { binaryId });

      // Convert to Blob and create object URL
      const blob = new Blob([new Uint8Array(data)], { type: 'audio/wav' });
      const url = URL.createObjectURL(blob);
      setAudioBlob(url);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsLoading(false);
    }
  };

  const togglePlay = async () => {
    if (!audioBlob) {
      await loadAudio();
    }

    if (audioRef.current) {
      if (isPlaying) {
        audioRef.current.pause();
      } else {
        audioRef.current.play();
      }
    }
  };

  const handleTimeUpdate = () => {
    if (audioRef.current) {
      setCurrentTime(audioRef.current.currentTime);
    }
  };

  const handleLoadedMetadata = () => {
    if (audioRef.current) {
      setDuration(audioRef.current.duration);
    }
  };

  const handleSeek = (e: React.ChangeEvent<HTMLInputElement>) => {
    const time = parseFloat(e.target.value);
    if (audioRef.current) {
      audioRef.current.currentTime = time;
      setCurrentTime(time);
    }
  };

  const formatTime = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  if (error) {
    return (
      <div className="audio-player error">
        <span className="error-text">Failed to load audio</span>
      </div>
    );
  }

  return (
    <div className="audio-player">
      {audioBlob && (
        <audio
          ref={audioRef}
          src={audioBlob}
          onTimeUpdate={handleTimeUpdate}
          onLoadedMetadata={handleLoadedMetadata}
          onPlay={() => setIsPlaying(true)}
          onPause={() => setIsPlaying(false)}
          onEnded={() => setIsPlaying(false)}
        />
      )}

      <button
        className="play-btn"
        onClick={togglePlay}
        disabled={isLoading}
        aria-label={isPlaying ? 'Pause' : 'Play'}
      >
        {isLoading ? (
          <span className="spinner-small" />
        ) : isPlaying ? (
          '||'
        ) : (
          '>'
        )}
      </button>

      {audioBlob && duration > 0 && (
        <>
          <input
            type="range"
            className="audio-seek"
            min={0}
            max={duration}
            value={currentTime}
            onChange={handleSeek}
            aria-label="Seek"
          />
          <span className="audio-time">
            {formatTime(currentTime)} / {formatTime(duration)}
          </span>
        </>
      )}

      {!audioBlob && !isLoading && (
        <span className="audio-label">Click to load audio</span>
      )}
    </div>
  );
};

export default AudioPlayer;

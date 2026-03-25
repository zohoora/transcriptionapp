import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { CalibrationStatus } from '../types';

const PHASES = ['connecting', 'emptyRoom', 'onePerson', 'twoPeople', 'threePlus'];
const PHASE_LABELS: Record<string, string> = {
  connecting: 'Connecting',
  emptyRoom: 'Empty Room',
  onePerson: '1 Person',
  twoPeople: '2 People',
  threePlus: '3+ People',
  computing: 'Computing',
  complete: 'Complete',
  failed: 'Failed',
};

const PHASE_INSTRUCTIONS: Record<string, string> = {
  connecting: 'Connecting to sensor...',
  emptyRoom: 'Please leave the room and close the door. The sensor needs to measure the empty room CO2 baseline. This takes about 3-5 minutes.',
  onePerson: 'Enter the room alone and close the door. Stay for 3-5 minutes until CO2 stabilizes.',
  twoPeople: 'Have a second person join you in the room. Wait for CO2 to stabilize (3-5 minutes).',
  threePlus: 'Have 3 or more people in the room. This helps calibrate the per-person CO2 contribution.',
  computing: 'Computing calibration results...',
  complete: 'Calibration complete.',
  failed: 'Calibration failed.',
};

function formatTime(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${s.toString().padStart(2, '0')}`;
}

function co2Color(ppm: number | null): string {
  if (ppm === null) return '#999';
  if (ppm < 450) return '#2e7d32';
  if (ppm < 600) return '#f9a825';
  return '#e65100';
}

function Sparkline({ data }: { data: number[] }) {
  if (data.length < 2) return null;
  const min = Math.min(...data) - 5;
  const max = Math.max(...data) + 5;
  const range = max - min || 1;
  const w = 200;
  const h = 40;
  const points = data.map((v, i) => {
    const x = (i / (data.length - 1)) * w;
    const y = h - ((v - min) / range) * h;
    return `${x},${y}`;
  }).join(' ');
  return (
    <svg width={w} height={h} style={{ display: 'block' }}>
      <polyline points={points} fill="none" stroke="#1a73e8" strokeWidth="1.5" />
    </svg>
  );
}

const CalibrationWindow: React.FC = () => {
  const params = new URLSearchParams(window.location.search);
  const roomId = params.get('roomId') || '';
  const sensorUrl = params.get('sensorUrl') || '';
  const roomName = params.get('roomName') || 'Room';

  const [status, setStatus] = useState<CalibrationStatus | null>(null);
  const [running, setRunning] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);

  // Listen for calibration updates
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let mounted = true;

    listen<CalibrationStatus>('calibration_update', (event) => {
      if (mounted) {
        setStatus(event.payload);
      }
    }).then((fn) => {
      if (mounted) unlisten = fn;
      else fn();
    });

    return () => {
      mounted = false;
      if (unlisten) unlisten();
    };
  }, []);

  const handleStart = useCallback(async () => {
    setStartError(null);
    try {
      await invoke('start_co2_calibration', { roomId, sensorUrl });
      setRunning(true);
    } catch (e) {
      setStartError(String(e));
    }
  }, [roomId, sensorUrl]);

  const handleStop = useCallback(async () => {
    try {
      await invoke('stop_co2_calibration');
    } catch (_) { /* ignore */ }
    setRunning(false);
  }, []);

  const handleAdvance = useCallback(async (skip: boolean) => {
    try {
      await invoke('advance_calibration_phase', { skip });
    } catch (_) { /* ignore */ }
  }, []);

  const handleSave = useCallback(async () => {
    if (!status?.result) return;
    setSaving(true);
    try {
      await invoke('update_room', {
        roomId,
        updates: {
          co2_baseline_ppm: status.result.baselinePpm,
        },
      });
      setSaved(true);
    } catch (e) {
      setStartError(String(e));
    } finally {
      setSaving(false);
    }
  }, [roomId, status]);

  const handleClose = useCallback(async () => {
    if (running) {
      try { await invoke('stop_co2_calibration'); } catch (_) { /* ignore */ }
    }
    const win = getCurrentWindow();
    await win.close();
  }, [running]);

  const phase = status?.phase || 'idle';
  const phaseIndex = PHASES.indexOf(phase);
  const isCalibrating = PHASES.includes(phase);

  return (
    <div className="calibration-window">
      {/* Header */}
      <div className="calibration-header">
        <div>
          <h1 className="calibration-title">CO2 Calibration</h1>
          <span className="calibration-room">{roomName}</span>
        </div>
        <button className="calibration-close" onClick={handleClose}>&times;</button>
      </div>

      {/* Pre-start */}
      {!running && phase === 'idle' && (
        <div className="calibration-start-panel">
          <p className="calibration-desc">
            This tool calibrates the CO2 sensor for this room. You'll be guided through several phases
            where the room will need to be empty, then occupied by different numbers of people. Each phase
            takes 3-5 minutes. You can skip phases if needed.
          </p>
          <p className="calibration-desc" style={{ fontSize: 11, opacity: 0.6 }}>
            Sensor: {sensorUrl}
          </p>
          {startError && <div className="calibration-error">{startError}</div>}
          <button className="calibration-btn-primary" onClick={handleStart}>
            Start Calibration
          </button>
        </div>
      )}

      {/* Live readings */}
      {running && status && (
        <>
          <div className="calibration-live">
            <div className="calibration-co2-display">
              <span className="calibration-co2-value" style={{ color: co2Color(status.currentCo2Ppm) }}>
                {status.currentCo2Ppm !== null ? Math.round(status.currentCo2Ppm) : '--'}
              </span>
              <span className="calibration-co2-unit">ppm</span>
            </div>
            <div className="calibration-secondary-readings">
              <span>{status.currentTempC !== null ? `${status.currentTempC.toFixed(1)}°C` : '--'}</span>
              <span>{status.currentHumidityPct !== null ? `${status.currentHumidityPct.toFixed(0)}% RH` : '--'}</span>
              <span className={`calibration-presence-dot ${status.mmwavePresent ? 'present' : 'absent'}`}>
                {status.mmwavePresent ? 'Present' : 'Empty'}
              </span>
              {!status.sensorConnected && <span className="calibration-error-inline">Sensor disconnected</span>}
            </div>
            <Sparkline data={status.sparkline} />
          </div>

          {/* Phase stepper */}
          {isCalibrating && (
            <div className="calibration-stepper">
              {PHASES.map((p, i) => {
                const completed = i < phaseIndex;
                const current = p === phase;
                const skipped = status.result?.phaseResults.find(r => r.phase === p)?.wasSkipped;
                return (
                  <div key={p} className={`calibration-step ${completed ? 'completed' : ''} ${current ? 'current' : ''} ${skipped ? 'skipped' : ''}`}>
                    <div className="calibration-step-dot">
                      {completed ? (skipped ? '—' : '✓') : i + 1}
                    </div>
                    <span className="calibration-step-label">{PHASE_LABELS[p]}</span>
                  </div>
                );
              })}
            </div>
          )}

          {/* Phase instructions */}
          {isCalibrating && (
            <div className="calibration-phase-panel">
              <h3 className="calibration-phase-title">{PHASE_LABELS[phase] || phase}</h3>
              <p className="calibration-phase-instruction">{PHASE_INSTRUCTIONS[phase]}</p>

              {phase === 'emptyRoom' && status.mmwavePresent && (
                <div className="calibration-warning">
                  Someone detected in the room — please ensure it is empty.
                </div>
              )}

              <div className="calibration-phase-stats">
                <span>{formatTime(status.phaseElapsedSecs)} elapsed</span>
                <span>{status.isStable ? '✓ Stable' : 'Stabilizing...'}</span>
                <span>{status.rateOfChangePpmPerMin >= 0 ? '+' : ''}{status.rateOfChangePpmPerMin.toFixed(1)} ppm/min</span>
                <span>{status.readingsInPhase} readings</span>
              </div>

              <div className="calibration-phase-actions">
                <button
                  className={`calibration-btn-primary ${status.isStable ? 'ready' : ''}`}
                  onClick={() => handleAdvance(false)}
                >
                  {status.isStable ? 'Next Phase' : 'Advance Early'}
                </button>
                <button className="calibration-btn-secondary" onClick={() => handleAdvance(true)}>
                  Skip
                </button>
                <button className="calibration-btn-danger" onClick={handleStop}>
                  Stop
                </button>
              </div>
            </div>
          )}

          {/* Computing */}
          {phase === 'computing' && (
            <div className="calibration-phase-panel">
              <h3 className="calibration-phase-title">Computing Results...</h3>
            </div>
          )}

          {/* Results */}
          {phase === 'complete' && status.result && (
            <div className="calibration-results">
              <h3 className="calibration-results-title">Calibration Results</h3>
              <div className="calibration-results-grid">
                <div className="calibration-result-item">
                  <span className="calibration-result-label">Baseline</span>
                  <span className="calibration-result-value">{status.result.baselinePpm.toFixed(0)} ppm</span>
                </div>
                <div className="calibration-result-item">
                  <span className="calibration-result-label">Per Person</span>
                  <span className="calibration-result-value">{status.result.ppmPerPerson.toFixed(1)} ppm</span>
                </div>
                <div className="calibration-result-item">
                  <span className="calibration-result-label">Window</span>
                  <span className="calibration-result-value">{status.result.recommendedWindowSecs}s</span>
                </div>
                <div className="calibration-result-item">
                  <span className="calibration-result-label">Rise Rate</span>
                  <span className="calibration-result-value">{status.result.riseRatePpmPerMin.toFixed(1)} ppm/min</span>
                </div>
                <div className="calibration-result-item">
                  <span className="calibration-result-label">Fall Rate</span>
                  <span className="calibration-result-value">{status.result.fallRatePpmPerMin.toFixed(1)} ppm/min</span>
                </div>
              </div>

              {status.result.ppmPerPerson < 15 && (
                <div className="calibration-warning">
                  Low per-person CO2 contribution detected. The room may be too well-ventilated for reliable CO2-based occupancy detection.
                </div>
              )}

              <div className="calibration-results-phases">
                <h4>Phase Summary</h4>
                {status.result.phaseResults.map((pr, i) => (
                  <div key={i} className="calibration-phase-result">
                    <span>{PHASE_LABELS[pr.phase] || pr.phase}</span>
                    <span>{pr.wasSkipped ? 'Skipped' : `${pr.stableAvgPpm.toFixed(0)} ppm (±${pr.stableStdDev.toFixed(1)})`}</span>
                  </div>
                ))}
              </div>

              <div className="calibration-results-actions">
                {!saved ? (
                  <>
                    <button className="calibration-btn-primary" onClick={handleSave} disabled={saving}>
                      {saving ? 'Saving...' : 'Save to Room'}
                    </button>
                    <button className="calibration-btn-secondary" onClick={handleClose}>
                      Discard
                    </button>
                  </>
                ) : (
                  <div className="calibration-saved">
                    ✓ Saved to {roomName}
                    <button className="calibration-btn-secondary" onClick={handleClose} style={{ marginLeft: 12 }}>
                      Close
                    </button>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Failed */}
          {phase === 'failed' && (
            <div className="calibration-phase-panel">
              <h3 className="calibration-phase-title">Calibration Failed</h3>
              <p className="calibration-error">{status.error || 'Sensor connection lost'}</p>
              <button className="calibration-btn-secondary" onClick={handleClose}>Close</button>
            </div>
          )}

          {status.error && phase !== 'failed' && (
            <div className="calibration-error-bar">{status.error}</div>
          )}
        </>
      )}
    </div>
  );
};

export default CalibrationWindow;

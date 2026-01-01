/**
 * EncounterBar - Active encounter display
 *
 * Shows:
 * - Patient name and encounter start time
 * - Sync status indicator
 * - Manual sync button
 * - End encounter button
 */

import React from 'react';
import type { Encounter, SyncStatus } from '../types';
import { formatLocalTime } from '../utils';

interface EncounterBarProps {
  encounter: Encounter;
  syncStatus: SyncStatus;
  isRecording: boolean;
  onSync?: () => void;
  onEndEncounter?: () => void;
  isSyncing?: boolean;
}

export function EncounterBar({
  encounter,
  syncStatus,
  isRecording,
  onSync,
  onEndEncounter,
  isSyncing = false,
}: EncounterBarProps) {
  const formatTime = (isoString: string): string => {
    try {
      return formatLocalTime(isoString);
    } catch {
      return '--:--';
    }
  };

  const formatDuration = (): string => {
    try {
      const start = new Date(encounter.startTime);
      const now = new Date();
      const diffMs = now.getTime() - start.getTime();
      const minutes = Math.floor(diffMs / 60000);
      const seconds = Math.floor((diffMs % 60000) / 1000);
      return `${minutes}:${seconds.toString().padStart(2, '0')}`;
    } catch {
      return '0:00';
    }
  };

  const getSyncStatusIcon = (): React.ReactNode => {
    const allSynced =
      syncStatus.encounterSynced &&
      syncStatus.transcriptSynced &&
      syncStatus.soapNoteSynced &&
      syncStatus.audioSynced;

    if (isSyncing) {
      return <span className="sync-icon syncing" title="Syncing..." />;
    }
    if (allSynced) {
      return <span className="sync-icon synced" title="All synced" />;
    }
    return <span className="sync-icon pending" title="Pending sync" />;
  };

  return (
    <div className="encounter-bar">
      <div className="encounter-info">
        <div className="encounter-patient">
          {encounter.patientName}
        </div>
        <div className="encounter-meta">
          <span className="encounter-time">
            Started {formatTime(encounter.startTime)}
          </span>
          <span className="encounter-duration">
            {formatDuration()}
          </span>
        </div>
      </div>

      <div className="encounter-actions">
        {getSyncStatusIcon()}

        {onSync && !isRecording && (
          <button
            className="encounter-sync-button"
            onClick={onSync}
            disabled={isSyncing}
            title="Sync to EMR"
          >
            {isSyncing ? 'Syncing...' : 'Sync'}
          </button>
        )}

        {onEndEncounter && !isRecording && (
          <button
            className="encounter-end-button"
            onClick={onEndEncounter}
            title="End encounter"
          >
            End
          </button>
        )}
      </div>
    </div>
  );
}

export default EncounterBar;

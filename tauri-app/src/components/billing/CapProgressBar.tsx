import React from 'react';
import type { CapWarningLevel } from '../../types';
import { capWarningColor } from './billingUtils';

interface CapProgressBarProps {
  label: string;
  current: number;
  limit: number;
  unit?: string;
  warningLevel: CapWarningLevel;
}

export const CapProgressBar: React.FC<CapProgressBarProps> = ({
  label, current, limit, unit = 'hrs', warningLevel,
}) => {
  const pct = limit > 0 ? Math.min((current / limit) * 100, 100) : 0;
  return (
    <div className="cap-progress-container">
      <div className="cap-progress-label">
        <span>{label}</span>
        <span>{current.toFixed(1)}{unit} / {limit}{unit}</span>
      </div>
      <div className="cap-progress-bar">
        <div
          className="cap-progress-fill"
          style={{ width: `${pct}%`, backgroundColor: capWarningColor(warningLevel) }}
        />
      </div>
    </div>
  );
};

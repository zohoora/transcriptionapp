import { memo, useMemo } from 'react';
import type { AnalysisCard } from '../../types';

interface SummaryBarProps {
  medCount: number;
  cards: AnalysisCard[];
}

/**
 * Compact row above the analysis results: med count + finding counts.
 * Mirrors `pharmacotherapy-refactorer/frontend/src/components/SummaryBar.tsx`.
 */
export const SummaryBar = memo(function SummaryBar({ medCount, cards }: SummaryBarProps) {
  const { critical, important } = useMemo(() => {
    let critical = 0;
    let important = 0;
    for (const c of cards) {
      if (c.severity === 'critical') critical += 1;
      else if (c.severity === 'important') important += 1;
    }
    return { critical, important };
  }, [cards]);

  return (
    <div className="summary-bar">
      <div className="summary-stat summary-stat-meds">
        <span className="summary-stat-value">{medCount}</span>
        <span className="summary-stat-label">medication{medCount === 1 ? '' : 's'}</span>
      </div>
      <div className="summary-stat">
        <span className="summary-stat-value">{cards.length}</span>
        <span className="summary-stat-label">finding{cards.length === 1 ? '' : 's'}</span>
      </div>
      {critical > 0 && (
        <div className="summary-stat summary-stat-critical">
          <span className="summary-stat-value">{critical}</span>
          <span className="summary-stat-label">critical</span>
        </div>
      )}
      {important > 0 && (
        <div className="summary-stat summary-stat-important">
          <span className="summary-stat-value">{important}</span>
          <span className="summary-stat-label">important</span>
        </div>
      )}
    </div>
  );
});

export default SummaryBar;

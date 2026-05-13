import { memo } from 'react';
import type { BurdenScores } from '../../types';

interface BurdenCardProps {
  label: string;
  value: number;
  maxValue: number;
  description: string;
}

function burdenBarClass(ratio: number): string {
  if (ratio >= 0.6) return 'burden-card-bar-fill burden-card-bar-high';
  if (ratio >= 0.3) return 'burden-card-bar-fill burden-card-bar-medium';
  return 'burden-card-bar-fill burden-card-bar-low';
}

function BurdenCard({ label, value, maxValue, description }: BurdenCardProps) {
  const ratio = Math.max(0, Math.min(1, value / maxValue));
  return (
    <div className="burden-card" title={description}>
      <div className="burden-card-header">
        <span className="burden-card-label">{label}</span>
        <span className="burden-card-value">{value.toFixed(1)}</span>
      </div>
      <div className="burden-card-bar">
        <div
          className={burdenBarClass(ratio)}
          style={{ width: `${ratio * 100}%` }}
        />
      </div>
      <div className="burden-card-scale">
        <span>0</span>
        <span>{maxValue}</span>
      </div>
    </div>
  );
}

interface RiskIndicatorProps {
  label: string;
  count: number;
  icon: React.ReactNode;
}

function RiskIndicator({ label, count, icon }: RiskIndicatorProps) {
  const active = count > 0;
  return (
    <div className={`risk-indicator ${active ? 'risk-indicator-active' : ''}`}>
      <span className="risk-indicator-icon">{icon}</span>
      <div className="risk-indicator-body">
        <span className="risk-indicator-count">{count}</span>
        <span className="risk-indicator-label">{label}</span>
      </div>
    </div>
  );
}

// SVG icons inlined from pharmacotherapy-refactorer's ScoresPanel.tsx for
// visual parity. Plain stroke icons, currentColor-driven for theming.
const HeartIcon = () => (
  <svg className="risk-icon-svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z" />
  </svg>
);
const BrainIcon = () => (
  <svg className="risk-icon-svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
  </svg>
);
const DropletIcon = () => (
  <svg className="risk-icon-svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z" />
  </svg>
);
const WalkingIcon = () => (
  <svg className="risk-icon-svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
  </svg>
);
const KidneyIcon = () => (
  <svg className="risk-icon-svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
  </svg>
);
const LiverIcon = () => (
  <svg className="risk-icon-svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
  </svg>
);
const PotassiumIcon = () => (
  <svg className="risk-icon-svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
  </svg>
);
const CNSIcon = () => (
  <svg className="risk-icon-svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
  </svg>
);

interface ScoresPanelProps {
  scores: BurdenScores;
}

/**
 * Replaces the older `BurdenPanel`. Two sections:
 *  1. Cumulative Burden Scores — three burden bars with thresholds 0.3/0.6
 *     mapping value/maxValue → low/medium/high color (mirrors
 *     `pharmacotherapy-refactorer/frontend/src/components/burden/BurdenCard.tsx`).
 *  2. Drug Risk Counts — eight icon-tagged risk tiles, including CNS
 *     Depressant (backend support added in v0.10.95 BurdenScores struct).
 */
export const ScoresPanel = memo(function ScoresPanel({ scores }: ScoresPanelProps) {
  return (
    <div className="scores-panel">
      <div className="scores-section">
        <h4 className="scores-section-title">Cumulative Burden Scores</h4>
        <div className="burden-cards">
          <BurdenCard
            label="Anticholinergic"
            value={scores.acbTotal}
            maxValue={10}
            description="ACB Score"
          />
          <BurdenCard
            label="Sedation"
            value={scores.sedationTotal}
            maxValue={10}
            description="Sedation Score"
          />
          <BurdenCard
            label="Constipation"
            value={scores.constipationTotal}
            maxValue={10}
            description="Constipation Risk"
          />
        </div>
      </div>

      <div className="scores-section">
        <h4 className="scores-section-title">Drug Risk Counts</h4>
        <div className="risk-indicators">
          <RiskIndicator label="QT Prolongation" count={scores.qtRiskCount} icon={<HeartIcon />} />
          <RiskIndicator label="Serotonergic" count={scores.serotonergicCount} icon={<BrainIcon />} />
          <RiskIndicator label="Bleeding Risk" count={scores.bleedingRiskCount} icon={<DropletIcon />} />
          <RiskIndicator label="Falls Risk" count={scores.fallsRiskCount} icon={<WalkingIcon />} />
          <RiskIndicator label="Nephrotoxic" count={scores.nephrotoxicCount} icon={<KidneyIcon />} />
          <RiskIndicator label="Hepatotoxic" count={scores.hepatotoxicCount} icon={<LiverIcon />} />
          <RiskIndicator label="Hyperkalemia" count={scores.hyperkalemiaCount} icon={<PotassiumIcon />} />
          <RiskIndicator label="CNS Depressant" count={scores.cnsDepressantCount} icon={<CNSIcon />} />
        </div>
      </div>
    </div>
  );
});

export default ScoresPanel;

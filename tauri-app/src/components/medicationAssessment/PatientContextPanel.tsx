import { memo, useMemo, useState } from 'react';
import type { AnalysisStrategy, PatientConditions } from '../../types';
import { computeAgeFromDob } from '../../utils';

const CONDITIONS: ReadonlyArray<{ key: keyof PatientConditions; label: string }> = [
  { key: 'ckd', label: 'CKD / Renal impairment' },
  { key: 'hepatic', label: 'Hepatic impairment' },
  { key: 'falls_risk', label: 'Falls risk' },
  { key: 'dementia', label: 'Dementia / Cognitive impairment' },
  { key: 'diabetes', label: 'Diabetes' },
  { key: 'afib', label: 'Atrial fibrillation' },
  { key: 'heart_failure', label: 'Heart failure' },
  { key: 'osteoporosis', label: 'Osteoporosis' },
];

const STRATEGIES: ReadonlyArray<{ value: AnalysisStrategy; label: string }> = [
  { value: 'safety_first', label: 'Safety First (prioritize risks)' },
  { value: 'cognition_first', label: 'Cognition First (prioritize ACB)' },
  { value: 'pill_burden_first', label: 'Pill Burden First (prioritize simplification)' },
];

interface PatientContextPanelProps {
  patientAge: number | null;
  /** Optional vision-extracted DOB. When the current age matches what we'd
   *  derive from this DOB, the UI shows a small "derived from DOB" hint to
   *  let the clinician know the field was auto-populated. */
  patientDob?: string | null;
  patientEgfr: number | null;
  patientConditions: PatientConditions;
  strategy: AnalysisStrategy;
  setPatientAge: (age: number | null) => void;
  setPatientEgfr: (egfr: number | null) => void;
  setPatientCondition: (key: keyof PatientConditions, value: boolean) => void;
  setStrategy: (strategy: AnalysisStrategy) => void;
}

export const PatientContextPanel = memo(function PatientContextPanel({
  patientAge,
  patientDob = null,
  patientEgfr,
  patientConditions,
  strategy,
  setPatientAge,
  setPatientEgfr,
  setPatientCondition,
  setStrategy,
}: PatientContextPanelProps) {
  const [isExpanded, setIsExpanded] = useState(false);

  // Only computed for the collapsed-state summary; skip the work when open.
  const contextSummary = useMemo(() => {
    if (isExpanded) return '';
    const parts: string[] = [];
    if (patientAge !== null) parts.push(`Age ${patientAge}`);
    if (patientEgfr !== null) parts.push(`eGFR ${patientEgfr}`);
    for (const c of CONDITIONS) {
      if (patientConditions[c.key]) parts.push(c.label);
    }
    return parts.join(' · ');
  }, [isExpanded, patientAge, patientEgfr, patientConditions]);

  // Show "derived from DOB" hint when the current age field still matches
  // what vision-DOB computes — i.e., it hasn't been clinician-overridden.
  // Recomputed on every render but `computeAgeFromDob` is cheap.
  const ageIsDobDerived =
    patientAge !== null &&
    patientDob !== null &&
    computeAgeFromDob(patientDob) === patientAge;

  return (
    <div className="patient-context-panel">
      <button
        type="button"
        className="pcp-header"
        onClick={() => setIsExpanded((v) => !v)}
        aria-expanded={isExpanded}
      >
        <div className="pcp-header-text">
          <span className="pcp-header-title">Patient Context</span>
          {!isExpanded && contextSummary && (
            <span className="pcp-header-summary">{contextSummary}</span>
          )}
          {!isExpanded && !contextSummary && (
            <span className="pcp-header-summary pcp-header-summary-muted">
              Optional — improves analysis quality
            </span>
          )}
        </div>
        <span className={`chevron ${isExpanded ? '' : 'collapsed'}`}>▾</span>
      </button>

      {isExpanded && (
        <div className="pcp-body">
          <div className="pcp-row">
            <label className="pcp-field">
              <span className="pcp-field-label">
                Age (years)
                {ageIsDobDerived && (
                  <span className="pcp-field-hint"> · derived from DOB</span>
                )}
              </span>
              <input
                type="number"
                className="pcp-input"
                value={patientAge ?? ''}
                onChange={(e) =>
                  setPatientAge(e.target.value === '' ? null : parseInt(e.target.value, 10))
                }
                min={0}
                max={150}
                placeholder="e.g. 78"
              />
            </label>
            <label className="pcp-field">
              <span className="pcp-field-label">eGFR (mL/min)</span>
              <input
                type="number"
                className="pcp-input"
                value={patientEgfr ?? ''}
                onChange={(e) =>
                  setPatientEgfr(e.target.value === '' ? null : parseFloat(e.target.value))
                }
                min={0}
                max={200}
                placeholder="e.g. 45"
              />
            </label>
          </div>

          <div className="pcp-conditions-section">
            <span className="pcp-field-label">Conditions</span>
            <div className="pcp-conditions">
              {CONDITIONS.map(({ key, label }) => (
                <label key={key} className="pcp-condition">
                  <input
                    type="checkbox"
                    checked={patientConditions[key]}
                    onChange={(e) => setPatientCondition(key, e.target.checked)}
                  />
                  <span>{label}</span>
                </label>
              ))}
            </div>
          </div>

          <label className="pcp-field">
            <span className="pcp-field-label">Analysis Strategy</span>
            <select
              className="pcp-input"
              value={strategy}
              onChange={(e) => setStrategy(e.target.value as AnalysisStrategy)}
            >
              {STRATEGIES.map(({ value, label }) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        </div>
      )}
    </div>
  );
});

export default PatientContextPanel;

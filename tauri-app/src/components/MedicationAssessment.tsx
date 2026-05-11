import { memo, useMemo, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { MarkdownContent } from './ClinicalChat';
import type {
  MedEntry,
  AnalysisResult,
  AnalysisCard,
  BurdenScores,
  CardSeverity,
} from '../types';
import type { ExtractionState } from '../hooks/useMedicationAssessment';

interface MedicationAssessmentProps {
  medList: MedEntry[];
  extractionState: ExtractionState;
  extractionError: string | null;
  analysis: AnalysisResult | null;
  isAnalyzing: boolean;
  analyzeError: string | null;
  parseText: string;
  setParseText: (text: string) => void;
  isParsing: boolean;
  parseError: string | null;
  addRow: () => void;
  updateRow: (index: number, patch: Partial<MedEntry>) => void;
  deleteRow: (index: number) => void;
  extract: () => Promise<void>;
  parseTypedMeds: () => Promise<void>;
  analyze: () => Promise<void>;
}

// Single source of truth for severity ranking + CSS class — keeps the
// switch + the sort table from drifting if a new severity lands.
const SEVERITY_META: Record<CardSeverity, { rank: number; cssClass: string }> = {
  critical: { rank: 3, cssClass: 'card-critical' },
  important: { rank: 2, cssClass: 'card-important' },
  convenience: { rank: 1, cssClass: 'card-convenience' },
  info: { rank: 0, cssClass: 'card-info' },
};

function severityMeta(s: string) {
  return SEVERITY_META[s.toLowerCase() as CardSeverity] ?? SEVERITY_META.info;
}

async function openScreenRecordingSettings() {
  try {
    await invoke('open_screen_recording_settings');
  } catch {
    // best effort
  }
}

function ExtractionStatus({
  state,
  count,
  error,
  onReextract,
}: {
  state: ExtractionState;
  count: number;
  error: string | null;
  onReextract: () => void;
}) {
  if (state === 'idle' || state === 'capturing') {
    return (
      <div className="med-status med-status-loading">
        <div className="spinner-small" />
        <span>Extracting medications from screenshot...</span>
      </div>
    );
  }
  if (state === 'extracted') {
    return (
      <div className="med-status med-status-extracted">
        <span>
          {count === 0
            ? 'Vision returned no medications — add them manually below.'
            : `Extracted ${count} medication${count === 1 ? '' : 's'} from the chart screenshot. Edit any rows that look wrong.`}
        </span>
        <button className="med-status-action" onClick={onReextract}>
          Re-extract
        </button>
      </div>
    );
  }
  if (state === 'permission_denied') {
    return (
      <div className="med-status med-status-warning">
        <span>
          Screenshot looked blank — likely a Screen Recording permission issue. Enter medications
          manually, or grant permission and re-extract.
        </span>
        <div className="med-status-actions">
          <button className="med-status-action" onClick={openScreenRecordingSettings}>
            Open settings
          </button>
          <button className="med-status-action" onClick={onReextract}>
            Re-extract
          </button>
        </div>
      </div>
    );
  }
  return (
    <div className="med-status med-status-error">
      <span>Couldn't extract medications: {error ?? 'unknown error'}</span>
      <button className="med-status-action" onClick={onReextract}>
        Try again
      </button>
    </div>
  );
}

const MedRow = memo(function MedRow({
  index,
  med,
  onUpdate,
  onDelete,
}: {
  index: number;
  med: MedEntry;
  onUpdate: (index: number, patch: Partial<MedEntry>) => void;
  onDelete: (index: number) => void;
}) {
  return (
    <div className="med-row">
      <input
        type="text"
        className="med-row-input med-row-name"
        value={med.name}
        onChange={(e) => onUpdate(index, { name: e.target.value })}
        placeholder="Drug name"
        aria-label={`Medication ${index + 1} name`}
      />
      <input
        type="text"
        className="med-row-input med-row-dose"
        value={med.dose ?? ''}
        onChange={(e) => onUpdate(index, { dose: e.target.value })}
        placeholder="Dose"
        aria-label={`Medication ${index + 1} dose`}
      />
      <input
        type="text"
        className="med-row-input med-row-freq"
        value={med.frequency ?? ''}
        onChange={(e) => onUpdate(index, { frequency: e.target.value })}
        placeholder="Frequency"
        aria-label={`Medication ${index + 1} frequency`}
      />
      <button
        className="med-row-delete"
        onClick={() => onDelete(index)}
        aria-label={`Delete medication ${index + 1}`}
        title="Remove from list"
      >
        ✕
      </button>
    </div>
  );
});

function BurdenPanel({ scores }: { scores: BurdenScores }) {
  const tiles = [
    { label: 'ACB', value: scores.acbTotal.toFixed(1), hint: 'Anticholinergic burden' },
    { label: 'Sedation', value: scores.sedationTotal.toFixed(1), hint: 'Sedation burden' },
    { label: 'Constipation', value: scores.constipationTotal.toFixed(1), hint: 'Constipation burden' },
  ];
  const risks: Array<[string, number]> = [
    ['QT', scores.qtRiskCount],
    ['Serotonergic', scores.serotonergicCount],
    ['Bleeding', scores.bleedingRiskCount],
    ['Falls', scores.fallsRiskCount],
    ['Nephrotox', scores.nephrotoxicCount],
    ['Hepatotox', scores.hepatotoxicCount],
    ['Hyper-K', scores.hyperkalemiaCount],
  ];
  const activeRisks = risks.filter(([, n]) => n > 0);

  return (
    <div className="burden-panel">
      <div className="burden-tiles">
        {tiles.map(({ label, value, hint }) => (
          <div key={label} className="burden-tile" title={hint}>
            <span className="burden-label">{label}</span>
            <span className="burden-value">{value}</span>
          </div>
        ))}
      </div>
      {activeRisks.length > 0 && (
        <div className="burden-risks">
          {activeRisks.map(([label, n]) => (
            <span key={label} className="burden-risk-pill">
              {label} ({n})
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function FindingCard({ card }: { card: AnalysisCard }) {
  return (
    <div className={`finding-card ${severityMeta(card.severity).cssClass}`}>
      <div className="finding-card-header">
        <span className="finding-card-severity">{card.severity.toUpperCase()}</span>
        <span className="finding-card-category">{card.category}</span>
      </div>
      <div className="finding-card-title">{card.title}</div>
      {card.medsInvolved.length > 0 && (
        <div className="finding-card-meds">
          {card.medsInvolved.map((m, i) => (
            <span key={i} className="finding-card-med-badge">
              {m}
            </span>
          ))}
        </div>
      )}
      {card.rationale && (
        <div className="finding-card-rationale">
          <MarkdownContent content={card.rationale} />
        </div>
      )}
      {card.action && <div className="finding-card-action">→ {card.action}</div>}
    </div>
  );
}

export const MedicationAssessment = memo(function MedicationAssessment(props: MedicationAssessmentProps) {
  const {
    medList,
    extractionState,
    extractionError,
    analysis,
    isAnalyzing,
    analyzeError,
    parseText,
    setParseText,
    isParsing,
    parseError,
    addRow,
    updateRow,
    deleteRow,
    extract,
    parseTypedMeds,
    analyze,
  } = props;

  const sortedCards = useMemo(() => {
    if (!analysis) return [];
    return [...analysis.cards].sort(
      (a, b) => severityMeta(b.severity).rank - severityMeta(a.severity).rank
    );
  }, [analysis]);

  const canAnalyze = medList.some((m) => m.name.trim().length > 0) && !isAnalyzing;
  const canParse = parseText.trim().length > 0 && !isParsing;

  const handleReextract = useCallback(() => {
    void extract();
  }, [extract]);

  const handleParseTyped = useCallback(() => {
    if (canParse) void parseTypedMeds();
  }, [canParse, parseTypedMeds]);

  const handleAnalyze = useCallback(() => {
    void analyze();
  }, [analyze]);

  // Cmd/Ctrl+Enter parses without leaving the textarea — the clinician
  // can type → submit → see structured rows without reaching for the mouse.
  const handleTextKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleParseTyped();
      }
    },
    [handleParseTyped]
  );

  return (
    <div className="medication-assessment">
      <ExtractionStatus
        state={extractionState}
        count={medList.length}
        error={extractionError}
        onReextract={handleReextract}
      />

      <div className="med-text-parse">
        <label className="med-text-parse-label" htmlFor="med-text-parse-input">
          Type the medication list, additions, or changes — the AI will normalize doses and spelling.
        </label>
        <textarea
          id="med-text-parse-input"
          className="med-text-parse-input"
          value={parseText}
          onChange={(e) => setParseText(e.target.value)}
          onKeyDown={handleTextKeyDown}
          placeholder="e.g. metformin 500 bid, lipitor 40 daily, asprin&#10;or: stop the gabapentin, add: pantoprazole 40 od"
          rows={3}
          disabled={isParsing}
        />
        <div className="med-text-parse-actions">
          <button
            className="med-text-parse-button"
            onClick={handleParseTyped}
            disabled={!canParse}
            title="Parse with AI (⌘↵ / Ctrl+↵)"
          >
            {isParsing ? 'Parsing...' : 'Parse with AI'}
          </button>
          {parseError && <span className="med-text-parse-error">{parseError}</span>}
        </div>
      </div>

      <div className="med-table">
        <div className="med-table-header">
          <span>Name</span>
          <span>Dose</span>
          <span>Frequency</span>
          <span />
        </div>
        {medList.length === 0 ? (
          <div className="med-table-empty">
            No medications yet. Click <strong>Add medication</strong> to start the list.
          </div>
        ) : (
          medList.map((med, i) => (
            <MedRow key={i} index={i} med={med} onUpdate={updateRow} onDelete={deleteRow} />
          ))
        )}
        <button className="med-table-add" onClick={addRow} aria-label="Add a medication">
          + Add medication
        </button>
      </div>

      <div className="med-analyze-bar">
        <button
          className="med-analyze-button"
          onClick={handleAnalyze}
          disabled={!canAnalyze}
          aria-label="Analyze medication list"
        >
          {isAnalyzing ? 'Analyzing...' : 'Analyze'}
        </button>
        {analyzeError && <div className="med-analyze-error">{analyzeError}</div>}
      </div>

      {analysis && (
        <div className="med-analysis-results">
          <BurdenPanel scores={analysis.burdenScores} />
          {sortedCards.length === 0 ? (
            <div className="med-analysis-empty">
              No findings — this regimen looks clean against the current ruleset.
            </div>
          ) : (
            <div className="med-cards">
              {sortedCards.map((c) => (
                <FindingCard key={c.id} card={c} />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
});

export default MedicationAssessment;

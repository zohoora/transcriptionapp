import { memo, useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { MedEntry } from '../types';
import type {
  ExtractionState,
  UseMedicationAssessmentResult,
} from '../hooks/useMedicationAssessment';
import { PatientContextPanel } from './medicationAssessment/PatientContextPanel';
import { AIPlanPanel } from './medicationAssessment/AIPlanPanel';

interface MedicationAssessmentProps {
  med: UseMedicationAssessmentResult;
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

const MedTextParser = memo(function MedTextParser({
  isParsing,
  parseError,
  onParse,
}: {
  isParsing: boolean;
  parseError: string | null;
  onParse: (text: string) => Promise<boolean>;
}) {
  // Local state so keystrokes don't re-render the medication table.
  const [text, setText] = useState('');

  const submit = useCallback(async () => {
    if (!text.trim() || isParsing) return;
    const ok = await onParse(text);
    if (ok) setText('');
  }, [text, isParsing, onParse]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        void submit();
      }
    },
    [submit]
  );

  const canSubmit = text.trim().length > 0 && !isParsing;

  return (
    <div className="med-text-parse">
      <label className="med-text-parse-label" htmlFor="med-text-parse-input">
        Type the medication list, additions, or changes — the AI will normalize doses and spelling.
      </label>
      <textarea
        id="med-text-parse-input"
        className="med-text-parse-input"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="e.g. metformin 500 bid, lipitor 40 daily, asprin&#10;or: stop the gabapentin, add: pantoprazole 40 od"
        rows={3}
        disabled={isParsing}
      />
      <button
        className="med-analyze-button med-text-parse-button"
        onClick={() => void submit()}
        disabled={!canSubmit}
        title="Parse with AI (⌘↵ / Ctrl+↵)"
      >
        {isParsing ? 'Parsing...' : 'Parse with AI'}
      </button>
      {parseError && <div className="med-analyze-error">{parseError}</div>}
    </div>
  );
});

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

export const MedicationAssessment = memo(function MedicationAssessment({ med }: MedicationAssessmentProps) {
  const canAnalyze = med.medList.some((m) => m.name.trim().length > 0) && !med.isAnalyzing;

  return (
    <div className="medication-assessment">
      <ExtractionStatus
        state={med.extractionState}
        count={med.medList.length}
        error={med.extractionError}
        onReextract={() => void med.extract()}
      />

      <MedTextParser
        isParsing={med.isParsing}
        parseError={med.parseError}
        onParse={med.parseTypedMeds}
      />

      <div className="med-table">
        <div className="med-table-header">
          <span>Name</span>
          <span>Dose</span>
          <span>Frequency</span>
          <span />
        </div>
        {med.medList.length === 0 ? (
          <div className="med-table-empty">
            No medications yet. Click <strong>Add medication</strong> to start the list.
          </div>
        ) : (
          med.medList.map((m, i) => (
            <MedRow key={i} index={i} med={m} onUpdate={med.updateRow} onDelete={med.deleteRow} />
          ))
        )}
        <button className="med-table-add" onClick={med.addRow} aria-label="Add a medication">
          + Add medication
        </button>
      </div>

      <PatientContextPanel
        patientAge={med.patientAge}
        patientEgfr={med.patientEgfr}
        patientConditions={med.patientConditions}
        strategy={med.strategy}
        setPatientAge={med.setPatientAge}
        setPatientEgfr={med.setPatientEgfr}
        setPatientCondition={med.setPatientCondition}
        setStrategy={med.setStrategy}
      />

      <div className="med-analyze-bar">
        <button
          className="med-analyze-button"
          onClick={() => void med.analyze()}
          disabled={!canAnalyze}
          aria-label="Analyze medication list"
        >
          {med.isAnalyzing ? 'Analyzing...' : 'Analyze'}
        </button>
        {med.analyzeError && <div className="med-analyze-error">{med.analyzeError}</div>}
      </div>

      {med.analysis && (
        <div className="med-analysis-results">
          <AIPlanPanel
            clarifyingQuestions={med.clarifyingQuestions}
            questionAnswers={med.questionAnswers}
            isLoadingQuestions={med.isLoadingQuestions}
            questionsError={med.questionsError}
            aiPlan={med.aiPlan}
            isGeneratingPlan={med.isGeneratingPlan}
            planError={med.planError}
            startPlanFlow={med.startPlanFlow}
            setQuestionAnswer={med.setQuestionAnswer}
            submitAnswersAndGeneratePlan={med.submitAnswersAndGeneratePlan}
            skipQuestionsAndGeneratePlan={med.skipQuestionsAndGeneratePlan}
            dismissPlan={med.dismissPlan}
          />
        </div>
      )}
    </div>
  );
});

export default MedicationAssessment;

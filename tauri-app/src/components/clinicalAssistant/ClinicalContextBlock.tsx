import { memo } from 'react';
import type { ExtractionState } from '../../hooks/useMedicationAssessment';

interface ClinicalContextBlockProps {
  clinicalContext: string | null;
  setClinicalContext: (text: string | null) => void;
  extractionState: ExtractionState;
}

/**
 * Sidebar block holding the free-form clinical context the vision call
 * extracted from the chart screenshot — lab/imaging report, problem list,
 * allergies, vitals, recent visits, anything else the model deemed
 * clinically relevant. Clinician-editable; the live value is what flows
 * to the chat backend each turn (see `useClinicalChat`).
 */
export const ClinicalContextBlock = memo(function ClinicalContextBlock({
  clinicalContext,
  setClinicalContext,
  extractionState,
}: ClinicalContextBlockProps) {
  const isCapturing = extractionState === 'capturing' || extractionState === 'idle';
  const isExtracted = extractionState === 'extracted';
  const hasText = clinicalContext != null && clinicalContext.trim().length > 0;

  return (
    <section className="ca-sidebar-section">
      <h2>Chart Context</h2>
      {isCapturing && !hasText && (
        <div className="ca-clinical-context-loading">
          Capturing chart context…
        </div>
      )}
      {!isCapturing && isExtracted && !hasText && (
        <div className="ca-clinical-context-empty">
          Nothing additional captured from the chart.
        </div>
      )}
      <textarea
        className="ca-clinical-context-textarea"
        value={clinicalContext ?? ''}
        onChange={(e) => {
          const value = e.target.value;
          setClinicalContext(value.length === 0 ? null : value);
        }}
        placeholder={
          isCapturing
            ? 'Waiting for chart capture…'
            : 'Notes from the chart (editable). The chat sees the current contents.'
        }
        rows={6}
        spellCheck={false}
        aria-label="Chart context (editable)"
      />
    </section>
  );
});

export default ClinicalContextBlock;

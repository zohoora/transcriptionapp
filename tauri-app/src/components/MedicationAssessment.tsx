import { memo } from 'react';
import type { UseMedicationAssessmentResult } from '../hooks/useMedicationAssessment';
import { AIPlanPanel } from './medicationAssessment/AIPlanPanel';

interface MedicationAssessmentProps {
  med: UseMedicationAssessmentResult;
}

/**
 * Right-pane tab. The med list + patient context inputs live in the
 * sidebar; this tab is now the action surface — Analyze button + AI Plan
 * flow.
 */
export const MedicationAssessment = memo(function MedicationAssessment({
  med,
}: MedicationAssessmentProps) {
  const hasMeds = med.medList.some((m) => m.name.trim().length > 0);
  const canAnalyze = hasMeds && !med.isAnalyzing;

  return (
    <div className="medication-assessment">
      {!hasMeds && (
        <div className="med-empty-hint">
          Add medications in the sidebar to run an analysis.
        </div>
      )}

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

import { memo, useMemo } from 'react';
import type { AIPlanResponse, ClarifyingQuestion, PlanStep } from '../../types';

interface AIPlanPanelProps {
  hasCards: boolean;
  clarifyingQuestions: ClarifyingQuestion[];
  questionAnswers: Record<string, string>;
  isLoadingQuestions: boolean;
  questionsError: string | null;
  aiPlan: AIPlanResponse | null;
  isGeneratingPlan: boolean;
  planError: string | null;
  startPlanFlow: () => Promise<void>;
  setQuestionAnswer: (questionId: string, answer: string) => void;
  submitAnswersAndGeneratePlan: () => Promise<void>;
  skipQuestionsAndGeneratePlan: () => Promise<void>;
  dismissPlan: () => void;
}

/** Per-action presentation: CSS class + step-title formatter. Single
 *  source of truth — adding a new action only needs an entry here. */
const ACTION_META: Record<PlanStep['action'], { cssClass: string; format: (s: PlanStep) => string }> = {
  stop: { cssClass: 'plan-step-stop', format: (s) => `STOP ${s.drug}` },
  add: { cssClass: 'plan-step-add', format: (s) => `ADD ${s.drug}` },
  substitute: {
    cssClass: 'plan-step-substitute',
    format: (s) => (s.newDrug ? `SUBSTITUTE ${s.drug} with ${s.newDrug}` : `SUBSTITUTE ${s.drug}`),
  },
  adjust: {
    cssClass: 'plan-step-adjust',
    format: (s) => (s.newDrug ? `ADJUST ${s.drug} to ${s.newDrug}` : `ADJUST ${s.drug}`),
  },
};

const ACTION_FALLBACK = ACTION_META.adjust;

type Stage =
  | 'idle'
  | 'loading_questions'
  | 'questions'
  | 'generating'
  | 'questions_error'
  | 'plan_error'
  | 'plan';

function deriveStage(p: AIPlanPanelProps): Stage {
  if (p.isLoadingQuestions) return 'loading_questions';
  if (p.isGeneratingPlan) return 'generating';
  if (p.aiPlan) return 'plan';
  if (p.questionsError) return 'questions_error';
  if (p.planError) return 'plan_error';
  if (p.clarifyingQuestions.length > 0) return 'questions';
  return 'idle';
}

interface ClarifyingQuestionsBlockProps {
  questions: ClarifyingQuestion[];
  answers: Record<string, string>;
  setAnswer: (id: string, answer: string) => void;
  onSkip: () => void;
  onSubmit: () => void;
}

function ClarifyingQuestionsBlock({
  questions,
  answers,
  setAnswer,
  onSkip,
  onSubmit,
}: ClarifyingQuestionsBlockProps) {
  // ignore stale answers from prior round
  const answeredCount = useMemo(
    () => questions.reduce((n, q) => n + (answers[q.id] ? 1 : 0), 0),
    [answers, questions]
  );
  const allAnswered = answeredCount === questions.length;

  return (
    <div className="plan-questions">
      <div className="plan-questions-header">
        <span className="plan-questions-title">Quick questions to improve your plan</span>
        <span className="plan-questions-subtitle">Optional — skip for general advice</span>
      </div>

      <div className="plan-questions-list">
        {questions.map((q, i) => (
          <div key={q.id} className="plan-question">
            <div className="plan-question-prompt">
              <span className="plan-question-number">{i + 1}.</span>
              <span>{q.question}</span>
            </div>
            {q.context && <p className="plan-question-context">{q.context}</p>}
            <div className="plan-question-options">
              {q.options.map((option) => {
                const selected = answers[q.id] === option;
                return (
                  <button
                    type="button"
                    key={option}
                    onClick={() => setAnswer(q.id, option)}
                    className={`plan-question-option ${selected ? 'plan-question-option-selected' : ''}`}
                  >
                    {option}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </div>

      <div className="plan-questions-actions">
        <button type="button" className="med-analyze-button plan-questions-secondary" onClick={onSkip}>
          Skip — Generate Plan
        </button>
        <button
          type="button"
          className="med-analyze-button"
          onClick={onSubmit}
          disabled={answeredCount === 0}
          title={allAnswered ? 'Generate plan with your answers' : 'Answer at least one question to continue'}
        >
          {allAnswered ? 'Continue' : `Continue (${answeredCount}/${questions.length})`}
        </button>
      </div>
    </div>
  );
}

export const AIPlanPanel = memo(function AIPlanPanel(props: AIPlanPanelProps) {
  const stage = useMemo(() => deriveStage(props), [props]);

  return (
    <div className="ai-plan-panel">
      <div className="ai-plan-header">
        <span className="ai-plan-title">Refactoring Suggestion</span>
        {props.aiPlan && (
          <button type="button" className="ai-plan-dismiss" onClick={props.dismissPlan}>
            Dismiss
          </button>
        )}
      </div>

      {renderStage(stage, props)}
    </div>
  );
});

function renderStage(stage: Stage, p: AIPlanPanelProps) {
  switch (stage) {
    case 'idle':
      return (
        <div className="ai-plan-idle">
          <p className="ai-plan-description">
            Generate an AI-powered step-by-step plan to optimize this medication regimen.
          </p>
          <button
            type="button"
            className="med-analyze-button"
            onClick={() => void p.startPlanFlow()}
            disabled={!p.hasCards}
          >
            Generate Plan
          </button>
          {!p.hasCards && (
            <p className="ai-plan-hint">Run an analysis first to enable plan generation.</p>
          )}
        </div>
      );

    case 'loading_questions':
      return (
        <div className="ai-plan-loading">
          <div className="spinner-small" />
          <span>Preparing plan...</span>
        </div>
      );

    case 'questions':
      return (
        <ClarifyingQuestionsBlock
          questions={p.clarifyingQuestions}
          answers={p.questionAnswers}
          setAnswer={p.setQuestionAnswer}
          onSkip={() => void p.skipQuestionsAndGeneratePlan()}
          onSubmit={() => void p.submitAnswersAndGeneratePlan()}
        />
      );

    case 'generating':
      return (
        <div className="ai-plan-loading">
          <div className="spinner-small" />
          <span>Generating refactoring plan...</span>
        </div>
      );

    case 'questions_error':
      return (
        <div className="med-analyze-error ai-plan-error">
          {p.questionsError}
          <button type="button" className="ai-plan-error-action" onClick={() => void p.startPlanFlow()}>
            Try Again
          </button>
        </div>
      );

    case 'plan_error':
      return (
        <div className="med-analyze-error ai-plan-error">
          {p.planError}
          <button type="button" className="ai-plan-error-action" onClick={() => void p.startPlanFlow()}>
            Try Again
          </button>
        </div>
      );

    case 'plan':
      return p.aiPlan ? <PlanResult plan={p.aiPlan} onRegenerate={() => void p.startPlanFlow()} /> : null;
  }
}

function PlanResult({ plan, onRegenerate }: { plan: AIPlanResponse; onRegenerate: () => void }) {
  return (
    <div className="ai-plan-result">
      {plan.summary && (
        <div className="plan-summary">
          <p>{plan.summary}</p>
        </div>
      )}

      {plan.steps && plan.steps.length > 0 && (
        <ol className="plan-steps">
          {plan.steps.map((step) => {
            const meta = ACTION_META[step.action] ?? ACTION_FALLBACK;
            return (
              <li key={step.stepNumber} className={`plan-step ${meta.cssClass}`}>
                <div className="plan-step-title">
                  Step {step.stepNumber}: {meta.format(step)}
                </div>
                {step.reason && <p className="plan-step-reason">{step.reason}</p>}
              </li>
            );
          })}
        </ol>
      )}

      {plan.finalMeds && plan.finalMeds.length > 0 && (
        <div className="plan-final">
          <span className="plan-final-label">Final Optimized Regimen</span>
          <p className="plan-final-list">{plan.finalMeds.join(', ')}</p>
        </div>
      )}

      <button type="button" className="med-analyze-button plan-regenerate" onClick={onRegenerate}>
        Regenerate Plan
      </button>
    </div>
  );
}

export default AIPlanPanel;

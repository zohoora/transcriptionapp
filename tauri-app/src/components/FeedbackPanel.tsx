import React, { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type {
  SessionFeedback,
  DetectionFeedbackCategory,
  ContentIssueType,
} from '../types';
import {
  DETECTION_FEEDBACK_LABELS,
  CONTENT_ISSUE_LABELS,
} from '../types';
import { createEmptyFeedback, cycleTriState } from '../utils';

interface FeedbackPanelProps {
  sessionId: string;
  date: string;
  feedback: SessionFeedback | null;
  onFeedbackChange: (fb: SessionFeedback) => void;
  /** Called after a write to feedback.json lands, so the parent can refresh
   *  the sidebar row's has_feedback / quality_rating without a full refetch. */
  onFeedbackSaved?: (fb: SessionFeedback) => void;
  isMultiPatient: boolean;
  activePatient: number;
  patientCount: number;
  isContinuousMode: boolean;
}

const DETECTION_CATEGORIES: DetectionFeedbackCategory[] = [
  'inappropriately_merged',
  'fragment',
  'wrong_nonclinical',
  'wrong_clinical',
  'other',
];

const CONTENT_ISSUES: ContentIssueType[] = [
  'missed_details',
  'inaccurate',
  'wrong_attribution',
  'hallucinated',
];

type AccuracyField =
  | 'splitCorrect'
  | 'mergeCorrect'
  | 'clinicalCorrect'
  | 'patientCountCorrect'
  | 'billingCorrect';

const ACCURACY_LABELS: Record<AccuracyField, string> = {
  splitCorrect: 'Session boundary (end) is correct',
  mergeCorrect: 'Merge with prior session was correct',
  clinicalCorrect: 'Clinical vs. non-clinical is correct',
  patientCountCorrect: 'Patient count / attribution is correct',
  billingCorrect: 'Billing codes are correct (locks in as ground truth)',
};

const FeedbackPanel: React.FC<FeedbackPanelProps> = ({
  sessionId,
  date,
  feedback,
  onFeedbackChange,
  onFeedbackSaved,
  isMultiPatient,
  activePatient,
  patientCount,
  isContinuousMode,
}) => {
  const [expanded, setExpanded] = useState(false);
  const [saveStatus, setSaveStatus] = useState<'idle' | 'saving' | 'saved'>('idle');
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const statusResetRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const feedbackRef = useRef(feedback);
  feedbackRef.current = feedback;
  // Flushed on cleanup so a rate-then-navigate-fast doesn't drop the debounced write.
  const pendingSaveRef = useRef<SessionFeedback | null>(null);
  const onFeedbackSavedRef = useRef(onFeedbackSaved);
  onFeedbackSavedRef.current = onFeedbackSaved;

  const saveFeedback = useCallback((fb: SessionFeedback) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    pendingSaveRef.current = fb;
    debounceRef.current = setTimeout(async () => {
      pendingSaveRef.current = null;
      setSaveStatus('saving');
      try {
        await invoke('save_session_feedback', {
          sessionId,
          date,
          feedback: fb,
        });
        onFeedbackSavedRef.current?.(fb);
        setSaveStatus('saved');
        if (statusResetRef.current) clearTimeout(statusResetRef.current);
        statusResetRef.current = setTimeout(() => setSaveStatus('idle'), 2000);
      } catch (e) {
        console.error('Failed to save feedback:', e);
        setSaveStatus('idle');
      }
    }, 500);
  }, [sessionId, date]);

  // On session change: flush any pending save so rate-then-navigate-fast sticks.
  useEffect(() => {
    const flushSessionId = sessionId;
    const flushDate = date;
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      if (statusResetRef.current) clearTimeout(statusResetRef.current);
      const pending = pendingSaveRef.current;
      if (!pending) return;
      pendingSaveRef.current = null;
      invoke('save_session_feedback', {
        sessionId: flushSessionId,
        date: flushDate,
        feedback: pending,
      })
        .then(() => onFeedbackSavedRef.current?.(pending))
        .catch(e => console.error('Failed to flush feedback on cleanup:', e));
    };
  }, [sessionId, date]);

  const updateFeedback = useCallback((updater: (prev: SessionFeedback) => SessionFeedback) => {
    const current = feedbackRef.current ?? createEmptyFeedback();
    const updated = updater({ ...current, updatedAt: new Date().toISOString() });
    onFeedbackChange(updated);
    saveFeedback(updated);
  }, [onFeedbackChange, saveFeedback]);

  const handleRating = (rating: 'good' | 'bad') => {
    updateFeedback(prev => ({
      ...prev,
      qualityRating: prev.qualityRating === rating ? null : rating,
    }));
    // Auto-expand on thumbs down
    if (rating === 'bad' && (!feedback || feedback.qualityRating !== 'bad')) {
      setExpanded(true);
    }
  };

  const handleDetectionCategory = (category: DetectionFeedbackCategory) => {
    updateFeedback(prev => ({
      ...prev,
      detectionFeedback: prev.detectionFeedback?.category === category
        ? null
        : { category, details: prev.detectionFeedback?.details ?? null },
    }));
  };

  const handleDetectionDetails = (details: string) => {
    updateFeedback(prev => ({
      ...prev,
      detectionFeedback: prev.detectionFeedback
        ? { ...prev.detectionFeedback, details: details || null }
        : null,
    }));
  };

  const handleContentIssue = (issue: ContentIssueType) => {
    updateFeedback(prev => {
      const existing = prev.patientFeedback.find(p => p.patientIndex === activePatient);
      const currentIssues = existing?.issues ?? [];
      const newIssues = currentIssues.includes(issue)
        ? currentIssues.filter(i => i !== issue)
        : [...currentIssues, issue];

      const otherPatients = prev.patientFeedback.filter(p => p.patientIndex !== activePatient);
      if (newIssues.length === 0 && !existing?.details) {
        return { ...prev, patientFeedback: otherPatients };
      }
      return {
        ...prev,
        patientFeedback: [
          ...otherPatients,
          {
            patientIndex: activePatient,
            issues: newIssues,
            details: existing?.details ?? null,
          },
        ],
      };
    });
  };

  const handleContentDetails = (details: string) => {
    updateFeedback(prev => {
      const existing = prev.patientFeedback.find(p => p.patientIndex === activePatient);
      const currentIssues = existing?.issues ?? [];
      const otherPatients = prev.patientFeedback.filter(p => p.patientIndex !== activePatient);
      if (currentIssues.length === 0 && !details) {
        return { ...prev, patientFeedback: otherPatients };
      }
      return {
        ...prev,
        patientFeedback: [
          ...otherPatients,
          { patientIndex: activePatient, issues: currentIssues, details: details || null },
        ],
      };
    });
  };

  const handleComments = (comments: string) => {
    updateFeedback(prev => ({ ...prev, comments: comments || null }));
  };

  const cycleAccuracy = (field: AccuracyField) => {
    updateFeedback(prev => ({
      ...prev,
      [field]: cycleTriState((prev[field] ?? null) as boolean | null),
    }));
  };

  const currentPatientFeedback = feedback?.patientFeedback.find(
    p => p.patientIndex === activePatient
  );

  return (
    <div className="feedback-panel">
      <div className="feedback-quick-row">
        <span className="feedback-label">Rate this note</span>
        <button
          className={`feedback-btn ${feedback?.qualityRating === 'good' ? 'active good' : ''}`}
          onClick={() => handleRating('good')}
          title="Good note"
        >
          <span aria-hidden="true">{'\u{1F44D}'}</span>
        </button>
        <button
          className={`feedback-btn ${feedback?.qualityRating === 'bad' ? 'active bad' : ''}`}
          onClick={() => handleRating('bad')}
          title="Needs improvement"
        >
          <span aria-hidden="true">{'\u{1F44E}'}</span>
        </button>
        {saveStatus === 'saving' && <span className="feedback-status">Saving...</span>}
        {saveStatus === 'saved' && <span className="feedback-status saved">Saved</span>}
        <button
          className="feedback-expand-link"
          onClick={() => setExpanded(prev => !prev)}
        >
          {expanded ? 'Hide details' : 'Add details'}
        </button>
      </div>

      {expanded && (
        <div className="feedback-details">
          {/* Accuracy — structured tri-state flags for regression corpus.
              Detection fields only apply in continuous mode; billing applies to all sessions. */}
          <div className="feedback-section">
            <div className="feedback-section-title">Accuracy</div>
            <div className="feedback-accuracy-group">
              {(Object.keys(ACCURACY_LABELS) as AccuracyField[])
                .filter(field => field === 'billingCorrect' || isContinuousMode)
                .map(field => {
                  const val = (feedback?.[field] ?? null) as boolean | null;
                  const icon = val === true ? '✓' : val === false ? '✗' : '○';
                  const cls = val === true ? 'correct' : val === false ? 'wrong' : 'unset';
                  return (
                    <button
                      key={field}
                      className={`feedback-accuracy-btn ${cls}`}
                      onClick={() => cycleAccuracy(field)}
                      title="Click to cycle: unset → correct → wrong → unset"
                    >
                      <span className="feedback-accuracy-icon" aria-hidden="true">{icon}</span>
                      <span className="feedback-accuracy-label">{ACCURACY_LABELS[field]}</span>
                    </button>
                  );
                })}
            </div>
          </div>

          {/* Detection feedback — continuous mode only */}
          {isContinuousMode && (
            <div className="feedback-section">
              <div className="feedback-section-title">Detection Quality</div>
              <div className="feedback-radio-group">
                {DETECTION_CATEGORIES.map(cat => (
                  <label key={cat} className="feedback-radio-label">
                    <input
                      type="radio"
                      name="detection-category"
                      checked={feedback?.detectionFeedback?.category === cat}
                      onChange={() => handleDetectionCategory(cat)}
                    />
                    {DETECTION_FEEDBACK_LABELS[cat]}
                  </label>
                ))}
              </div>
              {feedback?.detectionFeedback && (
                <textarea
                  className="feedback-textarea"
                  placeholder="Detection details (optional)..."
                  value={feedback.detectionFeedback.details ?? ''}
                  onChange={e => handleDetectionDetails(e.target.value)}
                  rows={2}
                />
              )}
            </div>
          )}

          {/* Content feedback */}
          <div className="feedback-section">
            <div className="feedback-section-title">
              Content Issues
              {isMultiPatient && ` (Patient ${activePatient + 1} of ${patientCount})`}
            </div>
            <div className="feedback-checkbox-group">
              {CONTENT_ISSUES.map(issue => (
                <label key={issue} className="feedback-checkbox-label">
                  <input
                    type="checkbox"
                    checked={currentPatientFeedback?.issues.includes(issue) ?? false}
                    onChange={() => handleContentIssue(issue)}
                  />
                  {CONTENT_ISSUE_LABELS[issue]}
                </label>
              ))}
            </div>
            <textarea
              className="feedback-textarea"
              placeholder="What was wrong or missing? (optional)..."
              value={currentPatientFeedback?.details ?? ''}
              onChange={e => handleContentDetails(e.target.value)}
              rows={2}
            />
          </div>

          {/* General comments */}
          <div className="feedback-section">
            <div className="feedback-section-title">Comments</div>
            <textarea
              className="feedback-textarea"
              placeholder="Any other feedback..."
              value={feedback?.comments ?? ''}
              onChange={e => handleComments(e.target.value)}
              rows={2}
            />
          </div>
        </div>
      )}
    </div>
  );
};

export default FeedbackPanel;

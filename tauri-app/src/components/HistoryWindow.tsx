import React, { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import { useAuth } from './AuthProvider';
import { useSoapNote } from '../hooks/useSoapNote';
import { useOllamaConnection } from '../hooks/useOllamaConnection';
import Calendar from './Calendar';
import AudioPlayer from './AudioPlayer';
import {
  HistoryActionBar,
  DeleteConfirmDialog,
  EditNameDialog,
  MergeConfirmDialog,
} from './cleanup';
import ConfirmPatientsBatchDialog from './ConfirmPatientsBatchDialog';
import FeedbackPanel from './FeedbackPanel';
import { BillingTab, DailySummaryView, MonthlySummaryView } from './billing';
import { clamp, formatDateForApi, formatLocalTime, formatLocalDateTime, formatDurationShort } from '../utils';
import type {
  LocalArchiveSummary,
  LocalArchiveDetails,
  LocalArchiveMetadata,
  ArchivedPatientNote,
  MultiPatientSoapResult,
  PatientSoapNote,
  SoapNote,
  EncounterSummary,
  EncounterDetails,
  SoapOptions,
  SessionFeedback,
  BillingRecord,
  Settings,
} from '../types';
import { DETAIL_LEVEL_LABELS } from '../types';
import { updateSessionFeedback } from '../utils';

type DetailTab = 'transcript' | 'soap' | 'handout' | 'billing' | 'insights';
type RightPaneMode = 'session' | 'daily_billing' | 'monthly_billing';
type DataSource = 'local' | 'medplum';
type ActiveDialog = 'none' | 'delete' | 'merge' | 'editName' | 'confirmPatient';
type SortField = 'time' | 'encounter' | 'patient' | 'words' | 'duration';
type SortDir = 'asc' | 'desc';
type FilterMode = 'all' | 'clinical' | 'non-clinical' | 'soap' | 'no-soap';

/** A sidebar row — either a normal session, one patient from a legacy multi-patient
 *  session (cosmetic fan-out on `patient_labels`), or one sibling from an auto-split
 *  multi-patient encounter (its own session_id, identified by `isSibling`). */
interface FlattenedSession extends LocalArchiveSummary {
  patientIndex: number | null;
  flattenedPatientName: string | null;
  isGroupFirst: boolean;
  isGroupLast: boolean;
  /** True when this row is an auto-split sibling (each sibling is its own session,
   *  unlike the legacy `patient_labels` fan-out which shared one session_id). */
  isSibling: boolean;
}

/** Build a unique key for a flattened sidebar entry */
function entryKey(entry: FlattenedSession): string {
  return entry.patientIndex !== null
    ? `${entry.session_id}:p:${entry.patientIndex}`
    : entry.session_id;
}

/** Extract the session ID from an entry key */
function sessionIdFromKey(key: string): string {
  const idx = key.indexOf(':p:');
  return idx >= 0 ? key.substring(0, idx) : key;
}

/** Extract the patient index from an entry key, or null for single-patient entries */
function patientIndexFromKey(key: string): number | null {
  const idx = key.indexOf(':p:');
  return idx >= 0 ? parseInt(key.substring(idx + 3), 10) : null;
}

const SESSION_FEEDBACK_PROMPT =
  "Review this single patient encounter transcript from a family physician in Ontario, Canada.\n\n" +
  "Output ONLY concise bullet points. No prose, no greetings, no sign-offs.\n\n" +
  "Format:\n" +
  "DONE WELL\n" +
  "- [specific positive observation]\n\n" +
  "CONSIDER\n" +
  "- [specific actionable suggestion]\n\n" +
  "KEY EVIDENCE\n" +
  "- <Lead author> <year> — <trial name or short description>: <one-sentence finding>. Why it matters here: <one sentence tying it to a specific decision in THIS encounter>. https://pubmed.ncbi.nlm.nih.gov/?term=<author+year+disambiguators>\n\n" +
  "Rules:\n" +
  "- 1-3 bullets per DONE WELL / CONSIDER section, 1-2 sentences each\n" +
  "- Be direct and specific. No filler or qualifiers.\n" +
  "- Do NOT use markdown headers (no #, ##). Plain text labels + bullets only.\n\n" +
  "KEY EVIDENCE rules (read very carefully — this section's bar is high):\n" +
  "- Output 0-3 references. ZERO is acceptable and PREFERRED over weak citations. If no landmark evidence in THIS encounter would have changed practice, OMIT the KEY EVIDENCE section entirely (no header, no bullets).\n" +
  "- ONLY cite a SPECIFIC named landmark study you can identify by lead author + year + (trial codename OR distinctive description). Examples of acceptable citations: SPRINT (Wright 2015), COMPASS (Eikelboom 2017), EMPA-REG OUTCOME (Zinman 2015), STOPP/START criteria (O'Mahony 2015), Choosing Wisely lists.\n" +
  "- Every citation must satisfy ALL three: (a) you can name lead author and approximate year, (b) the finding would have INFORMED OR CHANGED the management decision in this encounter (not merely affirm what the clinician already did), (c) the finding is the kind of thing a busy family physician might not have known or might have forgotten.\n" +
  "- Each line format exactly: <Author> <year> — <trial/study name or brief description>: <finding>. Why it matters here: <link to specific encounter content>. <PubMed URL>\n" +
  "- URL search-term strategy: include author surname + year + 2-3 unique disambiguators (trial codename when known, drug name, specific condition phrasing, comparator) so the target study is PubMed result #1. Example terms: 'Eikelboom+2017+COMPASS+rivaroxaban+aspirin' or 'Wright+2015+SPRINT+intensive+blood+pressure'. URL form exactly: https://pubmed.ncbi.nlm.nih.gov/?term=TERM1+TERM2+TERM3 (5-8 terms, no quotes, no field tags like [au], spaces as '+').\n" +
  "- If you cannot name a specific landmark study for a relevant decision, DO NOT substitute a generic topic search — omit that reference.\n" +
  "- NEVER fabricate a study name, author, trial codename, or year. If uncertain, omit. A missing reference is good; a fabricated one is harmful.";

const DAY_FEEDBACK_PROMPT =
  "/nothink\n" +
  "Review a family physician's clinic day in Ontario, Canada. " +
  "All patient encounter transcripts are below, separated by '--- Next Session ---'.\n\n" +
  "Output concise bullet points ONLY.\n\n" +
  "STRENGTHS\n" +
  "- [observation referencing specific patient/encounter]\n\n" +
  "AREAS FOR IMPROVEMENT\n" +
  "- [actionable suggestion referencing specific patient/encounter]\n\n" +
  "PATTERNS\n" +
  "- [recurring theme across sessions]\n\n" +
  "KEY EVIDENCE\n" +
  "- <Lead author> <year> — <trial name or short description>: <one-sentence finding>. Why it matters here: <one sentence tying it to a specific case or recurring pattern from TODAY>. https://pubmed.ncbi.nlm.nih.gov/?term=<author+year+disambiguators>\n\n" +
  "Constraints:\n" +
  "- 1-2 sentences per bullet\n" +
  "- Reference specific encounters in STRENGTHS / AREAS FOR IMPROVEMENT / PATTERNS\n" +
  "- Focus on clinical reasoning, communication, management\n" +
  "- Be direct. No hedging, no qualifiers, no filler\n" +
  "- NO letters, greetings, signatures, headers, or prose\n" +
  "- NO markdown (no #, **, etc). Plain text only\n" +
  "- 5-8 bullets per STRENGTHS / AREAS FOR IMPROVEMENT / PATTERNS section\n\n" +
  "KEY EVIDENCE rules (read very carefully — this section's bar is high):\n" +
  "- Output 0-3 references. ZERO is acceptable and PREFERRED over weak citations. If no landmark evidence in TODAY's clinical mix would have changed practice, OMIT the KEY EVIDENCE section entirely (no header, no bullets).\n" +
  "- ONLY cite a SPECIFIC named landmark study you can identify by lead author + year + (trial codename OR distinctive description). Examples of acceptable citations: SPRINT (Wright 2015), COMPASS (Eikelboom 2017), EMPA-REG OUTCOME (Zinman 2015), STOPP/START criteria (O'Mahony 2015), Choosing Wisely lists.\n" +
  "- Every citation must satisfy ALL three: (a) you can name lead author and approximate year, (b) the finding would have INFORMED OR CHANGED a management decision in a specific case from today (not merely affirm what was already done), (c) the finding is the kind of thing a busy family physician might not have known or might have forgotten.\n" +
  "- Each line format exactly: <Author> <year> — <trial/study name or brief description>: <finding>. Why it matters here: <link to specific case from today>. <PubMed URL>\n" +
  "- URL search-term strategy: include author surname + year + 2-3 unique disambiguators (trial codename when known, drug name, specific condition phrasing, comparator) so the target study is PubMed result #1. Example terms: 'Eikelboom+2017+COMPASS+rivaroxaban+aspirin' or 'Wright+2015+SPRINT+intensive+blood+pressure'. URL form exactly: https://pubmed.ncbi.nlm.nih.gov/?term=TERM1+TERM2+TERM3 (5-8 terms, no quotes, no field tags like [au], spaces as '+').\n" +
  "- If you cannot name a specific landmark study for a relevant decision, DO NOT substitute a generic topic search — omit that reference.\n" +
  "- NEVER fabricate a study name, author, trial codename, or year. If uncertain, omit. A missing reference is good; a fabricated one is harmful.";

// LLM output is rendered as text + React anchor nodes — never as HTML — so any
// fabricated tags appear as literal characters. Anchors open via plugin-shell
// to keep PubMed out of the in-app Tauri webview.
function renderFeedbackWithLinks(text: string): React.ReactNode[] {
  const urlPattern = /https?:\/\/[^\s<>"')\]]+/g;
  const out: React.ReactNode[] = [];
  let cursor = 0;
  for (const match of text.matchAll(urlPattern)) {
    const start = match.index ?? 0;
    const trailing = match[0].match(/[.,;:!?]+$/)?.[0] ?? '';
    const url = match[0].slice(0, match[0].length - trailing.length);
    if (start > cursor) {
      out.push(text.slice(cursor, start));
    }
    out.push(
      <a
        key={`fb-url-${start}`}
        href={url}
        className="clinical-feedback-link"
        rel="noopener noreferrer"
        onClick={(e) => {
          e.preventDefault();
          openExternal(url).catch(() => {});
        }}
      >
        {url}
      </a>,
    );
    cursor = start + url.length;
  }
  if (cursor < text.length) {
    out.push(text.slice(cursor));
  }
  return out;
}

interface FeedbackSectionProps {
  title: string;
  systemPrompt: string;
  cacheKey: string;
  llmConnected: boolean;
  disabled?: boolean;
  loadingText?: string;
  badge?: React.ReactNode;
  extraClass?: string;
  getTranscript: () => Promise<string>;
}

/** Collapsible LLM-generated clinical feedback section (unified for session and day) */
const FeedbackSection: React.FC<FeedbackSectionProps> = ({
  title,
  systemPrompt,
  cacheKey,
  llmConnected,
  disabled = false,
  loadingText = 'Generating feedback...',
  badge,
  extraClass,
  getTranscript,
}) => {
  const [expanded, setExpanded] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const cachedKeyRef = useRef<string | null>(null);
  const inFlightRef = useRef(false);

  useEffect(() => {
    if (cachedKeyRef.current !== cacheKey) {
      setFeedback(null);
      setError(null);
      setExpanded(false);
      cachedKeyRef.current = cacheKey;
    }
  }, [cacheKey]);

  const fetchFeedback = async () => {
    if (inFlightRef.current) return;
    inFlightRef.current = true;
    setLoading(true);
    setError(null);
    try {
      const transcript = await getTranscript();
      const result = await invoke<string>('generate_clinical_feedback', {
        systemPrompt,
        transcript,
      });
      setFeedback(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
      inFlightRef.current = false;
    }
  };

  const handleToggle = async () => {
    const next = !expanded;
    setExpanded(next);
    if (next && feedback === null && !loading) {
      await fetchFeedback();
    }
  };

  const handleRetry = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setError(null);
    await fetchFeedback();
  };

  return (
    <div className={`clinical-feedback-section${extraClass ? ` ${extraClass}` : ''}`}>
      <button
        className="clinical-feedback-header"
        onClick={handleToggle}
        disabled={!llmConnected || disabled}
      >
        <span className={`chevron-small ${expanded ? '' : 'collapsed'}`}>&#x25BE;</span>
        <span className="clinical-feedback-title">{title}</span>
        {badge}
        {!llmConnected && <span className="clinical-feedback-hint">LLM offline</span>}
      </button>
      {expanded && (
        <div className="clinical-feedback-body">
          {loading && (
            <div className="clinical-feedback-loading">{loadingText}</div>
          )}
          {error && (
            <div className="clinical-feedback-error">
              <span>{error}</span>
              <button className="clinical-feedback-retry" onClick={handleRetry}>Retry</button>
            </div>
          )}
          {feedback && (
            <p className="clinical-feedback-text">{renderFeedbackWithLinks(feedback)}</p>
          )}
        </div>
      )}
    </div>
  );
};

function formatDateForDisplay(date: Date): string {
  return date.toLocaleDateString('en-US', {
    weekday: 'long',
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });
}

/**
 * Recover per-patient SOAP notes from the combined `soap_note.txt` when the
 * per-patient `soap_patient_N.txt` files weren't synced from the originating
 * room (cross-room multi-patient bug — see ALLOWED_SESSION_FILES in
 * profile-service/src/store/sessions.rs).
 *
 * The combined SOAP is produced by Rust's `format_patient_notes_for_archive`
 * (see tauri-app/src-tauri/src/llm_client.rs) which writes each patient as:
 *
 *   === <label> ===
 *   <content>
 *
 * joined by `\n\n---\n\n`.
 *
 * Inputs:
 *   - `combined`: the full soap_note string (non-empty, has `=== ... ===` headers)
 *   - `labels`: the patientNotes entries (used for index/label to preserve UX
 *     identical to the healthy multi-patient render path)
 *
 * Returns one entry per element in `labels`, in input order. If the combined
 * SOAP can't be parsed into matching sections, the caller should fall back to
 * a single-note view — return `labels.map(... content: '')` is acceptable for
 * the caller to detect.
 */
function recoverPerPatientNotesFromCombined(
  combined: string,
  labels: ArchivedPatientNote[],
): { speaker_id: string; patient_label: string; content: string }[] {
  // Split on `=== <label> ===` headers. The captured group keeps labels in
  // alternation with content: ["", label0, content0, label1, content1, ...].
  const parts = combined.split(/^=== (.+?) ===\s*$/m);
  const sections: { label: string; content: string }[] = [];
  for (let i = 1; i < parts.length; i += 2) {
    const sectionContent = (parts[i + 1] ?? '')
      .replace(/\n\s*---\s*\n/g, '\n')
      .trim();
    sections.push({ label: parts[i].trim(), content: sectionContent });
  }
  return labels.map((pn, idx) => {
    const byLabel = sections.find(s => s.label === pn.label);
    const section = byLabel ?? sections[idx];
    return {
      speaker_id: `Patient ${pn.index}`,
      patient_label: pn.label,
      content: section?.content ?? '',
    };
  });
}

/** Serialize multi-patient SOAP notes into a single string for archive storage */
function serializeSoapNotes(notes: { patient_label: string; content: string }[]): string {
  return notes.map(n =>
    notes.length > 1
      ? `=== ${n.patient_label} ===\n\n${n.content}`
      : n.content
  ).join('\n\n---\n\n');
}

/**
 * Persist a regenerated SOAP to the local archive. Multi-patient results
 * MUST go through the dedicated command so `patient_labels.json` is
 * written — that file drives the per-sub-patient History row fan-out;
 * `save_local_soap_note` alone collapses the result back to a single row.
 *
 * For single-patient results, the regen-extracted patient identity from
 * `notes[0]` is forwarded to the backend so `metadata.json` gets the
 * vision-extracted name/DOB. Without this thread-through, a regen of a
 * malformed-SOAP session (the Amy Maddock 2026-05-14 case) recovers the
 * SOAP body but loses the identity the LLM returned.
 */
async function persistSoapResultToArchive(args: {
  sessionId: string;
  date: string;
  notes: PatientSoapNote[];
  detailLevel?: number;
  format?: string;
}): Promise<void> {
  const { sessionId, date, notes, detailLevel, format } = args;
  if (notes.length > 1) {
    await invoke('save_local_multi_patient_soap_note', {
      sessionId,
      date,
      notes,
      detailLevel,
      format,
    });
  } else {
    const single = notes[0];
    await invoke('save_local_soap_note', {
      sessionId,
      date,
      soapContent: serializeSoapNotes(notes),
      detailLevel,
      format,
      patientName: single?.extracted_patient_name ?? null,
      patientDob: single?.extracted_patient_dob ?? null,
    });
  }
}

const HistoryWindow: React.FC = () => {
  const { authState, isLoading: authLoading, login } = useAuth();

  // Use shared SOAP hook - handles LLM status, options persistence, and generation
  const {
    isGeneratingSoap,
    soapError,
    setSoapError,
    ollamaStatus,
    setOllamaStatus,
    soapOptions,
    setSoapOptions,
    updateSoapDetailLevel,
    updateSoapFormat,
    updateSessionCustomInstructions,
    generateSoapNote,
  } = useSoapNote();

  const [selectedDate, setSelectedDate] = useState(new Date());
  const [sessions, setSessions] = useState<LocalArchiveSummary[]>([]);
  const [selectedSession, setSelectedSession] = useState<LocalArchiveDetails | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [selectedPatientIndex, setSelectedPatientIndex] = useState<number | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [datesWithSessions, setDatesWithSessions] = useState<Set<string>>(new Set());
  const [migratingMultiPatient, setMigratingMultiPatient] = useState(false);

  // Sort and filter
  const [sortField, setSortField] = useState<SortField>('time');
  const [sortDir, setSortDir] = useState<SortDir>('asc');
  const [filterMode, setFilterMode] = useState<FilterMode>('all');

  // Resizable left pane. Drag drives the DOM directly via leftPaneRef and
  // commits to state only on mouseup, so a 60Hz drag doesn't trigger 60
  // re-renders of this 1500-line component (or 60 localStorage writes).
  // State (and the inline style fallback) take over after mouseup so a
  // subsequent re-render keeps the chosen width.
  const LEFT_PANE_MIN = 240;
  const LEFT_PANE_MAX = 640;
  const LEFT_PANE_DEFAULT = 320;
  const LEFT_PANE_KEY_STEP = 8;
  const LEFT_PANE_KEY_BIG_STEP = 32;
  const [leftPaneWidth, setLeftPaneWidth] = useState<number>(() => {
    try {
      const raw = window.localStorage?.getItem?.('historyLeftPaneWidth');
      const saved = Number(raw);
      if (Number.isFinite(saved) && saved >= LEFT_PANE_MIN && saved <= LEFT_PANE_MAX) {
        return saved;
      }
    } catch {
      // jsdom or sandboxed environments may not expose localStorage; fall through.
    }
    return LEFT_PANE_DEFAULT;
  });
  const leftPaneRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<{
    onMove: (ev: MouseEvent) => void;
    onUp: () => void;
    pendingWidth: number;
  } | null>(null);
  const beginPaneDrag = useCallback((startX: number, startWidth: number) => {
    const onMove = (ev: MouseEvent) => {
      const next = clamp(startWidth + (ev.clientX - startX), LEFT_PANE_MIN, LEFT_PANE_MAX);
      if (leftPaneRef.current) {
        leftPaneRef.current.style.width = `${next}px`;
      }
      if (dragRef.current) dragRef.current.pendingWidth = next;
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      const final = dragRef.current?.pendingWidth ?? startWidth;
      dragRef.current = null;
      setLeftPaneWidth(final);
      try {
        window.localStorage?.setItem?.('historyLeftPaneWidth', String(final));
      } catch {
        // ignore storage failures (e.g. private browsing quota)
      }
    };
    dragRef.current = { onMove, onUp, pendingWidth: startWidth };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, []);
  const onResizeMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startWidth = leftPaneRef.current?.getBoundingClientRect().width ?? leftPaneWidth;
    beginPaneDrag(e.clientX, startWidth);
  }, [beginPaneDrag, leftPaneWidth]);
  const onResizeKeyDown = useCallback((e: React.KeyboardEvent) => {
    const step = e.shiftKey ? LEFT_PANE_KEY_BIG_STEP : LEFT_PANE_KEY_STEP;
    let next: number | null = null;
    if (e.key === 'ArrowLeft') next = leftPaneWidth - step;
    else if (e.key === 'ArrowRight') next = leftPaneWidth + step;
    else if (e.key === 'Home') next = LEFT_PANE_MIN;
    else if (e.key === 'End') next = LEFT_PANE_MAX;
    if (next === null) return;
    e.preventDefault();
    const clamped = clamp(next, LEFT_PANE_MIN, LEFT_PANE_MAX);
    setLeftPaneWidth(clamped);
    try {
      window.localStorage?.setItem?.('historyLeftPaneWidth', String(clamped));
    } catch { /* ignore */ }
  }, [leftPaneWidth]);
  // Mid-drag unmount: detach window listeners and restore body styles so a
  // closed window doesn't leave the document in col-resize forever.
  useEffect(() => () => {
    if (dragRef.current) {
      window.removeEventListener('mousemove', dragRef.current.onMove);
      window.removeEventListener('mouseup', dragRef.current.onUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      dragRef.current = null;
    }
  }, []);

  // Data source — local archive is production storage, always the default
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const [dataSource, _setDataSource] = useState<DataSource>('local');
  const [settingsLoaded, setSettingsLoaded] = useState(false);

  // Global SOAP defaults (from settings, used as fallback for historical sessions)
  const [globalSoapDefaults, setGlobalSoapDefaults] = useState<SoapOptions>({
    detail_level: 5,
    format: 'problem_based',
    custom_instructions: '',
    session_custom_instructions: '',
  });

  // Detail view state
  const [activeTab, setActiveTab] = useState<DetailTab>('transcript');
  const [isEditing, setIsEditing] = useState(false);
  const [editedTranscript, setEditedTranscript] = useState('');
  const [copySuccess, setCopySuccess] = useState<string | null>(null);

  // Patient handout state
  const [handoutContent, setHandoutContent] = useState<string | null>(null);
  const [handoutLoading, setHandoutLoading] = useState(false);

  // Billing state
  const [billingRecord, setBillingRecord] = useState<BillingRecord | null>(null);
  const [billingLoading, setBillingLoading] = useState(false);
  const [billingDefaults, setBillingDefaults] = useState<{
    visitSetting: string; counsellingExhausted: boolean; isHospital: boolean;
  }>({ visitSetting: 'in_office', counsellingExhausted: false, isHospital: false });
  const [rightPaneMode, setRightPaneMode] = useState<RightPaneMode>('session');
  const [showModelMenu, setShowModelMenu] = useState(false);

  // SOAP display state (result stored locally since hook doesn't track per-session)
  const [soapResult, setSoapResult] = useState<MultiPatientSoapResult | null>(null);
  const [customInstructionsExpanded, setCustomInstructionsExpanded] = useState(false);
  const [activePatient, setActivePatient] = useState(0);

  // Feedback state
  const [feedback, setFeedback] = useState<SessionFeedback | null>(null);

  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [activeDialog, setActiveDialog] = useState<ActiveDialog>('none');
  const [cleanupMessage, setCleanupMessage] = useState<string | null>(null);
  const [confirmPatientSessions, setConfirmPatientSessions] = useState<
    LocalArchiveDetails[] | null
  >(null);

  // Day feedback modal state
  const [showDayFeedback, setShowDayFeedback] = useState(false);
  const [dayFeedbackText, setDayFeedbackText] = useState<string | null>(null);
  const [dayFeedbackLoading, setDayFeedbackLoading] = useState(false);
  const [dayFeedbackError, setDayFeedbackError] = useState<string | null>(null);
  const dayFeedbackCacheKey = useRef('');
  const dayFeedbackInFlight = useRef(false);

  // Reset day feedback cache when date or session count changes
  useEffect(() => {
    const key = `${formatDateForApi(selectedDate)}:${sessions.length}`;
    if (dayFeedbackCacheKey.current !== key) {
      dayFeedbackCacheKey.current = key;
      setDayFeedbackText(null);
      setDayFeedbackError(null);
    }
  }, [selectedDate, sessions.length]);

  // Fetch day feedback when modal opens
  useEffect(() => {
    if (!showDayFeedback || dayFeedbackText !== null || dayFeedbackInFlight.current) return;
    let cancelled = false;
    dayFeedbackInFlight.current = true;
    setDayFeedbackLoading(true);
    setDayFeedbackError(null);

    (async () => {
      try {
        const dateStr = formatDateForApi(selectedDate);
        // Fetch transcripts in batches of 5 to avoid overwhelming IPC
        const batchSize = 5;
        const transcripts: string[] = [];
        for (let i = 0; i < sessions.length; i += batchSize) {
          if (cancelled) break;
          const batch = sessions.slice(i, i + batchSize);
          const results = await Promise.allSettled(
            batch.map(s => invoke<LocalArchiveDetails>('get_local_session_details', { sessionId: s.session_id, date: dateStr }))
          );
          for (const r of results) {
            if (r.status === 'fulfilled' && r.value) {
              const t = (r.value as LocalArchiveDetails).transcript;
              if (t?.trim()) transcripts.push(t);
            }
          }
        }
        if (cancelled) return;
        if (transcripts.length === 0) throw new Error('No transcripts available');
        let combined = transcripts.join('\n\n--- Next Session ---\n\n');
        // Cap at ~15000 words to avoid overwhelming LLM
        const words = combined.split(/\s+/).length;
        if (words > 15000) {
          combined = combined.split(/\s+/).slice(0, 15000).join(' ') + '\n[truncated]';
        }
        const result = await invoke<string>('generate_clinical_feedback', {
          systemPrompt: DAY_FEEDBACK_PROMPT,
          transcript: combined,
        });
        if (!cancelled) setDayFeedbackText(result);
      } catch (e) {
        if (!cancelled) setDayFeedbackError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) setDayFeedbackLoading(false);
        dayFeedbackInFlight.current = false;
      }
    })();

    return () => { cancelled = true; dayFeedbackInFlight.current = false; };
  }, [showDayFeedback, dayFeedbackText, selectedDate, sessions]);

  // LLM connection check - sync to SOAP hook
  const { status: ollamaConnectionStatus } = useOllamaConnection();

  // Sync Ollama status from connection hook to SOAP hook
  useEffect(() => {
    if (ollamaConnectionStatus) {
      setOllamaStatus(ollamaConnectionStatus);
    }
  }, [ollamaConnectionStatus, setOllamaStatus]);

  // LLM connection status from hook
  const llmConnected = ollamaStatus?.connected ?? false;

  // Local archive is production storage — always ready
  useEffect(() => {
    setSettingsLoaded(true);
    // Load billing defaults from physician settings
    invoke<Settings>('get_settings').then(s => {
      setBillingDefaults({
        visitSetting: s.billing_default_visit_setting || 'in_office',
        counsellingExhausted: s.billing_counselling_exhausted || false,
        isHospital: s.billing_is_hospital || false,
      });
    }).catch(() => {});
  }, []);

  // Sync globalSoapDefaults with hook's soapOptions (for session metadata fallback)
  useEffect(() => {
    setGlobalSoapDefaults(soapOptions);
  }, [soapOptions]);

  // Fetch all dates that have sessions (for calendar highlighting)
  useEffect(() => {
    if (!settingsLoaded) return;

    const fetchDates = async () => {
      try {
        if (dataSource === 'local') {
          // Fetch from local archive
          const dates = await invoke<string[]>('get_local_session_dates');
          setDatesWithSessions(new Set(dates));
        } else if (dataSource === 'medplum' && authState.is_authenticated) {
          // Fetch from Medplum - get all encounters and extract unique dates
          const encounters = await invoke<EncounterSummary[]>('medplum_get_encounter_history', {
            startDate: null,
            endDate: null,
          });
          const dates = new Set<string>();
          encounters.forEach((enc) => {
            // Extract date from encounter date string (YYYY-MM-DD format)
            const dateOnly = enc.date.split('T')[0];
            dates.add(dateOnly);
          });
          setDatesWithSessions(dates);
        } else {
          // Not authenticated for Medplum - show empty
          setDatesWithSessions(new Set());
        }
      } catch (e) {
        console.error('Failed to fetch session dates:', e);
        setDatesWithSessions(new Set());
      }
    };
    fetchDates();
  }, [settingsLoaded, dataSource, authState.is_authenticated]);

  // Sorted and filtered sessions
  const sortedSessions = useMemo(() => {
    let filtered = sessions;
    if (filterMode === 'clinical') filtered = sessions.filter(s => !s.likely_non_clinical);
    else if (filterMode === 'non-clinical') filtered = sessions.filter(s => s.likely_non_clinical);
    else if (filterMode === 'soap') filtered = sessions.filter(s => s.has_soap_note);
    else if (filterMode === 'no-soap') filtered = sessions.filter(s => !s.has_soap_note);

    const sorted = [...filtered].sort((a, b) => {
      let cmp = 0;
      switch (sortField) {
        case 'time':
          cmp = (a.started_at || '').localeCompare(b.started_at || '');
          break;
        case 'encounter':
          cmp = (a.encounter_number ?? 999) - (b.encounter_number ?? 999);
          break;
        case 'patient':
          cmp = (a.patient_name || '\uffff').localeCompare(b.patient_name || '\uffff');
          break;
        case 'words':
          cmp = (a.word_count || 0) - (b.word_count || 0);
          break;
        case 'duration':
          cmp = (a.duration_ms || 0) - (b.duration_ms || 0);
          break;
      }
      return sortDir === 'desc' ? -cmp : cmp;
    });
    return sorted;
  }, [sessions, sortField, sortDir, filterMode]);

  const flattenedSessions: FlattenedSession[] = useMemo(() => {
    return sortedSessions.flatMap((session): FlattenedSession[] => {
      // Auto-split siblings: each sibling is its own session row. Just decorate
      // with group metadata so the sidebar can render shared-border styling and
      // a "1 of N" badge. patientIndex stays null (it's a single-patient session
      // from the SOAP-rendering perspective — no sub-patient tabs in detail pane).
      if (session.sibling_group_id && (session.sibling_group_size ?? 0) > 1) {
        const idx = session.sibling_index ?? 0;
        const size = session.sibling_group_size ?? 1;
        return [{
          ...session,
          patientIndex: null,
          flattenedPatientName: session.patient_name,
          isGroupFirst: idx === 0,
          isGroupLast: idx === size - 1,
          isSibling: true,
        }];
      }
      // Legacy cosmetic fan-out for archived multi-patient sessions whose layout
      // pre-dates auto-split. Will fade out as those sessions get migrated via
      // the "Split into separate sessions" backfill button in the detail pane.
      const labels = session.patient_labels;
      if (labels && labels.length > 1) {
        return labels.map((label, i) => ({
          ...session,
          patientIndex: i,
          flattenedPatientName: label,
          isGroupFirst: i === 0,
          isGroupLast: i === labels.length - 1,
          isSibling: false,
        }));
      }
      return [{
        ...session,
        patientIndex: null,
        flattenedPatientName: null,
        isGroupFirst: false,
        isGroupLast: false,
        isSibling: false,
      }];
    });
  }, [sortedSessions]);

  // Fetch sessions for selected date from local archive or Medplum
  const fetchSessions = useCallback(async () => {
    if (!settingsLoaded) return;

    setLoading(true);
    setError(null);

    try {
      const dateStr = formatDateForApi(selectedDate);

      if (dataSource === 'local') {
        const result = await invoke<LocalArchiveSummary[]>('get_local_sessions_by_date', {
          date: dateStr,
        });
        setSessions(result);
      } else if (dataSource === 'medplum') {
        if (!authState.is_authenticated) {
          setError('Sign in to Medplum to view session history');
          setSessions([]);
          return;
        }

        // Fetch from Medplum for the selected date
        const nextDay = new Date(selectedDate);
        nextDay.setDate(nextDay.getDate() + 1);
        const endDateStr = formatDateForApi(nextDay);

        const encounters = await invoke<EncounterSummary[]>('medplum_get_encounter_history', {
          startDate: dateStr,
          endDate: endDateStr,
        });

        // Convert EncounterSummary to LocalArchiveSummary format
        const converted: LocalArchiveSummary[] = encounters.map((enc) => ({
          session_id: enc.fhirId,
          date: enc.date,
          duration_ms: enc.durationMinutes ? enc.durationMinutes * 60 * 1000 : null,
          word_count: 0, // Not available from Medplum summary
          has_soap_note: enc.hasSoapNote,
          has_audio: enc.hasAudio,
          auto_ended: false, // Not tracked in Medplum
          charting_mode: null,
          encounter_number: null,
          patient_name: null,
          likely_non_clinical: null,
          has_feedback: null,
        }));
        setSessions(converted);
      }
    } catch (e) {
      const errMsg = e instanceof Error ? e.message : String(e);
      if (!errMsg.includes('not found')) {
        setError(errMsg);
      }
      setSessions([]);
    } finally {
      setLoading(false);
    }
  }, [selectedDate, settingsLoaded, dataSource, authState.is_authenticated]);

  useEffect(() => {
    if (settingsLoaded) {
      fetchSessions();
    }
  }, [fetchSessions, settingsLoaded]);

  const markSessionFeedbackSaved = useCallback((sessionId: string, fb: SessionFeedback) => {
    setSessions(prev => prev.map(s =>
      s.session_id === sessionId
        ? { ...s, has_feedback: true, quality_rating: fb.qualityRating }
        : s
    ));
  }, []);

  // Bound to the currently-selected session so FeedbackPanel + BillingTab
  // don't need to re-thread sessionId on every save.
  const handleSelectedSessionFeedbackSaved = useCallback((fb: SessionFeedback) => {
    if (!selectedSessionId) return;
    markSessionFeedbackSaved(selectedSessionId, fb);
  }, [selectedSessionId, markSessionFeedbackSaved]);

  const rateSessionInline = useCallback(async (entry: FlattenedSession, rating: 'good' | 'bad') => {
    const nextRating = entry.quality_rating === rating ? null : rating;
    try {
      const updated = await updateSessionFeedback(
        entry.session_id,
        entry.date,
        { qualityRating: nextRating },
        { skipLoad: !entry.has_feedback },
      );
      markSessionFeedbackSaved(entry.session_id, updated);
    } catch (e) {
      console.error('Failed to rate session inline:', e);
    }
  }, [markSessionFeedbackSaved]);

  // Fetch session details from local archive or Medplum
  const fetchSessionDetails = useCallback(async (session: LocalArchiveSummary, patientIndex?: number | null) => {
    setSelectedSessionId(session.session_id);
    setSelectedPatientIndex(patientIndex ?? null);
    setBillingRecord(null);
    setDetailLoading(true);
    setRightPaneMode('session');

    try {
      let details: LocalArchiveDetails;

      if (dataSource === 'local') {
        // Fetch from local archive
        const dateStr = formatDateForApi(selectedDate);
        details = await invoke<LocalArchiveDetails>('get_local_session_details', {
          sessionId: session.session_id,
          date: dateStr,
        });
      } else {
        // Fetch from Medplum
        const encDetails = await invoke<EncounterDetails>('medplum_get_encounter_details', {
          encounterId: session.session_id,
        });

        // Convert EncounterDetails to LocalArchiveDetails format
        const metadata: LocalArchiveMetadata = {
          session_id: encDetails.fhirId,
          started_at: encDetails.date,
          ended_at: null,
          duration_ms: encDetails.durationMinutes ? encDetails.durationMinutes * 60 * 1000 : null,
          segment_count: 0,
          word_count: encDetails.transcript ? encDetails.transcript.split(/\s+/).length : 0,
          has_soap_note: encDetails.hasSoapNote,
          has_audio: encDetails.hasAudio,
          auto_ended: false,
          auto_end_reason: null,
          soap_detail_level: null, // Not available from Medplum
          soap_format: null, // Not available from Medplum
          charting_mode: null,
          encounter_number: null,
          patient_name: null,
          likely_non_clinical: null,
        };

        details = {
          session_id: encDetails.fhirId,
          metadata,
          transcript: encDetails.transcript,
          soap_note: encDetails.soapNote,
          audio_path: encDetails.audioUrl, // This is a URL, not a local path
          patientNotes: null,
        };
      }

      setSelectedSession(details);
      setEditedTranscript(details.transcript || '');

      // Reset SOAP and feedback state
      setSoapResult(null);
      setSoapError(null);
      setActivePatient(patientIndex ?? 0);
      setFeedback(null);

      // Load SOAP options from metadata if available, otherwise use global defaults
      if (details.metadata.soap_detail_level !== null || details.metadata.soap_format !== null) {
        // Session has saved SOAP options - use them for regeneration
        setSoapOptions({
          detail_level: details.metadata.soap_detail_level ?? globalSoapDefaults.detail_level,
          format: details.metadata.soap_format ?? globalSoapDefaults.format,
          custom_instructions: globalSoapDefaults.custom_instructions,
          session_custom_instructions: '', // Per-session instructions are ephemeral, not stored
        });
      } else {
        // No saved options (old session or Medplum) - use global defaults
        setSoapOptions(globalSoapDefaults);
      }

      // If session has SOAP note, create result for display
      if (details.soap_note) {
        // Use per-patient notes when available (multi-patient encounter).
        // Recovery path: cross-room multi-patient sessions can have non-empty
        // patient labels but empty content strings, because soap_patient_*.txt
        // didn't always sync to the profile service. In that case, recover
        // per-patient content by parsing the combined soap_note delimiters.
        const hasMultiPatient = details.patientNotes && details.patientNotes.length > 1;
        const allPerPatientEmpty = hasMultiPatient &&
          details.patientNotes!.every(pn => !pn.content || pn.content.trim() === '');

        let notes: { speaker_id: string; patient_label: string; content: string }[];
        if (hasMultiPatient && !allPerPatientEmpty) {
          notes = details.patientNotes!.map(pn => ({
            speaker_id: `Patient ${pn.index}`,
            patient_label: pn.label,
            content: pn.content,
          }));
        } else if (hasMultiPatient && allPerPatientEmpty && details.soap_note) {
          notes = recoverPerPatientNotesFromCombined(details.soap_note, details.patientNotes!);
        } else {
          notes = [{
            speaker_id: 'Patient',
            patient_label: 'Patient',
            content: details.soap_note,
          }];
        }

        setSoapResult({
          notes,
          physician_speaker: null,
          generated_at: details.metadata.ended_at || new Date().toISOString(),
          model_used: 'archived',
        });
        setActiveTab('soap');
      } else {
        setActiveTab('transcript');
      }

      // Load feedback (local archive only)
      if (dataSource === 'local') {
        try {
          const fb = await invoke<SessionFeedback | null>('get_session_feedback', {
            sessionId: session.session_id, date: formatDateForApi(selectedDate),
          });
          setFeedback(fb);
        } catch {
          // Non-blocking — old sessions may not have feedback
        }
      }

    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSelectedSessionId(null);
    } finally {
      setDetailLoading(false);
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps -- useState setters are stable
  }, [selectedDate, dataSource, globalSoapDefaults, setSoapOptions, setSoapError]);

  // Generate SOAP note using shared hook (with optional model override)
  const handleGenerateSoap = useCallback(async (modelOverride?: string) => {
    if (!editedTranscript.trim()) return;

    // When viewing a specific patient in a flattened multi-patient session,
    // regenerate only that patient's SOAP note using the single-patient prompt.
    if (selectedPatientIndex !== null && soapResult && soapResult.notes.length > 1) {
      const targetNote = soapResult.notes[selectedPatientIndex];
      if (!targetNote) return;

      setSoapError(null);
      try {
        const singleNote = await invoke<SoapNote>('generate_soap_note', {
          transcript: editedTranscript,
          audioEvents: null,
          options: soapOptions,
          sessionId: selectedSession?.session_id ?? null,
          // Class 5 follow-up (2026-04-29): pass session date so the
          // backend's lookup_patient_summary uses the right archive
          // directory for older multi-patient regens. Same-day regens
          // still work without this — kept for backward compat.
          sessionDate: formatDateForApi(selectedDate),
          speakerContext: null,
          patientLabel: targetNote.patient_label,
          modelOverride: modelOverride || null,
        });

        // Merge the regenerated note back into the existing multi-patient result
        const updatedNotes = soapResult.notes.map((n, i) =>
          i === selectedPatientIndex
            ? { ...n, content: singleNote.content }
            : n
        );
        const updatedResult: MultiPatientSoapResult = {
          ...soapResult,
          notes: updatedNotes,
          generated_at: singleNote.generated_at,
          model_used: singleNote.model_used,
        };
        setSoapResult(updatedResult);

        // Copy regenerated note to clipboard
        try {
          await writeText(singleNote.content);
        } catch {
          // Non-blocking
        }

        // Save updated SOAP to archive
        if (selectedSession && dataSource === 'local') {
          try {
            await persistSoapResultToArchive({
              sessionId: selectedSession.session_id,
              date: formatDateForApi(selectedDate),
              notes: updatedResult.notes,
              detailLevel: soapOptions.detail_level,
              format: soapOptions.format,
            });
          } catch (saveErr) {
            console.error('Failed to save SOAP to archive:', saveErr);
          }
        }

        setGlobalSoapDefaults(soapOptions);
      } catch (e) {
        setSoapError(e instanceof Error ? e.message : String(e));
      }
      return;
    }

    // Default path: full session SOAP regeneration (auto-detects patients)
    const result = await generateSoapNote(
      editedTranscript,
      undefined, // audioEvents
      soapOptions,
      selectedSession?.session_id,
      modelOverride
    );

    if (!result) return; // Hook handles error state

    setSoapResult(result);
    setActivePatient(0);

    // Save SOAP note to archive (hook doesn't know about session context)
    if (selectedSession) {
      try {
        if (dataSource === 'local') {
          // Save to local archive with SOAP options. Multi-patient writes
          // also persist soap_patient_N.txt + patient_labels.json so the
          // History row fan-out picks up the sub-patients on next refresh.
          await persistSoapResultToArchive({
            sessionId: selectedSession.session_id,
            date: formatDateForApi(selectedDate),
            notes: result.notes,
            detailLevel: soapOptions.detail_level,
            format: soapOptions.format,
          });
        } else if (dataSource === 'medplum' && authState.is_authenticated) {
          // Save to Medplum (no metadata support)
          await invoke('medplum_add_soap_to_encounter', {
            encounterFhirId: selectedSession.session_id,
            soapNote: serializeSoapNotes(result.notes),
          });
        }
      } catch (saveErr) {
        console.error('Failed to save SOAP to archive:', saveErr);
      }

      // Update local global defaults after successful generation
      setGlobalSoapDefaults(soapOptions);
    }
  }, [editedTranscript, soapOptions, selectedSession, selectedDate, dataSource, authState.is_authenticated, generateSoapNote, selectedPatientIndex, soapResult, setSoapError]);

  const clearSelection = useCallback(() => {
    setSelectedSessionId(null);
    setSelectedPatientIndex(null);
    setSelectedSession(null);
    setIsEditing(false);
    setSoapResult(null);
    setSoapError(null);
    setHandoutContent(null);
    setBillingRecord(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- all omitted deps are stable (useState setters or useCallback-wrapped functions)
  }, []);

  // Clear selection when date changes
  useEffect(() => {
    clearSelection();
  }, [selectedDate, clearSelection]);

  // Load handout content when handout tab is selected
  useEffect(() => {
    if (activeTab !== 'handout' || !selectedSession) return;
    let cancelled = false;
    setHandoutLoading(true);
    invoke<string | null>('get_patient_handout', {
      sessionId: selectedSession.session_id,
      date: formatDateForApi(selectedDate),
    }).then(content => {
      if (!cancelled) setHandoutContent(content);
    }).catch(e => console.error('Failed to load handout:', e))
      .finally(() => { if (!cancelled) setHandoutLoading(false); });
    return () => { cancelled = true; };
  // eslint-disable-next-line react-hooks/exhaustive-deps -- session_id is the meaningful dependency
  }, [activeTab, selectedSession?.session_id, selectedDate]);

  // Load billing data when billing tab selected
  useEffect(() => {
    if (activeTab !== 'billing' || !selectedSession) return;
    let cancelled = false;
    setBillingLoading(true);
    invoke<BillingRecord | null>('get_session_billing', {
      sessionId: selectedSession.session_id,
      date: formatDateForApi(selectedDate),
    }).then(record => {
      if (!cancelled) setBillingRecord(record);
    }).catch(e => console.error('Failed to load billing:', e))
      .finally(() => { if (!cancelled) setBillingLoading(false); });
    return () => { cancelled = true; };
  // eslint-disable-next-line react-hooks/exhaustive-deps -- session_id is the meaningful dependency
  }, [activeTab, selectedSession?.session_id, selectedDate]);

  // Escape: clear checkbox selection first, then close the detail pane.
  // Refs keep the keydown listener stable across selection toggles so we don't
  // add/remove a window listener on every checkbox click.
  const selectedIdsRef = useRef(selectedIds);
  selectedIdsRef.current = selectedIds;
  const selectedSessionIdRef = useRef(selectedSessionId);
  selectedSessionIdRef.current = selectedSessionId;
  const clearSelectionRef = useRef(clearSelection);
  clearSelectionRef.current = clearSelection;
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (selectedIdsRef.current.size > 0) {
        setSelectedIds(new Set());
      } else if (selectedSessionIdRef.current) {
        clearSelectionRef.current();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  const handleCopy = async (text: string, field: string) => {
    try {
      await writeText(text);
      setCopySuccess(field);
      setTimeout(() => setCopySuccess(null), 2000);
    } catch (e) {
      console.error('Failed to copy:', e);
    }
  };

  const handleClose = async () => {
    try {
      const window = getCurrentWindow();
      await window.close();
    } catch (e) {
      console.error('Failed to close window:', e);
    }
  };

  const toggleEntrySelection = useCallback((key: string) => {
    setSelectedIds(prev => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }, []);

  // Get selected sessions in display order (for dialogs)
  /** Unique session IDs derived from selected entry keys */
  const selectedSessionIds = useMemo((): Set<string> => {
    return new Set(Array.from(selectedIds).map(sessionIdFromKey));
  }, [selectedIds]);

  const getSelectedSessions = useCallback((): LocalArchiveSummary[] => {
    return sessions.filter(s => selectedSessionIds.has(s.session_id));
  }, [sessions, selectedSessionIds]);

  // Post-operation: refresh list, renumber, clear selection, show message
  const afterCleanupOp = useCallback(async (message: string) => {
    setActiveDialog('none');
    setSelectedIds(new Set());
    clearSelection();
    setCleanupMessage(message);
    setTimeout(() => setCleanupMessage(null), 3000);

    // Renumber encounters and refresh
    const dateStr = formatDateForApi(selectedDate);
    try {
      await invoke('renumber_local_encounters', { date: dateStr });
    } catch (e) {
      console.error('Failed to renumber encounters:', e);
    }
    await fetchSessions();

    // Refresh calendar dates
    try {
      const dates = await invoke<string[]>('get_local_session_dates');
      setDatesWithSessions(new Set(dates));
    } catch (e) {
      console.error('Failed to refresh dates:', e);
    }
  }, [selectedDate, fetchSessions, clearSelection]);

  // Delete operation
  const handleDeleteConfirm = useCallback(async () => {
    const dateStr = formatDateForApi(selectedDate);
    const keys = Array.from(selectedIds);
    try {
      if (selectedPatientIndex !== null && selectedSession?.patientNotes && keys.length === 1) {
        const archiveIndex = selectedSession.patientNotes[selectedPatientIndex]?.index ?? (selectedPatientIndex + 1);
        await invoke('delete_patient_from_session', {
          sessionId: sessionIdFromKey(keys[0]),
          date: dateStr,
          patientIndex: archiveIndex,
        });
        await afterCleanupOp('Patient deleted');
      } else {
        const uniqueSessionIds = [...new Set(keys.map(sessionIdFromKey))];
        for (const id of uniqueSessionIds) {
          await invoke('delete_local_session', { sessionId: id, date: dateStr });
        }
        await afterCleanupOp(`Deleted ${uniqueSessionIds.length} session${uniqueSessionIds.length > 1 ? 's' : ''}`);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setActiveDialog('none');
    }
  }, [selectedDate, selectedIds, selectedPatientIndex, selectedSession, afterCleanupOp]);

  /** Detect if current selection represents a same-session patient merge */
  const isSameSessionPatientMerge = useMemo((): boolean => {
    const keys = Array.from(selectedIds);
    if (keys.length < 2) return false;
    const sessionIds = keys.map(sessionIdFromKey);
    const uniqueSessions = new Set(sessionIds);
    // All keys must share the same session ID, and at least one must have a patient index
    return uniqueSessions.size === 1 && keys.some(k => patientIndexFromKey(k) !== null);
  }, [selectedIds]);

  // Merge operation (cross-session merges only; same-session patient merges go through handlePatientMergeConfirm)
  const handleMergeConfirm = useCallback(async () => {
    const dateStr = formatDateForApi(selectedDate);
    const keys = Array.from(selectedIds);
    try {
      const uniqueSessionIds = [...new Set(keys.map(sessionIdFromKey))];
      await invoke<string>('merge_local_sessions', { sessionIds: uniqueSessionIds, date: dateStr });
      await afterCleanupOp(`Merged ${uniqueSessionIds.length} sessions`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      setActiveDialog('none');
      // Clear selection so error is visible (error is hidden when a session is selected)
      clearSelection();
    }
  }, [selectedDate, selectedIds, afterCleanupOp]);

  // Same-session patient merge confirm handler
  const handlePatientMergeConfirm = useCallback(async (newLabel: string) => {
    const dateStr = formatDateForApi(selectedDate);
    const keys = Array.from(selectedIds);
    const sessionId = sessionIdFromKey(keys[0]);

    try {
      // Fetch session details to get transcript and patient notes
      const details = await invoke<LocalArchiveDetails>('get_local_session_details', {
        sessionId,
        date: dateStr,
      });

      if (!details.transcript?.trim()) {
        throw new Error('Session has no transcript');
      }
      if (!details.patientNotes || details.patientNotes.length < 2) {
        throw new Error('Session does not have multiple patient notes');
      }

      // Determine which archive patient indices are being merged
      const mergedPatientIndices = keys
        .map(k => patientIndexFromKey(k))
        .filter((idx): idx is number => idx !== null);

      const mergedArchiveIndices = mergedPatientIndices.map(
        flatIdx => details.patientNotes![flatIdx]?.index ?? (flatIdx + 1)
      );

      // Build all_patients data for the LLM
      const allPatients = details.patientNotes.map(pn => ({
        index: pn.index,
        label: pn.label,
        content: pn.content,
      }));

      const mergedSoap = await invoke<string>('merge_patient_soaps', {
        sessionId,
        date: dateStr,
        mergedIndices: mergedArchiveIndices,
        newLabel: newLabel,
        transcript: details.transcript,
        allPatients,
      });

      if (mergedSoap) {
        await afterCleanupOp('Patient notes merged');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setActiveDialog('none');
    }
  }, [selectedDate, selectedIds, afterCleanupOp]);

  // Edit name operation
  const handleEditNameConfirm = useCallback(async (name: string) => {
    const dateStr = formatDateForApi(selectedDate);
    const key = Array.from(selectedIds)[0];
    const id = sessionIdFromKey(key);
    try {
      if (selectedPatientIndex !== null && selectedSession?.patientNotes) {
        const archiveIndex = selectedSession.patientNotes[selectedPatientIndex]?.index ?? (selectedPatientIndex + 1);
        await invoke('rename_patient_label', {
          sessionId: id,
          date: dateStr,
          patientIndex: archiveIndex,
          newLabel: name,
        });
      } else {
        await invoke('update_session_patient_name', { sessionId: id, date: dateStr, patientName: name });
      }
      await afterCleanupOp('Patient name updated');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setActiveDialog('none');
    }
  }, [selectedDate, selectedIds, selectedPatientIndex, selectedSession, afterCleanupOp]);

  // Open split window — passes context via URL query params
  const openSplitWindow = useCallback(async () => {
    const dateStr = formatDateForApi(selectedDate);
    const key = Array.from(selectedIds)[0];
    if (!key) return;
    const id = sessionIdFromKey(key);

    try {
      // Close existing split window if any
      const existing = await WebviewWindow.getByLabel('split');
      if (existing) {
        await existing.close();
      }

      const splitWindow = new WebviewWindow('split', {
        url: `split.html?sessionId=${encodeURIComponent(id)}&date=${encodeURIComponent(dateStr)}`,
        title: 'Split Session',
        width: 700,
        height: 800,
        minWidth: 500,
        minHeight: 400,
        resizable: true,
      });

      splitWindow.once('tauri://error', (e) => {
        console.error('Failed to open split window:', e);
      });
    } catch (e) {
      console.error('Error opening split window:', e);
    }
  }, [selectedDate, selectedIds]);

  // Listen for split_complete from SplitWindow
  // Store callback in ref to keep listener stable (avoid re-subscription on callback identity change)
  const afterCleanupOpRef = useRef(afterCleanupOp);
  afterCleanupOpRef.current = afterCleanupOp;

  useEffect(() => {
    let mounted = true;
    let cleanup: (() => void) | undefined;

    const setup = async () => {
      const unlisten = await listen('split_complete', () => {
        if (mounted) {
          afterCleanupOpRef.current('Session split into two');
        }
      });

      if (!mounted) {
        unlisten();
        return;
      }

      cleanup = unlisten;
    };

    setup();

    return () => {
      mounted = false;
      cleanup?.();
    };
  }, []);

  // SOAP regeneration for selected sessions
  const handleRegenSoap = useCallback(async () => {
    const dateStr = formatDateForApi(selectedDate);
    const uniqueIds = [...selectedSessionIds];
    let regenCount = 0;
    for (const id of uniqueIds) {
      try {
        const details = await invoke<LocalArchiveDetails>('get_local_session_details', {
          sessionId: id,
          date: dateStr,
        });
        if (details.transcript?.trim()) {
          const result = await generateSoapNote(details.transcript, undefined, soapOptions, id);
          if (result) {
            await persistSoapResultToArchive({
              sessionId: id,
              date: dateStr,
              notes: result.notes,
              detailLevel: soapOptions.detail_level,
              format: soapOptions.format,
            });
            regenCount++;
          }
        }
      } catch (e) {
        console.error(`Failed to regen SOAP for ${id}:`, e);
      }
    }
    await afterCleanupOp(`Regenerated SOAP for ${regenCount} session${regenCount !== 1 ? 's' : ''}`);
  }, [selectedDate, selectedSessionIds, soapOptions, generateSoapNote, afterCleanupOp]);

  // Load full details for each selected session so the batch dialog has
  // SOAP + transcript available for Medplum DocumentReference uploads.
  const handleOpenConfirmPatient = useCallback(async () => {
    const dateStr = formatDateForApi(selectedDate);
    const ids = [...selectedSessionIds];
    if (ids.length === 0) return;
    const results = await Promise.all(
      ids.map((id) =>
        invoke<LocalArchiveDetails>('get_local_session_details', {
          sessionId: id,
          date: dateStr,
        }).catch((e) => {
          console.error(`Failed to load details for ${id}:`, e);
          return null;
        }),
      ),
    );
    const loaded = results.filter((r): r is LocalArchiveDetails => r !== null);
    if (loaded.length === 0) {
      setError('Could not load any selected sessions');
      return;
    }
    setConfirmPatientSessions(loaded);
    setActiveDialog('confirmPatient');
  }, [selectedDate, selectedSessionIds]);

  // Derived values
  const hasTranscript = editedTranscript.trim().length > 0;
  const isModified = selectedSession?.transcript !== editedTranscript;
  const safeActivePatient = soapResult && activePatient < soapResult.notes.length ? activePatient : 0;
  const activeSoapContent = soapResult?.notes[safeActivePatient]?.content ?? null;
  const isMultiPatient = (soapResult?.notes.length ?? 0) > 1;

  return (
    <div className="history-window">
      {/* Shared header */}
      <div className="history-header">
        <h1>Session History</h1>
        <button className="close-btn" onClick={handleClose} aria-label="Close">
          &times;
        </button>
      </div>

      {/* Two-pane body */}
      <div className="history-split-pane">

        {/* LEFT PANE — calendar + session list */}
        <div
          className="history-left-pane"
          ref={leftPaneRef}
          style={{ width: leftPaneWidth }}
        >
          <div className="history-left-scroll">
          {/* Success message */}
          {cleanupMessage && (
            <div className="history-success-message">{cleanupMessage}</div>
          )}

          <div className="calendar-with-today">
            <Calendar
              selectedDate={selectedDate}
              onDateSelect={setSelectedDate}
              datesWithSessions={Array.from(datesWithSessions)}
            />
            {selectedDate.toDateString() !== new Date().toDateString() && (
              <button
                className="btn-today"
                onClick={() => setSelectedDate(new Date())}
              >
                Today
              </button>
            )}
          </div>

          <div className="sessions-section">
            <h2 className="sessions-date-header">
              {formatDateForDisplay(selectedDate)}
            </h2>

            {/* Sort & Filter controls */}
            {sessions.length > 0 && (
              <div className="sessions-toolbar">
                <select
                  className="sessions-sort-select"
                  value={`${sortField}-${sortDir}`}
                  onChange={(e) => {
                    const [f, d] = e.target.value.split('-') as [SortField, SortDir];
                    setSortField(f);
                    setSortDir(d);
                  }}
                >
                  <option value="time-asc">Oldest first</option>
                  <option value="time-desc">Newest first</option>
                  <option value="encounter-asc">Encounter #</option>
                  <option value="patient-asc">Patient A-Z</option>
                  <option value="words-desc">Most words</option>
                  <option value="duration-desc">Longest</option>
                </select>
                <select
                  className="sessions-filter-select"
                  value={filterMode}
                  onChange={(e) => setFilterMode(e.target.value as FilterMode)}
                >
                  <option value="all">All ({sessions.length})</option>
                  <option value="clinical">Clinical</option>
                  <option value="non-clinical">Non-clinical</option>
                  <option value="soap">Has SOAP</option>
                  <option value="no-soap">No SOAP</option>
                </select>
              </div>
            )}

            {loading ? (
              <div className="sessions-loading">
                <div className="spinner" />
              </div>
            ) : error && !selectedSession ? (
              <div className="sessions-error">
                <p>{error}</p>
                <button onClick={fetchSessions}>Retry</button>
              </div>
            ) : sessions.length === 0 ? (
              <div className="sessions-empty">
                <p>No sessions recorded on this date</p>
              </div>
            ) : (
              <div className="sessions-list">
                {flattenedSessions.map((entry) => {
                  const isGrouped = entry.patientIndex !== null || entry.isSibling;
                  const multiPatientClasses = isGrouped
                    ? ` multi-patient-group${entry.isGroupFirst ? ' group-first' : ''}${entry.isGroupLast ? ' group-last' : ''}${entry.isSibling ? ' sibling-row' : ''}`
                    : '';
                  const siblingPosition = entry.isSibling && entry.sibling_group_size
                    ? `${(entry.sibling_index ?? 0) + 1} of ${entry.sibling_group_size}`
                    : null;
                  const isActive = selectedSessionId === entry.session_id && selectedPatientIndex === entry.patientIndex;
                  const ek = entryKey(entry);
                  const isSelected = selectedIds.has(ek);
                  const displayName = entry.flattenedPatientName ?? entry.patient_name;
                  return (
                  <div
                    key={ek}
                    className={`session-item${isSelected ? ' selected' : ''}${isActive ? ' active' : ''}${multiPatientClasses}`}
                  >
                    <label className="history-checkbox" onClick={(e) => e.stopPropagation()}>
                      <input
                        type="checkbox"
                        checked={isSelected}
                        onChange={() => toggleEntrySelection(ek)}
                        aria-label={`Select session ${displayName ?? entry.session_id}`}
                      />
                    </label>
                    <button
                      className="session-item-body"
                      onClick={() => {
                        setSelectedPatientIndex(entry.patientIndex);
                        fetchSessionDetails(entry, entry.patientIndex);
                      }}
                    >
                      <div className="session-row-top">
                        <span className="session-time">{formatLocalTime(entry.started_at || entry.date)}</span>
                        {entry.duration_ms && (
                          <span className="session-duration">
                            {formatDurationShort(entry.duration_ms)}
                          </span>
                        )}
                      </div>
                      <div className="session-name">
                        {entry.charting_mode === 'continuous' && entry.encounter_number != null
                          ? `Encounter #${entry.encounter_number}${displayName ? ` \u2014 ${displayName}` : ''}`
                          : entry.word_count > 0
                            ? `${entry.word_count} words`
                            : 'Scribe Session'}
                      </div>
                      <div className="session-row-bottom">
                        <div className="session-badges">
                          {siblingPosition && (
                            <span
                              className="badge sibling-badge"
                              title="This patient was discussed in the same encounter as the linked rows above/below"
                            >
                              Patient {siblingPosition}
                            </span>
                          )}
                          {entry.likely_non_clinical && (
                            <span className="badge non-clinical-badge">Non-clinical</span>
                          )}
                          {entry.charting_mode === 'continuous' && (
                            <span className="badge charted-badge">Auto-charted</span>
                          )}
                          {entry.has_soap_note && (
                            <span className="badge soap-badge">SOAP</span>
                          )}
                          {entry.has_audio && (
                            <span className="badge audio-badge">Audio</span>
                          )}
                          {entry.auto_ended && (
                            <span className="badge auto-badge">Auto</span>
                          )}
                        </div>
                      </div>
                    </button>
                    <div className="session-item-rate" onClick={(e) => e.stopPropagation()}>
                      <button
                        className={`session-rate-btn${entry.quality_rating === 'good' ? ' active good' : ''}`}
                        onClick={() => rateSessionInline(entry, 'good')}
                        title="Mark this session's encounter detection + SOAP as correct"
                        aria-label="Mark correct"
                      >
                        {'\u{1F44D}'}
                      </button>
                      <button
                        className={`session-rate-btn${entry.quality_rating === 'bad' ? ' active bad' : ''}`}
                        onClick={() => rateSessionInline(entry, 'bad')}
                        title="Mark this session as needing review (open details to specify why)"
                        aria-label="Mark needs review"
                      >
                        {'\u{1F44E}'}
                      </button>
                    </div>
                  </div>
                  );
                })}
              </div>
            )}
          </div>

          {/* Contextual action bar — appears only when one or more rows selected */}
          {selectedIds.size > 0 && (
            <HistoryActionBar
              selectedCount={selectedIds.size}
              onMerge={() => setActiveDialog('merge')}
              onDelete={() => setActiveDialog('delete')}
              onEditName={() => setActiveDialog('editName')}
              onConfirmPatient={handleOpenConfirmPatient}
              onSplit={openSplitWindow}
              onRegenSoap={handleRegenSoap}
            />
          )}

          {/* Data source and auth status footer */}
          {!authLoading && selectedIds.size === 0 && (
            <div className="history-footer">
              <span className="data-source-indicator">
                {dataSource === 'local' ? '💾 Local Storage' : '☁️ Medplum'}
              </span>
              {dataSource === 'medplum' && !authState.is_authenticated && (
                <button className="auth-status not-authenticated" onClick={login}>
                  Sign in to view history
                </button>
              )}
              {dataSource === 'local' && authState.is_authenticated && (
                <span className="auth-status authenticated">
                  ☁️ Also synced to Medplum
                </span>
              )}
            </div>
          )}
          </div>{/* end history-left-scroll */}

          {/* Day Feedback + Billing buttons — pinned to bottom of sidebar */}
          {sessions.length > 0 && selectedIds.size === 0 && (
            <div className="history-left-bottom">
              <div className="billing-view-buttons">
                <button
                  className={`btn-small ${rightPaneMode === 'daily_billing' ? 'active' : ''}`}
                  onClick={() => setRightPaneMode(rightPaneMode === 'daily_billing' ? 'session' : 'daily_billing')}
                >
                  Daily Billing
                </button>
                <button
                  className={`btn-small ${rightPaneMode === 'monthly_billing' ? 'active' : ''}`}
                  onClick={() => setRightPaneMode(rightPaneMode === 'monthly_billing' ? 'session' : 'monthly_billing')}
                >
                  Monthly
                </button>
              </div>
              <button
                className="day-feedback-btn"
                onClick={() => setShowDayFeedback(true)}
                disabled={!llmConnected}
              >
                Day Feedback ({sessions.length} sessions)
              </button>
            </div>
          )}
        </div>

        {/* Resize handle — drag or use arrow keys to widen / shrink the left pane */}
        <div
          className="history-resizer"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize sidebar"
          aria-valuenow={leftPaneWidth}
          aria-valuemin={LEFT_PANE_MIN}
          aria-valuemax={LEFT_PANE_MAX}
          tabIndex={0}
          onMouseDown={onResizeMouseDown}
          onKeyDown={onResizeKeyDown}
        />

        {/* RIGHT PANE — session details or billing summaries */}
        <div className="history-right-pane">
          {rightPaneMode === 'daily_billing' ? (
            <DailySummaryView date={formatDateForApi(selectedDate)} />
          ) : rightPaneMode === 'monthly_billing' ? (
            <MonthlySummaryView endDate={formatDateForApi(selectedDate)} />
          ) : detailLoading ? (
            <div className="detail-empty-state">
              <div className="spinner" />
            </div>
          ) : selectedSession ? (
            <div className="detail-content">
              {/* Session Summary Bar */}
              <div className="session-summary">
                <span className="summary-time">{formatLocalTime(selectedSession.metadata.started_at)}</span>
                {selectedSession.metadata.duration_ms && (
                  <span className="summary-duration">{formatDurationShort(selectedSession.metadata.duration_ms)}</span>
                )}
                <span className="summary-words">{selectedSession.metadata.word_count} words</span>
                {selectedSession.metadata.likely_non_clinical && (
                  <span className="summary-badge non-clinical">Non-clinical</span>
                )}
                {selectedSession.metadata.charting_mode === 'continuous' && (
                  <span className="summary-badge charted">
                    Auto-charted{selectedSession.metadata.encounter_number != null ? ` #${selectedSession.metadata.encounter_number}` : ''}{selectedSession.metadata.patient_name ? ` \u2014 ${selectedSession.metadata.patient_name}` : ''}
                  </span>
                )}
                {selectedSession.metadata.auto_ended && (
                  <span className="summary-badge auto">Auto-ended</span>
                )}
              </div>

              {/* Tab Navigation */}
              <div className="review-tabs">
                <button
                  className={`review-tab ${activeTab === 'transcript' ? 'active' : ''}`}
                  onClick={() => setActiveTab('transcript')}
                >
                  Transcript
                  {isModified && <span className="tab-badge">edited</span>}
                </button>
                <button
                  className={`review-tab ${activeTab === 'soap' ? 'active' : ''}`}
                  onClick={() => setActiveTab('soap')}
                  disabled={!hasTranscript}
                >
                  SOAP
                  {soapResult && <span className="tab-badge done">✓</span>}
                </button>
                {selectedSession?.metadata?.has_patient_handout && (
                  <button
                    className={`review-tab ${activeTab === 'handout' ? 'active' : ''}`}
                    onClick={() => setActiveTab('handout')}
                  >
                    Handout
                  </button>
                )}
                {selectedSession?.metadata?.has_soap_note && (
                  <button
                    className={`review-tab ${activeTab === 'billing' ? 'active' : ''}`}
                    onClick={() => setActiveTab('billing')}
                  >
                    Billing
                    {billingRecord?.status === 'confirmed' && <span className="tab-badge" style={{color: 'var(--accent-idle, #22c55e)'}}>✓</span>}
                    {billingRecord?.status === 'draft' && <span className="tab-badge">draft</span>}
                  </button>
                )}
                <button
                  className={`review-tab ${activeTab === 'insights' ? 'active' : ''}`}
                  onClick={() => setActiveTab('insights')}
                >
                  Insights
                </button>
              </div>

              {/* Tab Content */}
              <div className="review-tab-content">
                {/* Transcript Tab */}
                {activeTab === 'transcript' && (
                  <div className="tab-panel transcript-panel">
                    <div className="panel-header">
                      <div className="panel-actions">
                        {hasTranscript && (
                          <>
                            <button
                              className={`btn-small ${isEditing ? 'active' : ''}`}
                              onClick={() => setIsEditing(!isEditing)}
                            >
                              {isEditing ? 'Done' : 'Edit'}
                            </button>
                            <button
                              className={`btn-small copy-btn ${copySuccess === 'transcript' ? 'success' : ''}`}
                              onClick={() => handleCopy(editedTranscript, 'transcript')}
                            >
                              {copySuccess === 'transcript' ? 'Copied!' : 'Copy'}
                            </button>
                          </>
                        )}
                      </div>
                    </div>

                    <div className="panel-body">
                      {selectedPatientIndex !== null && isMultiPatient && (
                        <div className="shared-transcript-label">
                          Shared transcript ({soapResult?.notes.length} patients in this encounter)
                        </div>
                      )}
                      {hasTranscript ? (
                        isEditing ? (
                          <textarea
                            className="transcript-editor"
                            value={editedTranscript}
                            onChange={(e) => setEditedTranscript(e.target.value)}
                            placeholder="Edit transcript..."
                          />
                        ) : (
                          <div className="transcript-display">
                            {editedTranscript.split('\n\n').map((paragraph, i) => (
                              <p key={i}>{paragraph}</p>
                            ))}
                          </div>
                        )
                      ) : (
                        <div className="panel-empty">No transcript recorded</div>
                      )}
                    </div>
                  </div>
                )}

                {/* SOAP Tab */}
                {activeTab === 'soap' && (
                  <div className="tab-panel soap-panel">
                    {/* SOAP Options */}
                    {!isGeneratingSoap && (
                      <div className="soap-options">
                        {/* Detail Level Slider */}
                        <div className="soap-option-row">
                          <label className="soap-option-label">
                            Detail: {DETAIL_LEVEL_LABELS[soapOptions.detail_level]?.name || 'Standard'}
                          </label>
                          <div className="soap-detail-slider">
                            <input
                              type="range"
                              min="1"
                              max="10"
                              value={soapOptions.detail_level}
                              onChange={(e) => updateSoapDetailLevel(parseInt(e.target.value))}
                              className="detail-slider"
                              aria-label="SOAP note detail level"
                            />
                            <span className="detail-value">{soapOptions.detail_level}</span>
                          </div>
                        </div>

                        {/* Format Toggle */}
                        <div className="soap-option-row">
                          <label className="soap-option-label">Format</label>
                          <div className="soap-format-toggle">
                            <button
                              className={`format-btn ${soapOptions.format === 'problem_based' ? 'active' : ''}`}
                              onClick={() => updateSoapFormat('problem_based')}
                            >
                              Problem
                            </button>
                            <button
                              className={`format-btn ${soapOptions.format === 'comprehensive' ? 'active' : ''}`}
                              onClick={() => updateSoapFormat('comprehensive')}
                            >
                              Comprehensive
                            </button>
                          </div>
                        </div>

                        {/* Session-specific additional context for SOAP regeneration */}
                        <div className="soap-option-row custom-instructions">
                          <button
                            className="custom-instructions-toggle"
                            onClick={() => setCustomInstructionsExpanded(!customInstructionsExpanded)}
                          >
                            <span className={`chevron-small ${customInstructionsExpanded ? '' : 'collapsed'}`}>&#9660;</span>
                            Additional Context
                            {soapOptions.session_custom_instructions?.trim() && (
                              <span className="custom-badge">Active</span>
                            )}
                          </button>
                          {customInstructionsExpanded && (
                            <textarea
                              className="custom-instructions-input"
                              value={soapOptions.session_custom_instructions}
                              onChange={(e) => updateSessionCustomInstructions(e.target.value)}
                              placeholder="Add context for this encounter (e.g., epidural injection was performed, patient brought imaging results, etc.)"
                              rows={3}
                            />
                          )}
                        </div>
                      </div>
                    )}

                    {/* Generate Button */}
                    {!soapResult && !isGeneratingSoap && !soapError && (
                      <button
                        className="btn-generate"
                        onClick={() => handleGenerateSoap()}
                        disabled={!llmConnected || !hasTranscript}
                      >
                        {!hasTranscript ? 'No transcript' : llmConnected ? 'Generate SOAP Note' : 'LLM not connected'}
                      </button>
                    )}

                    {/* Loading State */}
                    {isGeneratingSoap && (
                      <div className="soap-loading">
                        <div className="spinner-small" />
                        <span>Generating SOAP note...</span>
                      </div>
                    )}

                    {/* Error State */}
                    {soapError && (
                      <div className="soap-error">
                        <span>{soapError}</span>
                        <button className="btn-retry-small" onClick={() => handleGenerateSoap()}>
                          Retry
                        </button>
                      </div>
                    )}

                    {/* SOAP Display */}
                    {soapResult && activeSoapContent && (
                      <div className="soap-display">
                        <div className="soap-header">
                          <span className="soap-timestamp">
                            {soapResult.model_used !== 'archived'
                              ? `Generated ${formatLocalDateTime(soapResult.generated_at)}`
                              : 'Previously generated'}
                          </span>
                          <div className="soap-actions">
                            <button
                              className={`btn-small copy-btn ${copySuccess === 'soap' ? 'success' : ''}`}
                              onClick={() => handleCopy(activeSoapContent, 'soap')}
                            >
                              {copySuccess === 'soap' ? 'Copied!' : 'Copy'}
                            </button>
                            <div className="soap-regen-group" style={{ position: 'relative' }}>
                              <button
                                className="btn-small"
                                onClick={() => handleGenerateSoap()}
                                disabled={isGeneratingSoap || !llmConnected}
                              >
                                Regenerate
                              </button>
                              <button
                                className="btn-small soap-model-toggle"
                                onClick={() => setShowModelMenu(prev => !prev)}
                                disabled={isGeneratingSoap || !llmConnected}
                                title="Choose model for regeneration"
                              >
                                &#9662;
                              </button>
                              {showModelMenu && (
                                <div className="soap-model-menu">
                                  <button onClick={() => { setShowModelMenu(false); handleGenerateSoap(); }}>
                                    Default (soap-model-fast)
                                  </button>
                                  <button onClick={() => { setShowModelMenu(false); handleGenerateSoap('soap-alt'); }}>
                                    Alt Model (soap-alt)
                                  </button>
                                  <button onClick={() => { setShowModelMenu(false); handleGenerateSoap('soap-alt-2'); }}>
                                    Alt Model 2 (soap-alt-2)
                                  </button>
                                </div>
                              )}
                            </div>
                          </div>
                        </div>

                        {/* Multi-patient info and tabs */}
                        {isMultiPatient && selectedPatientIndex === null && (
                          <div className="multi-patient-soap">
                            <div className="patient-info">
                              <span className="physician-label">
                                Physician: {soapResult.physician_speaker || 'Not identified'}
                              </span>
                              <span className="patient-count">
                                {soapResult.notes.length} patients detected
                              </span>
                            </div>
                            <div className="patient-tabs">
                              {soapResult.notes.map((note, i) => (
                                <button
                                  key={i}
                                  className={`patient-tab ${activePatient === i ? 'active' : ''}`}
                                  onClick={() => setActivePatient(i)}
                                >
                                  {note.patient_label}
                                  <span className="speaker-id">({note.speaker_id})</span>
                                </button>
                              ))}
                            </div>
                            {/* Backfill: legacy multi-patient sessions (no sibling_group_id)
                                can be split into separate sessions, each independently billable. */}
                            {!selectedSession?.metadata.sibling_group_id && (
                              <div className="multi-patient-migrate">
                                <button
                                  className="btn-small btn-primary"
                                  disabled={migratingMultiPatient}
                                  onClick={async () => {
                                    if (!selectedSession || !selectedSessionId) return;
                                    const n = soapResult.notes.length;
                                    const ok = window.confirm(
                                      `Split this session into ${n} separate sessions, one per patient?\n\n` +
                                      `Each new session will have its own SOAP and billing record. ` +
                                      `The current combined billing will be replaced with per-patient billing ` +
                                      `(this may take ~${n * 15} seconds while billing re-extracts).\n\n` +
                                      `This cannot be undone.`
                                    );
                                    if (!ok) return;
                                    setMigratingMultiPatient(true);
                                    try {
                                      await invoke<string[]>('migrate_legacy_multipatient_session', {
                                        sessionId: selectedSessionId,
                                        date: formatDateForApi(selectedDate),
                                      });
                                      setSelectedSession(null);
                                      setSelectedSessionId(null);
                                      setSelectedPatientIndex(null);
                                      await fetchSessions();
                                    } catch (e) {
                                      const msg = e instanceof Error ? e.message : String(e);
                                      window.alert(`Migration failed: ${msg}`);
                                    } finally {
                                      setMigratingMultiPatient(false);
                                    }
                                  }}
                                  title="Each patient becomes its own session in the sidebar with its own billing record"
                                >
                                  {migratingMultiPatient
                                    ? 'Splitting…'
                                    : `Split into ${soapResult.notes.length} separate sessions`}
                                </button>
                              </div>
                            )}
                          </div>
                        )}
                        {isMultiPatient && selectedPatientIndex !== null && (
                          <div className="multi-patient-soap">
                            <div className="patient-info">
                              <span className="physician-label">
                                Physician: {soapResult.physician_speaker || 'Not identified'}
                              </span>
                              <span className="patient-count">
                                Patient {selectedPatientIndex + 1} of {soapResult.notes.length}
                              </span>
                            </div>
                          </div>
                        )}

                        <div className="soap-content">
                          <pre className="soap-text-content">{activeSoapContent}</pre>
                        </div>

                        {soapResult.model_used !== 'archived' && (
                          <div className="soap-meta">
                            <span className="soap-model">Model: {soapResult.model_used}</span>
                          </div>
                        )}

                        {/* Session Clinical Feedback */}
                        {selectedSession.transcript && (
                          <FeedbackSection
                            title="Session Feedback"
                            systemPrompt={SESSION_FEEDBACK_PROMPT}
                            cacheKey={selectedSession.session_id}
                            llmConnected={llmConnected}
                            disabled={!selectedSession.transcript.trim()}
                            getTranscript={() => Promise.resolve(selectedSession.transcript!)}
                          />
                        )}

                        {dataSource === 'local' && (
                          <FeedbackPanel
                            sessionId={selectedSession.session_id}
                            date={formatDateForApi(selectedDate)}
                            feedback={feedback}
                            onFeedbackChange={setFeedback}
                            onFeedbackSaved={handleSelectedSessionFeedbackSaved}
                            isMultiPatient={isMultiPatient}
                            activePatient={activePatient}
                            patientCount={soapResult.notes.length}
                            isContinuousMode={selectedSession.metadata.charting_mode === 'continuous'}
                          />
                        )}
                      </div>
                    )}
                  </div>
                )}

                {/* Handout Tab */}
                {activeTab === 'handout' && (
                  <div className="tab-panel handout-panel">
                    {handoutLoading ? (
                      <div className="loading-text">Loading handout...</div>
                    ) : handoutContent ? (
                      <>
                        <div className="handout-content" style={{ whiteSpace: 'pre-wrap', fontFamily: 'Georgia, serif', lineHeight: 1.6 }}>
                          {handoutContent}
                        </div>
                        <div className="handout-actions" style={{ marginTop: 8, display: 'flex', gap: 8 }}>
                          <button
                            className="btn-small"
                            onClick={async () => {
                              await writeText(handoutContent);
                              setCopySuccess('handout');
                              setTimeout(() => setCopySuccess(null), 1500);
                            }}
                          >
                            {copySuccess === 'handout' ? 'Copied!' : 'Copy'}
                          </button>
                        </div>
                      </>
                    ) : (
                      <div className="empty-state">No handout available</div>
                    )}
                  </div>
                )}

                {/* Billing Tab */}
                {activeTab === 'billing' && (
                  <BillingTab
                    record={billingRecord}
                    loading={billingLoading}
                    sessionId={selectedSession?.session_id || ''}
                    date={formatDateForApi(selectedDate)}
                    patientDob={selectedSession?.metadata?.patient_dob ?? null}
                    onRecordChange={setBillingRecord}
                    defaultVisitSetting={billingDefaults.visitSetting}
                    defaultCounsellingExhausted={billingDefaults.counsellingExhausted}
                    defaultIsHospital={billingDefaults.isHospital}
                    onFeedbackSaved={handleSelectedSessionFeedbackSaved}
                  />
                )}

                {/* Insights Tab */}
                {activeTab === 'insights' && (
                  <div className="tab-panel insights-panel">
                    {/* Session Info */}
                    <div className="insight-card">
                      <div className="insight-card-header">Session Info</div>
                      <div className="insight-card-body">
                        <div className="insight-metric">
                          <span className="metric-label">Started</span>
                          <span className="metric-value">{formatLocalDateTime(selectedSession.metadata.started_at)}</span>
                        </div>
                        {selectedSession.metadata.ended_at && (
                          <div className="insight-metric">
                            <span className="metric-label">Ended</span>
                            <span className="metric-value">{formatLocalDateTime(selectedSession.metadata.ended_at)}</span>
                          </div>
                        )}
                        <div className="insight-metric">
                          <span className="metric-label">Duration</span>
                          <span className="metric-value">{formatDurationShort(selectedSession.metadata.duration_ms)}</span>
                        </div>
                        <div className="insight-metric">
                          <span className="metric-label">Words</span>
                          <span className="metric-value">{selectedSession.metadata.word_count}</span>
                        </div>
                      </div>
                    </div>

                    {/* Storage Info */}
                    <div className="insight-card">
                      <div className="insight-card-header">Storage</div>
                      <div className="insight-card-body">
                        <div className="insight-metric">
                          <span className="metric-label">Transcript</span>
                          <span className="metric-value">{selectedSession.transcript ? '✓ Saved' : '✗ None'}</span>
                        </div>
                        <div className="insight-metric">
                          <span className="metric-label">SOAP Note</span>
                          <span className="metric-value">{selectedSession.metadata.has_soap_note ? '✓ Saved' : '✗ None'}</span>
                        </div>
                        <div className="insight-metric">
                          <span className="metric-label">Audio</span>
                          <span className="metric-value">{selectedSession.audio_path ? '✓ Saved' : '✗ None'}</span>
                        </div>
                      </div>
                    </div>

                    {/* Auto-end Info */}
                    {selectedSession.metadata.auto_ended && (
                      <div className="insight-card">
                        <div className="insight-card-header">Auto-End</div>
                        <div className="insight-card-body">
                          <div className="insight-metric">
                            <span className="metric-label">Reason</span>
                            <span className="metric-value">{selectedSession.metadata.auto_end_reason || 'Silence detected'}</span>
                          </div>
                        </div>
                      </div>
                    )}

                    {/* Audio Player */}
                    {selectedSession.audio_path && (
                      <div className="insight-card">
                        <div className="insight-card-header">Audio Recording</div>
                        <div className="insight-card-body audio-player-container">
                          <AudioPlayer
                            audioUrl={
                              dataSource === 'medplum'
                                ? selectedSession.audio_path // Medplum provides a URL
                                : `file://${selectedSession.audio_path}` // Local file path
                            }
                          />
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          ) : (
            <div className="detail-empty-state">
              <span className="empty-icon">&#128203;</span>
              <span>Select a session to view details</span>
            </div>
          )}
        </div>
      </div>

      {/* Cleanup dialogs */}
      {activeDialog === 'delete' && (
        <DeleteConfirmDialog
          sessions={getSelectedSessions()}
          onConfirm={handleDeleteConfirm}
          onCancel={() => setActiveDialog('none')}
        />
      )}
      {activeDialog === 'merge' && (
        <MergeConfirmDialog
          sessions={getSelectedSessions()}
          onConfirm={handleMergeConfirm}
          onCancel={() => setActiveDialog('none')}
          isSameSessionPatientMerge={isSameSessionPatientMerge}
          selectedPatientNames={
            isSameSessionPatientMerge
              ? Array.from(selectedIds).map(k => {
                  const pIdx = patientIndexFromKey(k);
                  const sid = sessionIdFromKey(k);
                  const session = sessions.find(s => s.session_id === sid);
                  return pIdx !== null && session?.patient_labels?.[pIdx]
                    ? session.patient_labels[pIdx]
                    : `Patient ${(pIdx ?? 0) + 1}`;
                })
              : undefined
          }
          onPatientMergeConfirm={handlePatientMergeConfirm}
        />
      )}
      {activeDialog === 'editName' && selectedIds.size === 1 && (
        <EditNameDialog
          currentName={getSelectedSessions()[0]?.patient_name ?? null}
          onConfirm={handleEditNameConfirm}
          onCancel={() => setActiveDialog('none')}
        />
      )}
      {activeDialog === 'confirmPatient' && confirmPatientSessions && (
        <ConfirmPatientsBatchDialog
          sessions={confirmPatientSessions}
          date={formatDateForApi(selectedDate)}
          onConfirmed={async () => {
            setActiveDialog('none');
            setConfirmPatientSessions(null);
            await afterCleanupOp(
              confirmPatientSessions.length === 1
                ? 'Patient confirmed and synced to EMR'
                : `${confirmPatientSessions.length} patients confirmed and synced to EMR`,
            );
          }}
          onCancel={() => {
            setActiveDialog('none');
            setConfirmPatientSessions(null);
          }}
        />
      )}

      {/* Day Feedback Modal */}
      {showDayFeedback && (
        <div className="day-feedback-overlay" onClick={() => setShowDayFeedback(false)}>
          <div className="day-feedback-modal" onClick={e => e.stopPropagation()}>
            <div className="day-feedback-modal-header">
              <h3>Day Feedback</h3>
              <button className="day-feedback-close" onClick={() => setShowDayFeedback(false)}>&times;</button>
            </div>
            <div className="day-feedback-modal-body">
              {dayFeedbackLoading && (
                <div className="day-feedback-modal-loading">Analyzing {sessions.length} sessions...</div>
              )}
              {dayFeedbackError && (
                <div className="day-feedback-modal-error">
                  {dayFeedbackError}
                  <button onClick={() => { setDayFeedbackText(null); setDayFeedbackError(null); }}>Retry</button>
                </div>
              )}
              {dayFeedbackText && (
                <div className="day-feedback-modal-text">{renderFeedbackWithLinks(dayFeedbackText)}</div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default HistoryWindow;

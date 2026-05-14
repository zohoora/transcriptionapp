import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { formatErrorMessage } from '../utils';
import { useClinicalChat } from '../hooks/useClinicalChat';
import { useMedicationAssessment } from '../hooks/useMedicationAssessment';
import ClinicalChatTab from './ClinicalChatTab';
import MedicationAssessment from './MedicationAssessment';
import { Sidebar } from './clinicalAssistant/Sidebar';
import type { Settings } from '../types';

type Tab = 'chat' | 'meds';

/**
 * Standalone Clinical Assistant window.
 *
 * Layout: header on top, then a two-pane body — a persistent left sidebar
 * (patient identity, medications, patient context, allergies placeholder)
 * and a right tabs pane (Chat, Medication Assessment, future tabs).
 *
 * Tabs read from the sidebar via the shared `useMedicationAssessment` hook,
 * which holds the med list + vision-extracted patient identity + patient
 * context. The Chat tab's LLM call still receives the med list as system
 * context; the sidebar makes the attached context visible.
 */
const ClinicalAssistantWindow: React.FC = () => {
  const [activeTab, setActiveTab] = useState<Tab>('chat');
  const [settings, setSettings] = useState<Settings | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<Settings>('get_settings')
      .then((s) => {
        if (!cancelled) setSettings(s);
      })
      .catch((e) => {
        if (!cancelled) setSettingsError(formatErrorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const med = useMedicationAssessment();

  const chat = useClinicalChat(
    settings?.llm_router_url ?? '',
    settings?.llm_api_key ?? '',
    settings?.llm_client_id ?? '',
    med.medList
  );

  // Auto-extract on window mount so the extracted med list is available
  // as system context for the chat tab (the default tab) as soon as the
  // clinician asks their first question. The Re-extract button in the
  // Meds tab still triggers a fresh capture if the chart has changed.
  const didAutoExtractRef = useRef(false);
  useEffect(() => {
    if (didAutoExtractRef.current) return;
    if (med.extractionState !== 'idle') return;
    didAutoExtractRef.current = true;
    void med.extract();
  }, [med.extractionState, med.extract]);

  const handleClose = useCallback(async () => {
    try {
      await getCurrentWindow().close();
    } catch {
      // No-op
    }
  }, []);

  if (settingsError) {
    return (
      <div className="clinical-assistant-window">
        <div className="ca-error">
          Couldn't load app settings: {settingsError}
          <button className="ca-error-action" onClick={handleClose}>
            Close
          </button>
        </div>
      </div>
    );
  }

  if (!settings) {
    return (
      <div className="clinical-assistant-window">
        <div className="ca-loading">
          <div className="spinner-small" />
          <span>Loading...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="clinical-assistant-window">
      <header className="ca-header">
        <h1>Clinical Assistant</h1>
        <button className="ca-header-close" onClick={handleClose} aria-label="Close window">
          ✕
        </button>
      </header>

      <div className="ca-body">
        <Sidebar med={med} />

        <div className="ca-tabs-pane">
          <nav className="ca-tabs" role="tablist">
            <button
              role="tab"
              aria-selected={activeTab === 'chat'}
              className={`ca-tab ${activeTab === 'chat' ? 'ca-tab-active' : ''}`}
              onClick={() => setActiveTab('chat')}
            >
              Chat
            </button>
            <button
              role="tab"
              aria-selected={activeTab === 'meds'}
              className={`ca-tab ${activeTab === 'meds' ? 'ca-tab-active' : ''}`}
              onClick={() => setActiveTab('meds')}
            >
              Medication Assessment
              {med.medList.length > 0 && (
                <span className="ca-tab-count">{med.medList.length}</span>
              )}
            </button>
          </nav>

          <main className="ca-tab-content">
            {activeTab === 'chat' && (
              <ClinicalChatTab
                messages={chat.messages}
                isLoading={chat.isLoading}
                error={chat.error}
                onSendMessage={chat.sendMessage}
                onClear={chat.clearChat}
              />
            )}
            {activeTab === 'meds' && <MedicationAssessment med={med} />}
          </main>
        </div>
      </div>
    </div>
  );
};

export default ClinicalAssistantWindow;

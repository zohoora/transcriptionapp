import { useCallback, useRef } from 'react';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

const WINDOW_LABEL = 'clinical-assistant';

/**
 * Open (or focus) the standalone Clinical Assistant window.
 *
 * Mirrors the pattern used by SplitWindow / PatientHandoutEditor: check
 * for an existing window by label, focus if present, else create a new
 * one. The `opening` ref guards against double-click races where two
 * parallel calls both see "no window" and both try to create.
 *
 * Window dimensions (1000×750) accommodate the two-pane layout — a 300px
 * left sidebar plus the right tabs pane. minWidth 850 keeps the right pane
 * usable (~550px) at the smallest allowed size.
 */
export function useClinicalAssistantWindow() {
  const openingRef = useRef(false);

  const openClinicalAssistant = useCallback(async () => {
    if (openingRef.current) return;
    openingRef.current = true;
    try {
      const existing = await WebviewWindow.getByLabel(WINDOW_LABEL).catch(() => null);
      if (existing) {
        await existing.setFocus();
        return;
      }
      new WebviewWindow(WINDOW_LABEL, {
        url: 'clinical-assistant.html',
        title: 'Clinical Assistant',
        width: 1000,
        height: 750,
        minWidth: 850,
        minHeight: 500,
        resizable: true,
      });
    } finally {
      openingRef.current = false;
    }
  }, []);

  return { openClinicalAssistant };
}

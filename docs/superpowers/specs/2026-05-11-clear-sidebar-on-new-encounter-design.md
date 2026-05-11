# Clear sidebar state on new encounter detection (continuous mode)

**Date:** 2026-05-11
**Branch:** `feat/clear-sidebar-on-new-encounter`
**Status:** Approved

## Goal

When continuous mode detects a new encounter (auto LLM/sensor split or manual "New Patient"), clear the per-encounter sidebar state in `ContinuousMode.tsx` so the new patient sees a clean panel:

| Item | Owning hook | Today |
|---|---|---|
| "Pssst…" predictive hint + image-concept text | `usePredictiveHint` | Persists across encounters (only clears when `isRecording=false`) |
| Differential diagnosis (top 3) | `usePredictiveHint` | Persists across encounters |
| MIIS image suggestions | `useMiisImages` | Persists across encounters (cleanup effect only fires when `sessionId === null`) |
| AI-generated images | `useAiImages` | Already clears correctly on `sessionId` change |
| Patient biomarker pulse + voice trends | `usePatientBiomarkers` | Resets only on manual "New Patient" button (via `handleNewPatient` in orchestrator), not on auto-detected splits |

The other per-encounter signals already reset on the `encounter_detected` event in `useContinuousMode.ts:120-128` (`encounterNotes`, `currentPatientName`, `transcriptionStalled`).

## Non-goals

- New backend events or commands. The reset is signal-driven from the existing `encounterSessionId`, which is already regenerated on every `encounter_detected` event (`useContinuousMode.ts:126`) and exposed by the orchestrator.
- "Restoring" pre-split state on a server-side merge-back. When a split is undone server-side, the sidebar's already-cleared state is treated as transient. Out of scope.
- Resetting `liveTranscript`, `stats`, or `recent_encounters` — those are either backend-driven (`liveTranscript`, `stats`) or intentionally cumulative (`recent_encounters`).

## Approach

`encounterSessionId` is the trigger signal — already plumbed through `useContinuousModeOrchestrator` and changes synchronously on every `encounter_detected` event. Each affected hook owns its own "clean up when my key changes" logic, matching the established pattern in `useAiImages.ts:37-41`.

### `usePredictiveHint.ts`

Add an optional `resetKey?: string` to `UsePredictiveHintOptions`.

1. **State clear** — new `useEffect([resetKey])` clears `hint`, `concepts`, `imagePrompt`, `differentialDiagnoses`, `lastUpdated`, and the `lastGeneratedRef` ref. Resetting `lastGeneratedRef` is required so the new encounter's first transcript chunk is not skipped as "unchanged".
2. **Re-arm 5s initial delay** — add `resetKey` to the dep array of the existing `useEffect([isRecording, generateHint], …)`. Cleanup tears down the running 30s interval; the body re-arms `setTimeout(generateHint, INITIAL_DELAY_MS)` (5s) and `setInterval(generateHint, HINT_INTERVAL_MS)` (30s) — so the new encounter gets a fast first hint instead of waiting up to 30s.

When `resetKey` is `undefined` (session/recording mode callers), the effects no-op on initial mount and never fire again.

### `useMiisImages.ts`

Drop the `if (!sessionId)` guard on the cleanup effect (lines 188-195) so it fires on **any** `sessionId` change (matching `useAiImages.ts:37-41`):

```ts
useEffect(() => {
  setSuggestions([]);
  setSuggestionSetId(null);
  lastConceptsRef.current = '';
}, [sessionId]);
```

The fetch effect already guards on `sessionId` and `concepts.length === 0`, so a momentarily-cleared concepts array won't cause a refetch storm.

### `useContinuousModeOrchestrator.ts`

1. Pass `resetKey: encounterSessionId` to `usePredictiveHint`.
2. Add `useEffect(() => { resetPatientBiomarkers(); }, [encounterSessionId, resetPatientBiomarkers])` so auto-detected splits get the same biomarker reset that manual "New Patient" already triggers via `handleNewPatient`.

### `useAiImages.ts`

No change — already correct.

### `ContinuousMode.tsx` / `RecordingMode.tsx`

No change. Both consume `differentialDiagnoses` and `predictiveHint` as props from the orchestrator (continuous) or directly from the hook (recording, where `resetKey` stays `undefined`).

## Tests

All new tests are inline frontend Vitest cases.

### `tauri-app/src/hooks/usePredictiveHint.test.ts` (new file)

- `resetKey change clears hint state while isRecording stays true`
- `resetKey change re-arms the 5s INITIAL_DELAY_MS for the next hint`
- `omitting resetKey preserves existing isRecording-only behavior` (regression guard for session-mode callers)

### `tauri-app/src/hooks/useMiisImages.test.ts` (new file)

- `changing sessionId from one value to another clears suggestions and lastConceptsRef`
- `setting sessionId to null also clears` (regression guard)

### `tauri-app/src/hooks/useContinuousModeOrchestrator.test.ts` (extend)

- `emitting continuous_mode_event encounter_detected resets patient biomarkers`

## Edge cases

- **Initial mount.** `resetKey` is set on first render of `usePredictiveHint`. The state-clear effect fires once; state is already empty → no-op. Same for the orchestrator's biomarker reset.
- **Continuous mode stop → restart.** `useContinuousMode` regenerates `encounterSessionId` on the `started` event (line 101). The reset effects fire once during the restart; the `isRecording=false` branch already cleared state during the stop → no-op.
- **Merge-back.** When the server-side merge engine undoes a split, the new encounter's freshly-cleared sidebar may briefly show its first hint before the merge-back completes. Acceptable — restoring "pre-split" sidebar state is out of scope.
- **Session mode callers** (`RecordingMode.tsx`). Don't pass `resetKey`. The hook's effects no-op on the missing prop; behavior is identical to today.

## Files touched

| File | Change |
|---|---|
| `tauri-app/src/hooks/usePredictiveHint.ts` | Add `resetKey?: string` prop + clearing effect + 5s re-arm |
| `tauri-app/src/hooks/useMiisImages.ts` | Drop `if (!sessionId)` guard on cleanup effect |
| `tauri-app/src/hooks/useContinuousModeOrchestrator.ts` | Pass `resetKey`; add biomarker reset effect |
| `tauri-app/src/hooks/usePredictiveHint.test.ts` | New file (3 tests) |
| `tauri-app/src/hooks/useMiisImages.test.ts` | New file (2 tests) |
| `tauri-app/src/hooks/useContinuousModeOrchestrator.test.ts` | Extend (1 test) |

No backend changes; no settings/config changes; no profile-service changes.

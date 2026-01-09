# ADR-0008: Medplum EMR Integration

## Status

Accepted (Updated January 2025 - Auto-Sync)

## Context

A clinical transcription app needs to integrate with Electronic Medical Record (EMR) systems to:

1. Store transcripts and SOAP notes persistently
2. Associate recordings with patient encounters
3. Enable review of historical sessions
4. Sync audio recordings for compliance/audit purposes

We needed a FHIR-compliant backend that supports:
- OAuth 2.0 authentication (for clinic SSO integration)
- Standard FHIR resources (Patient, Encounter, DocumentReference, Media)
- Self-hosted deployment option (for data sovereignty)

## Decision

Integrate with **Medplum** as the EMR backend using OAuth 2.0 + PKCE flow.

### Authentication
- Use `fabricscribe://oauth/callback` deep link for OAuth redirect
- PKCE flow for security (no client secret stored in app)
- Session persistence via `~/.transcriptionapp/medplum_auth.json`
- Auto-refresh tokens before expiration

### FHIR Resources

| Resource | Purpose |
|----------|---------|
| `Practitioner` | Authenticated user (from OAuth) |
| `Patient` | Patient lookup/selection |
| `Encounter` | Recording session container |
| `DocumentReference` | Transcript and SOAP note documents |
| `Media` | Audio recording reference |
| `Binary` | Actual audio file (WAV) |

### Tagging Convention
- All app-created encounters tagged with `urn:fabricscribe|scribe-session`
- Enables filtering app encounters from other clinical encounters

### Multi-window Architecture
- Main sidebar: recording workflow
- History window: separate Tauri webview for browsing past encounters
- IPC communication between windows

### Timestamp Handling
- Store UTC in FHIR resources (`Utc::now().to_rfc3339()`)
- Display local timezone to user
- Date queries account for timezone boundaries

### Auto-Sync on Session Complete (January 2025)

**Problem**: Users might complete a recording session but forget to sync to Medplum, losing the transcript and audio. SOAP note generation happens after the session completes, so waiting for SOAP before sync risks data loss.

**Decision**: Auto-sync transcript + audio immediately when session completes (if user is authenticated and auto-sync enabled). Add SOAP note to the existing encounter when generated later.

**Implementation**:
1. `SyncResult` returns `encounterId` and `encounterFhirId` for tracking
2. Frontend stores synced encounter state in `useMedplumSync` hook
3. `useEffect` triggers sync when session state becomes `completed`
4. New `medplum_add_soap_to_encounter` command adds DocumentReference to existing encounter
5. SOAP generation automatically updates the synced encounter

**Alternative Considered**: Wait for SOAP before syncing
- Rejected because: user might never generate SOAP, losing data
- Auto-sync ensures data preservation even without SOAP

## Consequences

### Positive

- FHIR-compliant data storage (interoperable with other systems)
- OAuth SSO enables enterprise deployment
- Self-hosted Medplum option for data sovereignty
- Standard resources enable future integrations (lab results, medications)
- Separate history window doesn't block recording workflow
- **Auto-sync prevents data loss** - transcript/audio preserved even if user forgets to sync
- **SOAP updates existing encounter** - no duplicate encounters when generating SOAP later

### Negative

- Requires Medplum server deployment (additional infrastructure)
- OAuth flow complexity (deep links, PKCE, token refresh)
- FHIR learning curve for developers unfamiliar with healthcare standards
- Multi-window adds complexity vs. single-page app
- **Auto-sync creates encounters without SOAP** - may need cleanup if SOAP never generated
- **Encounter tracking state** - frontend must track synced encounter IDs for updates

## References

- [Medplum Documentation](https://www.medplum.com/docs)
- [FHIR Encounter Resource](https://hl7.org/fhir/encounter.html)
- [OAuth 2.0 PKCE](https://oauth.net/2/pkce/)
- [Tauri Deep Links](https://tauri.app/v1/guides/features/deep-linking/)

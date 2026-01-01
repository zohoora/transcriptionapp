# ADR-0008: Medplum EMR Integration

## Status

Accepted

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

## Consequences

### Positive

- FHIR-compliant data storage (interoperable with other systems)
- OAuth SSO enables enterprise deployment
- Self-hosted Medplum option for data sovereignty
- Standard resources enable future integrations (lab results, medications)
- Separate history window doesn't block recording workflow

### Negative

- Requires Medplum server deployment (additional infrastructure)
- OAuth flow complexity (deep links, PKCE, token refresh)
- FHIR learning curve for developers unfamiliar with healthcare standards
- Multi-window adds complexity vs. single-page app

## References

- [Medplum Documentation](https://www.medplum.com/docs)
- [FHIR Encounter Resource](https://hl7.org/fhir/encounter.html)
- [OAuth 2.0 PKCE](https://oauth.net/2/pkce/)
- [Tauri Deep Links](https://tauri.app/v1/guides/features/deep-linking/)

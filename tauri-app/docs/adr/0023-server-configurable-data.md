# ADR-0023: Server-Configurable Data

## Status

Accepted (Apr 2026). **Phase 2 threshold wiring landed** after the original Phase 1 ship — see "Implementation status" below.

## Context

Three kinds of operational data change faster than our release cadence:

1. **LLM prompt templates** — encounter detection wording, clinical content check, multi-patient check/detect/split, SOAP prompts. When we discover a better phrasing (through forensic review of a production day), we want to deploy it to all rooms today without cutting a release.

2. **Billing rules** — OHIP Schedule of Benefits updates roughly yearly, with correction bulletins in between. Fee amounts, new codes, and cap changes should not require an app rebuild.

3. **Detection thresholds** — force-split word caps, confidence gates, sensor accelerate windows, etc. Tuning these requires observing production behavior; a server-side knob is much faster than a release.

Constraints:

- **Fallback must be compiled-in defaults** — the clinic must keep working if the profile service is unreachable at startup (network partition, server restart, etc.).
- **No PHI in config pulls** — the config endpoints return generic operational data; no patient information flows through them.
- **No restart required** — config refreshes on startup and when the server announces a version bump.

## Decision

Implement a **three-tier fallback** for all server-configurable data:

```
Tier 1: Server fetch   (profile service /config/*, on startup + version bump)
    ↓ unreachable / error
Tier 2: Local cache    (~/.transcriptionapp/server_config_cache.json, last-good snapshot)
    ↓ missing / corrupt
Tier 3: Compiled default (the `unwrap_or_else(|| "..." .to_string())` path in every builder)
```

### Data shape

Three typed structs live in `profile-service/src/types.rs` (mirrored in tauri's `server_config.rs`):

| Struct | Overrides | Enforcement |
|--------|-----------|-------------|
| `PromptTemplates` | All LLM prompt builder defaults | Every builder accepts `Option<&PromptTemplates>` and uses `unwrap_or_else` for the default |
| `BillingData` | OHIP codes, companion mappings, rule engine tables | Rule engine accepts `Option<&BillingData>` |
| `DetectionThresholds` | `ABSOLUTE_WORD_CAP`, `FORCE_SPLIT_WORD_THRESHOLD`, force-split consecutive limit, confidence gates, `MIN_WORDS_FOR_CLINICAL_CHECK`, `MULTI_PATIENT_CHECK_WORD_THRESHOLD`, SOAP + billing timeouts | Wired into `DetectionEvalContext.server_thresholds` (evaluate_detection) and into runtime branches in `continuous_mode.rs` via primitive captures; `check_clinical_content`, `generate_and_archive_soap`, `extract_and_archive_billing` accept `Option<usize/u64>` threshold overrides |

Each struct has a **version counter**. A shared `config_version.json` increments every time any of the three is updated. Clients fetch `GET /config/version` cheaply and only pull the body when the version differs from their cached value.

### Server routes

| Route | Purpose |
|-------|---------|
| `GET /config/version` | Return the current shared version number (cheap) |
| `GET /config/prompts` / `PUT /config/prompts` | Full-replace semantics — send the complete `PromptTemplates` body |
| `GET /config/billing` / `PUT /config/billing` | Full-replace semantics |
| `GET /config/thresholds` / `PUT /config/thresholds` | Full-replace semantics |

Full-replace was chosen over PATCH because the structs are small, an atomic swap is simpler than merging partial updates, and admins editing the JSON directly are less likely to create broken intermediate states.

### Tauri-side wiring

`SharedServerConfig` is an `Arc<RwLock<ServerConfig>>` held in Tauri managed state. A background task polls `/config/version` every 60s; on a bump it re-fetches all three sections and replaces the `Arc` contents. Every prompt-builder, rule-engine entry point, and detection evaluator accepts `Option<&T>` for the server override and transparently falls through to the compiled default when `None`.

### Empty-string sentinel

For `PromptTemplates`, an *empty string* in a field means "use the compiled default" — allowing the server to override some fields but keep others. This means `PromptTemplates::default()` on the server intentionally serializes all-empty strings, letting every prompt fall through to the tauri compiled default until explicitly overridden.

### Caching behavior

- On successful fetch: write to `~/.transcriptionapp/server_config_cache.json` via atomic rename.
- On startup: attempt `GET /config/version`; on failure, read the cache.
- On cache miss: fall through to compiled defaults.
- The cache is never consulted after a successful fetch — the in-memory `Arc` is authoritative once loaded.

## Consequences

### Positive

- **Zero-downtime rule updates** — SOB changes or prompt tweaks go live without a release.
- **Offline-tolerant** — clinic keeps working during server outages; worst case it runs on the last-good config or the baked-in defaults.
- **Auditable** — the server has a single JSON file per config type that version-controls cleanly.
- **Testable** — unit tests can pass a synthetic `PromptTemplates` / `BillingData` / `DetectionThresholds` to any builder without touching the network.

### Negative

- **Drift risk between clients**: if three rooms pull at different times around a config push, they'll briefly disagree. Acceptable because updates are rare and the window is ≤60s.
- **No per-room or per-physician overrides in Phase 1** — the config is clinic-wide. Physician-specific prompt tuning is a potential Phase 2 feature.
- **Empty-string sentinel can be confusing** — "empty string means default" is a convention that needs documenting wherever `PromptTemplates` is serialized.

### Implementation status

**Phase 1 — original ship** (prompts + billing rules wired into prompt builders and rule engine).

**Phase 2 — threshold wiring** (landed after Phase 1). `DetectionThresholds` now flows from the server all the way to the runtime decisions:

- `DetectionEvalContext.server_thresholds` is populated at `continuous_mode.rs` by snapshotting `SharedServerConfig.thresholds` into an `Arc<DetectionThresholds>` at continuous-mode start. `evaluate_detection()` already consumed this Option — it now sees real values instead of `None`.
- Primitive threshold values (`force_check_word_threshold`, `min_words_for_clinical_check`, `multi_patient_check_word_threshold`, SOAP + billing timeouts) are captured as locals after the Arc snapshot and referenced directly in branches outside `evaluate_detection`: the pre-detection force-check gate, clinical-content gating, multi-patient safety net, and the flush-on-stop path.
- `check_clinical_content`, `generate_and_archive_soap`, and `extract_and_archive_billing` gained `min_words_override`/`soap_timeout_override`/`billing_timeout_override` parameters. Production call-sites in `continuous_mode.rs` pass `Some(...)`; recovery paths (orphaned SOAP, orphaned billing, merge-regen) pass `None` and fall back to the compiled defaults.
- Snapshot cadence: thresholds are captured **once per continuous-mode start**, same as prompts and billing data. A server-side `PUT /config/thresholds` takes effect the next time continuous mode is started (either by the physician or by sleep-mode auto-restart at 6 AM).

**Not wired yet** (follow-up):

- `SCREENSHOT_STALE_GRACE_SECS` in `screenshot_task.rs` — still a compiled constant. Low priority; this threshold is rarely tuned.
- `MULTI_PATIENT_DETECT_WORD_THRESHOLD` — not in the server struct at all (semantically distinct from `multi_patient_split_min_words`). Would require a new field.
- Per-physician prompt overrides — single clinic-wide config today.
- History view for config changes (diff against previous version).
- PATCH semantics for targeted field updates (currently full-replace).

## References

- `profile-service/src/store/config_data.rs` — server-side store
- `profile-service/src/routes/config_data.rs` — 7 route handlers
- `tauri-app/src-tauri/src/server_config.rs` — client-side `SharedServerConfig` + polling
- `profile-service/CLAUDE.md` → "Common Tasks" → "Add/update prompt template"
- Root `CLAUDE.md` → "Server-Configurable Data (Phase 1)"

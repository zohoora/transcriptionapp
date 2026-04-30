//! Shared `--replay-only` helper for the experiment CLIs.
//!
//! Both `soap_experiment_cli` and `billing_experiment_cli` need to read
//! archived `response_raw` from `replay_bundle.json` (schema v5+) without
//! issuing live LLM calls. The fetch + parse + JSON-pointer-extract logic
//! is identical except for the pointer; this module is the single source
//! of truth so the two CLIs can't drift on date parsing, fetcher
//! configuration, or pointer typo handling.

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, TimeZone, Utc};

use crate::replay_fetch::ArchiveFetcher;

/// Fetch `replay_bundle.json` for `(session_id, date)` from local archive
/// or profile service, then read `response_raw` at the given JSON pointer.
///
/// Returns the raw LLM response string. Errors when:
///   - `date` doesn't parse as `YYYY-MM-DD`
///   - The session has no `replay_bundle.json` (older sessions)
///   - The bundle's JSON is malformed
///   - The pointer doesn't resolve to a string (older schema)
///
/// Multi-patient bundles join per-patient raws with `\n---\n`; callers
/// that only want the first block can `.split("\n---\n").next()`. SOAP
/// experiment CLI does this; billing experiment CLI doesn't need to (a
/// session has at most one billing extraction).
pub async fn replay_response(
    fetcher: &ArchiveFetcher,
    session_id: &str,
    date: &str,
    pointer: &str,
) -> Result<String> {
    let parsed_date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| anyhow!("date parse: {e}"))?;
    let dt = parsed_date
        .and_hms_opt(12, 0, 0)
        .map(|naive| Utc.from_utc_datetime(&naive))
        .ok_or_else(|| anyhow!("date conversion failed"))?;

    let bytes = fetcher
        .fetch_replay_bundle_raw(session_id, &dt)
        .await
        .map_err(|e| anyhow!("replay_bundle fetch: {e}"))?
        .ok_or_else(|| anyhow!(
            "session {session_id} has no replay_bundle.json — replay-only mode requires it"
        ))?;
    let bundle: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| anyhow!("replay_bundle parse: {e}"))?;
    let raw = bundle
        .pointer(pointer)
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!(
            "session {session_id} replay_bundle has no {pointer} (older schema?)"
        ))?;
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_bad_date_format() {
        let f = ArchiveFetcher::local_only();
        let err = replay_response(&f, "sid", "not-a-date", "/x").await.unwrap_err();
        assert!(err.to_string().contains("date parse"), "got: {err}");
    }

    #[tokio::test]
    async fn returns_error_when_bundle_missing() {
        let f = ArchiveFetcher::local_only();
        // Local-only fetcher with a UUID that won't exist anywhere → returns None bundle.
        let err = replay_response(&f, "00000000-0000-0000-0000-000000000000", "2026-04-29", "/x")
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("has no replay_bundle.json")
                || err.to_string().contains("replay_bundle fetch"),
            "got: {err}"
        );
    }
}

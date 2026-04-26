//! Local-then-server archive fetcher for replay CLIs (v0.10.62+).
//!
//! Background: the History Window in `commands/archive.rs` falls back to
//! profile-service when a session isn't found in the local archive. The replay
//! CLIs (`labeled_regression_cli`, `detection_replay_cli`, etc.) historically
//! only read local archive — so on a multi-room clinic, a CLI run on Room 6
//! sees only Room 6's sessions. This module gives every CLI the same
//! local-then-server fallback the UI has.
//!
//! Reads `~/.transcriptionapp/room_config.json` to discover the profile
//! service URL + active physician ID. If either is missing, the fetcher
//! degrades to local-only (same behavior as before).

use std::path::PathBuf;
use anyhow::{anyhow, Result};

use crate::local_archive::{self, ArchiveDetails, ArchiveSummary};
use crate::profile_client::ProfileClient;
use crate::room_config::RoomConfig;

/// Resolves session data by trying the local archive first and falling back
/// to profile-service when local doesn't have it. Construct once per CLI run
/// via [`ArchiveFetcher::from_env`].
pub struct ArchiveFetcher {
    physician_id: Option<String>,
    profile_client: Option<ProfileClient>,
}

impl ArchiveFetcher {
    /// Construct from `~/.transcriptionapp/room_config.json`. Returns a
    /// fetcher that degrades to local-only when no `active_physician_id` /
    /// `profile_server_url` is configured (common in test environments).
    pub fn from_env() -> Result<Self> {
        let cfg = RoomConfig::load().map_err(|e| anyhow!("room_config load: {e}"))?;
        let Some(cfg) = cfg else {
            return Ok(Self { physician_id: None, profile_client: None });
        };
        let physician_id = cfg.active_physician_id.clone();
        let urls = cfg.all_server_urls();
        // Profile API key isn't stored in room_config; pass None — profile-service
        // currently doesn't enforce auth on archive read endpoints.
        let profile_client = if !urls.is_empty() {
            Some(ProfileClient::new(&urls, None))
        } else {
            None
        };
        Ok(Self { physician_id, profile_client })
    }

    /// Local-only fetcher for tests + offline use.
    pub fn local_only() -> Self {
        Self { physician_id: None, profile_client: None }
    }

    /// Fetch full session details (metadata + transcript + SOAP). Tries local
    /// first, falls back to server. Returns `Err` only when both miss.
    pub async fn fetch_session(&self, session_id: &str, date: &str) -> Result<ArchiveDetails> {
        // Local first
        if let Ok(details) = local_archive::get_session(session_id, date) {
            return Ok(details);
        }
        // Server fallback
        let (Some(client), Some(phys)) = (&self.profile_client, &self.physician_id) else {
            return Err(anyhow!("Session not found locally and no profile server configured"));
        };
        client.get_session(phys, session_id).await.map_err(|e| {
            anyhow!("Session not found locally; server fetch also failed: {e}")
        })
    }

    /// Fetch billing.json. Returns `Ok(None)` when the session has no billing
    /// (either locally or on server). Returns `Err` only on transport / parse
    /// failures.
    pub async fn fetch_billing(
        &self,
        session_id: &str,
        date: &chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<crate::billing::BillingRecord>> {
        // Local first
        if let Ok(opt) = local_archive::get_billing_record(session_id, date) {
            if opt.is_some() {
                return Ok(opt);
            }
        }
        // Server fallback
        let (Some(client), Some(phys)) = (&self.profile_client, &self.physician_id) else {
            return Ok(None);
        };
        client
            .download_billing_record(phys, session_id)
            .await
            .map_err(|e| anyhow!("Server billing fetch: {e}"))
    }

    /// Fetch raw replay_bundle.json bytes (returns `Ok(None)` when neither
    /// local nor server has it). Caller deserializes — schema is internal.
    pub async fn fetch_replay_bundle_raw(
        &self,
        session_id: &str,
        date: &chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<Vec<u8>>> {
        // Local first
        if let Ok(dir) = local_archive::get_session_archive_dir(session_id, date) {
            let path: PathBuf = dir.join("replay_bundle.json");
            if path.exists() {
                let bytes = std::fs::read(&path)
                    .map_err(|e| anyhow!("Read local replay_bundle.json: {e}"))?;
                return Ok(Some(bytes));
            }
        }
        // Server fallback
        let (Some(client), Some(phys)) = (&self.profile_client, &self.physician_id) else {
            return Ok(None);
        };
        client
            .download_session_file(phys, session_id, "replay_bundle.json")
            .await
            .map_err(|e| anyhow!("Server replay_bundle fetch: {e}"))
    }

    /// List all sessions for a given date. Tries local first, falls back to
    /// profile-service when local has no entries.
    pub async fn list_sessions_for_date(&self, date: &str) -> Result<Vec<ArchiveSummary>> {
        // Local
        if let Ok(local) = local_archive::list_sessions_by_date(date) {
            if !local.is_empty() {
                return Ok(local);
            }
        }
        // Server fallback
        let (Some(client), Some(phys)) = (&self.profile_client, &self.physician_id) else {
            return Ok(Vec::new());
        };
        client
            .get_sessions_by_date(phys, date)
            .await
            .map_err(|e| anyhow!("Server sessions-for-date fetch: {e}"))
    }

    /// True iff a profile-service connection is configured. CLIs can short-
    /// circuit messaging on local-only mode.
    pub fn has_server(&self) -> bool {
        self.profile_client.is_some() && self.physician_id.is_some()
    }

    /// Parse a `YYYY-MM-DD` string as a UTC midnight `DateTime`. Used by the
    /// per-session bundle iterator below to satisfy `fetch_replay_bundle_raw`'s
    /// `&DateTime<Utc>` arg.
    fn parse_date(date: &str) -> Result<chrono::DateTime<chrono::Utc>> {
        let naive = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|e| anyhow!("invalid date '{date}' (expected YYYY-MM-DD): {e}"))?;
        let dt = naive
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow!("could not build midnight UTC for {date}"))?;
        Ok(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc))
    }

    /// Iterate every session for `date` (local → server-fallback via
    /// `list_sessions_for_date` + `fetch_replay_bundle_raw`) and return parsed
    /// replay bundles. Sessions whose bundle is missing or malformed are
    /// skipped silently — the count reported by the caller will reflect only
    /// usable bundles. Display label is `"<date>/<session_id>"` so output
    /// matches the filesystem-walk style used by the bundle CLIs.
    pub async fn list_replay_bundles_for_date(
        &self,
        date: &str,
    ) -> Result<Vec<(String, crate::replay_bundle::ReplayBundle)>> {
        let parsed_date = Self::parse_date(date)?;
        let sessions = self.list_sessions_for_date(date).await?;
        let mut out = Vec::with_capacity(sessions.len());
        for s in sessions {
            let Ok(Some(bytes)) = self.fetch_replay_bundle_raw(&s.session_id, &parsed_date).await
            else {
                continue;
            };
            let Ok(bundle) = serde_json::from_slice::<crate::replay_bundle::ReplayBundle>(&bytes)
            else {
                continue;
            };
            out.push((format!("{}/{}", date, s.session_id), bundle));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_only_constructor() {
        let f = ArchiveFetcher::local_only();
        assert!(!f.has_server());
    }

    #[tokio::test]
    async fn test_local_only_returns_error_when_session_missing() {
        let f = ArchiveFetcher::local_only();
        let err = f.fetch_session("nonexistent-session-id", "2026-04-25").await;
        assert!(err.is_err(), "expected error from local-only fetcher with no session");
    }

    #[tokio::test]
    async fn test_local_only_fetch_billing_returns_none_for_unknown() {
        let f = ArchiveFetcher::local_only();
        // Use a date in the past that's unlikely to have any session;
        // get_billing_record returns Err for invalid path → we propagate.
        let date = chrono::Utc::now();
        let result = f.fetch_billing("00000000-0000-0000-0000-000000000000", &date).await;
        // Either Ok(None) (graceful path) or Err (path validation) — both acceptable.
        match result {
            Ok(None) => {}
            Ok(Some(_)) => panic!("unexpected billing record for nonexistent session"),
            Err(_) => {}
        }
    }

    #[test]
    fn test_parse_date_valid() {
        let dt = ArchiveFetcher::parse_date("2026-04-24").expect("valid date");
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2026-04-24");
    }

    #[test]
    fn test_parse_date_invalid_format_errs() {
        assert!(ArchiveFetcher::parse_date("04-24-2026").is_err());
        assert!(ArchiveFetcher::parse_date("not-a-date").is_err());
    }

    #[tokio::test]
    async fn test_list_replay_bundles_for_date_local_only_empty() {
        let f = ArchiveFetcher::local_only();
        // Future date with no sessions → empty Vec (not an error).
        let result = f.list_replay_bundles_for_date("2099-01-01").await;
        match result {
            Ok(v) => assert!(v.is_empty()),
            Err(_) => {}
        }
    }

    #[tokio::test]
    async fn test_list_sessions_for_date_local_only_empty_returns_empty_when_no_server() {
        let f = ArchiveFetcher::local_only();
        // A future date that won't have local sessions.
        let result = f.list_sessions_for_date("2099-01-01").await;
        match result {
            Ok(v) => assert!(v.is_empty()),
            Err(_) => {} // acceptable — local archive read may error on missing dir
        }
    }
}

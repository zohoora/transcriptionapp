//! Medplum client_credentials token proxy (v0.10.49+).
//!
//! Profile-service holds the Medplum ClientApplication's `client_id` +
//! `client_secret` in env vars and mints FHIR access tokens on behalf of
//! rooms. Keeps the secret off every workstation.
//!
//! A fresh token is minted on first request and cached in memory until it
//! expires (minus a safety margin). Rooms call `POST /medplum/token` to
//! retrieve a valid bearer token.

use crate::error::ApiError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Env-configured Medplum client credentials. Loaded at startup; `None` if
/// any of the three vars are missing (proxy endpoint returns 503 in that
/// case). Secrets are NEVER serialized or logged.
#[derive(Clone)]
pub struct MedplumAuthConfig {
    pub base_url: String,
    pub client_id: String,
    pub client_secret: String,
}

impl MedplumAuthConfig {
    /// Reads `MEDPLUM_BASE_URL`, `MEDPLUM_CLIENT_ID`, `MEDPLUM_CLIENT_SECRET`.
    /// Returns `None` if any var is missing or empty — the token endpoint
    /// then returns an error indicating the proxy is unconfigured.
    pub fn from_env() -> Option<Self> {
        Self::from_values(
            std::env::var("MEDPLUM_BASE_URL").ok(),
            std::env::var("MEDPLUM_CLIENT_ID").ok(),
            std::env::var("MEDPLUM_CLIENT_SECRET").ok(),
        )
    }

    /// Pure parser, split out so unit tests don't race `std::env`.
    pub fn from_values(
        base_url: Option<String>,
        client_id: Option<String>,
        client_secret: Option<String>,
    ) -> Option<Self> {
        let base_url = base_url?;
        let client_id = client_id?;
        let client_secret = client_secret?;
        if base_url.is_empty() || client_id.is_empty() || client_secret.is_empty() {
            return None;
        }
        Some(Self {
            base_url,
            client_id,
            client_secret,
        })
    }
}

/// Response body for `POST /medplum/token`. Mirrors the FHIR-over-OAuth
/// token shape minus the refresh/id bits we don't need.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    /// Seconds until the token returned here expires.
    pub expires_in: u64,
}

/// In-memory cache entry. Keeps `access_token` + the instant at which it
/// becomes stale (expiry minus refresh safety margin).
#[derive(Clone)]
struct CachedToken {
    access_token: String,
    refresh_at: Instant,
    /// Seconds returned by Medplum — passed back to clients verbatim when
    /// the cached entry is still fresh so they can compute their own expiry.
    expires_in: u64,
    /// Wall-clock snapshot at mint. Used to compute a remaining `expires_in`
    /// when serving a cached token.
    minted_at: Instant,
}

pub struct MedplumAuthProxy {
    pub config: Option<MedplumAuthConfig>,
    /// Mutex guards a single-flight refresh — concurrent callers wait for
    /// one network round-trip rather than racing Medplum.
    cache: Arc<Mutex<Option<CachedToken>>>,
    http: reqwest::Client,
}

/// Safety margin: refresh when the token has < this much life left, so
/// clients never get handed a token about to expire mid-request.
const REFRESH_MARGIN: Duration = Duration::from_secs(60);

impl MedplumAuthProxy {
    pub fn new(config: Option<MedplumAuthConfig>) -> Self {
        Self {
            config,
            cache: Arc::new(Mutex::new(None)),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
        }
    }

    /// Returns a valid Medplum access token, minting one on cache miss or
    /// imminent expiry. Single-flight via the cache mutex.
    pub async fn get_token(&self) -> Result<TokenResponse, ApiError> {
        let Some(cfg) = &self.config else {
            return Err(ApiError::Internal(
                "Medplum proxy not configured — set MEDPLUM_BASE_URL, MEDPLUM_CLIENT_ID, MEDPLUM_CLIENT_SECRET".into(),
            ));
        };

        let mut guard = self.cache.lock().await;
        if let Some(entry) = guard.as_ref() {
            if Instant::now() < entry.refresh_at {
                let elapsed = entry.minted_at.elapsed().as_secs();
                let remaining = entry.expires_in.saturating_sub(elapsed);
                return Ok(TokenResponse {
                    access_token: entry.access_token.clone(),
                    expires_in: remaining,
                });
            }
        }

        let fresh = self.mint_token(cfg).await?;
        let minted_at = Instant::now();
        let lifetime = Duration::from_secs(fresh.expires_in);
        let refresh_at = minted_at + lifetime.saturating_sub(REFRESH_MARGIN);
        *guard = Some(CachedToken {
            access_token: fresh.access_token.clone(),
            refresh_at,
            expires_in: fresh.expires_in,
            minted_at,
        });
        info!(
            event = "medplum_token_minted",
            expires_in = fresh.expires_in,
            "minted fresh Medplum access token"
        );
        Ok(fresh)
    }

    async fn mint_token(&self, cfg: &MedplumAuthConfig) -> Result<TokenResponse, ApiError> {
        let url = format!("{}/oauth2/token", cfg.base_url.trim_end_matches('/'));
        let response = self
            .http
            .post(&url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", cfg.client_id.as_str()),
                ("client_secret", cfg.client_secret.as_str()),
            ])
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "medplum token mint: network error");
                ApiError::Internal(format!("Medplum token mint failed: {}", e))
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let truncated: String = body.chars().take(200).collect();
            return Err(ApiError::Internal(format!(
                "Medplum token mint returned {}: {}",
                status, truncated
            )));
        }
        let parsed: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("Medplum token parse: {}", e)))?;
        let access_token = parsed["access_token"]
            .as_str()
            .ok_or_else(|| ApiError::Internal("Medplum response missing access_token".into()))?
            .to_string();
        let expires_in = parsed["expires_in"].as_u64().unwrap_or(3600);
        Ok(TokenResponse {
            access_token,
            expires_in,
        })
    }

    /// For tests + admin: wipe the cache so the next request re-mints.
    #[allow(dead_code)]
    pub async fn invalidate(&self) {
        *self.cache.lock().await = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Uses from_values() directly so tests don't race on std::env.

    #[test]
    fn config_rejects_any_missing_value() {
        assert!(MedplumAuthConfig::from_values(None, Some("c".into()), Some("s".into())).is_none());
        assert!(MedplumAuthConfig::from_values(Some("u".into()), None, Some("s".into())).is_none());
        assert!(MedplumAuthConfig::from_values(Some("u".into()), Some("c".into()), None).is_none());
    }

    #[test]
    fn config_accepts_all_present() {
        let cfg = MedplumAuthConfig::from_values(
            Some("http://example.test".into()),
            Some("cid".into()),
            Some("csec".into()),
        )
        .expect("cfg present");
        assert_eq!(cfg.base_url, "http://example.test");
        assert_eq!(cfg.client_id, "cid");
        assert_eq!(cfg.client_secret, "csec");
    }

    #[test]
    fn config_rejects_empty_strings() {
        assert!(MedplumAuthConfig::from_values(
            Some("".into()),
            Some("cid".into()),
            Some("csec".into())
        )
        .is_none());
        assert!(MedplumAuthConfig::from_values(
            Some("u".into()),
            Some("".into()),
            Some("csec".into())
        )
        .is_none());
        assert!(MedplumAuthConfig::from_values(
            Some("u".into()),
            Some("cid".into()),
            Some("".into())
        )
        .is_none());
    }

    #[tokio::test]
    async fn get_token_returns_error_when_unconfigured() {
        let proxy = MedplumAuthProxy::new(None);
        let r = proxy.get_token().await;
        assert!(r.is_err());
        match r.unwrap_err() {
            ApiError::Internal(m) => assert!(m.contains("not configured"), "msg: {m}"),
            e => panic!("unexpected error: {e:?}"),
        }
    }
}

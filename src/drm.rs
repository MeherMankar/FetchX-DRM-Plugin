//! DRM key acquisition for Widevine and PlayReady via remote CDM servers.
//! Env vars read at runtime:
//!   FETCHX_WVD_PATH            - local Widevine device file path (optional)
//!   FETCHX_REMOTE_CDM_URL      - remote pywidevine CDM URL (optional)
//!   FETCHX_REMOTE_CDM_DEVICE   - device name on remote CDM (default: "default")
//!   FETCHX_PRD_PATH            - local PlayReady device file path (optional)
//!   FETCHX_PLAYREADY_REMOTE_URL - remote PlayReady CDM URL (optional)

use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyPair {
    pub kid: String, // hex
    pub key: String, // hex
}

#[derive(Debug, Error)]
pub enum DrmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("DRM system not configured: {0}")]
    NotConfigured(String),
    #[error("Remote CDM error: {0}")]
    RemoteCdm(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ── Widevine remote CDM protocol (pywidevine-compatible) ──────────────────────

#[derive(Serialize)]
struct WvOpenSessionRequest {
    device: String,
}

#[derive(Deserialize)]
struct WvOpenSessionResponse {
    session_id: String,
}

#[derive(Serialize)]
struct WvGetChallengeRequest {
    session_id: String,
    init_data: String,  // base64 PSSH
    license_type: String,
    privacy_mode: bool,
}

#[derive(Deserialize)]
struct WvGetChallengeResponse {
    challenge_b64: String,
}

#[derive(Serialize)]
struct WvParseLicenseRequest {
    session_id: String,
    license_message: String, // base64
}

#[derive(Deserialize)]
struct WvGetKeysResponse {
    keys: Vec<WvKey>,
}

#[derive(Deserialize)]
struct WvKey {
    #[serde(rename = "kid")]
    kid: String,
    #[serde(rename = "key")]
    key: String,
}

// ── PlayReady remote CDM protocol ─────────────────────────────────────────────

#[derive(Serialize)]
struct PrOpenSessionRequest {
    device: String,
}

#[derive(Deserialize)]
struct PrOpenSessionResponse {
    session_id: String,
}

#[derive(Serialize)]
struct PrGetChallengeRequest {
    session_id: String,
    init_data: String,
}

#[derive(Deserialize)]
struct PrGetChallengeResponse {
    challenge: String,
}

#[derive(Serialize)]
struct PrParseLicenseRequest {
    session_id: String,
    license_message: String,
}

#[derive(Deserialize)]
struct PrGetKeysResponse {
    keys: Vec<PrKey>,
}

#[derive(Deserialize)]
struct PrKey {
    kid: String,
    key: String,
}

// ── Public API ────────────────────────────────────────────────────────────────

pub async fn acquire_keys_widevine(
    pssh_b64: &str,
    license_url: &str,
    client: &Client,
) -> Result<Vec<KeyPair>, DrmError> {
    let remote_url = std::env::var("FETCHX_REMOTE_CDM_URL").ok();
    let device = std::env::var("FETCHX_REMOTE_CDM_DEVICE")
        .unwrap_or_else(|_| "default".to_string());

    let Some(base_url) = remote_url else {
        return Err(DrmError::NotConfigured(
            "FETCHX_REMOTE_CDM_URL is not set. Configure a remote CDM URL in FetchX Settings → DRM."
                .to_string(),
        ));
    };

    // 1. Open session
    let open_resp: WvOpenSessionResponse = client
        .post(format!("{base_url}/open_session"))
        .json(&WvOpenSessionRequest { device: device.clone() })
        .send()
        .await?
        .error_for_status()
        .map_err(DrmError::Http)?
        .json()
        .await?;
    let session_id = open_resp.session_id;

    // 2. Get challenge
    let challenge_resp: WvGetChallengeResponse = client
        .post(format!("{base_url}/get_license_challenge"))
        .json(&WvGetChallengeRequest {
            session_id: session_id.clone(),
            init_data: pssh_b64.to_string(),
            license_type: "STREAMING".to_string(),
            privacy_mode: false,
        })
        .send()
        .await?
        .error_for_status()
        .map_err(DrmError::Http)?
        .json()
        .await?;

    // 3. Send challenge to license server
    let challenge_bytes = STANDARD
        .decode(&challenge_resp.challenge_b64)
        .map_err(|e| DrmError::RemoteCdm(format!("base64 decode: {e}")))?
        ;
    let license_resp = client
        .post(license_url)
        .header("Content-Type", "application/octet-stream")
        .body(challenge_bytes)
        .send()
        .await?
        .error_for_status()
        .map_err(DrmError::Http)?
        .bytes()
        .await?;
    let license_b64 = STANDARD.encode(&license_resp);

    // 4. Parse license
    client
        .post(format!("{base_url}/parse_license"))
        .json(&WvParseLicenseRequest {
            session_id: session_id.clone(),
            license_message: license_b64,
        })
        .send()
        .await?
        .error_for_status()
        .map_err(DrmError::Http)?;

    // 5. Get keys
    let keys_resp: WvGetKeysResponse = client
        .get(format!("{base_url}/get_keys/{session_id}"))
        .send()
        .await?
        .error_for_status()
        .map_err(DrmError::Http)?
        .json()
        .await?;

    // 6. Close session (best-effort)
    let _ = client
        .delete(format!("{base_url}/close_session/{session_id}"))
        .send()
        .await;

    Ok(keys_resp
        .keys
        .into_iter()
        .map(|k| KeyPair { kid: k.kid, key: k.key })
        .collect())
}

pub async fn acquire_keys_playready(
    pssh_b64: &str,
    license_url: &str,
    client: &Client,
) -> Result<Vec<KeyPair>, DrmError> {
    let remote_url = std::env::var("FETCHX_PLAYREADY_REMOTE_URL").ok();
    let device = std::env::var("FETCHX_REMOTE_CDM_DEVICE")
        .unwrap_or_else(|_| "default".to_string());

    let Some(base_url) = remote_url else {
        return Err(DrmError::NotConfigured(
            "FETCHX_PLAYREADY_REMOTE_URL is not set. Configure a PlayReady CDM URL in FetchX Settings → DRM."
                .to_string(),
        ));
    };

    // 1. Open session
    let open_resp: PrOpenSessionResponse = client
        .post(format!("{base_url}/open_session"))
        .json(&PrOpenSessionRequest { device: device.clone() })
        .send()
        .await?
        .error_for_status()
        .map_err(DrmError::Http)?
        .json()
        .await?;
    let session_id = open_resp.session_id;

    // 2. Get challenge
    let challenge_resp: PrGetChallengeResponse = client
        .post(format!("{base_url}/get_license_challenge"))
        .json(&PrGetChallengeRequest {
            session_id: session_id.clone(),
            init_data: pssh_b64.to_string(),
        })
        .send()
        .await?
        .error_for_status()
        .map_err(DrmError::Http)?
        .json()
        .await?;

    // 3. Send challenge to license server
    let license_resp = client
        .post(license_url)
        .header("Content-Type", "application/soap+xml;charset=UTF-8")
        .body(challenge_resp.challenge)
        .send()
        .await?
        .error_for_status()
        .map_err(DrmError::Http)?
        .text()
        .await?;

    // 4. Parse license
    client
        .post(format!("{base_url}/parse_license"))
        .json(&PrParseLicenseRequest {
            session_id: session_id.clone(),
            license_message: license_resp,
        })
        .send()
        .await?
        .error_for_status()
        .map_err(DrmError::Http)?;

    // 5. Get keys
    let keys_resp: PrGetKeysResponse = client
        .get(format!("{base_url}/get_keys/{session_id}"))
        .send()
        .await?
        .error_for_status()
        .map_err(DrmError::Http)?
        .json()
        .await?;

    // 6. Close session (best-effort)
    let _ = client
        .delete(format!("{base_url}/close_session/{session_id}"))
        .send()
        .await;

    Ok(keys_resp
        .keys
        .into_iter()
        .map(|k| KeyPair { kid: k.kid, key: k.key })
        .collect())
}

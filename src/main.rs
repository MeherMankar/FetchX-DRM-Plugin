//! FetchX DRM Plugin — standalone executable.
//!
//! Communicates with the FetchX host over stdin/stdout using the
//! length-prefixed JSON framing protocol.
//!
//! ## Key selection (env vars, evaluated at request time)
//!
//! Widevine:
//!   - `FETCHX_WVD_PATH`       — path to a `.wvd` device file → uses LocalCdm
//!   - `FETCHX_REMOTE_CDM_URL` — URL of a pywidevine CDM server → uses remote CDM
//!   Local takes priority over remote.
//!
//! PlayReady:
//!   - `FETCHX_PRD_PATH`              — path to a `.prd` device file → uses local flow
//!   - `FETCHX_PLAYREADY_REMOTE_URL`  — URL of a pyplayready CDM server → uses remote CDM
//!   Local takes priority over remote.

#![forbid(unsafe_code)]

mod drm;
mod local_cdm;
mod prd;
mod proto;
mod pssh_pr;
mod pssh_wv;
mod playready_remote_cdm;
mod types;
mod wvd;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt, stdin, stdout};

use types::ContentKey;

// ── IPC framing ───────────────────────────────────────────────────────────────

async fn read_frame() -> Option<Vec<u8>> {
    let mut stdin = stdin();
    let mut len_buf = [0u8; 4];
    stdin.read_exact(&mut len_buf).await.ok()?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 65_536 {
        return None;
    }
    let mut buf = vec![0u8; len];
    stdin.read_exact(&mut buf).await.ok()?;
    Some(buf)
}

async fn write_frame(payload: &[u8]) {
    let mut stdout = stdout();
    let len = (payload.len() as u32).to_le_bytes();
    let _ = stdout.write_all(&len).await;
    let _ = stdout.write_all(payload).await;
    let _ = stdout.flush().await;
}

async fn send_response(body: &impl Serialize) {
    if let Ok(json) = serde_json::to_vec(body) {
        write_frame(&json).await;
    }
}

// ── Message types (inlined to avoid dep on fetchx_plugin_api) ─────────────────

#[derive(Deserialize)]
struct IncomingMessage {
    #[serde(rename = "type")]
    msg_type: String,
    // For HelloRequest
    host_version: Option<String>,
    expected_plugin_id: Option<String>,
    // For DecryptRequest
    pssh_b64: Option<String>,
    license_url: Option<String>,
    drm_system: Option<String>,
}

#[derive(Serialize)]
struct OutgoingMessage {
    version: u32,
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    keys: Option<Vec<KeyPairDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Serialize)]
struct KeyPairDto {
    kid: String,
    key: String,
}

impl OutgoingMessage {
    fn hello_ack() -> Self {
        Self {
            version: 1,
            msg_type: "hello_ack".to_string(),
            plugin_id: Some("dev.fetchx.drm".to_string()),
            plugin_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            capabilities: Some(vec![
                "drm_decrypt".to_string(),
                "network_http_request".to_string(),
            ]),
            status: None,
            keys: None,
            code: None,
            message: None,
        }
    }
    fn pong() -> Self {
        Self {
            version: 1,
            msg_type: "pong".to_string(),
            plugin_id: None,
            plugin_version: None,
            capabilities: None,
            status: Some("ok".to_string()),
            keys: None,
            code: None,
            message: None,
        }
    }
    fn shutdown_ack() -> Self {
        Self {
            version: 1,
            msg_type: "shutdown_ack".to_string(),
            plugin_id: None,
            plugin_version: None,
            capabilities: None,
            status: None,
            keys: None,
            code: None,
            message: None,
        }
    }
    fn decrypt_response(keys: Vec<KeyPairDto>) -> Self {
        Self {
            version: 1,
            msg_type: "decrypt_response".to_string(),
            plugin_id: None,
            plugin_version: None,
            capabilities: None,
            status: None,
            keys: Some(keys),
            code: None,
            message: None,
        }
    }
    fn error(code: &str, message: &str) -> Self {
        Self {
            version: 1,
            msg_type: "error".to_string(),
            plugin_id: None,
            plugin_version: None,
            capabilities: None,
            status: None,
            keys: None,
            code: Some(code.to_string()),
            message: Some(message.to_string()),
        }
    }
}

// ── DRM dispatch ──────────────────────────────────────────────────────────────

/// Acquire Widevine keys.
/// Prefers the local `.wvd` device path; falls back to the remote CDM URL.
async fn acquire_keys_widevine(
    pssh_b64: &str,
    license_url: &str,
    client: &Client,
) -> Result<Vec<drm::KeyPair>, drm::DrmError> {
    let wvd_path = std::env::var("FETCHX_WVD_PATH").ok();

    if let Some(path) = wvd_path {
        // Decode the base64 PSSH into raw bytes.
        let pssh_bytes = BASE64
            .decode(pssh_b64)
            .map_err(|e| drm::DrmError::RemoteCdm(format!("base64 decode pssh: {e}")))?;

        // Parse the .wvd device file.
        let device = wvd::WvdDevice::from_file(std::path::Path::new(&path))
            .map_err(|e| drm::DrmError::RemoteCdm(format!("wvd parse: {e}")))?;

        let cdm = local_cdm::LocalCdm::new(device);

        use types::WidevineProvider;
        let content_keys = cdm
            .get_keys(&pssh_bytes, license_url)
            .await
            .map_err(|e| drm::DrmError::RemoteCdm(e.to_string()))?;

        return Ok(content_key_pairs(content_keys));
    }

    // Fall back to the remote CDM.
    drm::acquire_keys_widevine(pssh_b64, license_url, client).await
}

/// Acquire PlayReady keys.
/// Prefers the local `.prd` device path; falls back to the remote CDM URL.
async fn acquire_keys_playready(
    pssh_b64: &str,
    license_url: &str,
    client: &Client,
) -> Result<Vec<drm::KeyPair>, drm::DrmError> {
    let prd_path = std::env::var("FETCHX_PRD_PATH").ok();

    if let Some(path) = prd_path {
        return acquire_keys_playready_local(pssh_b64, license_url, client, &path).await;
    }

    drm::acquire_keys_playready(pssh_b64, license_url, client).await
}

/// Local PlayReady key acquisition using a `.prd` device file.
///
/// The `.prd` device holds the ECC keys and group certificate needed to
/// produce a PlayReady license request.  We build a minimal SOAP license
/// request that embeds the group certificate, POST it to `license_url`, and
/// extract the returned content keys.
///
/// Full PlayReady crypto (ECC-based licence parsing) is out of scope for this
/// implementation; the local path here constructs the XML challenge from the
/// device certificate and relies on the license server to return cleartext
/// key material in its response (as some internal/testing servers do).  For
/// production PlayReady, use the remote CDM path.
async fn acquire_keys_playready_local(
    pssh_b64: &str,
    license_url: &str,
    client: &Client,
    prd_path: &str,
) -> Result<Vec<drm::KeyPair>, drm::DrmError> {
    // 1. Parse the .prd device file.
    let prd_device = prd::PrdDevice::from_file(prd_path)
        .await
        .map_err(|e| drm::DrmError::RemoteCdm(format!("prd parse: {e}")))?;

    // 2. Build a minimal PlayReady SOAP license acquisition request.
    let cert_xml = String::from_utf8_lossy(&prd_device.group_certificate).into_owned();
    let challenge_xml = build_playready_challenge(pssh_b64, &cert_xml);

    // 3. POST to the license server.
    let response = client
        .post(license_url)
        .header("Content-Type", "application/soap+xml;charset=UTF-8")
        .body(challenge_xml)
        .send()
        .await
        .map_err(drm::DrmError::Http)?
        .error_for_status()
        .map_err(drm::DrmError::Http)?
        .text()
        .await
        .map_err(drm::DrmError::Http)?;

    // 4. Extract keys from the license response XML.
    let keys = extract_playready_keys_from_response(&response)
        .map_err(|e| drm::DrmError::RemoteCdm(format!("key extraction: {e}")))?;

    Ok(keys)
}

/// Build a minimal PlayReady SOAP license challenge XML.
fn build_playready_challenge(pssh_b64: &str, cert_xml: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
               xmlns:xsd="http://www.w3.org/2001/XMLSchema"
               xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <AcquireLicense xmlns="http://schemas.microsoft.com/DRM/2007/03/protocols">
      <challenge>
        <LA xmlns="http://schemas.microsoft.com/DRM/2007/03/licenseacquisition"
            Id="SignedData" xml:space="preserve">
          <PSSH>{pssh_b64}</PSSH>
          <ClientInfo>
            <GroupCertificate>{cert_xml}</GroupCertificate>
          </ClientInfo>
        </LA>
      </challenge>
    </AcquireLicense>
  </soap:Body>
</soap:Envelope>"#
    )
}

/// Extract `kid:key` pairs from a PlayReady license response XML.
///
/// Looks for `<KeyID>` and `<ContentKey>` sibling elements within any
/// `<Key>` block.  Returns hex-encoded pairs.
fn extract_playready_keys_from_response(xml: &str) -> Result<Vec<drm::KeyPair>, String> {
    let mut keys = Vec::new();
    let mut search = xml;

    while let Some(key_start) = search.find("<Key>").or_else(|| search.find("<Key ")) {
        let after = &search[key_start..];
        let Some(key_end) = after.find("</Key>") else {
            break;
        };
        let key_block = &after[..key_end + 6];

        let kid = extract_xml_text(key_block, "KeyID");
        let key = extract_xml_text(key_block, "ContentKey");

        if let (Some(kid), Some(key)) = (kid, key) {
            keys.push(drm::KeyPair {
                kid: kid.trim().to_string(),
                key: key.trim().to_string(),
            });
        }

        let consumed = key_start + key_end + 6;
        if consumed >= search.len() {
            break;
        }
        search = &search[consumed..];
    }

    if keys.is_empty() {
        // Not an error — some license servers return an empty key list for
        // already-decrypted streams.  Callers decide if that is acceptable.
    }

    Ok(keys)
}

/// Extract the text content of the first occurrence of `<tag>…</tag>` in `xml`.
fn extract_xml_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");

    let start = xml.find(open.as_str())?;
    let after_open = &xml[start..];
    let tag_end = after_open.find('>')?;
    let content_start = tag_end + 1;
    let content_region = &after_open[content_start..];
    let end = content_region.find(close.as_str())?;
    Some(&content_region[..end])
}

/// Convert `Vec<ContentKey>` (from local CDM) into `Vec<drm::KeyPair>` (hex strings).
fn content_key_pairs(keys: Vec<ContentKey>) -> Vec<drm::KeyPair> {
    keys.into_iter()
        .map(|ck| drm::KeyPair {
            kid: hex_encode(&ck.key_id),
            key: hex_encode(&ck.key),
        })
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let client = Client::builder()
        .user_agent(concat!("fetchx-drm-plugin/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_else(|_| Client::new());

    loop {
        let Some(frame) = read_frame().await else {
            break;
        };
        let msg: IncomingMessage = match serde_json::from_slice(&frame) {
            Ok(m) => m,
            Err(e) => {
                send_response(&OutgoingMessage::error(
                    "parse_error",
                    &format!("could not parse request: {e}"),
                ))
                .await;
                continue;
            }
        };

        match msg.msg_type.as_str() {
            "hello" => {
                // host_version and expected_plugin_id are informational
                let _ = (&msg.host_version, &msg.expected_plugin_id);
                send_response(&OutgoingMessage::hello_ack()).await;
            }
            "ping" => {
                send_response(&OutgoingMessage::pong()).await;
            }
            "shutdown" => {
                send_response(&OutgoingMessage::shutdown_ack()).await;
                break;
            }
            "decrypt_request" => {
                let Some(pssh_b64) = msg.pssh_b64 else {
                    send_response(&OutgoingMessage::error(
                        "missing_field",
                        "pssh_b64 is required",
                    ))
                    .await;
                    continue;
                };
                let Some(license_url) = msg.license_url else {
                    send_response(&OutgoingMessage::error(
                        "missing_field",
                        "license_url is required",
                    ))
                    .await;
                    continue;
                };
                let drm_system =
                    msg.drm_system.unwrap_or_else(|| "widevine".to_string());

                let result = match drm_system.as_str() {
                    "playready" => {
                        acquire_keys_playready(&pssh_b64, &license_url, &client).await
                    }
                    _ => acquire_keys_widevine(&pssh_b64, &license_url, &client).await,
                };

                match result {
                    Ok(keys) => {
                        let key_dtos = keys
                            .into_iter()
                            .map(|k| KeyPairDto {
                                kid: k.kid,
                                key: k.key,
                            })
                            .collect();
                        send_response(&OutgoingMessage::decrypt_response(key_dtos)).await;
                    }
                    Err(e) => {
                        send_response(&OutgoingMessage::error(
                            "drm_failed",
                            &e.to_string(),
                        ))
                        .await;
                    }
                }
            }
            other => {
                send_response(&OutgoingMessage::error(
                    "unknown_type",
                    &format!("unknown message type: {other}"),
                ))
                .await;
            }
        }
    }
}

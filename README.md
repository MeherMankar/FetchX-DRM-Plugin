# FetchX DRM Plugin

Adds Widevine and PlayReady DRM decryption to [FetchX](https://github.com/MeherMankar/FetchX).

## What it does

Intercepts DRM-protected HLS/DASH streams and obtains content decryption keys using your own Widevine or PlayReady device credentials.

Supports:
- **Widevine** via local `.wvd` device file (OpenWV pure-Rust CDM — no native library required)
- **Widevine** via a remote pywidevine-compatible CDM server
- **PlayReady** via local `.prd` device file
- **PlayReady** via a remote pyplayready-compatible CDM server

## Requirements

- FetchX desktop app installed
- One of:
  - A Widevine device file (`.wvd`) — from your own device or a pywidevine-compatible source
  - A remote CDM server URL (pywidevine or pyplayready)

## Installation

### From GitHub (recommended)

In FetchX: **Settings → Plugins → Install from GitHub**

Enter: `MeherMankar/fetchx-drm-plugin`

FetchX will download the latest release binary for your platform automatically.

### Manual

1. Download the latest release from [Releases](../../releases)
2. Extract to your FetchX plugins directory:
   - Windows: `%APPDATA%\FetchX\plugins\dev.fetchx.drm\`
   - Linux: `~/.local/share/fetchx/plugins/dev.fetchx.drm/`
   - macOS: `~/Library/Application Support/FetchX/plugins/dev.fetchx.drm/`
3. Restart FetchX — the plugin appears in **Settings → Plugins**

### Build from source

```bash
git clone https://github.com/MeherMankar/fetchx-drm-plugin
cd fetchx-drm-plugin
cargo build --release
# Binary at: target/release/fetchx-drm-plugin(.exe)
```

## Configuration

Set in **FetchX Settings → DRM Plugin** or via environment variables:

| Setting | Env var | Description |
|---|---|---|
| Widevine device file | `FETCHX_WVD_PATH` | Path to your `.wvd` file |
| Widevine remote CDM URL | `FETCHX_REMOTE_CDM_URL` | pywidevine server URL |
| Widevine remote device name | `FETCHX_REMOTE_CDM_DEVICE` | Device name on CDM server |
| PlayReady device file | `FETCHX_PRD_PATH` | Path to your `.prd` file |
| PlayReady remote CDM URL | `FETCHX_PLAYREADY_REMOTE_URL` | pyplayready server URL |

Priority: local device file → remote CDM server.

## How it works

1. FetchX detects a DRM-protected HLS/DASH stream
2. Sends a `decrypt_request` IPC message to this plugin with the PSSH box and license URL
3. The plugin acquires content keys from the CDM (local or remote)
4. Returns key pairs to FetchX which decrypts each media segment
5. The final downloaded file is clean and playable

## Legal

Only use with content and credentials you are licensed to access. The plugin does not bypass hardware-enforced DRM (Widevine L1, SL3000). It works with software-level credentials (Widevine L3, PlayReady SL2000 and below).

## License

MIT

# FetchX DRM Plugin — Installation Guide

## FetchX Data Directory Locations

| Platform | Default path |
|----------|--------------|
| **Windows** | `%APPDATA%\FetchX\plugins\` |
| **Linux** | `~/.local/share/fetchx/plugins/` |
| **macOS** | `~/Library/Application Support/FetchX/plugins/` |
| **Android** | Managed by FetchX app (use in-app installer) |

## Step-by-step: Windows

1. Download `fetchx-drm-plugin-windows-x86_64.zip` from [Releases](../../releases).
2. Create the directory: `%APPDATA%\FetchX\plugins\dev.fetchx.drm\`
3. Extract the ZIP into that directory. You should have:
   ```
   %APPDATA%\FetchX\plugins\dev.fetchx.drm\
     fetchx-drm-plugin.exe
     plugin.json
   ```
4. Restart FetchX.
5. Open **Settings → Plugins** — `FetchX DRM` should appear as **Registered**.
6. Click **Enable** and approve the requested permissions.

## Step-by-step: Linux

1. Download `fetchx-drm-plugin-linux-x86_64.tar.gz` from [Releases](../../releases).
2. Create the directory: `mkdir -p ~/.local/share/fetchx/plugins/dev.fetchx.drm`
3. Extract: `tar -xzf fetchx-drm-plugin-linux-x86_64.tar.gz -C ~/.local/share/fetchx/plugins/dev.fetchx.drm`
4. Make executable: `chmod +x ~/.local/share/fetchx/plugins/dev.fetchx.drm/fetchx-drm-plugin`
5. Restart FetchX.

## Step-by-step: macOS

1. Download `fetchx-drm-plugin-macos-x86_64.tar.gz` (or `aarch64` for Apple Silicon).
2. Create the directory: `mkdir -p "$HOME/Library/Application Support/FetchX/plugins/dev.fetchx.drm"`
3. Extract the archive into that directory.
4. If Gatekeeper blocks execution, run: `xattr -d com.apple.quarantine fetchx-drm-plugin`
5. Restart FetchX.

## Verification

Open **FetchX → Settings → Plugins**.
The plugin should appear in the list with status **Registered** or **Running**.

If it shows **Failed**, check:
- The executable is present and has execute permission (Linux/macOS)
- `plugin.json` is present and valid JSON
- FetchX has permission to read the plugins directory

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Plugin not detected | Ensure the folder is named exactly `dev.fetchx.drm` |
| `plugin.json` invalid | Download a fresh copy from the release |
| DRM keys not acquired | Check that `FETCHX_REMOTE_CDM_URL` is set in Settings → DRM |
| Network error | Ensure FetchX is allowed through your firewall |
| Plugin crashes on start | Check FetchX logs: Help → View Logs |

## Uninstall

Delete the `dev.fetchx.drm/` folder from the plugins directory and restart FetchX.

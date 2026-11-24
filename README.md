# Riptide — Native GPUI YouTube Music Client

A native Rust desktop client for YouTube Music built with **GPUI** (from Zed) and styled after **TIDAL**'s dark, audiophile-grade monochrome UI.

## How It Works (ToS-Friendly Architecture)

Instead of reverse-engineering private YouTube APIs, extracting audio stream URLs, or spoofing client headers, **Riptide** runs the genuine `https://music.youtube.com` web application inside a persistent embedded **Microsoft Edge WebView2** browser session (`wry` + `tao`) on a dedicated background UI thread:

- **Native GPUI Frontend (`src/app.rs`, `src/ui/*`)**: Renders a hardware-accelerated, TIDAL-modeled interface (obsidian surfaces, hairline borders, cyan `#00FFFF` accents, top-edge full-width progress scrubber, card grids, tracklist tables, collapsible Queue & Lyrics drawer, and native GPUI text input).
- **Webapp Bridge (`src/webview.rs`, `src/bridge.js`)**: Observes the live YouTube Music DOM via `MutationObserver` and streams structured state snapshots over IPC (`window.ipc.postMessage`) to GPUI. Clicking any card, track, filter chip, playlist, transport button, or search query in GPUI executes the corresponding click/navigation directly on the live YouTube Music webapp.
- **Persistent Google Sign-In**: Cookies and session data are stored locally in `%LOCALAPPDATA%\riptide\webview2`. Click **Sign In** or **Show WebView** (or press `Ctrl+Shift+W`) at any time to reveal the real WebView2 window for standard Google authentication, 2FA, or inspecting the live webapp; closing the WebView window simply hides it again without interrupting playback.
- **Native Artwork Loading (`src/http_image_client.rs`)**: Registers a `reqwest`-backed `gpui::http_client::HttpClient` so GPUI's `img()` element fetches, decodes, and GPU-caches upscaled album artwork natively.

## Running

Prerequisites on Windows:
- Rust toolchain (`stable-x86_64-pc-windows-msvc`)
- Microsoft Edge WebView2 Runtime (pre-installed on Windows 10/11)

```powershell
cargo run --release
```

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Space` | Play / Pause (when search bar is not focused) |
| `Ctrl + K` | Focus Search bar |
| `Enter` | Submit Search |
| `Esc` | Clear / Blur Search bar |
| `Alt + Left` / `Alt + Right` | Navigate Back / Forward |
| `Ctrl + Shift + W` | Toggle visibility of the underlying YouTube Music WebView2 window |

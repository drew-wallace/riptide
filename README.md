# Riptide — Native GPUI YouTube Music Client

A native Rust desktop client for YouTube Music built with **GPUI** (from Zed) and styled after **TIDAL**'s dark, audiophile-grade monochrome UI.

## UI & UX Differences from YouTube Music

Riptide reimagines the YouTube Music web experience as a distraction-free, TIDAL-inspired native desktop app:

- **TIDAL-Styled Native Interface**: Replaces YouTube Music's browser UI with hardware-accelerated obsidian monochrome surfaces, hairline borders, electric cyan (`#00FFFF`) accents, larger monochrome iconography, and a custom OS taskbar/titlebar icon.
- **Personalized Release Feed *(New Feature)***: Adds a TIDAL-style **Feed** tab below **Home** that YouTube Music lacks—tracking new albums, EPs, and singles from your **Subscribed**, **Collection**, or **Both** artists, grouped into Sunday-start calendar buckets (**This week**, **Last week**, **This month**, **Last month**, **Older**) with **All / Albums / Singles & EPs** filters. Includes an unread badge that clears automatically when you visit Feed, on-hover **▶** play buttons on artwork, a far-right `⋯` action menu, and configurable automatic refresh schedules (**On app open**, **Once daily** at a chosen time, and **Run immediately if missed**).
- **Cleaner Browsing & Tracklists**: Strips out visual clutter such as redundant page eyebrows and "Featured other playlists" shelves inside playlist views, omits empty artwork placeholders on album tracklists (showing track numbers with hover play glyphs and switching the metadata column to `PLAYS` when appropriate), and auto-hydrates long playlists so suggested tracks load without manual scrolling.
- **Consistent Click & Link Behavior**: Clicking a row for an album/playlist opens its detail page while clicking the artwork's hover play button starts playback; clickable artist and album links across the player bar, track rows, headers, and queue underline on hover.
- **Native Desktop Navigation & Smooth Scrolling**: Maintains a unified Back/Forward history stack across YouTube Music pages, Feed, and Settings—with **mouse side Back/Forward button** support, automatic scroll-to-top on new pages, and exact scroll-position restoration when stepping through history. All scrollable panes use VSync-locked smooth scrolling paired with viewport virtualization for large feeds and long playlists.
- **Upgraded Player Bar**: Features a full-width top-edge TIDAL progress scrubber with click-and-drag seeking and a live **hover timestamp preview** pill, a drag-scrubbing volume slider with a live percentage tooltip, hover tooltips with shortcut badges on controls, and **persisted volume & mute state** across restarts so YouTube Music doesn't reset your volume to 100%.
- **Non-Intrusive Side Drawer & Native Modals**: Combines **Up Next**, **Lyrics**, and **Related** in a collapsible right-hand drawer without taking over the main stage, and replaces interrupting webapp popups with native GPUI dialogs for **New Playlist**, **Unsubscribe/Confirmation**, **Save to Playlist**, **Share** (one-click clipboard copy + external share targets), and a high-res (`1200×1200`) **Artwork Lightbox** when clicking hero cover art.

## How It Works (ToS-Friendly Architecture)

Instead of reverse-engineering private YouTube APIs, extracting audio stream URLs, or spoofing client headers, **Riptide** runs the genuine `https://music.youtube.com` web application inside a persistent embedded **Microsoft Edge WebView2** browser session (`wry` + `tao`) on a dedicated background UI thread:

- **Native GPUI Frontend (`src/app.rs`, `src/ui/*`)**: Renders a hardware-accelerated, TIDAL-modeled interface (obsidian surfaces, hairline borders, cyan `#00FFFF` accents, top-edge full-width progress scrubber, card grids, tracklist tables, collapsible Queue & Lyrics drawer, and native GPUI text input).
- **Webapp Bridge (`src/webview.rs`, `src/bridge.js`)**: Observes the live YouTube Music DOM via `MutationObserver` and streams structured state snapshots over IPC (`window.ipc.postMessage`) to GPUI. Clicking any card, track, filter chip, playlist, transport button, or search query in GPUI executes the corresponding click/navigation directly on the live YouTube Music webapp.
- **Persistent Google Sign-In**: Cookies and session data are stored locally in `%LOCALAPPDATA%\riptide\webview2`. Click **Sign In** or **Show WebView** (or press `Ctrl+Shift+W`) at any time to reveal the real WebView2 window for standard Google authentication, 2FA, or inspecting the live webapp; closing the WebView window simply hides it again without interrupting playback.
- **Native Artwork Loading (`src/http_image_client.rs`)**: Registers a `reqwest`-backed `gpui::http_client::HttpClient` so GPUI's `img()` element fetches, decodes, and GPU-caches upscaled album artwork natively.

## Network & API Transparency (ToS-Friendly by Design)

Riptide is designed to avoid reverse-engineering or abusing YouTube Music's private APIs:

- **No private Innertube API calls**: Neither the Rust binary nor the bridge scripts call `youtubei/v1/*`, extract raw audio stream URLs, spoof client headers, or bypass playback/auth flows. All YouTube Music API traffic and audio streaming are performed directly by Google's genuine webapp inside Microsoft Edge WebView2.
- **No telemetry or third-party backend**: There is no analytics, crash reporting, or external server—your session, settings, and caches stay strictly on your machine under `%LOCALAPPDATA%\riptide`.

### Complete Outbound Request Audit

| Destination | Mechanism | Auth / Cookies | Purpose & Pacing |
|---|---|---|---|
| `https://music.youtube.com/*` | Embedded WebView2 (`src/webview.rs`, `src/bridge.js`) | Your normal WebView2 Google session | All browsing, search, playback, likes, playlists, queue, lyrics, related tracks, and Feed artist discovery—driven via DOM clicks and `yt-navigate` on the real webapp. |
| `https://api.deezer.com/search/artist` & `/artist/{id}/albums` | Rust `reqwest` (`src/deezer.rs`) | None (unauthenticated public API) | Primary release-date and catalog lookup for the personalized **Feed**. Artist ID mappings are cached locally (`deezer_artist_map.json`), and requests are paced at ≥135 ms (~7.4 req/s) with automatic exponential backoff. |
| Artwork CDNs (`lh3.googleusercontent.com`, `yt3.ggpht.com`, `cdn-images.dzcdn.net`, etc.) | Rust `reqwest` (`src/http_image_client.rs`) | None (no cookies) | Cookie-free `GET` requests for album covers and artist portraits currently rendered in the UI (virtualized lists only fetch visible items). |
| `https://www.youtube.com/watch?v={id}` *(fallback only)* | Hidden muted WebView2 (`src/feed_collector.js`) | Shared WebView2 session | Only runs for artists not confidently matched on Deezer whose release date isn't already cached in `feed_cache.json`, reading `"Released on: YYYY-MM-DD"` from the standard watch page (spaced ≥110 ms apart). |
| External Share targets (YouTube, Deezer, X, Reddit, etc.) | OS default browser (`cx.open_url`) | Handled by your browser | Triggered strictly when you explicitly click a target inside the Share modal. |

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

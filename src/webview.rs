use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::WindowBuilder,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use wry::{WebContext, WebView, WebViewBuilder};

#[cfg(target_os = "windows")]
use tao::platform::windows::EventLoopBuilderExtWindows;
#[cfg(target_os = "windows")]
use wry::WebViewBuilderExtWindows;

use crate::deezer;
use crate::protocol::{
    BridgeState, FeedArtist, FeedArtistSource, FeedCollectorPayload, FeedRelease, Route,
    WebCommand, WebEvent,
};
use crate::settings::FeedCache;

#[derive(Debug, Clone, Serialize)]
struct YtProbeTarget {
    idx: usize,
    video_id: String,
    title: String,
    year: Option<u16>,
}

struct PendingFeedDates {
    run: u64,
    scanned: usize,
    total: usize,
    /// Releases already collected from Deezer before WebView fallback runs.
    base_releases: Vec<FeedRelease>,
    /// Combined releases (Deezer + fallback) once fallback phase 1 completes.
    releases: Vec<FeedRelease>,
    targets: Vec<YtProbeTarget>,
}

#[derive(Debug, Deserialize)]
struct CollectorArtistsReady {
    #[serde(default)]
    run: u64,
    #[serde(default)]
    artists: Vec<FeedArtist>,
}

#[derive(Debug, Deserialize)]
struct CollectorFallbackProgress {
    #[serde(default)]
    run: u64,
    #[serde(default)]
    status: String,
    #[serde(default)]
    fallback_done: usize,
    #[serde(default)]
    fallback_total: usize,
    #[serde(default)]
    releases: Vec<FeedRelease>,
}

#[derive(Debug, Deserialize)]
struct CollectorPhase1Done {
    #[serde(default)]
    run: u64,
    #[serde(default)]
    min_recent_year: u16,
    #[serde(default)]
    releases: Vec<FeedRelease>,
}

#[derive(Debug, Deserialize)]
struct YtProbeReady {
    #[serde(default)]
    run: u64,
    #[serde(default)]
    release_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YtProbeItem {
    #[serde(default)]
    run: u64,
    #[serde(default)]
    idx: usize,
    #[serde(default)]
    title: String,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    done_count: usize,
    #[serde(default)]
    total_count: usize,
}

#[derive(Debug, Deserialize)]
struct YtProbeDone {
    #[serde(default)]
    run: u64,
}

const YTM_URL: &str = "https://music.youtube.com/";
const BRIDGE_JS: &str = include_str!("bridge.js");
const FEED_COLLECTOR_JS: &str = include_str!("feed_collector.js");

#[cfg(target_os = "windows")]
const WEBVIEW_BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
     --autoplay-policy=no-user-gesture-required \
     --disable-background-timer-throttling \
     --disable-renderer-backgrounding \
     --disable-backgrounding-occluded-windows";

const AUTO_HIDE_NONE: u8 = 0;
const AUTO_HIDE_AFTER_SIGN_IN: u8 = 1;

/// Handle held by the GPUI frontend to control the underlying hidden WebView2
/// YouTube Music browser session.
#[derive(Clone)]
pub struct WebController {
    proxy: EventLoopProxy<WebCommand>,
    visible: Arc<AtomicBool>,
}

impl WebController {
    pub fn send(&self, cmd: WebCommand) {
        let _ = self.proxy.send_event(cmd);
    }

    pub fn is_webview_visible(&self) -> bool {
        self.visible.load(Ordering::Relaxed)
    }
}

/// Spawns the dedicated OS WebView thread running the genuine YouTube Music webapp
/// (plus a shared-session hidden background collector WebView for building the Feed).
pub fn spawn_ytm_webview() -> Result<(WebController, UnboundedReceiver<WebEvent>)> {
    let (event_tx, event_rx) = unbounded_channel::<WebEvent>();
    let (proxy_tx, proxy_rx) = std::sync::mpsc::channel::<Result<EventLoopProxy<WebCommand>>>();
    let visible_flag = Arc::new(AtomicBool::new(false));
    let visible_for_thread = visible_flag.clone();

    std::thread::Builder::new()
        .name("ytm-webview2".to_string())
        .spawn(move || {
            run_webview_thread(proxy_tx, event_tx, visible_for_thread);
        })
        .context("failed to spawn webview thread")?;

    let proxy = proxy_rx
        .recv()
        .context("webview thread exited before creating event loop")??;

    Ok((
        WebController {
            proxy,
            visible: visible_flag,
        },
        event_rx,
    ))
}

fn webview_data_dir() -> PathBuf {
    let dir = crate::settings::app_data_dir().join("webview2");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run_webview_thread(
    proxy_tx: std::sync::mpsc::Sender<Result<EventLoopProxy<WebCommand>>>,
    event_tx: UnboundedSender<WebEvent>,
    visible_flag: Arc<AtomicBool>,
) {
    let mut builder = EventLoopBuilder::<WebCommand>::with_user_event();
    #[cfg(target_os = "windows")]
    {
        builder.with_any_thread(true);
    }
    let event_loop = builder.build();
    let proxy = event_loop.create_proxy();
    let ipc_proxy = event_loop.create_proxy();

    let window = match WindowBuilder::new()
        .with_title("Riptide — YouTube Music Webapp (Sign In / Inspector)")
        .with_inner_size(LogicalSize::new(1280.0, 840.0))
        .with_visible(false)
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(err) => {
            let _ = proxy_tx.send(Err(anyhow::anyhow!("window init failed: {err}")));
            return;
        }
    };

    // Secondary permanently-hidden window for the Feed background collector (shares cookies/WebContext)
    let collector_window = WindowBuilder::new()
        .with_title("Riptide Feed Collector")
        .with_inner_size(LogicalSize::new(1120.0, 760.0))
        .with_visible(false)
        .build(&event_loop)
        .ok();

    let auto_hide_mode = Arc::new(AtomicU8::new(AUTO_HIDE_NONE));
    let auto_hide_for_ipc = auto_hide_mode.clone();

    let mut web_context = WebContext::new(Some(webview_data_dir()));
    let ipc_tx = event_tx.clone();

    let mut wv_builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(YTM_URL)
        .with_autoplay(true)
        .with_initialization_script(BRIDGE_JS)
        .with_ipc_handler(move |request| {
            let body = request.body();
            match serde_json::from_str::<BridgeState>(body) {
                Ok(state) => {
                    if auto_hide_for_ipc.load(Ordering::Relaxed) == AUTO_HIDE_AFTER_SIGN_IN
                        && state.account.is_signed_in
                        && state.url.starts_with("https://music.youtube.com")
                    {
                        auto_hide_for_ipc.store(AUTO_HIDE_NONE, Ordering::Relaxed);
                        let _ = ipc_proxy.send_event(WebCommand::SetWebViewVisible(false));
                    }
                    let _ = ipc_tx.send(WebEvent::StateSync(Box::new(state)));
                }
                Err(err) => {
                    let _ = ipc_tx.send(WebEvent::StatusMessage(format!(
                        "Bridge parse warning: {err}"
                    )));
                }
            }
        });

    #[cfg(target_os = "windows")]
    {
        wv_builder = wv_builder.with_additional_browser_args(WEBVIEW_BROWSER_ARGS);
    }

    let webview = match wv_builder.build(&window) {
        Ok(wv) => wv,
        Err(err) => {
            let _ = proxy_tx.send(Err(anyhow::anyhow!("WebView2 init failed: {err}")));
            return;
        }
    };

    // Build hidden Feed Collector WebView sharing the exact same WebContext & browser args
    let collector_proxy = event_loop.create_proxy();
    let collector_webview: Option<WebView> = collector_window.as_ref().and_then(|cw| {
        let mut cb = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_url("about:blank")
            .with_initialization_script(FEED_COLLECTOR_JS)
            .with_ipc_handler(move |request| {
                let _ = collector_proxy.send_event(WebCommand::CollectorIpc(
                    request.body().to_string(),
                ));
            });
        #[cfg(target_os = "windows")]
        {
            cb = cb.with_additional_browser_args(WEBVIEW_BROWSER_ARGS);
        }
        cb.build(cw).ok()
    });

    let feed_run_counter = Arc::new(AtomicU64::new(1));
    let loop_proxy = event_loop.create_proxy();
    let mut pending_feed_dates: Option<PendingFeedDates> = None;

    let _ = proxy_tx.send(Ok(proxy));
    let _ = event_tx.send(WebEvent::StatusMessage(
        "Connecting to YouTube Music webapp...".to_string(),
    ));

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if window_id == window.id() {
                    window.set_visible(false);
                    visible_flag.store(false, Ordering::Relaxed);
                    auto_hide_mode.store(AUTO_HIDE_NONE, Ordering::Relaxed);
                    let _ = event_tx.send(WebEvent::WebViewVisibilityChanged(false));
                }
            }
            Event::UserEvent(cmd) => match cmd {
                WebCommand::Shutdown => {
                    *control_flow = ControlFlow::Exit;
                }
                WebCommand::SetWebViewVisible(show) => {
                    window.set_visible(show);
                    if show {
                        window.set_focus();
                    }
                    visible_flag.store(show, Ordering::Relaxed);
                    if !show {
                        auto_hide_mode.store(AUTO_HIDE_NONE, Ordering::Relaxed);
                    }
                    let _ = event_tx.send(WebEvent::WebViewVisibilityChanged(show));
                }
                WebCommand::ToggleWebViewVisible => {
                    let next = !visible_flag.load(Ordering::Relaxed);
                    window.set_visible(next);
                    if next {
                        window.set_focus();
                    }
                    visible_flag.store(next, Ordering::Relaxed);
                    auto_hide_mode.store(AUTO_HIDE_NONE, Ordering::Relaxed);
                    let _ = event_tx.send(WebEvent::WebViewVisibilityChanged(next));
                }
                WebCommand::OpenSignIn => {
                    auto_hide_mode.store(AUTO_HIDE_AFTER_SIGN_IN, Ordering::Relaxed);
                    window.set_visible(true);
                    window.set_focus();
                    visible_flag.store(true, Ordering::Relaxed);
                    let _ = event_tx.send(WebEvent::WebViewVisibilityChanged(true));
                    eval_js(&webview, "window.__ytmMask && window.__ytmMask.openSignIn();");
                }
                WebCommand::RefreshFeed(source) => {
                    pending_feed_dates = None;
                    if let Some(ref cw) = collector_webview {
                        let run = feed_run_counter.fetch_add(1, Ordering::Relaxed) + 1;
                        let start_path = match source {
                            FeedArtistSource::Collection => "library/artists",
                            FeedArtistSource::Subscribed | FeedArtistSource::Both => {
                                "library/subscriptions"
                            }
                        };
                        let url = format!(
                            "https://music.youtube.com/{start_path}#__ytm_feed={}&run={run}",
                            source.as_slug()
                        );
                        let _ = cw.load_url(&url);
                    }
                }
                WebCommand::CollectorIpc(body) => {
                    handle_collector_ipc(
                        &body,
                        collector_webview.as_ref(),
                        &mut pending_feed_dates,
                        &feed_run_counter,
                        &loop_proxy,
                        &event_tx,
                    );
                }
                WebCommand::DeezerFeedFinished {
                    run,
                    total_artists,
                    releases,
                    fallback_artists,
                } => {
                    if feed_run_counter.load(Ordering::Relaxed) != run {
                        return;
                    }
                    if fallback_artists.is_empty() || collector_webview.is_none() {
                        finish_pending_feed(
                            PendingFeedDates {
                                run,
                                scanned: total_artists,
                                total: total_artists,
                                base_releases: Vec::new(),
                                releases,
                                targets: Vec::new(),
                            },
                            collector_webview.as_ref(),
                            &event_tx,
                        );
                        pending_feed_dates = None;
                    } else {
                        let fb_count = fallback_artists.len();
                        let scanned_so_far = total_artists.saturating_sub(fb_count);
                        let _ = event_tx.send(WebEvent::FeedUpdate(FeedCollectorPayload {
                            kind: "feed_update".to_string(),
                            status: format!(
                                "Falling back to YouTube Music for {fb_count} unmatched artist{}…",
                                if fb_count == 1 { "" } else { "s" }
                            ),
                            scanned: scanned_so_far,
                            total: total_artists,
                            releases: releases.clone(),
                            done: false,
                        }));

                        pending_feed_dates = Some(PendingFeedDates {
                            run,
                            scanned: scanned_so_far,
                            total: total_artists,
                            base_releases: releases,
                            releases: Vec::new(),
                            targets: Vec::new(),
                        });

                        if let (Some(cw), Ok(fb_json)) = (
                            collector_webview.as_ref(),
                            serde_json::to_string(&fallback_artists),
                        ) {
                            let script = format!(
                                "window.__ytmRunFallbackScan && window.__ytmRunFallbackScan({fb_json}, {run});"
                            );
                            eval_js(cw, &script);
                        }
                    }
                }
                other => dispatch_js_command(&webview, other),
            },
            _ => {}
        }
    });
}

fn eval_js(webview: &WebView, script: &str) {
    let _ = webview.evaluate_script(script);
}

fn js_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

fn route_slug(route: Route) -> &'static str {
    match route {
        Route::Home => "home",
        Route::Explore => "explore",
        Route::Library => "library",
        Route::LibraryPlaylists => "library_playlists",
        Route::LibrarySongs => "library_songs",
        Route::LibraryAlbums => "library_albums",
        Route::LibraryArtists => "library_artists",
        Route::Search => "search",
        Route::Playlist => "playlist",
        Route::Album => "album",
        Route::Artist => "artist",
        Route::Other => "home",
    }
}

fn dispatch_js_command(webview: &WebView, cmd: WebCommand) {
    let script = match cmd {
        WebCommand::TogglePlay => "window.__ytmMask?.togglePlay();".to_string(),
        WebCommand::NextTrack => "window.__ytmMask?.next();".to_string(),
        WebCommand::PrevTrack => "window.__ytmMask?.prev();".to_string(),
        WebCommand::Seek(frac) => format!("window.__ytmMask?.seek({frac:.6});"),
        WebCommand::SetVolume(vol) => format!("window.__ytmMask?.setVolume({vol});"),
        WebCommand::ToggleMute => "window.__ytmMask?.toggleMute();".to_string(),
        WebCommand::ToggleLike => "window.__ytmMask?.toggleLike();".to_string(),
        WebCommand::ToggleDislike => "window.__ytmMask?.toggleDislike();".to_string(),
        WebCommand::ToggleShuffle => "window.__ytmMask?.toggleShuffle();".to_string(),
        WebCommand::CycleRepeat => "window.__ytmMask?.cycleRepeat();".to_string(),
        WebCommand::NavigateRoute(route) => {
            format!("window.__ytmMask?.navigateRoute({});", js_str(route_slug(route)))
        }
        WebCommand::NavigateUrl(url) => {
            format!("window.__ytmMask?.navigateUrl({});", js_str(&url))
        }
        WebCommand::PlayRelease { href, video_id } => {
            let vid_js = video_id
                .as_deref()
                .map(js_str)
                .unwrap_or_else(|| "null".to_string());
            format!(
                "window.__ytmMask?.playRelease({}, {vid_js});",
                js_str(&href)
            )
        }
        WebCommand::ResolveRelease {
            artist,
            title,
            release_type,
            auto_play,
        } => format!(
            "window.__ytmMask?.resolveRelease({}, {}, {}, {});",
            js_str(&artist),
            js_str(&title),
            js_str(&release_type),
            if auto_play { "true" } else { "false" }
        ),
        WebCommand::Search(query) => {
            format!("window.__ytmMask?.search({});", js_str(&query))
        }
        WebCommand::ClickItem(id) => {
            format!("window.__ytmMask?.clickItem({});", js_str(&id))
        }
        WebCommand::OpenMenu(id) => {
            format!("window.__ytmMask?.openMenu({});", js_str(&id))
        }
        WebCommand::CloseMenu => "window.__ytmMask?.closeMenu();".to_string(),
        WebCommand::SelectMenuItem(id) => {
            format!("window.__ytmMask?.selectMenuItem({});", js_str(&id))
        }
        WebCommand::CreatePlaylist {
            title,
            description,
            privacy,
        } => format!(
            "window.__ytmMask?.createPlaylist({}, {}, {});",
            js_str(&title),
            js_str(&description),
            js_str(&privacy)
        ),
        WebCommand::GoBack => "window.__ytmMask?.goBack();".to_string(),
        WebCommand::GoForward => "window.__ytmMask?.goForward();".to_string(),
        WebCommand::Refresh => "window.__ytmMask?.refresh();".to_string(),
        WebCommand::ScrollMore => "window.__ytmMask?.scrollMore();".to_string(),
        WebCommand::RequestQueue => "window.__ytmMask?.requestQueue();".to_string(),
        WebCommand::RequestLyrics => "window.__ytmMask?.requestLyrics();".to_string(),
        WebCommand::RequestRelated => "window.__ytmMask?.requestRelated();".to_string(),
        WebCommand::ClosePlayerPage => "window.__ytmMask?.closePlayerPage();".to_string(),
        WebCommand::RefreshFeed(_)
        | WebCommand::CollectorIpc(_)
        | WebCommand::DeezerFeedFinished { .. }
        | WebCommand::SetWebViewVisible(_)
        | WebCommand::ToggleWebViewVisible
        | WebCommand::OpenSignIn
        | WebCommand::Shutdown => return,
    };
    eval_js(webview, &script);
}

fn merge_feed_releases(base: &[FeedRelease], extra: Vec<FeedRelease>) -> Vec<FeedRelease> {
    let mut out = base.to_vec();
    let mut seen: HashSet<String> = base
        .iter()
        .map(|r| {
            format!(
                "{}|{}",
                r.artist_name.trim().to_lowercase(),
                r.title.trim().to_lowercase()
            )
        })
        .collect();
    for r in extra {
        let key = format!(
            "{}|{}",
            r.artist_name.trim().to_lowercase(),
            r.title.trim().to_lowercase()
        );
        if seen.insert(key) {
            out.push(r);
        }
    }
    out
}

fn apply_resolved_date(release: &mut FeedRelease, date: Option<String>) {
    if let Some(d) = date.filter(|s| s.len() >= 10) {
        if release.year.is_none() {
            if let Ok(y) = d[..4].parse::<u16>() {
                release.year = Some(y);
            }
        }
        release.release_date = Some(d);
    }
}

fn finish_pending_feed(
    pending: PendingFeedDates,
    collector: Option<&WebView>,
    event_tx: &UnboundedSender<WebEvent>,
) {
    let _ = event_tx.send(WebEvent::FeedUpdate(FeedCollectorPayload {
        kind: "feed_update".to_string(),
        status: format!("Up to date • {} artists scanned", pending.total),
        scanned: pending.total,
        total: pending.total,
        releases: pending.releases,
        done: true,
    }));
    if let Some(cw) = collector {
        let _ = cw.load_url("about:blank");
    }
}

fn handle_collector_ipc(
    body: &str,
    collector: Option<&WebView>,
    pending_slot: &mut Option<PendingFeedDates>,
    active_run: &Arc<AtomicU64>,
    loop_proxy: &EventLoopProxy<WebCommand>,
    event_tx: &UnboundedSender<WebEvent>,
) {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(body) else {
        return;
    };
    let kind = val.get("kind").and_then(|k| k.as_str()).unwrap_or("");

    match kind {
        "feed_update" => {
            if let Ok(payload) = serde_json::from_value::<FeedCollectorPayload>(val) {
                let _ = event_tx.send(WebEvent::FeedUpdate(payload));
            }
        }
        "feed_artists_ready" => {
            let Ok(msg) = serde_json::from_value::<CollectorArtistsReady>(val) else {
                return;
            };
            if active_run.load(Ordering::Relaxed) != msg.run {
                return;
            }
            let total_artists = msg.artists.len();
            let _ = event_tx.send(WebEvent::FeedUpdate(FeedCollectorPayload {
                kind: "feed_update".to_string(),
                status: format!("Found {total_artists} artists • checking Deezer releases…"),
                scanned: 0,
                total: total_artists,
                releases: Vec::new(),
                done: false,
            }));

            let run = msg.run;
            let artists = msg.artists;
            let active_run_bg = active_run.clone();
            let event_tx_bg = event_tx.clone();
            let proxy_bg = loop_proxy.clone();

            std::thread::Builder::new()
                .name("ytm-deezer-feed".to_string())
                .spawn(move || {
                    if let Some(out) =
                        deezer::collect_releases_via_deezer(artists, run, active_run_bg, event_tx_bg)
                    {
                        let _ = proxy_bg.send_event(WebCommand::DeezerFeedFinished {
                            run,
                            total_artists,
                            releases: out.releases,
                            fallback_artists: out.fallback_artists,
                        });
                    }
                })
                .ok();
        }
        "feed_fallback_progress" => {
            let Ok(prog) = serde_json::from_value::<CollectorFallbackProgress>(val) else {
                return;
            };
            let Some(pending) = pending_slot.as_ref() else {
                return;
            };
            if pending.run != prog.run {
                return;
            }
            let combined = merge_feed_releases(&pending.base_releases, prog.releases);
            let scanned = pending
                .total
                .saturating_sub(prog.fallback_total)
                .saturating_add(prog.fallback_done)
                .min(pending.total);
            let _ = event_tx.send(WebEvent::FeedUpdate(FeedCollectorPayload {
                kind: "feed_update".to_string(),
                status: prog.status,
                scanned,
                total: pending.total,
                releases: combined,
                done: false,
            }));
        }
        "feed_phase1_done" => {
            let Ok(mut phase1) = serde_json::from_value::<CollectorPhase1Done>(val) else {
                return;
            };
            if active_run.load(Ordering::Relaxed) != phase1.run {
                return;
            }

            let (base_releases, total_artists) = match pending_slot.as_ref() {
                Some(p) if p.run == phase1.run => (p.base_releases.clone(), p.total),
                _ => (Vec::new(), 0),
            };

            // Reuse previously verified exact release dates from local cache for fallback items
            let cached = FeedCache::load();
            let mut cached_dates: HashMap<String, (Option<String>, Option<String>)> =
                HashMap::new();
            for r in cached.releases {
                if r.release_date.is_some() || r.video_id.is_some() {
                    let key = format!("{}|{}", r.href.to_lowercase(), r.title.to_lowercase());
                    cached_dates.insert(key, (r.release_date, r.video_id));
                }
            }
            for r in &mut phase1.releases {
                let key = format!("{}|{}", r.href.to_lowercase(), r.title.to_lowercase());
                if let Some((c_date, c_vid)) = cached_dates.get(&key) {
                    if r.release_date.is_none() && c_date.is_some() {
                        apply_resolved_date(r, c_date.clone());
                    }
                    if r.video_id.is_none() && c_vid.is_some() {
                        r.video_id = c_vid.clone();
                    }
                }
            }

            let combined = merge_feed_releases(&base_releases, phase1.releases);
            let min_year = phase1.min_recent_year;
            let targets: Vec<YtProbeTarget> = combined
                .iter()
                .enumerate()
                .filter_map(|(idx, r)| {
                    if !r.has_ytm_href() || r.release_date.is_some() {
                        return None;
                    }
                    let is_recent = r.is_latest || r.year.is_some_and(|y| y >= min_year);
                    if !is_recent {
                        return None;
                    }
                    let vid = r.video_id.as_ref()?.trim();
                    if vid.len() != 11 {
                        return None;
                    }
                    Some(YtProbeTarget {
                        idx,
                        video_id: vid.to_string(),
                        title: r.title.clone(),
                        year: r.year,
                    })
                })
                .take(250)
                .collect();

            if targets.is_empty() || collector.is_none() {
                finish_pending_feed(
                    PendingFeedDates {
                        run: phase1.run,
                        scanned: total_artists,
                        total: total_artists,
                        base_releases: Vec::new(),
                        releases: combined,
                        targets: Vec::new(),
                    },
                    collector,
                    event_tx,
                );
                *pending_slot = None;
                return;
            }

            let first_vid = targets[0].video_id.clone();
            let first_title = targets[0].title.clone();
            let total_targets = targets.len();
            let run = phase1.run;

            let _ = event_tx.send(WebEvent::FeedUpdate(FeedCollectorPayload {
                kind: "feed_update".to_string(),
                status: format!(
                    "Checking exact fallback date: {first_title} (1/{total_targets})…"
                ),
                scanned: total_artists,
                total: total_artists,
                releases: combined.clone(),
                done: false,
            }));

            *pending_slot = Some(PendingFeedDates {
                run,
                scanned: total_artists,
                total: total_artists,
                base_releases: Vec::new(),
                releases: combined,
                targets,
            });

            if let Some(cw) = collector {
                let url = format!(
                    "https://www.youtube.com/watch?v={first_vid}#__ytm_yt_dates=1&run={run}"
                );
                let _ = cw.load_url(&url);
            }
        }
        "yt_probe_ready" => {
            let Ok(ready) = serde_json::from_value::<YtProbeReady>(val) else {
                return;
            };
            let Some(pending) = pending_slot.as_mut() else {
                return;
            };
            if pending.run != ready.run {
                return;
            }
            if let Some(first) = pending.targets.first() {
                if let Some(rel) = pending.releases.get_mut(first.idx) {
                    apply_resolved_date(rel, ready.release_date);
                }
            }
            let total_targets = pending.targets.len();
            if total_targets <= 1 {
                if let Some(done_pending) = pending_slot.take() {
                    finish_pending_feed(done_pending, collector, event_tx);
                }
                return;
            }

            let _ = event_tx.send(WebEvent::FeedUpdate(FeedCollectorPayload {
                kind: "feed_update".to_string(),
                status: format!("Checking exact release dates (1/{total_targets})…"),
                scanned: pending.scanned,
                total: pending.total,
                releases: pending.releases.clone(),
                done: false,
            }));

            let remaining = &pending.targets[1..];
            if let (Some(cw), Ok(batch_json)) = (collector, serde_json::to_string(remaining)) {
                let script = format!(
                    "window.__ytmRunYtDateBatch && window.__ytmRunYtDateBatch({batch_json}, {});",
                    pending.run
                );
                eval_js(cw, &script);
            } else if let Some(done_pending) = pending_slot.take() {
                finish_pending_feed(done_pending, collector, event_tx);
            }
        }
        "yt_probe_item" => {
            let Ok(item) = serde_json::from_value::<YtProbeItem>(val) else {
                return;
            };
            let Some(pending) = pending_slot.as_mut() else {
                return;
            };
            if pending.run != item.run {
                return;
            }
            if let Some(rel) = pending.releases.get_mut(item.idx) {
                apply_resolved_date(rel, item.release_date);
            }
            let total_all = pending.targets.len();
            let current = (item.done_count + 1).min(total_all);
            let label = if !item.title.is_empty() {
                format!(
                    "Checking exact release date: {} ({current}/{total_all})…",
                    item.title
                )
            } else {
                format!(
                    "Checking exact release dates ({}/{})…",
                    current,
                    item.total_count + 1
                )
            };
            let _ = event_tx.send(WebEvent::FeedUpdate(FeedCollectorPayload {
                kind: "feed_update".to_string(),
                status: label,
                scanned: pending.scanned,
                total: pending.total,
                releases: pending.releases.clone(),
                done: false,
            }));
        }
        "yt_probe_done" => {
            let Ok(done) = serde_json::from_value::<YtProbeDone>(val) else {
                return;
            };
            if pending_slot.as_ref().is_some_and(|p| p.run == done.run) {
                if let Some(done_pending) = pending_slot.take() {
                    finish_pending_feed(done_pending, collector, event_tx);
                }
            }
        }
        _ => {}
    }
}

use std::collections::HashSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use gpui::{
    App, ClipboardItem, Context, Entity, FocusHandle, Focusable, KeyBinding, MouseButton,
    NavigationDirection, Pixels, Point, ScrollWheelEvent, Subscription, Timer, Window, actions,
    div, point, prelude::*, px,
};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::protocol::{
    BridgeState, FeedArtistSource, FeedRelease, PlaybackMode, Route, ShareDialog, ShareTarget,
    VideoEmbedMode, VideoUiAction, VideoViewport, WebCommand, WebEvent,
};
use crate::settings::{AppSettings, FeedCache};
use crate::smooth_scroll::SmoothScroller;
use crate::text_input::{SearchInput, SearchInputEvent};
use crate::theme::TidalTheme;
use crate::time_util::{
    FeedBucket, classify_release_bucket, most_recent_scheduled_unix, now_local, now_unix_secs,
};
use crate::ui::{content, drawer, feed_view, overlays, player_bar, settings_view, sidebar, topbar};
use crate::webview::WebController;
use crate::win_video;

actions!(
    ytm_app,
    [
        TogglePlayPause,
        FocusSearchBar,
        ToggleWebView,
        NavBack,
        NavForward,
        DismissOverlay,
    ]
);

pub fn register_app_keybindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("space", TogglePlayPause, Some("YtmApp")),
        KeyBinding::new("ctrl-k", FocusSearchBar, None),
        KeyBinding::new("ctrl-shift-w", ToggleWebView, None),
        KeyBinding::new("alt-left", NavBack, None),
        KeyBinding::new("alt-right", NavForward, None),
        KeyBinding::new("escape", DismissOverlay, Some("YtmApp")),
    ]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    Webapp,
    Feed,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedFilter {
    All,
    Albums,
    SinglesAndEps,
}

pub const FEED_HEADER_HEIGHT: f32 = 42.0;
pub const FEED_ROW_HEIGHT: f32 = 76.0;

#[derive(Debug, Clone, Copy)]
pub enum FeedDisplayRow {
    Header(&'static str),
    Release(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerTab {
    Queue,
    Lyrics,
    Related,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistPrivacy {
    Public,
    Unlisted,
    Private,
}

impl PlaylistPrivacy {
    pub fn as_str(self) -> &'static str {
        match self {
            PlaylistPrivacy::Public => "public",
            PlaylistPrivacy::Unlisted => "unlisted",
            PlaylistPrivacy::Private => "private",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    Content,
    Feed,
    Settings,
    Sidebar,
    Drawer,
}

#[derive(Debug, Clone)]
pub struct ArtworkLightbox {
    pub url: String,
    pub is_circular: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NavTarget {
    Webapp(String),
    Feed,
    Settings,
}

#[derive(Debug, Clone)]
struct NavHistoryEntry {
    target: NavTarget,
    scroll: Point<Pixels>,
}

pub struct YtmTidalApp {
    pub state: BridgeState,
    pub controller: WebController,
    pub active_view: ActiveView,
    pub settings: AppSettings,
    pub feed_releases: Vec<FeedRelease>,
    /// Pre-grouped, flattened rows for O(log N) virtualized Feed rendering (no per-frame cloning).
    pub feed_rows: Vec<FeedDisplayRow>,
    /// Cumulative Y offset of each entry in `feed_rows` (length == `feed_rows.len()`).
    pub feed_row_offsets: Vec<f32>,
    pub feed_total_height: f32,
    pub feed_filter: FeedFilter,
    pub feed_loading: bool,
    pub feed_status: String,
    pub feed_scanned: usize,
    pub feed_total: usize,
    pub feed_last_refreshed_unix: Option<i64>,
    pub seen_feed_keys: HashSet<String>,
    pub content_scroll: SmoothScroller,
    pub feed_scroll: SmoothScroller,
    pub settings_scroll: SmoothScroller,
    pub sidebar_scroll: SmoothScroller,
    pub drawer_scroll: SmoothScroller,
    pub search_input: Entity<SearchInput>,
    pub new_playlist_title: Entity<SearchInput>,
    pub new_playlist_desc: Entity<SearchInput>,
    pub new_playlist_privacy: PlaylistPrivacy,
    pub show_new_playlist_modal: bool,
    pub editing_playlist_form: bool,
    pub menu_open: bool,
    pub menu_position: Point<Pixels>,
    pub feed_item_menu: Option<(FeedRelease, Point<Pixels>)>,
    pub local_share_dialog: Option<ShareDialog>,
    pub share_copied_until: Option<Instant>,
    pub artwork_lightbox: Option<ArtworkLightbox>,
    pub drawer_tab: Option<DrawerTab>,
    pub video_pip: bool,
    pub video_fullscreen: bool,
    /// Exact client-space `(x, y, w, h, scale_factor)` of the 16:9 video slot (Normal or PiP).
    pub video_slot_bounds: Arc<Mutex<Option<(f32, f32, f32, f32, f32)>>>,
    last_video_viewport: Option<VideoViewport>,
    pub webview_visible: bool,
    pub status_message: String,
    pub dragging_progress: Arc<AtomicBool>,
    pub dragging_volume: Arc<AtomicBool>,
    pub progress_hover_active: Arc<AtomicBool>,
    pub progress_hover: Option<(f32, Pixels)>,
    pub volume_hover_active: Arc<AtomicBool>,
    pub volume_hover: Option<(f32, Pixels)>,
    nav_history: Vec<NavHistoryEntry>,
    nav_idx: Option<usize>,
    /// URL we intentionally navigated WebView to during Back/Forward (do not push as a new entry)
    expected_history_url: Option<String>,
    pending_restore_scroll: Option<(String, Point<Pixels>, u8)>,
    startup_feed_checked: bool,
    last_bridge_session_id: String,
    volume_restored_on_connect: bool,
    volume_restored_on_media: bool,
    last_volume_ipc: Instant,
    last_seek_ipc: Instant,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

fn sort_feed_releases(releases: &mut [FeedRelease]) {
    releases.sort_by(|a, b| {
        let a_date = a.release_date.as_deref().unwrap_or("");
        let b_date = b.release_date.as_deref().unwrap_or("");
        b_date
            .cmp(a_date)
            .then_with(|| b.is_latest.cmp(&a.is_latest))
            .then_with(|| b.year.unwrap_or(0).cmp(&a.year.unwrap_or(0)))
            .then_with(|| a.artist_name.to_lowercase().cmp(&b.artist_name.to_lowercase()))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
}

impl YtmTidalApp {
    pub fn new(
        controller: WebController,
        mut event_rx: UnboundedReceiver<WebEvent>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = AppSettings::load();
        let cache = FeedCache::load();
        let mut initial_releases = if cache.source == settings.feed_source {
            cache.releases
        } else {
            Vec::new()
        };
        sort_feed_releases(&mut initial_releases);
        let initial_scanned = if !initial_releases.is_empty() {
            cache.artists_scanned
        } else {
            0
        };
        let initial_last_refreshed = if cache.source == settings.feed_source {
            cache.last_refreshed_unix
        } else {
            None
        };
        let seen_feed_keys: HashSet<String> = cache.seen_release_keys.into_iter().collect();

        let search_input =
            cx.new(|cx| SearchInput::new("Search artists, albums, tracks, playlists…", cx));
        let new_playlist_title = cx.new(|cx| SearchInput::new_field("Playlist title", cx));
        let new_playlist_desc = cx.new(|cx| SearchInput::new_field("Optional description", cx));

        let search_sub = cx.subscribe(&search_input, |this, _, ev, cx| match ev {
            SearchInputEvent::Submit(query) => {
                this.dismiss_drawer_for_nav();
                this.save_active_scroll();
                this.active_view = ActiveView::Webapp;
                this.expected_history_url = None;
                this.controller.send(WebCommand::Search(query.clone()));
                this.status_message = format!("Searching YouTube Music for \"{query}\"…");
                cx.notify();
            }
            SearchInputEvent::Clear | SearchInputEvent::Cancel => {}
        });

        let title_sub = cx.subscribe(&new_playlist_title, |this, _, ev, cx| match ev {
            SearchInputEvent::Submit(_) => {
                this.submit_new_playlist(cx);
            }
            SearchInputEvent::Cancel => {
                this.close_new_playlist_modal(cx);
            }
            SearchInputEvent::Clear => {}
        });

        let desc_sub = cx.subscribe(&new_playlist_desc, |this, _, ev, cx| match ev {
            SearchInputEvent::Submit(_) => {
                this.submit_new_playlist(cx);
            }
            SearchInputEvent::Cancel => {
                this.close_new_playlist_modal(cx);
            }
            SearchInputEvent::Clear => {}
        });

        // Background async task driving zero-poll reactive UI updates from WebView2 IPC
        cx.spawn(async move |this, cx| {
            while let Some(event) = event_rx.recv().await {
                let updated = this.update(cx, |app, cx| {
                    match event {
                        WebEvent::StateSync(mut new_state) => {
                            if !new_state.bridge_session_id.is_empty()
                                && new_state.bridge_session_id != app.last_bridge_session_id
                            {
                                app.last_bridge_session_id = new_state.bridge_session_id.clone();
                                app.volume_restored_on_connect = false;
                                app.volume_restored_on_media = false;
                            }
                            if app.dragging_volume.load(Ordering::Relaxed)
                                || !app.volume_restored_on_connect
                            {
                                new_state.player.volume = app.state.player.volume;
                                new_state.player.is_muted = app.state.player.is_muted;
                            }
                            if app.dragging_progress.load(Ordering::Relaxed) {
                                new_state.player.current_time = app.state.player.current_time;
                            }

                            let on_watch_or_player =
                                new_state.player_page_open || is_watch_url(&new_state.url);
                            if on_watch_or_player
                                && new_state.shelves.is_empty()
                                && new_state.header.is_none()
                                && (!app.state.shelves.is_empty() || app.state.header.is_some())
                            {
                                new_state.shelves = app.state.shelves.clone();
                                new_state.header = app.state.header.clone();
                                new_state.chips = app.state.chips.clone();
                                new_state.empty_state = app.state.empty_state.clone();
                                new_state.page_ready = app.state.page_ready;
                                new_state.has_more = app.state.has_more;
                                new_state.route = app.state.route;
                            }

                            let prev_url = app.state.url.clone();
                            let next_url = new_state.url.clone();
                            if !next_url.is_empty() && next_url != prev_url {
                                app.handle_webapp_url_transition(&next_url);
                            }

                            let prev_had_form = app.state.playlist_form.is_some();
                            app.state = *new_state;
                            if let Some(ref form) = app.state.playlist_form {
                                if !prev_had_form && !app.show_new_playlist_modal {
                                    let title = form.title.clone();
                                    let desc = form.description.clone();
                                    app.new_playlist_privacy = match form.privacy.as_str() {
                                        "public" => PlaylistPrivacy::Public,
                                        "unlisted" => PlaylistPrivacy::Unlisted,
                                        _ => PlaylistPrivacy::Private,
                                    };
                                    app.editing_playlist_form = true;
                                    app.new_playlist_title.update(cx, |inp, cx| {
                                        inp.set_text(&title, cx);
                                    });
                                    app.new_playlist_desc.update(cx, |inp, cx| {
                                        inp.set_text(&desc, cx);
                                    });
                                }
                            } else {
                                app.editing_playlist_form = false;
                            }
                            if app.state.connected {
                                app.status_message.clear();
                                if !app.volume_restored_on_connect {
                                    app.volume_restored_on_connect = true;
                                    let target_vol = if app.settings.is_muted {
                                        0
                                    } else {
                                        app.settings.volume.min(100)
                                    };
                                    app.state.player.volume = app.settings.volume.min(100);
                                    app.state.player.is_muted = app.settings.is_muted;
                                    app.controller.send(WebCommand::SetVolume(target_vol));
                                }
                            }
                            if app.state.player.has_media && !app.volume_restored_on_media {
                                app.volume_restored_on_media = true;
                                let target_vol = if app.settings.is_muted {
                                    0
                                } else {
                                    app.settings.volume.min(100)
                                };
                                app.controller.send(WebCommand::SetVolume(target_vol));
                            }

                            if app.state.player.playback_mode != PlaybackMode::Video {
                                app.video_pip = false;
                                app.video_fullscreen = false;
                            }

                            // Trigger startup / missed-schedule Feed refresh once signed in
                            app.check_startup_feed_schedule(cx);

                            if app.active_view == ActiveView::Webapp {
                                app.try_apply_pending_scroll_restore();
                            }
                        }
                        WebEvent::FeedUpdate(mut payload) => {
                            app.feed_status = payload.status;
                            app.feed_scanned = payload.scanned;
                            app.feed_total = payload.total;
                            if !payload.releases.is_empty() || payload.done {
                                sort_feed_releases(&mut payload.releases);
                                app.feed_releases = payload.releases;
                                app.rebuild_feed_rows();
                            }
                            if payload.done {
                                app.feed_loading = false;
                                let now = now_unix_secs();
                                app.feed_last_refreshed_unix = Some(now);
                                if app.active_view == ActiveView::Feed {
                                    app.mark_all_feed_seen_silent();
                                } else {
                                    app.save_feed_cache();
                                }
                            }
                        }
                        WebEvent::WebViewVisibilityChanged(visible) => {
                            app.webview_visible = visible;
                        }
                        WebEvent::VideoAction(action) => match action {
                            VideoUiAction::EnterPip => app.enter_video_pip(cx),
                            VideoUiAction::ExitPipToNormal => app.exit_video_pip_to_normal(cx),
                            VideoUiAction::ClosePip => app.close_video_pip(cx),
                            VideoUiAction::ToggleFullscreen => app.toggle_video_fullscreen(cx),
                            VideoUiAction::ExitFullscreen => app.exit_video_fullscreen(cx),
                        },
                        WebEvent::StatusMessage(msg) => {
                            app.status_message = msg;
                        }
                    }
                    cx.notify();
                });

                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();

        // Lightweight background scheduler checking daily Feed refresh every 30s
        cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_secs(30)).await;
                let keep_going = this.update(cx, |app, cx| {
                    app.check_daily_feed_schedule(cx);
                });
                if keep_going.is_err() {
                    break;
                }
            }
        })
        .detach();

        // Lightweight 40ms video viewport position tracker (active ONLY while Video mode is showing)
        cx.spawn(async move |this, cx| {
            loop {
                Timer::after(Duration::from_millis(40)).await;
                let keep_going = this.update(cx, |app, _cx| {
                    app.sync_embedded_video_rect(false);
                });
                if keep_going.is_err() {
                    break;
                }
            }
        })
        .detach();

        let webview_visible = controller.is_webview_visible();
        let past = Instant::now() - Duration::from_secs(1);

        let mut initial_state = BridgeState::default();
        initial_state.player.volume = settings.volume.min(100);
        initial_state.player.is_muted = settings.is_muted;

        let mut app = Self {
            state: initial_state,
            controller,
            active_view: ActiveView::Webapp,
            settings,
            feed_releases: initial_releases,
            feed_rows: Vec::new(),
            feed_row_offsets: Vec::new(),
            feed_total_height: 0.0,
            feed_filter: FeedFilter::All,
            feed_loading: false,
            feed_status: String::new(),
            feed_scanned: initial_scanned,
            feed_total: initial_scanned,
            feed_last_refreshed_unix: initial_last_refreshed,
            seen_feed_keys,
            content_scroll: SmoothScroller::new(),
            feed_scroll: SmoothScroller::new(),
            settings_scroll: SmoothScroller::new(),
            sidebar_scroll: SmoothScroller::new(),
            drawer_scroll: SmoothScroller::new(),
            search_input,
            new_playlist_title,
            new_playlist_desc,
            new_playlist_privacy: PlaylistPrivacy::Private,
            show_new_playlist_modal: false,
            editing_playlist_form: false,
            menu_open: false,
            menu_position: point(px(200.0), px(200.0)),
            feed_item_menu: None,
            local_share_dialog: None,
            share_copied_until: None,
            artwork_lightbox: None,
            drawer_tab: None,
            video_pip: false,
            video_fullscreen: false,
            video_slot_bounds: Arc::new(Mutex::new(None)),
            last_video_viewport: None,
            webview_visible,
            status_message: "Launching embedded YouTube Music webapp…".to_string(),
            dragging_progress: Arc::new(AtomicBool::new(false)),
            dragging_volume: Arc::new(AtomicBool::new(false)),
            progress_hover_active: Arc::new(AtomicBool::new(false)),
            progress_hover: None,
            volume_hover_active: Arc::new(AtomicBool::new(false)),
            volume_hover: None,
            nav_history: Vec::new(),
            nav_idx: None,
            expected_history_url: None,
            pending_restore_scroll: None,
            startup_feed_checked: false,
            last_bridge_session_id: String::new(),
            volume_restored_on_connect: false,
            volume_restored_on_media: false,
            last_volume_ipc: past,
            last_seek_ipc: past,
            focus_handle: cx.focus_handle(),
            _subscriptions: vec![search_sub, title_sub, desc_sub],
        };
        app.rebuild_feed_rows();
        app
    }

    pub fn rebuild_feed_rows(&mut self) {
        let today = now_local();
        let filter = self.feed_filter;
        let mut this_week = Vec::new();
        let mut last_week = Vec::new();
        let mut this_month = Vec::new();
        let mut last_month = Vec::new();
        let mut older = Vec::new();

        for (idx, r) in self.feed_releases.iter().enumerate() {
            let matches_filter = match filter {
                FeedFilter::All => true,
                FeedFilter::Albums => r.release_type.eq_ignore_ascii_case("album"),
                FeedFilter::SinglesAndEps => {
                    r.release_type.eq_ignore_ascii_case("single")
                        || r.release_type.eq_ignore_ascii_case("ep")
                }
            };
            if !matches_filter {
                continue;
            }
            match classify_release_bucket(r.release_date.as_deref(), r.year, r.is_latest, today) {
                FeedBucket::ThisWeek => this_week.push(idx),
                FeedBucket::LastWeek => last_week.push(idx),
                FeedBucket::ThisMonth => this_month.push(idx),
                FeedBucket::LastMonth => last_month.push(idx),
                FeedBucket::Older => older.push(idx),
            }
        }

        self.feed_rows.clear();
        self.feed_row_offsets.clear();
        let mut y = 0.0_f32;
        for (bucket, indices) in [
            (FeedBucket::ThisWeek, this_week),
            (FeedBucket::LastWeek, last_week),
            (FeedBucket::ThisMonth, this_month),
            (FeedBucket::LastMonth, last_month),
            (FeedBucket::Older, older),
        ] {
            if indices.is_empty() {
                continue;
            }
            self.feed_row_offsets.push(y);
            self.feed_rows.push(FeedDisplayRow::Header(bucket.label()));
            y += FEED_HEADER_HEIGHT;

            for idx in indices {
                self.feed_row_offsets.push(y);
                self.feed_rows.push(FeedDisplayRow::Release(idx));
                y += FEED_ROW_HEIGHT;
            }
        }
        self.feed_total_height = y;
    }

    pub fn set_feed_filter(&mut self, filter: FeedFilter, cx: &mut Context<Self>) {
        if self.feed_filter == filter {
            return;
        }
        self.feed_filter = filter;
        self.rebuild_feed_rows();
        self.feed_scroll.set_immediate(point(px(0.0), px(0.0)));
        cx.notify();
    }

    pub fn release_key(r: &FeedRelease) -> String {
        format!("{}|{}", r.href.to_lowercase(), r.title.to_lowercase())
    }

    pub fn is_release_unseen(&self, r: &FeedRelease) -> bool {
        !self.seen_feed_keys.contains(&Self::release_key(r))
    }

    pub fn unread_feed_count(&self) -> usize {
        self.feed_releases
            .iter()
            .filter(|r| self.is_release_unseen(r))
            .count()
    }

    fn save_feed_cache(&self) {
        let cache = FeedCache {
            source: self.settings.feed_source,
            artists_scanned: self.feed_scanned,
            last_refreshed_unix: self.feed_last_refreshed_unix,
            seen_release_keys: self.seen_feed_keys.iter().cloned().collect(),
            releases: self.feed_releases.clone(),
        };
        cache.save();
    }

    pub fn mark_all_feed_seen_silent(&mut self) {
        let mut changed = false;
        for r in &self.feed_releases {
            if self.seen_feed_keys.insert(Self::release_key(r)) {
                changed = true;
            }
        }
        if changed {
            self.save_feed_cache();
        }
    }

    pub fn handle_smooth_scroll(
        &mut self,
        target: ScrollTarget,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        let scroller = match target {
            ScrollTarget::Content => &mut self.content_scroll,
            ScrollTarget::Feed => &mut self.feed_scroll,
            ScrollTarget::Settings => &mut self.settings_scroll,
            ScrollTarget::Sidebar => &mut self.sidebar_scroll,
            ScrollTarget::Drawer => &mut self.drawer_scroll,
        };
        let started = scroller.on_wheel(event);
        if started || scroller.is_animating() {
            cx.notify();
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.drawer_tab.is_some() || self.nav_idx.is_some_and(|i| i > 0)
    }

    pub fn can_go_forward(&self) -> bool {
        self.nav_idx
            .is_some_and(|i| i + 1 < self.nav_history.len())
    }

    fn current_webapp_history_url(&self) -> Option<String> {
        let idx = self.nav_idx?;
        if let Some(NavHistoryEntry {
            target: NavTarget::Webapp(url),
            ..
        }) = self.nav_history.get(idx)
            && !is_watch_url(url)
        {
            return Some(url.clone());
        }
        for i in (0..=idx).rev() {
            if let Some(NavHistoryEntry {
                target: NavTarget::Webapp(url),
                ..
            }) = self.nav_history.get(i)
                && !is_watch_url(url)
            {
                return Some(url.clone());
            }
        }
        Some("https://music.youtube.com/".to_string())
    }

    fn current_scroll_offset(&self) -> Point<Pixels> {
        match self.active_view {
            ActiveView::Webapp => self.content_scroll.offset(),
            ActiveView::Feed => self.feed_scroll.offset(),
            ActiveView::Settings => self.settings_scroll.offset(),
        }
    }

    fn save_active_scroll(&mut self) {
        let offset = self.current_scroll_offset();
        if let Some(idx) = self.nav_idx
            && let Some(entry) = self.nav_history.get_mut(idx)
        {
            entry.scroll = offset;
        }
    }

    fn push_nav_entry(&mut self, target: NavTarget) {
        self.save_active_scroll();
        if let Some(idx) = self.nav_idx {
            if self.nav_history.get(idx).is_some_and(|e| e.target == target) {
                return;
            }
            self.nav_history.truncate(idx + 1);
        }
        self.nav_history.push(NavHistoryEntry {
            target,
            scroll: point(px(0.0), px(0.0)),
        });
        self.nav_idx = Some(self.nav_history.len() - 1);
    }

    fn handle_webapp_url_transition(&mut self, next_url: &str) {
        // Never record /watch?v=... player-overlay URLs in browse history;
        // Up Next / Queue is a drawer over the current browse page, not a browse route.
        if is_watch_url(next_url) {
            return;
        }

        // Ignore transitions we explicitly triggered via unified Back/Forward navigation
        if let Some(ref expected) = self.expected_history_url {
            if expected == next_url || urls_match_loosely(expected, next_url) {
                self.expected_history_url = None;
                if let Some(idx) = self.nav_idx
                    && let Some(entry) = self.nav_history.get_mut(idx)
                {
                    entry.target = NavTarget::Webapp(next_url.to_string());
                }
                return;
            }
            self.expected_history_url = None;
        }

        // First page load initializes history
        if self.nav_history.is_empty() {
            self.nav_history.push(NavHistoryEntry {
                target: NavTarget::Webapp(next_url.to_string()),
                scroll: point(px(0.0), px(0.0)),
            });
            self.nav_idx = Some(0);
            self.content_scroll.set_immediate(point(px(0.0), px(0.0)));
            return;
        }

        // Normal forward webapp navigation (whether triggered from Webapp, Feed, or Settings)
        self.push_nav_entry(NavTarget::Webapp(next_url.to_string()));
        self.pending_restore_scroll = None;
        self.content_scroll.set_immediate(point(px(0.0), px(0.0)));
    }

    fn try_apply_pending_scroll_restore(&mut self) {
        let Some((target_url, saved_offset, remaining)) = self.pending_restore_scroll.clone()
        else {
            return;
        };

        if !urls_match_loosely(&target_url, &self.state.url) {
            return;
        }

        let has_content = !self.state.shelves.is_empty() || self.state.header.is_some();
        if !has_content {
            return;
        }

        self.content_scroll.set_immediate(saved_offset);
        let needed_y: f32 = (-saved_offset.y).into();
        let max_y = self.content_scroll.max_offset_y();

        if max_y + 2.0 >= needed_y || remaining <= 1 {
            self.pending_restore_scroll = None;
        } else {
            self.pending_restore_scroll = Some((target_url, saved_offset, remaining - 1));
        }
    }

    fn check_startup_feed_schedule(&mut self, cx: &mut Context<Self>) {
        if self.startup_feed_checked || !self.state.account.is_signed_in {
            return;
        }
        self.startup_feed_checked = true;

        if !self.settings.feed_enabled || self.feed_loading {
            return;
        }

        if self.settings.feed_refresh_on_open {
            self.refresh_feed(cx);
            return;
        }

        if self.settings.feed_refresh_daily && self.settings.feed_run_if_missed {
            let now = now_unix_secs();
            let sched = most_recent_scheduled_unix(
                now,
                now_local(),
                self.settings.feed_daily_hour,
                self.settings.feed_daily_minute,
            );
            if self.feed_last_refreshed_unix.is_none_or(|ts| ts < sched) {
                self.refresh_feed(cx);
            }
        }
    }

    fn check_daily_feed_schedule(&mut self, cx: &mut Context<Self>) {
        if !self.settings.feed_enabled
            || !self.settings.feed_refresh_daily
            || !self.state.account.is_signed_in
            || self.feed_loading
        {
            return;
        }

        let now = now_unix_secs();
        let sched = most_recent_scheduled_unix(
            now,
            now_local(),
            self.settings.feed_daily_hour,
            self.settings.feed_daily_minute,
        );
        let already_ran = self.feed_last_refreshed_unix.is_some_and(|ts| ts >= sched);
        if already_ran {
            return;
        }

        let within_live_window = (now - sched) <= 120;
        if within_live_window || self.settings.feed_run_if_missed {
            self.refresh_feed(cx);
        }
    }

    fn dismiss_drawer_for_nav(&mut self) {
        if self.drawer_tab.take().is_some() {
            self.controller.send(WebCommand::ClosePlayerPage(None));
        }
    }

    pub fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        self.dismiss_drawer_for_nav();
        let was_local = self.active_view != ActiveView::Webapp;
        self.save_active_scroll();
        self.active_view = ActiveView::Webapp;
        self.expected_history_url = None;
        self.pending_restore_scroll = None;

        // If returning from Feed/Settings to the exact route WebView is already displaying, push it explicitly
        if was_local
            && self.state.route == route
            && !self.state.url.is_empty()
            && !is_watch_url(&self.state.url)
        {
            let url = self.state.url.clone();
            self.push_nav_entry(NavTarget::Webapp(url));
            self.content_scroll.set_immediate(point(px(0.0), px(0.0)));
        }

        self.controller.send(WebCommand::NavigateRoute(route));
        cx.notify();
    }

    pub fn navigate_url_from_local(&mut self, url: String, cx: &mut Context<Self>) {
        self.dismiss_drawer_for_nav();
        let was_local = self.active_view != ActiveView::Webapp;
        self.save_active_scroll();
        self.active_view = ActiveView::Webapp;
        self.expected_history_url = None;
        self.pending_restore_scroll = None;

        let full = if url.starts_with("http") {
            url.clone()
        } else {
            format!("https://music.youtube.com/{}", url.trim_start_matches('/'))
        };

        if was_local && !is_watch_url(&full) && urls_match_loosely(&full, &self.state.url) {
            self.push_nav_entry(NavTarget::Webapp(self.state.url.clone()));
            self.content_scroll.set_immediate(point(px(0.0), px(0.0)));
        } else {
            self.controller.send(WebCommand::NavigateUrl(url));
        }
        cx.notify();
    }

    pub fn open_feed(&mut self, cx: &mut Context<Self>) {
        if !self.settings.feed_enabled {
            return;
        }
        self.dismiss_drawer_for_nav();
        self.push_nav_entry(NavTarget::Feed);
        self.active_view = ActiveView::Feed;
        self.feed_scroll.set_immediate(point(px(0.0), px(0.0)));
        // Navigating to the Feed immediately marks all currently loaded items as viewed
        self.mark_all_feed_seen_silent();
        if self.feed_releases.is_empty() && !self.feed_loading && self.state.account.is_signed_in {
            self.refresh_feed(cx);
        } else {
            cx.notify();
        }
    }

    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.dismiss_drawer_for_nav();
        self.push_nav_entry(NavTarget::Settings);
        self.active_view = ActiveView::Settings;
        self.settings_scroll.set_immediate(point(px(0.0), px(0.0)));
        cx.notify();
    }

    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.drawer_tab.is_some() {
            self.close_drawer(cx);
            return;
        }
        let Some(idx) = self.nav_idx else { return };
        if idx == 0 {
            return;
        }
        self.save_active_scroll();
        self.step_to_history_index(idx - 1, cx);
    }

    pub fn go_forward(&mut self, cx: &mut Context<Self>) {
        self.dismiss_drawer_for_nav();
        let Some(idx) = self.nav_idx else { return };
        if idx + 1 >= self.nav_history.len() {
            return;
        }
        self.save_active_scroll();
        self.step_to_history_index(idx + 1, cx);
    }

    fn step_to_history_index(&mut self, target_idx: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.nav_history.get(target_idx).cloned() else {
            return;
        };
        self.nav_idx = Some(target_idx);

        match entry.target {
            NavTarget::Feed => {
                self.active_view = ActiveView::Feed;
                self.pending_restore_scroll = None;
                self.feed_scroll.set_immediate(entry.scroll);
                self.mark_all_feed_seen_silent();
            }
            NavTarget::Settings => {
                self.active_view = ActiveView::Settings;
                self.pending_restore_scroll = None;
                self.settings_scroll.set_immediate(entry.scroll);
            }
            NavTarget::Webapp(url) => {
                self.active_view = ActiveView::Webapp;
                self.pending_restore_scroll = Some((url.clone(), entry.scroll, 12));
                if urls_match_loosely(&url, &self.state.url) {
                    self.expected_history_url = None;
                    self.content_scroll.set_immediate(entry.scroll);
                    if self.state.player_page_open {
                        self.controller
                            .send(WebCommand::ClosePlayerPage(Some(url)));
                    }
                } else {
                    self.expected_history_url = Some(url.clone());
                    self.controller.send(WebCommand::NavigateUrl(url));
                }
            }
        }
        cx.notify();
    }

    pub fn refresh_feed(&mut self, cx: &mut Context<Self>) {
        self.feed_loading = true;
        self.feed_scanned = 0;
        self.feed_total = 0;
        self.feed_status = "Starting background YouTube Music Feed scan…".to_string();
        self.controller
            .send(WebCommand::RefreshFeed(self.settings.feed_source));
        cx.notify();
    }

    pub fn set_feed_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.settings.feed_enabled = enabled;
        self.settings.save();
        if !enabled && self.active_view == ActiveView::Feed {
            self.active_view = ActiveView::Webapp;
        }
        cx.notify();
    }

    pub fn set_feed_source(&mut self, source: FeedArtistSource, cx: &mut Context<Self>) {
        if self.settings.feed_source == source {
            return;
        }
        self.settings.feed_source = source;
        self.settings.save();
        self.feed_releases.clear();
        self.rebuild_feed_rows();
        self.feed_scanned = 0;
        self.feed_total = 0;
        self.feed_last_refreshed_unix = None;
        if self.settings.feed_enabled && self.state.account.is_signed_in {
            self.refresh_feed(cx);
        } else {
            cx.notify();
        }
    }

    pub fn set_feed_refresh_on_open(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.settings.feed_refresh_on_open = enabled;
        self.settings.save();
        cx.notify();
    }

    pub fn set_feed_refresh_daily(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.settings.feed_refresh_daily = enabled;
        self.settings.save();
        cx.notify();
    }

    pub fn set_feed_run_if_missed(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.settings.feed_run_if_missed = enabled;
        self.settings.save();
        cx.notify();
    }

    pub fn set_daily_refresh_time(&mut self, hour: u8, minute: u8, cx: &mut Context<Self>) {
        self.settings.feed_daily_hour = hour % 24;
        self.settings.feed_daily_minute = minute % 60;
        self.settings.save();
        cx.notify();
    }

    pub fn step_daily_refresh_time(&mut self, delta_minutes: i32, cx: &mut Context<Self>) {
        let total = (self.settings.feed_daily_hour as i32) * 60
            + (self.settings.feed_daily_minute as i32);
        let next = (total + delta_minutes).rem_euclid(24 * 60);
        self.settings.feed_daily_hour = (next / 60) as u8;
        self.settings.feed_daily_minute = (next % 60) as u8;
        self.settings.save();
        cx.notify();
    }

    pub fn set_progress_hover(&mut self, hover: Option<(f32, Pixels)>, cx: &mut Context<Self>) {
        self.progress_hover = hover;
        cx.notify();
    }

    pub fn set_volume_hover(&mut self, hover: Option<(f32, Pixels)>, cx: &mut Context<Self>) {
        self.volume_hover = hover;
        cx.notify();
    }

    pub fn open_new_playlist_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_open = false;
        self.show_new_playlist_modal = true;
        self.new_playlist_privacy = PlaylistPrivacy::Private;
        self.new_playlist_title.update(cx, |input, cx| {
            input.clear(cx);
            input.focus(window);
        });
        self.new_playlist_desc.update(cx, |input, cx| {
            input.clear(cx);
        });
        cx.notify();
    }

    pub fn close_new_playlist_modal(&mut self, cx: &mut Context<Self>) {
        self.show_new_playlist_modal = false;
        if self.editing_playlist_form {
            self.editing_playlist_form = false;
            self.controller.send(WebCommand::CloseMenu);
        }
        cx.notify();
    }

    pub fn submit_new_playlist(&mut self, cx: &mut Context<Self>) {
        let title = self.new_playlist_title.read(cx).text().trim().to_string();
        if title.is_empty() {
            return;
        }
        let description = self.new_playlist_desc.read(cx).text().trim().to_string();
        let privacy = self.new_playlist_privacy.as_str().to_string();

        if self.editing_playlist_form {
            self.editing_playlist_form = false;
            self.controller.send(WebCommand::SavePlaylistForm {
                title,
                description,
                privacy,
            });
        } else {
            self.show_new_playlist_modal = false;
            self.controller.send(WebCommand::CreatePlaylist {
                title,
                description,
                privacy,
            });
        }
        cx.notify();
    }

    fn has_active_overlay(&self) -> bool {
        self.artwork_lightbox.is_some()
            || self.local_share_dialog.is_some()
            || self.state.share_dialog.is_some()
            || self.state.confirm_dialog.is_some()
            || self.show_new_playlist_modal
            || self.editing_playlist_form
            || self.state.playlist_form.is_some()
            || !self.state.add_to_playlist.is_empty()
            || self.feed_item_menu.is_some()
            || (self.menu_open && !self.state.popup_menu.is_empty())
    }

    pub fn enter_video_pip(&mut self, cx: &mut Context<Self>) {
        if self.state.player.playback_mode != PlaybackMode::Video {
            return;
        }
        self.video_fullscreen = false;
        self.video_pip = true;
        if let Ok(mut guard) = self.video_slot_bounds.lock() {
            *guard = None;
        }
        if self.drawer_tab.is_some() {
            self.close_drawer(cx);
        } else {
            self.sync_embedded_video_rect(true);
            cx.notify();
        }
    }

    pub fn exit_video_pip_to_normal(&mut self, cx: &mut Context<Self>) {
        self.video_pip = false;
        self.video_fullscreen = false;
        if let Ok(mut guard) = self.video_slot_bounds.lock() {
            *guard = None;
        }
        self.open_drawer(DrawerTab::Queue, cx);
    }

    pub fn close_video_pip(&mut self, cx: &mut Context<Self>) {
        self.video_pip = false;
        self.video_fullscreen = false;
        self.sync_embedded_video_rect(true);
        cx.notify();
    }

    pub fn toggle_video_fullscreen(&mut self, cx: &mut Context<Self>) {
        if self.video_fullscreen {
            self.exit_video_fullscreen(cx);
        } else if self.state.player.playback_mode == PlaybackMode::Video {
            self.video_fullscreen = true;
            self.sync_embedded_video_rect(true);
            cx.notify();
        }
    }

    pub fn exit_video_fullscreen(&mut self, cx: &mut Context<Self>) {
        if !self.video_fullscreen {
            return;
        }
        self.video_fullscreen = false;
        self.sync_embedded_video_rect(true);
        cx.notify();
    }

    pub fn sync_embedded_video_rect(&mut self, force_hide_check: bool) {
        let is_video = self.state.player.playback_mode == PlaybackMode::Video;
        let mode = if !is_video || self.webview_visible || self.has_active_overlay() {
            None
        } else if self.video_fullscreen {
            Some(VideoEmbedMode::Fullscreen)
        } else if self.drawer_tab.is_some() {
            Some(VideoEmbedMode::Normal)
        } else if self.video_pip {
            Some(VideoEmbedMode::Pip)
        } else {
            None
        };

        let next_viewport = mode.and_then(|m| {
            let rect = match m {
                VideoEmbedMode::Fullscreen => win_video::host_monitor_screen_rect("Riptide"),
                VideoEmbedMode::Normal | VideoEmbedMode::Pip => {
                    let slot = self.video_slot_bounds.lock().ok().and_then(|g| *g);
                    slot.and_then(|(x, y, w, h, scale)| {
                        win_video::host_video_screen_rect("Riptide", x, y, w, h, scale)
                    })
                }
            }?;
            Some(VideoViewport { rect, mode: m })
        });

        if next_viewport != self.last_video_viewport
            || (force_hide_check
                && next_viewport.is_none()
                && self.last_video_viewport.is_some())
        {
            self.last_video_viewport = next_viewport;
            self.controller
                .send(WebCommand::SyncVideoViewport(next_viewport));
        }
    }

    pub fn open_context_menu(
        &mut self,
        menu_id: String,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.feed_item_menu = None;
        self.menu_position = position;
        self.menu_open = true;
        self.controller.send(WebCommand::OpenMenu(menu_id));
        cx.notify();
    }

    pub fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = false;
        self.feed_item_menu = None;
        self.controller.send(WebCommand::CloseMenu);
        cx.notify();
    }

    pub fn open_feed_item_menu(
        &mut self,
        release: FeedRelease,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.menu_open = false;
        self.feed_item_menu = Some((release, position));
        cx.notify();
    }

    pub fn close_feed_item_menu(&mut self, cx: &mut Context<Self>) {
        self.feed_item_menu = None;
        cx.notify();
    }

    pub fn open_feed_release(&mut self, rel: &FeedRelease, cx: &mut Context<Self>) {
        self.feed_item_menu = None;
        if rel.has_ytm_href() {
            self.navigate_url_from_local(rel.href.clone(), cx);
        } else {
            self.save_active_scroll();
            self.active_view = ActiveView::Webapp;
            self.expected_history_url = None;
            self.content_scroll.set_immediate(point(px(0.0), px(0.0)));
            self.status_message = format!("Opening \"{}\" on YouTube Music…", rel.title);
            self.controller.send(WebCommand::ResolveRelease {
                artist: rel.artist_name.clone(),
                title: rel.title.clone(),
                release_type: rel.release_type.clone(),
                auto_play: false,
            });
            cx.notify();
        }
    }

    pub fn play_feed_release(&mut self, rel: &FeedRelease, cx: &mut Context<Self>) {
        self.feed_item_menu = None;
        if rel.has_ytm_href() {
            if rel.video_id.is_none() {
                self.active_view = ActiveView::Webapp;
            }
            self.controller.send(WebCommand::PlayRelease {
                href: rel.href.clone(),
                video_id: rel.video_id.clone(),
            });
        } else {
            self.status_message = format!("Playing \"{}\" on YouTube Music…", rel.title);
            self.controller.send(WebCommand::ResolveRelease {
                artist: rel.artist_name.clone(),
                title: rel.title.clone(),
                release_type: rel.release_type.clone(),
                auto_play: true,
            });
        }
        cx.notify();
    }

    pub fn open_feed_share_dialog(&mut self, rel: &FeedRelease, cx: &mut Context<Self>) {
        self.feed_item_menu = None;
        let full_url = rel.shareable_url();
        let mut targets = Vec::new();
        if let Some(ref vid) = rel.video_id {
            targets.push(ShareTarget {
                id: String::new(),
                label: "YouTube".to_string(),
                url: Some(format!("https://www.youtube.com/watch?v={vid}")),
            });
        }
        if let Some(ref ext) = rel.external_url {
            if !ext.is_empty() {
                targets.push(ShareTarget {
                    id: String::new(),
                    label: "Deezer".to_string(),
                    url: Some(ext.clone()),
                });
            }
        }
        targets.push(ShareTarget {
            id: String::new(),
            label: "Open in Browser".to_string(),
            url: Some(full_url.clone()),
        });
        self.local_share_dialog = Some(ShareDialog {
            title: format!("Share \"{}\"", rel.title),
            url: full_url,
            targets,
            copy_id: None,
            close_id: None,
        });
        self.share_copied_until = None;
        cx.notify();
    }

    pub fn copy_text_with_feedback(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.share_copied_until = Some(Instant::now() + Duration::from_secs(2));
        cx.notify();
        cx.spawn(async move |this, cx| {
            Timer::after(Duration::from_secs(2)).await;
            let _ = this.update(cx, |app, cx| {
                if app
                    .share_copied_until
                    .is_some_and(|t| Instant::now() >= t)
                {
                    app.share_copied_until = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn is_share_copied(&self) -> bool {
        self.share_copied_until
            .is_some_and(|until| Instant::now() < until)
    }

    pub fn close_share_dialog(&mut self, cx: &mut Context<Self>) {
        self.local_share_dialog = None;
        self.share_copied_until = None;
        if let Some(ref dlg) = self.state.share_dialog {
            if let Some(ref cid) = dlg.close_id {
                self.controller.send(WebCommand::ClickItem(cid.clone()));
            } else {
                self.controller.send(WebCommand::CloseMenu);
            }
        }
        cx.notify();
    }

    pub fn open_artwork_lightbox(
        &mut self,
        url: String,
        is_circular: bool,
        cx: &mut Context<Self>,
    ) {
        self.artwork_lightbox = Some(ArtworkLightbox {
            url: upscale_artwork_url(&url),
            is_circular,
        });
        cx.notify();
    }

    pub fn close_artwork_lightbox(&mut self, cx: &mut Context<Self>) {
        self.artwork_lightbox = None;
        cx.notify();
    }

    pub fn set_volume_live(&mut self, frac: f32, commit: bool, cx: &mut Context<Self>) {
        let vol = (frac.clamp(0.0, 1.0) * 100.0).round() as u8;
        self.state.player.volume = vol;
        self.state.player.is_muted = vol == 0;
        self.settings.volume = vol;
        self.settings.is_muted = vol == 0;
        let now = Instant::now();
        if commit || now.duration_since(self.last_volume_ipc) >= Duration::from_millis(35) {
            self.last_volume_ipc = now;
            self.controller.send(WebCommand::SetVolume(vol));
        }
        if commit {
            self.settings.save();
        }
        cx.notify();
    }

    pub fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let next_muted = !self.state.player.is_muted;
        self.state.player.is_muted = next_muted;
        self.settings.is_muted = next_muted;
        if !next_muted && self.settings.volume == 0 {
            self.settings.volume = 50;
            self.state.player.volume = 50;
            self.controller.send(WebCommand::SetVolume(50));
        } else {
            self.controller.send(WebCommand::ToggleMute);
        }
        self.settings.save();
        cx.notify();
    }

    pub fn seek_live(&mut self, frac: f32, commit: bool, cx: &mut Context<Self>) {
        let clamped = frac.clamp(0.0, 1.0) as f64;
        if self.state.player.duration > 0.0 {
            self.state.player.current_time = clamped * self.state.player.duration;
        }
        let now = Instant::now();
        if commit || now.duration_since(self.last_seek_ipc) >= Duration::from_millis(80) {
            self.last_seek_ipc = now;
            self.controller.send(WebCommand::Seek(clamped));
        }
        cx.notify();
    }

    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input.update(cx, |input, _| {
            input.focus(window);
        });
    }

    pub fn close_drawer(&mut self, cx: &mut Context<Self>) {
        self.drawer_tab = None;
        if self.active_view == ActiveView::Webapp {
            let fallback_url = self.current_webapp_history_url();
            if let Some(ref url) = fallback_url
                && (is_watch_url(&self.state.url) || self.state.player_page_open)
            {
                self.expected_history_url = Some(url.clone());
            }
            self.controller
                .send(WebCommand::ClosePlayerPage(fallback_url));
        } else {
            self.controller.send(WebCommand::ClosePlayerPage(None));
        }
        cx.notify();
    }

    pub fn toggle_drawer(&mut self, tab: DrawerTab, cx: &mut Context<Self>) {
        if self.drawer_tab == Some(tab) {
            self.close_drawer(cx);
        } else {
            self.open_drawer(tab, cx);
        }
    }

    pub fn open_drawer(&mut self, tab: DrawerTab, cx: &mut Context<Self>) {
        if self.drawer_tab != Some(tab) {
            self.drawer_scroll.set_immediate(point(px(0.0), px(0.0)));
        }
        if self.video_pip {
            self.video_pip = false;
            if let Ok(mut guard) = self.video_slot_bounds.lock() {
                *guard = None;
            }
        }
        self.drawer_tab = Some(tab);
        match tab {
            DrawerTab::Queue => self.controller.send(WebCommand::RequestQueue),
            DrawerTab::Lyrics => self.controller.send(WebCommand::RequestLyrics),
            DrawerTab::Related => self.controller.send(WebCommand::RequestRelated),
        }
        cx.notify();
    }

    pub fn move_queue_item(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let len = self.state.queue.len();
        if from == to || from >= len || to >= len {
            return;
        }
        let item = self.state.queue.remove(from);
        self.state.queue.insert(to, item);
        self.controller.send(WebCommand::MoveQueueItem { from, to });
        cx.notify();
    }

    pub fn play_queue_item(&mut self, idx: usize, cx: &mut Context<Self>) {
        // Optimistically mark the clicked row as playing without scrolling Up Next
        for (i, item) in self.state.queue.iter_mut().enumerate() {
            item.is_playing = i == idx;
        }
        self.controller.send(WebCommand::PlayQueueItem(idx));
        cx.notify();
    }

    fn any_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.search_input.read(cx).is_focused(window)
            || self.new_playlist_title.read(cx).is_focused(window)
            || self.new_playlist_desc.read(cx).is_focused(window)
    }

    fn on_toggle_play(&mut self, _: &TogglePlayPause, window: &mut Window, cx: &mut Context<Self>) {
        if !self.any_input_focused(window, cx)
            && !self.show_new_playlist_modal
            && !self.editing_playlist_form
        {
            self.controller.send(WebCommand::TogglePlay);
        }
    }

    fn on_focus_search(&mut self, _: &FocusSearchBar, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_search(window, cx);
    }

    fn on_toggle_webview(&mut self, _: &ToggleWebView, _: &mut Window, _: &mut Context<Self>) {
        self.controller.send(WebCommand::ToggleWebViewVisible);
    }

    fn on_nav_back(&mut self, _: &NavBack, _: &mut Window, cx: &mut Context<Self>) {
        self.go_back(cx);
    }

    fn on_nav_forward(&mut self, _: &NavForward, _: &mut Window, cx: &mut Context<Self>) {
        self.go_forward(cx);
    }

    fn on_dismiss_overlay(
        &mut self,
        _: &DismissOverlay,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.video_fullscreen {
            self.exit_video_fullscreen(cx);
        } else if self.artwork_lightbox.is_some() {
            self.close_artwork_lightbox(cx);
        } else if self.local_share_dialog.is_some() || self.state.share_dialog.is_some() {
            self.close_share_dialog(cx);
        } else if self.show_new_playlist_modal || self.editing_playlist_form {
            self.close_new_playlist_modal(cx);
        } else if let Some(ref dlg) = self.state.confirm_dialog {
            if let Some(ref cancel_id) = dlg.cancel_id {
                self.controller
                    .send(WebCommand::ClickItem(cancel_id.clone()));
            } else {
                self.controller.send(WebCommand::CloseMenu);
            }
        } else if self.feed_item_menu.is_some() {
            self.close_feed_item_menu(cx);
        } else if self.menu_open || !self.state.add_to_playlist.is_empty() {
            self.close_context_menu(cx);
        }
    }
}

pub fn upscale_artwork_url(url: &str) -> String {
    if url.contains("lh3.googleusercontent.com")
        || url.contains("yt3.ggpht.com")
        || url.contains("yt3.googleusercontent.com")
    {
        if let Some(idx) = url.rfind("=w") {
            return format!("{}=w1200-h1200-l90-rj", &url[..idx]);
        }
        if let Some(idx) = url.rfind("=s") {
            return format!("{}=s1200-c-k-c0x00ffffff-no-rj", &url[..idx]);
        }
    }
    url.to_string()
}

fn urls_match_loosely(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim_end_matches('/').to_lowercase();
    norm(a) == norm(b)
}

fn is_watch_url(url: &str) -> bool {
    let trimmed = url.trim().to_lowercase();
    let path = if let Some(rest) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    {
        match rest.find('/') {
            Some(i) => &rest[i + 1..],
            None => "",
        }
    } else {
        trimmed.trim_start_matches('/')
    };
    path == "watch" || path.starts_with("watch?") || path.starts_with("watch/")
}

impl Focusable for YtmTidalApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for YtmTidalApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.active_view == ActiveView::Webapp {
            self.try_apply_pending_scroll_restore();
        }

        // Advance all active VSync-locked smooth scrollers using exact frame delta time
        let mut any_scrolling = false;
        any_scrolling |= self.content_scroll.step();
        any_scrolling |= self.feed_scroll.step();
        any_scrolling |= self.settings_scroll.step();
        any_scrolling |= self.sidebar_scroll.step();
        any_scrolling |= self.drawer_scroll.step();
        if any_scrolling {
            let entity = cx.entity_id();
            window.on_next_frame(move |_, cx| cx.notify(entity));
        }

        let show_drawer = self.drawer_tab.is_some();
        let show_video_pip = self.video_pip
            && !show_drawer
            && !self.video_fullscreen
            && self.state.player.playback_mode == PlaybackMode::Video;
        let overlay = overlays::render_overlays(self, window, cx);
        let active_view = self.active_view;
        self.sync_embedded_video_rect(true);

        div()
            .relative()
            .key_context("YtmApp")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_toggle_play))
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_toggle_webview))
            .on_action(cx.listener(Self::on_nav_back))
            .on_action(cx.listener(Self::on_nav_forward))
            .on_action(cx.listener(Self::on_dismiss_overlay))
            // Support mouse side Back / Forward buttons anywhere in the window
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|this, _, _, cx| {
                    this.go_back(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|this, _, _, cx| {
                    this.go_forward(cx);
                }),
            )
            .flex()
            .flex_col()
            .size_full()
            .bg(TidalTheme::bg_void())
            .text_color(TidalTheme::text_primary())
            // Upper shell: Sidebar + Main Viewport + Optional Right Drawer
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(sidebar::render_sidebar(self, cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(topbar::render_topbar(self, cx))
                            .child(if show_drawer {
                                drawer::render_now_playing_stage(self, window, cx).into_any_element()
                            } else {
                                match active_view {
                                    ActiveView::Webapp => {
                                        content::render_content(self, window, cx).into_any_element()
                                    }
                                    ActiveView::Feed => {
                                        feed_view::render_feed_view(self, window, cx)
                                            .into_any_element()
                                    }
                                    ActiveView::Settings => {
                                        settings_view::render_settings_view(self, cx)
                                            .into_any_element()
                                    }
                                }
                            }),
                    )
                    .when(show_drawer, |el| el.child(drawer::render_drawer(self, cx))),
            )
            // Persistent Bottom TIDAL-style Player Bar
            .child(player_bar::render_player_bar(self, window, cx))
            // Floating bottom-right Picture-in-Picture (Miniplayer) 16:9 video slot
            .when(show_video_pip, |el| {
                el.child(drawer::render_video_pip_slot(self))
            })
            // Floating overlays (Confirm Dialog, New Playlist Modal, Context Menu, Save to Playlist)
            .when_some(overlay, |el, modal| el.child(modal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_watch_urls_without_false_positives() {
        assert!(is_watch_url("https://music.youtube.com/watch?v=abc123&list=PL1"));
        assert!(is_watch_url("https://music.youtube.com/watch"));
        assert!(is_watch_url("/watch?v=abc123"));
        assert!(is_watch_url("watch?v=abc123"));
        assert!(!is_watch_url("https://music.youtube.com/"));
        assert!(!is_watch_url("https://music.youtube.com/playlist?list=PL1"));
        assert!(!is_watch_url("https://music.youtube.com/browse/MPREb_123"));
        assert!(!is_watch_url("https://music.youtube.com/search?q=watch+tower"));
    }
}

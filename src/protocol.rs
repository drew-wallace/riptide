use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    #[default]
    Home,
    Explore,
    Library,
    LibraryPlaylists,
    LibrarySongs,
    LibraryAlbums,
    LibraryArtists,
    Search,
    Playlist,
    Album,
    Artist,
    Other,
}

impl Route {
    pub fn label(self) -> &'static str {
        match self {
            Route::Home => "Home",
            Route::Explore => "Explore",
            Route::Library => "Library",
            Route::LibraryPlaylists => "Playlists",
            Route::LibrarySongs => "Tracks",
            Route::LibraryAlbums => "Albums",
            Route::LibraryArtists => "Artists",
            Route::Search => "Search",
            Route::Playlist => "Playlist",
            Route::Album => "Album",
            Route::Artist => "Artist",
            Route::Other => "Browse",
        }
    }

    pub fn empty_fallback(self) -> (&'static str, &'static str) {
        match self {
            Route::LibraryAlbums => (
                "No albums in your library yet",
                "Albums you save on YouTube Music will appear here.",
            ),
            Route::LibraryPlaylists => (
                "No playlists yet",
                "Create a playlist or save playlists on YouTube Music to see them here.",
            ),
            Route::LibrarySongs => (
                "No saved tracks yet",
                "Songs you like or add to your library will appear here.",
            ),
            Route::LibraryArtists => (
                "No subscribed artists yet",
                "Artists you subscribe to on YouTube Music will appear here.",
            ),
            Route::Library => (
                "Your library is empty",
                "Save playlists, albums, tracks, or artists to build your collection.",
            ),
            Route::Search => (
                "No search results found",
                "Try a different artist, album, track, or playlist query.",
            ),
            Route::Playlist => (
                "This playlist has no tracks yet",
                "Add songs from search or suggestions to populate this playlist.",
            ),
            _ => (
                "Nothing to show here yet",
                "Try refreshing or browsing another section from the sidebar.",
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FeedArtistSource {
    Subscribed,
    Collection,
    #[default]
    Both,
}

impl FeedArtistSource {
    pub fn as_slug(self) -> &'static str {
        match self {
            FeedArtistSource::Subscribed => "subscribed",
            FeedArtistSource::Collection => "collection",
            FeedArtistSource::Both => "both",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FeedArtistSource::Subscribed => "Subscribed Artists",
            FeedArtistSource::Collection => "Collection Artists",
            FeedArtistSource::Both => "Subscribed & Collection",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct FeedArtist {
    pub name: String,
    pub channel_id: String,
    pub href: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct FeedRelease {
    pub title: String,
    pub artist_name: String,
    pub artist_href: String,
    pub release_type: String,
    pub year: Option<u16>,
    /// Exact ISO release date (`YYYY-MM-DD`) when discovered from Deezer or YouTube.
    pub release_date: Option<String>,
    /// Track video ID (first song of an album/EP or single track) when sourced from YouTube Music.
    pub video_id: Option<String>,
    pub is_latest: bool,
    pub subtitle: String,
    pub artwork_url: Option<String>,
    pub href: String,
    /// Optional external reference URL (e.g. Deezer album page) when sourced outside YTM.
    pub external_url: Option<String>,
}

impl FeedRelease {
    /// Returns `true` if `href` points directly to a YouTube Music browse/playlist/watch route
    /// (as opposed to a Deezer-sourced item that resolves on YouTube Music on demand).
    pub fn has_ytm_href(&self) -> bool {
        !self.href.is_empty() && !self.href.starts_with("deezer:")
    }

    /// Best shareable URL for Copy Link / Share modal.
    pub fn shareable_url(&self) -> String {
        if self.has_ytm_href() {
            if self.href.starts_with("http") {
                self.href.clone()
            } else {
                format!(
                    "https://music.youtube.com/{}",
                    self.href.trim_start_matches('/')
                )
            }
        } else {
            let query = format!("{} {}", self.artist_name.trim(), self.title.trim());
            format!(
                "https://music.youtube.com/search?q={}",
                urlencoding::encode(query.trim())
            )
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FeedCollectorPayload {
    pub kind: String,
    pub status: String,
    pub scanned: usize,
    pub total: usize,
    pub releases: Vec<FeedRelease>,
    pub done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LikeStatus {
    #[default]
    None,
    Like,
    Dislike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

/// A text span that optionally links to an Artist, Album, or Playlist inside YouTube Music.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TextLink {
    pub text: String,
    pub href: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountState {
    pub is_signed_in: bool,
    pub name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerState {
    pub has_media: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub subtitle_links: Vec<TextLink>,
    pub artwork_url: Option<String>,
    pub is_playing: bool,
    pub current_time: f64,
    pub duration: f64,
    pub volume: u8,
    pub is_muted: bool,
    pub like_status: LikeStatus,
    pub repeat_mode: RepeatMode,
    pub is_shuffled: bool,
}

impl PlayerState {
    pub fn progress(&self) -> f32 {
        if self.duration > 0.0 {
            (self.current_time / self.duration).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterChip {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HeaderActionKind {
    Primary,
    #[default]
    Secondary,
    Toggle,
    Menu,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HeaderAction {
    pub id: String,
    pub label: String,
    pub kind: HeaderActionKind,
    pub is_active: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PageHeader {
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub subtitle_links: Vec<TextLink>,
    pub description: String,
    pub artwork_url: Option<String>,
    pub is_circular: bool,
    pub actions: Vec<HeaderAction>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EmptyPageState {
    pub title: String,
    pub subtitle: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfirmDialog {
    pub title: String,
    pub body: String,
    pub confirm_id: String,
    pub confirm_label: String,
    pub cancel_id: Option<String>,
    pub cancel_label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ShareTarget {
    pub id: String,
    pub label: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ShareDialog {
    pub title: String,
    pub url: String,
    pub targets: Vec<ShareTarget>,
    pub copy_id: Option<String>,
    pub close_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShelfKind {
    #[default]
    Grid,
    List,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaItem {
    pub id: String,
    pub play_id: Option<String>,
    pub add_id: Option<String>,
    pub like_id: Option<String>,
    pub dislike_id: Option<String>,
    pub like_status: LikeStatus,
    pub menu_id: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub secondary: String,
    pub meta_links: Vec<TextLink>,
    pub duration: String,
    pub artwork_url: Option<String>,
    pub href: Option<String>,
    pub is_explicit: bool,
    pub is_playing: bool,
    pub is_circular: bool,
    pub is_new_playlist: bool,
    pub is_browse_item: bool,
    pub badge: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ContentShelf {
    pub title: String,
    pub subtitle: String,
    pub kind: ShelfKind,
    pub items: Vec<MediaItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SidebarPlaylist {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub href: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueItem {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_links: Vec<TextLink>,
    pub duration: String,
    pub artwork_url: Option<String>,
    pub is_playing: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MenuAction {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AddToPlaylistOption {
    pub id: String,
    pub title: String,
    pub subtitle: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LyricsStatus {
    #[default]
    Idle,
    Loading,
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LyricsState {
    pub status: LyricsStatus,
    pub text: String,
    pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BridgeState {
    pub connected: bool,
    pub url: String,
    pub page_title: String,
    pub route: Route,
    pub is_loading: bool,
    pub page_ready: bool,
    pub has_more: bool,
    pub modal_open: bool,
    pub player_page_open: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub empty_state: Option<EmptyPageState>,
    pub confirm_dialog: Option<ConfirmDialog>,
    pub share_dialog: Option<ShareDialog>,
    pub account: AccountState,
    pub player: PlayerState,
    pub chips: Vec<FilterChip>,
    pub header: Option<PageHeader>,
    pub shelves: Vec<ContentShelf>,
    pub sidebar_playlists: Vec<SidebarPlaylist>,
    pub queue: Vec<QueueItem>,
    pub lyrics: LyricsState,
    pub related_shelves: Vec<ContentShelf>,
    pub popup_menu: Vec<MenuAction>,
    pub add_to_playlist: Vec<AddToPlaylistOption>,
}

#[derive(Debug, Clone)]
pub enum WebCommand {
    TogglePlay,
    NextTrack,
    PrevTrack,
    Seek(f64),
    SetVolume(u8),
    ToggleMute,
    ToggleLike,
    ToggleDislike,
    ToggleShuffle,
    CycleRepeat,
    NavigateRoute(Route),
    NavigateUrl(String),
    PlayRelease {
        href: String,
        video_id: Option<String>,
    },
    ResolveRelease {
        artist: String,
        title: String,
        release_type: String,
        auto_play: bool,
    },
    Search(String),
    ClickItem(String),
    OpenMenu(String),
    CloseMenu,
    SelectMenuItem(String),
    CreatePlaylist {
        title: String,
        description: String,
        privacy: String,
    },
    GoBack,
    GoForward,
    Refresh,
    ScrollMore,
    RequestQueue,
    RequestLyrics,
    RequestRelated,
    ClosePlayerPage,
    RefreshFeed(FeedArtistSource),
    #[doc(hidden)]
    CollectorIpc(String),
    #[doc(hidden)]
    DeezerFeedFinished {
        run: u64,
        total_artists: usize,
        releases: Vec<FeedRelease>,
        fallback_artists: Vec<FeedArtist>,
    },
    SetWebViewVisible(bool),
    ToggleWebViewVisible,
    OpenSignIn,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum WebEvent {
    StateSync(Box<BridgeState>),
    FeedUpdate(FeedCollectorPayload),
    WebViewVisibilityChanged(bool),
    StatusMessage(String),
}

pub fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00".to_string();
    }
    let total = seconds.floor() as u64;
    let hours = total / 3600;
    let mins = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours}:{mins:02}:{secs:02}")
    } else {
        format!("{mins}:{secs:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_timestamps() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(65.4), "1:05");
        assert_eq!(format_time(3661.0), "1:01:01");
        assert_eq!(format_time(-5.0), "0:00");
        assert_eq!(format_time(f64::NAN), "0:00");
    }

    #[test]
    fn deserializes_bridge_state_json() {
        let json = r#"{
            "connected": true,
            "page_ready": true,
            "player_page_open": true,
            "url": "https://music.youtube.com/channel/UC123",
            "route": "artist",
            "confirm_dialog": {
                "title": "Unsubscribe from M83?",
                "body": "",
                "confirm_id": "act-confirm",
                "confirm_label": "Unsubscribe",
                "cancel_id": "act-cancel",
                "cancel_label": "Cancel"
            },
            "player": {
                "has_media": true,
                "title": "Midnight City",
                "artist": "M83",
                "album": "Hurry Up, We're Dreaming",
                "subtitle_links": [
                    { "text": "M83", "href": "channel/UC123" },
                    { "text": " • ", "href": null },
                    { "text": "Hurry Up, We're Dreaming", "href": "browse/MPREb_123" }
                ]
            },
            "related_shelves": [{
                "title": "You might also like",
                "kind": "list",
                "items": [{ "id": "act-rel-1", "title": "Wait", "subtitle": "M83" }]
            }]
        }"#;
        let state: BridgeState = serde_json::from_str(json).expect("valid bridge state");
        assert!(state.player_page_open);
        assert_eq!(state.player.subtitle_links.len(), 3);
        assert_eq!(
            state.player.subtitle_links[0].href.as_deref(),
            Some("channel/UC123")
        );
        assert_eq!(
            state.confirm_dialog.as_ref().unwrap().confirm_label,
            "Unsubscribe"
        );
        assert_eq!(state.related_shelves.len(), 1);
    }

    #[test]
    fn deserializes_feed_collector_payload() {
        let json = r#"{
            "kind": "feed_update",
            "status": "Scanning Joe Bonamassa (2/10)...",
            "scanned": 2,
            "total": 10,
            "done": false,
            "releases": [{
                "title": "Christmas Boogie",
                "artist_name": "Joe Bonamassa",
                "artist_href": "channel/UCabc",
                "release_type": "Album",
                "year": 2026,
                "is_latest": true,
                "subtitle": "Album • 2026",
                "href": "browse/MPREb_xyz"
            }]
        }"#;
        let payload: FeedCollectorPayload =
            serde_json::from_str(json).expect("valid feed payload");
        assert_eq!(payload.scanned, 2);
        assert_eq!(payload.releases.len(), 1);
        assert_eq!(payload.releases[0].year, Some(2026));
        assert!(payload.releases[0].is_latest);
    }
}

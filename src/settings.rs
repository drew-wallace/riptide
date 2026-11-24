use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::protocol::{FeedArtistSource, FeedRelease};

fn default_true() -> bool {
    true
}

fn default_volume() -> u8 {
    80
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub feed_enabled: bool,
    pub feed_source: FeedArtistSource,
    /// Automatically refresh Feed when the app launches and connects.
    #[serde(default = "default_true")]
    pub feed_refresh_on_open: bool,
    /// Automatically refresh Feed once daily at `feed_daily_hour:feed_daily_minute`.
    #[serde(default = "default_true")]
    pub feed_refresh_daily: bool,
    /// Daily refresh hour in 24-hour local time (default: 0 = 12:00 AM).
    pub feed_daily_hour: u8,
    /// Daily refresh minute (0, 15, 30, 45; default: 0).
    pub feed_daily_minute: u8,
    /// Run a refresh immediately if the scheduled daily time was missed.
    #[serde(default = "default_true")]
    pub feed_run_if_missed: bool,
    /// Persisted playback volume (0..=100).
    #[serde(default = "default_volume")]
    pub volume: u8,
    /// Persisted mute state.
    #[serde(default)]
    pub is_muted: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            feed_enabled: true,
            feed_source: FeedArtistSource::Both,
            feed_refresh_on_open: true,
            feed_refresh_daily: true,
            feed_daily_hour: 0,
            feed_daily_minute: 0,
            feed_run_if_missed: true,
            volume: default_volume(),
            is_muted: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FeedCache {
    pub source: FeedArtistSource,
    pub artists_scanned: usize,
    pub last_refreshed_unix: Option<i64>,
    pub seen_release_keys: Vec<String>,
    pub releases: Vec<FeedRelease>,
}

pub fn app_data_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
    let new_dir = base.join("riptide");
    let legacy_dir = base.join("ytm-native-mask");
    if !new_dir.exists() && legacy_dir.exists() {
        if std::fs::rename(&legacy_dir, &new_dir).is_err() {
            return legacy_dir;
        }
    }
    let _ = std::fs::create_dir_all(&new_dir);
    new_dir
}

fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

fn feed_cache_path() -> PathBuf {
    app_data_dir().join("feed_cache.json")
}

impl AppSettings {
    pub fn load() -> Self {
        std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(settings_path(), json);
        }
    }
}

impl FeedCache {
    pub fn load() -> Self {
        std::fs::read_to_string(feed_cache_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(feed_cache_path(), json);
        }
    }
}

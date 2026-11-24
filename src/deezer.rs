use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::protocol::{FeedArtist, FeedCollectorPayload, FeedRelease, WebEvent};
use crate::settings::app_data_dir;
use crate::time_util::now_unix_secs;

const DEEZER_API_BASE: &str = "https://api.deezer.com";
/// Deezer allows 50 requests / 5 seconds (10 req/s). 135 ms spacing (~7.4 req/s) stays safely underneath.
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(135);
const MAX_RETRIES: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeezerArtistMapping {
    pub deezer_id: u64,
    pub deezer_name: String,
    pub nb_fan: u64,
    pub updated_unix: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DeezerArtistMap {
    /// Keyed by YTM artist `channel_id` (or normalized name fallback).
    pub by_channel: HashMap<String, DeezerArtistMapping>,
}

fn artist_map_path() -> PathBuf {
    app_data_dir().join("deezer_artist_map.json")
}

impl DeezerArtistMap {
    pub fn load() -> Self {
        std::fs::read_to_string(artist_map_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(artist_map_path(), json);
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DeezerErrorObj {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DeezerArtistSearchResponse {
    #[serde(default)]
    data: Vec<DeezerArtistHit>,
    #[serde(default)]
    error: Option<DeezerErrorObj>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeezerArtistHit {
    id: u64,
    name: String,
    #[serde(default)]
    nb_fan: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DeezerAlbumsResponse {
    #[serde(default)]
    data: Vec<DeezerAlbumHit>,
    #[serde(default)]
    error: Option<DeezerErrorObj>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeezerAlbumHit {
    id: u64,
    title: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    cover_xl: Option<String>,
    #[serde(default)]
    cover_big: Option<String>,
    #[serde(default)]
    cover_medium: Option<String>,
    #[serde(default)]
    release_date: String,
    #[serde(default)]
    record_type: String,
    #[serde(default)]
    explicit_lyrics: bool,
}

pub struct DeezerRunOutput {
    pub releases: Vec<FeedRelease>,
    pub fallback_artists: Vec<FeedArtist>,
}

enum FetchOutcome {
    Bytes(Vec<u8>),
    RateLimited,
    TransportError,
    HttpError,
}

fn normalize_artist_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let lower = raw
        .trim()
        .replace(['’', '‘', '`'], "'")
        .replace(['“', '”'], "\"")
        .to_lowercase();

    for word in lower.split(|c: char| !c.is_alphanumeric() && c != '&') {
        if word.is_empty() {
            continue;
        }
        let mapped = if word == "&" { "and" } else { word };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(mapped);
    }
    out
}

fn strip_leading_the(norm: &str) -> &str {
    norm.strip_prefix("the ").unwrap_or(norm)
}

/// Disambiguates Deezer artist search hits for a YouTube Music artist name.
/// Prefers exact case-insensitive matches (highest fan count), then normalized matches
/// (`&` <-> `and`, smart quotes, punctuation). Returns `None` when no confident match exists
/// so the caller automatically falls back to the YouTube Music WebView scraper.
fn pick_best_artist_match(target_name: &str, hits: &[DeezerArtistHit]) -> Option<DeezerArtistHit> {
    let target_trim = target_name.trim();
    if target_trim.is_empty() || hits.is_empty() {
        return None;
    }

    // Tier 1: Exact case-insensitive match (highest fan count wins)
    let exact: Vec<&DeezerArtistHit> = hits
        .iter()
        .filter(|h| h.name.trim().eq_ignore_ascii_case(target_trim))
        .collect();
    if let Some(best) = exact.into_iter().max_by_key(|h| h.nb_fan) {
        return Some(best.clone());
    }

    // Tier 2: Normalized match (e.g. "&" vs "and", curly apostrophes, punctuation)
    let target_norm = normalize_artist_name(target_trim);
    let norm_hits: Vec<&DeezerArtistHit> = hits
        .iter()
        .filter(|h| normalize_artist_name(&h.name) == target_norm)
        .collect();
    if let Some(best) = norm_hits.into_iter().max_by_key(|h| h.nb_fan) {
        return Some(best.clone());
    }

    // Tier 3: Leading "The " tolerance when fan count confirms a major artist
    let target_no_the = strip_leading_the(&target_norm);
    if !target_no_the.is_empty() {
        let the_hits: Vec<&DeezerArtistHit> = hits
            .iter()
            .filter(|h| {
                let hn = normalize_artist_name(&h.name);
                strip_leading_the(&hn) == target_no_the && h.nb_fan >= 500
            })
            .collect();
        if let Some(best) = the_hits.into_iter().max_by_key(|h| h.nb_fan) {
            return Some(best.clone());
        }
    }

    None
}

fn map_record_type(raw: &str) -> Option<&'static str> {
    match raw.trim().to_lowercase().as_str() {
        "album" => Some("Album"),
        "ep" => Some("EP"),
        "single" => Some("Single"),
        "compile" | "compilation" => None,
        "" => Some("Release"),
        _ => Some("Release"),
    }
}

fn valid_iso_date(s: &str) -> Option<(String, u16)> {
    let trimmed = s.trim();
    if trimmed.len() < 10 || trimmed.starts_with("0000") {
        return None;
    }
    let ymd = &trimmed[..10];
    let bytes = ymd.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: u16 = ymd[0..4].parse().ok()?;
    let month: u8 = ymd[5..7].parse().ok()?;
    let day: u8 = ymd[8..10].parse().ok()?;
    if year < 1950 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((ymd.to_string(), year))
}

fn pace_request(last_req: &mut Option<Instant>) {
    if let Some(prev) = *last_req {
        let elapsed = prev.elapsed();
        if elapsed < MIN_REQUEST_INTERVAL {
            std::thread::sleep(MIN_REQUEST_INTERVAL - elapsed);
        }
    }
    *last_req = Some(Instant::now());
}

fn get_bytes_with_backoff(
    rt: &tokio::runtime::Runtime,
    client: &reqwest::Client,
    url: &str,
    last_req: &mut Option<Instant>,
) -> FetchOutcome {
    let backoffs_ms = [1200_u64, 2500, 5000];
    for attempt in 0..=MAX_RETRIES {
        pace_request(last_req);
        let resp_res = rt.block_on(async {
            let resp = client.get(url).timeout(Duration::from_secs(10)).send().await;
            match resp {
                Ok(r) => {
                    let status = r.status();
                    if status.as_u16() == 429 || status.is_server_error() {
                        return FetchOutcome::RateLimited;
                    }
                    if !status.is_success() {
                        return FetchOutcome::HttpError;
                    }
                    match r.bytes().await {
                        Ok(b) => FetchOutcome::Bytes(b.to_vec()),
                        Err(_) => FetchOutcome::TransportError,
                    }
                }
                Err(_) => FetchOutcome::TransportError,
            }
        });

        match resp_res {
            FetchOutcome::Bytes(bytes) => {
                // Deezer also signals quota throttling inside HTTP 200 JSON (`{"error":{"code":4}}`)
                if is_deezer_quota_json(&bytes) {
                    if attempt < MAX_RETRIES {
                        std::thread::sleep(Duration::from_millis(backoffs_ms[attempt]));
                        continue;
                    }
                    return FetchOutcome::RateLimited;
                }
                return FetchOutcome::Bytes(bytes);
            }
            FetchOutcome::RateLimited => {
                if attempt < MAX_RETRIES {
                    std::thread::sleep(Duration::from_millis(backoffs_ms[attempt]));
                    continue;
                }
                return FetchOutcome::RateLimited;
            }
            FetchOutcome::TransportError => {
                if attempt == 0 {
                    std::thread::sleep(Duration::from_millis(600));
                    continue;
                }
                return FetchOutcome::TransportError;
            }
            FetchOutcome::HttpError => return FetchOutcome::HttpError,
        }
    }
    FetchOutcome::RateLimited
}

fn is_deezer_quota_json(bytes: &[u8]) -> bool {
    if bytes.len() > 512 {
        return false;
    }
    #[derive(Deserialize)]
    struct ErrEnvelope {
        error: Option<DeezerErrorObj>,
    }
    if let Ok(env) = serde_json::from_slice::<ErrEnvelope>(bytes) {
        if let Some(err) = env.error {
            return err.code == 4 || err.message.to_lowercase().contains("quota");
        }
    }
    false
}

fn album_hits_to_releases(artist: &FeedArtist, hits: Vec<DeezerAlbumHit>) -> Vec<FeedRelease> {
    let mut out = Vec::new();
    let mut seen_norm: HashSet<String> = HashSet::new();

    // Sort newest first, preferring explicit over clean when same title+date exist
    let mut sorted = hits;
    sorted.sort_by(|a, b| {
        b.release_date
            .cmp(&a.release_date)
            .then_with(|| b.explicit_lyrics.cmp(&a.explicit_lyrics))
            .then_with(|| a.id.cmp(&b.id))
    });

    for alb in sorted {
        let Some(rec_type) = map_record_type(&alb.record_type) else {
            continue;
        };
        let title = alb.title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let parsed_date = valid_iso_date(&alb.release_date);
        let (release_date, year) = match parsed_date {
            Some((ymd, y)) => (Some(ymd), Some(y)),
            None => (None, None),
        };

        // Deduplicate Clean / Explicit twins with identical normalized title + release date
        let dedup_key = format!(
            "{}|{}|{}",
            normalize_artist_name(&title),
            release_date.as_deref().unwrap_or(""),
            rec_type
        );
        if !seen_norm.insert(dedup_key) {
            continue;
        }

        let artwork = alb
            .cover_xl
            .or(alb.cover_big)
            .or(alb.cover_medium)
            .filter(|u| !u.is_empty());

        let subtitle = match year {
            Some(y) => format!("{rec_type} • {y}"),
            None => rec_type.to_string(),
        };

        out.push(FeedRelease {
            title,
            artist_name: artist.name.clone(),
            artist_href: artist.href.clone(),
            release_type: rec_type.to_string(),
            year,
            release_date,
            video_id: None,
            is_latest: false,
            subtitle,
            artwork_url: artwork,
            href: format!("deezer:album:{}", alb.id),
            external_url: if alb.link.is_empty() {
                Some(format!("https://www.deezer.com/album/{}", alb.id))
            } else {
                Some(alb.link)
            },
        });
    }

    out
}

/// Runs the primary Deezer Public API Feed collection across all discovered YTM artists.
/// Streams live progress updates to GPUI and returns both collected Deezer releases and
/// any artists that should automatically fall back to the YouTube Music WebView scraper.
pub fn collect_releases_via_deezer(
    artists: Vec<FeedArtist>,
    run: u64,
    active_run: Arc<AtomicU64>,
    event_tx: UnboundedSender<WebEvent>,
) -> Option<DeezerRunOutput> {
    let total = artists.len();
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Some(DeezerRunOutput {
            releases: Vec::new(),
            fallback_artists: artists,
        });
    };

    let Ok(client) = reqwest::Client::builder()
        .user_agent("riptide/0.1 (Desktop Music Client)")
        .pool_max_idle_per_host(4)
        .build()
    else {
        return Some(DeezerRunOutput {
            releases: Vec::new(),
            fallback_artists: artists,
        });
    };

    let mut artist_map = DeezerArtistMap::load();
    let mut map_dirty = false;
    let mut all_releases: Vec<FeedRelease> = Vec::new();
    let mut fallback_artists: Vec<FeedArtist> = Vec::new();
    let mut last_req: Option<Instant> = None;
    let mut consecutive_transport_errors: usize = 0;

    for (idx, artist) in artists.iter().enumerate() {
        if active_run.load(Ordering::Relaxed) != run {
            return None;
        }

        // If Deezer is unreachable across 3 consecutive artists, hand all remaining straight to fallback
        if consecutive_transport_errors >= 3 {
            fallback_artists.extend(artists[idx..].iter().cloned());
            break;
        }

        let _ = event_tx.send(WebEvent::FeedUpdate(FeedCollectorPayload {
            kind: "feed_update".to_string(),
            status: format!("Checking {} ({}/{})…", artist.name, idx + 1, total),
            scanned: idx,
            total,
            releases: all_releases.clone(),
            done: false,
        }));

        let cache_key = if !artist.channel_id.is_empty() {
            artist.channel_id.clone()
        } else {
            format!("name:{}", normalize_artist_name(&artist.name))
        };

        // Step 1: Resolve Deezer artist ID (using local cache when available)
        let mut deezer_id = artist_map.by_channel.get(&cache_key).map(|m| m.deezer_id);
        let mut transport_failed = false;

        if deezer_id.is_none() {
            let search_url = format!(
                "{DEEZER_API_BASE}/search/artist?q={}&limit=10",
                urlencoding::encode(artist.name.trim())
            );
            match get_bytes_with_backoff(&rt, &client, &search_url, &mut last_req) {
                FetchOutcome::Bytes(bytes) => {
                    consecutive_transport_errors = 0;
                    if let Ok(parsed) = serde_json::from_slice::<DeezerArtistSearchResponse>(&bytes)
                    {
                        if parsed.error.is_none() {
                            if let Some(best) = pick_best_artist_match(&artist.name, &parsed.data) {
                                deezer_id = Some(best.id);
                                artist_map.by_channel.insert(
                                    cache_key.clone(),
                                    DeezerArtistMapping {
                                        deezer_id: best.id,
                                        deezer_name: best.name,
                                        nb_fan: best.nb_fan,
                                        updated_unix: now_unix_secs(),
                                    },
                                );
                                map_dirty = true;
                            }
                        }
                    }
                }
                FetchOutcome::TransportError => {
                    consecutive_transport_errors += 1;
                    transport_failed = true;
                }
                FetchOutcome::RateLimited | FetchOutcome::HttpError => {
                    transport_failed = true;
                }
            }
        }

        let Some(dz_id) = deezer_id else {
            fallback_artists.push(artist.clone());
            let _ = event_tx.send(WebEvent::FeedUpdate(FeedCollectorPayload {
                kind: "feed_update".to_string(),
                status: format!(
                    "Queued {} for YouTube Music fallback ({}/{})…",
                    artist.name,
                    idx + 1,
                    total
                ),
                scanned: idx + 1,
                total,
                releases: all_releases.clone(),
                done: false,
            }));
            continue;
        };

        if transport_failed {
            fallback_artists.push(artist.clone());
            continue;
        }

        // Step 2: Fetch artist's albums, EPs, and singles with exact YYYY-MM-DD release dates
        let albums_url = format!("{DEEZER_API_BASE}/artist/{dz_id}/albums?limit=50");
        match get_bytes_with_backoff(&rt, &client, &albums_url, &mut last_req) {
            FetchOutcome::Bytes(bytes) => {
                consecutive_transport_errors = 0;
                match serde_json::from_slice::<DeezerAlbumsResponse>(&bytes) {
                    Ok(parsed) if parsed.error.is_none() => {
                        let artist_releases = album_hits_to_releases(artist, parsed.data);
                        if artist_releases.is_empty() {
                            fallback_artists.push(artist.clone());
                        } else {
                            all_releases.extend(artist_releases);
                        }
                    }
                    _ => {
                        fallback_artists.push(artist.clone());
                    }
                }
            }
            FetchOutcome::TransportError => {
                consecutive_transport_errors += 1;
                fallback_artists.push(artist.clone());
            }
            FetchOutcome::RateLimited | FetchOutcome::HttpError => {
                fallback_artists.push(artist.clone());
            }
        }

        let _ = event_tx.send(WebEvent::FeedUpdate(FeedCollectorPayload {
            kind: "feed_update".to_string(),
            status: format!(
                "Scanned {}/{} artists ({} releases)…",
                idx + 1,
                total,
                all_releases.len()
            ),
            scanned: idx + 1,
            total,
            releases: all_releases.clone(),
            done: false,
        }));
    }

    if map_dirty {
        artist_map.save();
    }

    if active_run.load(Ordering::Relaxed) != run {
        return None;
    }

    Some(DeezerRunOutput {
        releases: all_releases,
        fallback_artists,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disambiguates_artist_by_exact_name_and_fan_count() {
        let hits = vec![
            DeezerArtistHit {
                id: 1,
                name: "Petra".to_string(),
                nb_fan: 85_000,
            },
            DeezerArtistHit {
                id: 2,
                name: "Petra".to_string(),
                nb_fan: 400,
            },
            DeezerArtistHit {
                id: 3,
                name: "Petra Marklund".to_string(),
                nb_fan: 120_000,
            },
        ];
        let picked = pick_best_artist_match("petra", &hits).expect("should match exact");
        assert_eq!(picked.id, 1);
    }

    #[test]
    fn normalizes_ampersand_and_quotes_but_rejects_unrelated() {
        let hits = vec![DeezerArtistHit {
            id: 42,
            name: "Girish and the Chronicles".to_string(),
            nb_fan: 9_000,
        }];
        let picked =
            pick_best_artist_match("Girish & The Chronicles", &hits).expect("should normalize &");
        assert_eq!(picked.id, 42);

        let unrelated = vec![DeezerArtistHit {
            id: 99,
            name: "Totally Different Band".to_string(),
            nb_fan: 500_000,
        }];
        assert!(pick_best_artist_match("Blacktop Mojo", &unrelated).is_none());
    }

    #[test]
    fn converts_deezer_albums_and_skips_compilations_and_duplicates() {
        let artist = FeedArtist {
            name: "Blacktop Mojo".to_string(),
            channel_id: "UCtest".to_string(),
            href: "channel/UCtest".to_string(),
        };
        let hits = vec![
            DeezerAlbumHit {
                id: 1001,
                title: "Discount Fireworks".to_string(),
                link: "https://www.deezer.com/album/1001".to_string(),
                cover_xl: Some("https://cdn-images.dzcdn.net/1000x1000.jpg".to_string()),
                cover_big: None,
                cover_medium: None,
                release_date: "2026-11-06".to_string(),
                record_type: "album".to_string(),
                explicit_lyrics: true,
            },
            DeezerAlbumHit {
                id: 1002,
                title: "Discount Fireworks".to_string(),
                link: "https://www.deezer.com/album/1002".to_string(),
                cover_xl: None,
                cover_big: None,
                cover_medium: None,
                release_date: "2026-11-06".to_string(),
                record_type: "album".to_string(),
                explicit_lyrics: false,
            },
            DeezerAlbumHit {
                id: 1003,
                title: "Rock Anthems Compilation".to_string(),
                link: String::new(),
                cover_xl: None,
                cover_big: None,
                cover_medium: None,
                release_date: "2026-10-01".to_string(),
                record_type: "compile".to_string(),
                explicit_lyrics: false,
            },
        ];

        let releases = album_hits_to_releases(&artist, hits);
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].title, "Discount Fireworks");
        assert_eq!(releases[0].release_date.as_deref(), Some("2026-11-06"));
        assert_eq!(releases[0].release_type, "Album");
        assert_eq!(releases[0].href, "deezer:album:1001");
        assert!(!releases[0].has_ytm_href());
    }
}

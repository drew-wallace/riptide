(() => {
  if (window.__ytmFeedCollectorBooted) return;
  window.__ytmFeedCollectorBooted = true;

  const host = (window.location.hostname || "").toLowerCase();
  const hash = window.location.hash || "";
  const isYtmFeedRun = host === "music.youtube.com" && hash.includes("__ytm_feed=");
  const isYtDateProbe =
    (host === "www.youtube.com" || host === "youtube.com") && hash.includes("__ytm_yt_dates=");

  if (!isYtmFeedRun && !isYtDateProbe) return;

  const params = new URLSearchParams(hash.replace(/^#/, ""));
  const runId = parseInt(params.get("run") || "0", 10) || 0;

  // Permanently silence all media inside this hidden background collector WebView
  function silenceMedia(el) {
    if (!el) return;
    try {
      el.muted = true;
      el.volume = 0;
      if (typeof el.pause === "function" && !el.paused) el.pause();
    } catch (_) {}
  }

  for (const evt of ["play", "playing", "loadedmetadata", "canplay", "volumechange"]) {
    window.addEventListener(
      evt,
      e => {
        if (e.target && (e.target.tagName === "VIDEO" || e.target.tagName === "AUDIO")) {
          silenceMedia(e.target);
        }
      },
      true
    );
  }

  function postRaw(obj) {
    try {
      if (window.ipc && typeof window.ipc.postMessage === "function") {
        window.ipc.postMessage(JSON.stringify(obj));
      }
    } catch (_) {}
  }

  function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  function isReasonableDate(ymd, expectedYear) {
    if (!ymd || !/^\d{4}-\d{2}-\d{2}$/.test(ymd)) return false;
    const y = parseInt(ymd.slice(0, 4), 10);
    const m = parseInt(ymd.slice(5, 7), 10);
    const d = parseInt(ymd.slice(8, 10), 10);
    if (!Number.isFinite(y) || y < 1950 || m < 1 || m > 12 || d < 1 || d > 31) return false;
    if (expectedYear && Math.abs(y - expectedYear) > 1) return false;
    return true;
  }

  function extractDateFromText(text, expectedYear) {
    if (!text || typeof text !== "string") return null;
    // Highest priority: official YouTube Art Track DDEX line ("Released on: YYYY-MM-DD")
    const ddex = text.match(/Released on:\s*(\d{4}-\d{2}-\d{2})/i);
    if (ddex && isReasonableDate(ddex[1], expectedYear)) return ddex[1];

    const ymd = text.match(/^(\d{4}-\d{2}-\d{2})/);
    if (ymd && isReasonableDate(ymd[1], expectedYear)) return ymd[1];
    return null;
  }

  function extractDateFromWatchHtml(html, expectedYear) {
    if (!html || typeof html !== "string") return null;

    // 1. Official commercial release date in Art Track description ("Released on: YYYY-MM-DD")
    const ddexAll = html.match(/Released on:\s*(\d{4}-\d{2}-\d{2})/gi);
    if (ddexAll) {
      for (const hit of ddexAll) {
        const m = hit.match(/(\d{4}-\d{2}-\d{2})/);
        if (m && isReasonableDate(m[1], expectedYear)) return m[1];
      }
    }

    // 2. YouTube microformat publishDate ("publishDate":"2026-11-13" or ISO timestamp)
    const pubMatches = html.match(/"publishDate"\s*:\s*"(\d{4}-\d{2}-\d{2})/gi);
    if (pubMatches) {
      for (const hit of pubMatches) {
        const m = hit.match(/(\d{4}-\d{2}-\d{2})/);
        if (m && isReasonableDate(m[1], expectedYear)) return m[1];
      }
    }

    // 3. Schema.org <meta itemprop="datePublished" content="2026-11-13...">
    const metaPub = html.match(/itemprop=["']datePublished["'][^>]*content=["'](\d{4}-\d{2}-\d{2})/i);
    if (metaPub && isReasonableDate(metaPub[1], expectedYear)) return metaPub[1];

    // 4. Fallback microformat uploadDate
    const uplMatches = html.match(/"uploadDate"\s*:\s*"(\d{4}-\d{2}-\d{2})/gi);
    if (uplMatches) {
      for (const hit of uplMatches) {
        const m = hit.match(/(\d{4}-\d{2}-\d{2})/);
        if (m && isReasonableDate(m[1], expectedYear)) return m[1];
      }
    }

    return null;
  }

  function extractDateFromLiveYoutubePage(expectedYear) {
    // 1. Inspect `window.ytInitialPlayerResponse` directly (fastest & cleanest)
    const pr = window.ytInitialPlayerResponse;
    if (pr && typeof pr === "object") {
      const shortDesc = pr.videoDetails && pr.videoDetails.shortDescription;
      const d1 = extractDateFromText(shortDesc, expectedYear);
      if (d1) return d1;

      const micro = pr.microformat && pr.microformat.playerMicroformatRenderer;
      if (micro) {
        const mDesc = micro.description && micro.description.simpleText;
        const d2 = extractDateFromText(mDesc, expectedYear);
        if (d2) return d2;

        const pub = extractDateFromText(micro.publishDate, expectedYear);
        if (pub) return pub;

        const upl = extractDateFromText(micro.uploadDate, expectedYear);
        if (upl) return upl;
      }
    }

    // 2. Inspect `<meta itemprop="datePublished">`
    const metaEl = document.querySelector('meta[itemprop="datePublished"]');
    if (metaEl) {
      const dMeta = extractDateFromText(metaEl.getAttribute("content") || "", expectedYear);
      if (dMeta) return dMeta;
    }

    // 3. Fallback: scan inline script tags & rendered description text
    const html = document.documentElement ? document.documentElement.innerHTML : "";
    return extractDateFromWatchHtml(html, expectedYear);
  }

  // ============================================================================
  // MODE B: www.youtube.com/watch?v=<vid>#__ytm_yt_dates=1&run=<run>
  // Reads exact "Released on: YYYY-MM-DD" metadata from standard YouTube watch pages.
  // ============================================================================
  if (isYtDateProbe) {
    setInterval(() => {
      for (const el of document.querySelectorAll("video, audio")) silenceMedia(el);
      const mp = document.querySelector("#movie_player");
      if (mp) {
        try {
          if (typeof mp.mute === "function") mp.mute();
          if (typeof mp.pauseVideo === "function") mp.pauseVideo();
        } catch (_) {}
      }
    }, 150);

    window.__ytmRunYtDateBatch = async function (items, expectedRun) {
      if (expectedRun !== runId) return;
      const list = Array.isArray(items) ? items : [];
      const total = list.length;

      for (let i = 0; i < list.length; i++) {
        const item = list[i];
        let foundDate = null;
        if (item && item.video_id) {
          try {
            const ctrl = typeof AbortController !== "undefined" ? new AbortController() : null;
            const timer = ctrl ? setTimeout(() => ctrl.abort(), 6500) : null;
            const resp = await fetch("/watch?v=" + encodeURIComponent(item.video_id), {
              credentials: "include",
              signal: ctrl ? ctrl.signal : undefined,
            });
            if (timer) clearTimeout(timer);
            if (resp && resp.ok) {
              const html = await resp.text();
              foundDate = extractDateFromWatchHtml(html, item.year || null);
            }
          } catch (_) {}
        }

        postRaw({
          kind: "yt_probe_item",
          run: runId,
          idx: item.idx,
          title: item.title || "",
          release_date: foundDate,
          done_count: i + 1,
          total_count: total,
        });

        await sleep(110);
      }

      postRaw({
        kind: "yt_probe_done",
        run: runId,
      });
    };

    const runFirstProbe = async () => {
      const start = Date.now();
      let date = null;
      while (Date.now() - start < 3200) {
        for (const el of document.querySelectorAll("video, audio")) silenceMedia(el);
        date = extractDateFromLiveYoutubePage(null);
        if (date) break;
        if (window.ytInitialPlayerResponse && document.readyState !== "loading") {
          break;
        }
        await sleep(140);
      }
      const vid = new URLSearchParams(window.location.search).get("v") || "";
      postRaw({
        kind: "yt_probe_ready",
        run: runId,
        video_id: vid,
        release_date: date,
      });
    };

    if (document.readyState === "loading") {
      window.addEventListener("DOMContentLoaded", () => setTimeout(runFirstProbe, 220), {
        once: true,
      });
    } else {
      setTimeout(runFirstProbe, 220);
    }
    return;
  }

  // ============================================================================
  // MODE A: music.youtube.com/...#__ytm_feed=<source>&run=<run>
  // Scans subscribed/collection artists, collects releases, and resolves track #1's video_id.
  // ============================================================================
  const source = params.get("__ytm_feed") || "both"; // "subscribed" | "collection" | "both"

  function postFeedUpdate(payload) {
    postRaw({ kind: "feed_update", run: runId, ...payload });
  }

  function cleanText(node) {
    if (!node) return "";
    return (node.textContent || "").replace(/\s+/g, " ").trim();
  }

  function isHiddenEl(el) {
    if (!el) return true;
    let cur = el;
    while (cur && cur !== document.body && cur !== document.documentElement) {
      if (cur.hasAttribute && cur.hasAttribute("hidden")) return true;
      if (cur.style && (cur.style.display === "none" || cur.style.visibility === "hidden")) return true;
      cur = cur.parentElement;
    }
    return false;
  }

  function normalizeAndUpscaleUrl(url) {
    if (!url || typeof url !== "string") return null;
    let u = url.trim();
    if (!u || u.startsWith("data:") || u.startsWith("blob:")) return null;
    if (u.startsWith("//")) u = "https:" + u;
    if (!u.startsWith("http://") && !u.startsWith("https://")) return null;
    if (
      u.includes("lh3.googleusercontent.com") ||
      u.includes("yt3.ggpht.com") ||
      u.includes("yt3.googleusercontent.com")
    ) {
      u = u
        .replace(/=w\d+-h\d+[^&]*/i, "=w544-h544-l90-rj")
        .replace(/=s\d+[^&]*/i, "=s544-c-k-c0x00ffffff-no-rj");
    }
    return u;
  }

  function findBestThumbnailInObject(obj, depth = 0, visited = new Set()) {
    if (!obj || typeof obj !== "object" || depth > 7 || visited.has(obj)) return null;
    visited.add(obj);
    if (Array.isArray(obj.thumbnails) && obj.thumbnails.length > 0) {
      let best = null;
      let bestScore = -1;
      for (const t of obj.thumbnails) {
        if (t && typeof t.url === "string") {
          const score = (t.width || 0) * (t.height || 0);
          if (score >= bestScore) {
            bestScore = score;
            best = t.url;
          }
        }
      }
      const norm = normalizeAndUpscaleUrl(best);
      if (norm) return norm;
    }
    const keys = [
      "thumbnailRenderer",
      "musicThumbnailRenderer",
      "croppedSquareThumbnailRenderer",
      "thumbnail",
      "data",
    ];
    for (const k of keys) {
      if (obj[k] && typeof obj[k] === "object") {
        const found = findBestThumbnailInObject(obj[k], depth + 1, visited);
        if (found) return found;
      }
    }
    return null;
  }

  function getImgSrc(root) {
    if (!root) return null;
    const hosts = [
      root.querySelector && root.querySelector("ytmusic-thumbnail-renderer"),
      root.querySelector && root.querySelector("yt-img-shadow"),
      root,
    ];
    for (const hostEl of hosts) {
      if (!hostEl) continue;
      if (typeof hostEl.src === "string") {
        const direct = normalizeAndUpscaleUrl(hostEl.src);
        if (direct) return direct;
      }
      const cands = [hostEl.data, hostEl.__data && hostEl.__data.data, hostEl.__data, hostEl.thumbnail];
      for (const d of cands) {
        if (d) {
          const u = findBestThumbnailInObject(d);
          if (u) return u;
        }
      }
    }
    if (root.querySelectorAll) {
      for (const img of root.querySelectorAll("img")) {
        const u = normalizeAndUpscaleUrl(
          img.currentSrc || img.src || img.getAttribute("src") || img.getAttribute("data-src")
        );
        if (u) return u;
      }
    }
    return null;
  }

  function extractBrowseId(el) {
    if (!el) return null;
    const link =
      el.querySelector &&
      (el.querySelector(".title a.yt-simple-endpoint[href]") ||
        el.querySelector("a.yt-simple-endpoint[href]") ||
        el.querySelector("a[href]"));
    if (link) {
      const href = (link.getAttribute("href") || "").replace(/^\/+/, "");
      if (href.startsWith("channel/")) return href.slice("channel/".length).split("?")[0];
      if (href.startsWith("browse/")) return href.slice("browse/".length).split("?")[0];
      if (href.startsWith("playlist?")) {
        const m = href.match(/[?&]list=([^&]+)/);
        if (m) return "VL" + decodeURIComponent(m[1]);
      }
    }
    const data = el.data || (el.__data && el.__data.data) || el.__data;
    const ep =
      data &&
      (data.navigationEndpoint ||
        (data.title &&
          data.title.runs &&
          data.title.runs[0] &&
          data.title.runs[0].navigationEndpoint));
    if (ep && ep.browseEndpoint && ep.browseEndpoint.browseId) {
      return ep.browseEndpoint.browseId;
    }
    return null;
  }

  /**
   * Robust bounded BFS over Polymer `.data` objects to locate a real 11-char YouTube `videoId`
   * regardless of nesting (`playlistItemData.videoId`, `watchEndpoint.videoId`, `playNavigationEndpoint`, etc.).
   */
  function deepFindVideoId(rootObj, maxNodes = 600) {
    if (!rootObj || typeof rootObj !== "object") return null;
    const visited = new Set();
    const queue = [rootObj];
    let seenCount = 0;

    while (queue.length > 0 && seenCount < maxNodes) {
      const cur = queue.shift();
      if (!cur || typeof cur !== "object" || visited.has(cur)) continue;
      if (typeof Node !== "undefined" && cur instanceof Node) continue;
      visited.add(cur);
      seenCount++;

      if (typeof cur.videoId === "string" && /^[A-Za-z0-9_-]{11}$/.test(cur.videoId)) {
        return cur.videoId;
      }

      if (Array.isArray(cur)) {
        for (let i = 0; i < Math.min(cur.length, 12); i++) {
          if (cur[i] && typeof cur[i] === "object") queue.push(cur[i]);
        }
      } else {
        const keys = Object.keys(cur);
        for (let i = 0; i < keys.length; i++) {
          const k = keys[i];
          if (
            k === "thumbnails" ||
            k === "trackingParams" ||
            k === "accessibility" ||
            k === "loggingContext" ||
            k === "icon" ||
            k.startsWith("__")
          ) {
            continue;
          }
          const v = cur[k];
          if (v && typeof v === "object") queue.push(v);
        }
      }
    }
    return null;
  }

  function normalizeArtistChannelId(rawId) {
    if (!rawId) return null;
    if (rawId.startsWith("MPLAUC")) return rawId.slice(4);
    if (rawId.startsWith("UC")) return rawId;
    return rawId;
  }

  function spaNavigateEndpoint(endpoint) {
    const app = document.querySelector("ytmusic-app");
    if (app && endpoint) {
      app.dispatchEvent(
        new CustomEvent("yt-navigate", {
          bubbles: true,
          composed: true,
          detail: { endpoint },
        })
      );
    }
  }

  function spaNavigateBrowse(browseId) {
    spaNavigateEndpoint({ browseEndpoint: { browseId } });
  }

  function getActiveBrowseResponse() {
    const bp = document.querySelector("#browse-page");
    if (!bp) return null;
    const list = Array.from(bp.querySelectorAll("ytmusic-browse-response")).filter(
      el => !isHiddenEl(el)
    );
    return list.length > 0 ? list[list.length - 1] : bp;
  }

  async function waitForBrowseContent(timeoutMs = 7000, expectedToken = null) {
    const start = Date.now();
    const tokenClean = expectedToken ? expectedToken.replace(/^VL/, "") : null;
    while (Date.now() - start < timeoutMs) {
      if (tokenClean && !window.location.href.includes(tokenClean)) {
        await sleep(140);
        continue;
      }
      const root = getActiveBrowseResponse();
      if (root) {
        const hasSpinner = !!root.querySelector(
          "ytmusic-skeleton:not([hidden]), tp-yt-paper-spinner-lite[active]"
        );
        const hasItems = !!root.querySelector(
          "ytmusic-responsive-list-item-renderer, ytmusic-two-row-item-renderer, ytmusic-message-renderer, ytmusic-carousel-shelf-renderer"
        );
        if (hasItems && !hasSpinner) {
          await sleep(240);
          return root;
        }
      }
      await sleep(160);
    }
    return getActiveBrowseResponse();
  }

  async function scrollToLoadAllArtists() {
    let lastCount = 0;
    let stagnant = 0;
    for (let step = 0; step < 24; step++) {
      const root = getActiveBrowseResponse();
      const count = root
        ? root.querySelectorAll(
            "ytmusic-responsive-list-item-renderer, ytmusic-two-row-item-renderer"
          ).length
        : 0;
      const hasCont =
        root &&
        !!root.querySelector(
          "ytmusic-continuation-item-renderer:not([hidden]), tp-yt-paper-spinner-lite[active]"
        );
      if (count > lastCount) {
        lastCount = count;
        stagnant = 0;
      } else {
        stagnant++;
      }
      if (!hasCont && stagnant >= 2) break;
      if (stagnant >= 4) break;
      try {
        window.scrollBy(0, 2200);
        const layout = document.querySelector("ytmusic-app-layout");
        if (layout) layout.scrollTop += 2200;
      } catch (_) {}
      await sleep(340);
    }
    try {
      window.scrollTo(0, 0);
      const layout = document.querySelector("ytmusic-app-layout");
      if (layout) layout.scrollTop = 0;
    } catch (_) {}
  }

  function scrapeArtistsFromPage(root) {
    if (!root) return [];
    const artists = [];
    const items = root.querySelectorAll(
      "ytmusic-responsive-list-item-renderer, ytmusic-two-row-item-renderer"
    );
    for (const el of items) {
      if (isHiddenEl(el)) continue;
      const titleEl = el.querySelector(".title, .title-column, yt-formatted-string.title");
      const name = cleanText(titleEl);
      if (!name) continue;
      const rawId = extractBrowseId(el);
      const channelId = normalizeArtistChannelId(rawId);
      if (!channelId || (!channelId.startsWith("UC") && !channelId.startsWith("MPLA"))) continue;
      artists.push({
        name,
        channel_id: channelId,
        href: channelId.startsWith("UC") ? "channel/" + channelId : "browse/" + channelId,
      });
    }
    return artists;
  }

  function formatDateYmd(date) {
    const y = date.getFullYear();
    const m = String(date.getMonth() + 1).padStart(2, "0");
    const d = String(date.getDate()).padStart(2, "0");
    return `${y}-${m}-${d}`;
  }

  function parseRelativeDate(text) {
    if (!text) return null;
    const lower = text.toLowerCase();
    const now = new Date();
    if (lower.includes("today") || lower.includes("hour ago") || lower.includes("hours ago")) {
      return formatDateYmd(now);
    }
    if (lower.includes("yesterday")) {
      const d = new Date(now.getTime() - 86400 * 1000);
      return formatDateYmd(d);
    }
    const m = lower.match(/(\d+)\s+(day|week|month|year)s?\s+ago/);
    if (!m) return null;
    const n = parseInt(m[1], 10);
    const unit = m[2];
    const d = new Date(now);
    if (unit === "day") d.setDate(d.getDate() - n);
    else if (unit === "week") d.setDate(d.getDate() - n * 7);
    else if (unit === "month") d.setMonth(d.getMonth() - n);
    else if (unit === "year") d.setFullYear(d.getFullYear() - n);
    return formatDateYmd(d);
  }

  function parseReleaseMeta(subtitle, shelfTitle) {
    const lowerSub = (subtitle || "").toLowerCase();
    const lowerShelf = (shelfTitle || "").toLowerCase();

    let releaseType = "Release";
    if (lowerSub.includes("single") || lowerShelf.includes("single")) releaseType = "Single";
    else if (lowerSub.includes("ep") || lowerShelf.includes("ep")) releaseType = "EP";
    else if (lowerSub.includes("album") || lowerShelf.includes("album")) releaseType = "Album";

    let year = null;
    const m = (subtitle || "").match(/\b(19|20)\d{2}\b/);
    if (m) year = parseInt(m[0], 10);

    const relDate = parseRelativeDate(subtitle);
    const isLatest =
      lowerSub.includes("latest") ||
      lowerSub.includes("new") ||
      lowerShelf.includes("latest");

    return { releaseType, year, relDate, isLatest };
  }

  function extractFirstTrackVideoIdFromReleasePage(root) {
    if (!root) return null;

    // 1. Check first 4 track rows for explicit watch links or Polymer data
    const rows = Array.from(root.querySelectorAll("ytmusic-responsive-list-item-renderer")).filter(
      r => !isHiddenEl(r)
    );
    for (const row of rows.slice(0, 4)) {
      const watchLink = row.querySelector("a[href*='watch?v=']");
      if (watchLink) {
        const m = (watchLink.getAttribute("href") || "").match(/[?&]v=([A-Za-z0-9_-]{11})/);
        if (m) return m[1];
      }
      const fromData = deepFindVideoId(row.data || (row.__data && row.__data.data) || row.__data, 350);
      if (fromData) return fromData;
    }

    // 2. Check header Play button
    const playBtns = root.querySelectorAll(
      "ytmusic-responsive-header-renderer ytmusic-play-button-renderer, ytmusic-detail-header-renderer ytmusic-play-button-renderer, ytmusic-play-button-renderer"
    );
    for (const btn of Array.from(playBtns).slice(0, 4)) {
      const fromBtn = deepFindVideoId(btn.data || (btn.__data && btn.__data.data) || btn.__data, 200);
      if (fromBtn) return fromBtn;
    }

    // 3. Fallback: scan the active browse response Polymer data directly
    return deepFindVideoId(root.data || (root.__data && root.__data.data) || root.__data, 900);
  }

  function scrapeArtistReleases(root, artist) {
    if (!root) return [];
    const releases = [];
    const seen = new Set();
    const currentYear = new Date().getFullYear();

    const shelves = Array.from(
      root.querySelectorAll(
        "ytmusic-carousel-shelf-renderer, ytmusic-shelf-renderer, ytmusic-card-shelf-renderer, ytmusic-music-card-shelf-renderer"
      )
    ).filter(s => !isHiddenEl(s));

    for (const shelf of shelves) {
      const titleEl = shelf.querySelector(
        "ytmusic-carousel-shelf-basic-header-renderer .title, ytmusic-card-shelf-header-basic-renderer .title, .header .title, h2.title"
      );
      const shelfTitle = cleanText(titleEl);
      const lower = shelfTitle.toLowerCase();

      const isReleaseShelf =
        lower.includes("album") ||
        lower.includes("single") ||
        lower.includes("ep") ||
        lower.includes("latest") ||
        lower.includes("release");
      if (!isReleaseShelf) continue;
      if (lower.includes("featured on") || lower.includes("similar") || lower.includes("fans might")) {
        continue;
      }

      const tag = shelf.tagName.toLowerCase();
      let cards = Array.from(shelf.querySelectorAll("ytmusic-two-row-item-renderer")).filter(
        c => !c.hasAttribute("hidden")
      );
      if (
        cards.length === 0 &&
        (tag === "ytmusic-card-shelf-renderer" || tag === "ytmusic-music-card-shelf-renderer")
      ) {
        cards = [shelf];
      }

      for (let i = 0; i < Math.min(cards.length, 20); i++) {
        const card = cards[i];
        const title = cleanText(
          card.querySelector(".main-card-content-container .title, .title a, .title")
        );
        if (!title) continue;
        const subtitle = cleanText(
          card.querySelector(".main-card-content-container .subtitle, .subtitle")
        );
        const rawBrowseId = extractBrowseId(card);
        const link = card.querySelector("a.yt-simple-endpoint[href]");
        let href = link ? (link.getAttribute("href") || "").replace(/^\/+/, "") : "";
        if (!href && rawBrowseId) {
          href = rawBrowseId.startsWith("VL")
            ? "playlist?list=" + encodeURIComponent(rawBrowseId.slice(2))
            : "browse/" + encodeURIComponent(rawBrowseId);
        }
        if (!href) continue;

        const key = (href + "|" + title).toLowerCase();
        if (seen.has(key)) continue;
        seen.add(key);

        const { releaseType, year, relDate, isLatest } = parseReleaseMeta(subtitle, shelfTitle);
        const hrefVideoMatch = href.match(/[?&]v=([A-Za-z0-9_-]{11})/);
        const videoId =
          (hrefVideoMatch && hrefVideoMatch[1]) ||
          deepFindVideoId(card.data || (card.__data && card.__data.data) || card.__data, 250);

        releases.push({
          title,
          artist_name: artist.name,
          artist_href: artist.href,
          release_type: releaseType,
          year,
          release_date: relDate,
          video_id: videoId,
          is_latest: isLatest || (i === 0 && year !== null && year >= currentYear),
          subtitle,
          artwork_url: getImgSrc(card),
          href,
          _browseId: rawBrowseId,
        });
      }
    }

    return releases;
  }

  function stripInternalFields(releases) {
    return releases.map(({ _browseId, ...rest }) => rest);
  }

  async function runFeedCollection() {
    postFeedUpdate({
      status: "Loading your artist list from YouTube Music…",
      scanned: 0,
      total: 0,
      releases: [],
      done: false,
    });

    await waitForBrowseContent(9000);
    await sleep(450);

    const signInLink = document.querySelector(
      "ytmusic-nav-bar a.sign-in-link, ytmusic-nav-bar a[href*='ServiceLogin']"
    );
    if (signInLink && !isHiddenEl(signInLink)) {
      postFeedUpdate({
        status: "Sign in to YouTube Music to build your personalized Feed.",
        scanned: 0,
        total: 0,
        releases: [],
        done: true,
      });
      setTimeout(() => {
        window.location.href = "about:blank";
      }, 300);
      return;
    }

    const artistMap = new Map();
    const addArtists = list => {
      for (const a of list) {
        if (!artistMap.has(a.channel_id)) {
          artistMap.set(a.channel_id, a);
        }
      }
    };

    const currentPath = window.location.pathname.toLowerCase();

    if (source === "subscribed" || source === "both") {
      if (!currentPath.includes("subscriptions")) {
        spaNavigateBrowse("FEmusic_library_corpus_artists");
        await sleep(450);
      }
      postFeedUpdate({
        status: "Collecting all subscribed artists…",
        scanned: 0,
        total: 0,
        releases: [],
        done: false,
      });
      await waitForBrowseContent(7000);
      await scrollToLoadAllArtists();
      const root = getActiveBrowseResponse();
      addArtists(scrapeArtistsFromPage(root));
    }

    if (source === "collection" || source === "both") {
      if (!window.location.pathname.toLowerCase().includes("/library/artists")) {
        spaNavigateBrowse("FEmusic_library_corpus_track_artists");
        await sleep(500);
      }
      postFeedUpdate({
        status: "Collecting all artists from your library…",
        scanned: 0,
        total: 0,
        releases: [],
        done: false,
      });
      await waitForBrowseContent(7000);
      await scrollToLoadAllArtists();
      const root = getActiveBrowseResponse();
      addArtists(scrapeArtistsFromPage(root));
    }

    const artists = Array.from(artistMap.values());
    if (artists.length === 0) {
      postFeedUpdate({
        status: "No artists found for the selected Feed source.",
        scanned: 0,
        total: 0,
        releases: [],
        done: true,
      });
      setTimeout(() => {
        window.location.href = "about:blank";
      }, 300);
      return;
    }

    // Hand full artist roster to Rust so Deezer Public API runs first, with automatic WebView fallback
    postRaw({
      kind: "feed_artists_ready",
      run: runId,
      artists,
    });
  }

  /**
   * Invoked by Rust (`webview.rs`) ONLY for artists that Deezer could not confidently match
   * or fetch. Runs the existing YouTube Music WebView artist-page scraper + track #1 video_id
   * discovery, then hands off to `www.youtube.com/watch?v=...` exact-date resolution.
   */
  window.__ytmRunFallbackScan = async function (fallbackArtists, expectedRun) {
    if (expectedRun !== runId) return;
    const artists = Array.isArray(fallbackArtists) ? fallbackArtists : [];
    const now = new Date();
    const currentYear = now.getFullYear();
    const minRecentYear = now.getMonth() < 2 ? currentYear - 1 : currentYear;

    const allReleases = [];
    const globalSeen = new Set();

    for (let i = 0; i < artists.length; i++) {
      const artist = artists[i];
      const channelId = artist.channel_id || artist.channelId;
      if (!channelId) continue;

      postRaw({
        kind: "feed_fallback_progress",
        run: runId,
        status: `YouTube Music fallback: ${artist.name} (${i + 1}/${artists.length})…`,
        fallback_done: i,
        fallback_total: artists.length,
        releases: stripInternalFields(allReleases),
      });

      spaNavigateBrowse(channelId);
      await sleep(320);
      const root = await waitForBrowseContent(6000, channelId);
      const found = scrapeArtistReleases(root, artist);

      for (const r of found) {
        const key = (r.href + "|" + r.title).toLowerCase();
        if (!globalSeen.has(key)) {
          globalSeen.add(key);
          allReleases.push(r);
        }
      }

      postRaw({
        kind: "feed_fallback_progress",
        run: runId,
        status: `YouTube Music fallback: ${i + 1}/${artists.length} artists…`,
        fallback_done: i + 1,
        fallback_total: artists.length,
        releases: stripInternalFields(allReleases),
      });
    }

    const recentNeedsVideo = allReleases
      .filter(
        r =>
          !r.release_date &&
          !r.video_id &&
          r._browseId &&
          (r.is_latest || (r.year !== null && r.year >= minRecentYear))
      )
      .slice(0, 250);

    for (let j = 0; j < recentNeedsVideo.length; j++) {
      const rel = recentNeedsVideo[j];
      postRaw({
        kind: "feed_fallback_progress",
        run: runId,
        status: `Locating track info: ${rel.title} (${j + 1}/${recentNeedsVideo.length})…`,
        fallback_done: artists.length,
        fallback_total: artists.length,
        releases: stripInternalFields(allReleases),
      });

      spaNavigateBrowse(rel._browseId);
      await sleep(280);
      const root = await waitForBrowseContent(5500, rel._browseId);
      const vid = extractFirstTrackVideoIdFromReleasePage(root);
      if (vid) {
        rel.video_id = vid;
      }
    }

    postRaw({
      kind: "feed_phase1_done",
      run: runId,
      scanned: artists.length,
      total: artists.length,
      min_recent_year: minRecentYear,
      releases: stripInternalFields(allReleases),
    });
  };

  if (document.readyState === "loading") {
    window.addEventListener("DOMContentLoaded", () => setTimeout(runFeedCollection, 450), {
      once: true,
    });
  } else {
    setTimeout(runFeedCollection, 450);
  }
})();

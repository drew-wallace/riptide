(() => {
  if (window.__ytmMaskInitialized) return;
  window.__ytmMaskInitialized = true;

  // Prevent YouTube Music from ever popping up a "Leave site?" browser confirmation dialog
  try {
    const origAddEventListener = window.addEventListener.bind(window);
    window.addEventListener = function (type, listener, options) {
      if (type === "beforeunload") return;
      return origAddEventListener(type, listener, options);
    };
    Object.defineProperty(window, "onbeforeunload", {
      configurable: true,
      get() { return null; },
      set() {},
    });
  } catch (_) {}

  const registry = new Map();
  let nextActionId = 1;
  let lastPayloadJson = "";
  let syncTimer = null;
  let lyricsRequested = false;
  let relatedRequested = false;
  let lastTrackedUrl = window.location.href;
  let lastUrlChangedAt = Date.now();
  let playlistHydrationToken = 0;

  function registerAction(el) {
    if (!el) return "";
    const id = "act-" + (nextActionId++);
    registry.set(id, el);
    return id;
  }

  function cleanText(node) {
    if (!node) return "";
    return (node.textContent || "").replace(/\s+/g, " ").trim();
  }

  function isHiddenEl(el) {
    if (!el) return true;
    let cur = el;
    while (cur && cur !== document.body && cur !== document.documentElement) {
      if (cur.hasAttribute && cur.hasAttribute("hidden")) {
        return true;
      }
      if (cur.style && (cur.style.display === "none" || cur.style.visibility === "hidden")) {
        return true;
      }
      cur = cur.parentElement;
    }
    try {
      const cs = window.getComputedStyle(el);
      if (cs && (cs.display === "none" || cs.visibility === "hidden")) return true;
    } catch (_) {}
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

    const priorityKeys = [
      "thumbnailRenderer",
      "musicThumbnailRenderer",
      "croppedSquareThumbnailRenderer",
      "thumbnail",
      "foregroundThumbnail",
      "background",
      "data",
    ];
    for (const k of priorityKeys) {
      if (obj[k] && typeof obj[k] === "object") {
        const found = findBestThumbnailInObject(obj[k], depth + 1, visited);
        if (found) return found;
      }
    }

    if (Array.isArray(obj)) {
      for (const item of obj.slice(0, 4)) {
        const found = findBestThumbnailInObject(item, depth + 1, visited);
        if (found) return found;
      }
    } else {
      for (const k of Object.keys(obj)) {
        if (
          k === "menu" ||
          k === "badges" ||
          k === "subtitle" ||
          k === "title" ||
          k === "overlay" ||
          k === "navigationEndpoint"
        ) {
          continue;
        }
        const v = obj[k];
        if (v && typeof v === "object") {
          const found = findBestThumbnailInObject(v, depth + 1, visited);
          if (found) return found;
        }
      }
    }
    return null;
  }

  function getImgSrc(root) {
    if (!root) return null;

    const thumbHosts = [
      root.querySelector && root.querySelector("ytmusic-thumbnail-renderer"),
      root.querySelector && root.querySelector("yt-img-shadow"),
      root,
    ];
    for (const host of thumbHosts) {
      if (!host) continue;
      if (typeof host.src === "string") {
        const direct = normalizeAndUpscaleUrl(host.src);
        if (direct) return direct;
      }
      const candidates = [host.data, host.__data && host.__data.data, host.__data, host.thumbnail];
      for (const dataObj of candidates) {
        if (dataObj) {
          const url = findBestThumbnailInObject(dataObj);
          if (url) return url;
        }
      }
    }

    if (root.querySelectorAll) {
      const imgs = root.querySelectorAll("img");
      for (const img of imgs) {
        const raw =
          img.currentSrc ||
          img.src ||
          img.getAttribute("src") ||
          img.getAttribute("data-src");
        const norm = normalizeAndUpscaleUrl(raw);
        if (norm) return norm;
      }
    }
    return null;
  }

  function hrefFromEndpoint(ep) {
    if (!ep || typeof ep !== "object") return null;
    if (ep.browseEndpoint && ep.browseEndpoint.browseId) {
      const bid = ep.browseEndpoint.browseId;
      if (bid.startsWith("VL")) {
        return "playlist?list=" + encodeURIComponent(bid.slice(2));
      }
      return "browse/" + encodeURIComponent(bid);
    }
    if (ep.urlEndpoint && ep.urlEndpoint.url) {
      return ep.urlEndpoint.url;
    }
    return null;
  }

  function extractEndpointHref(el) {
    if (!el) return null;
    const link =
      el.querySelector &&
      (el.querySelector(".title a.yt-simple-endpoint[href]") ||
        el.querySelector("a.yt-simple-endpoint[href]") ||
        el.querySelector("a[href]"));
    if (link) {
      const href = link.getAttribute("href");
      if (href && href !== "#" && !href.startsWith("javascript:")) {
        return href;
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
    return hrefFromEndpoint(ep);
  }

  function cleanHrefPath(rawHref) {
    if (!rawHref) return "";
    return rawHref
      .replace(/^https?:\/\/music\.youtube\.com/i, "")
      .replace(/^\/+/, "");
  }

  function isBrowsePath(path) {
    if (!path) return false;
    return (
      path.startsWith("playlist") ||
      path.startsWith("browse/") ||
      path.startsWith("channel/")
    );
  }

  /**
   * Extracts structured clickable links (Artist, Album, Playlist) + plain separator runs
   * from any yt-formatted-string or metadata container.
   */
  function extractTextLinks(container) {
    if (!container) return [];
    const links = [];

    // 1. Prefer Polymer `runs` on yt-formatted-string if available (gives exact browseIds)
    const fmtStrings =
      container.tagName && container.tagName.toLowerCase() === "yt-formatted-string"
        ? [container]
        : Array.from(container.querySelectorAll("yt-formatted-string"));

    if (fmtStrings.length > 0) {
      for (let idx = 0; idx < fmtStrings.length; idx++) {
        const fs = fmtStrings[idx];
        if (isHiddenEl(fs)) continue;
        if (idx > 0 && links.length > 0) {
          links.push({ text: " • ", href: null });
        }
        const runs = (fs.data && fs.data.runs) || (fs.__data && fs.__data.text && fs.__data.text.runs);
        if (Array.isArray(runs) && runs.length > 0) {
          for (const run of runs) {
            const txt = (run && run.text) || "";
            if (!txt) continue;
            const epHref = hrefFromEndpoint(run.navigationEndpoint);
            const clean = epHref ? cleanHrefPath(epHref) : "";
            links.push({
              text: txt,
              href: isBrowsePath(clean) ? clean : null,
            });
          }
          continue;
        }
        // Fallback: walk child nodes of yt-formatted-string
        for (const node of Array.from(fs.childNodes)) {
          if (node.nodeType === Node.ELEMENT_NODE && node.tagName.toLowerCase() === "a") {
            const txt = cleanText(node);
            const raw = node.getAttribute("href");
            const clean = cleanHrefPath(raw);
            if (txt) {
              links.push({ text: txt, href: isBrowsePath(clean) ? clean : null });
            }
          } else {
            const txt = (node.textContent || "").replace(/\s+/g, " ");
            if (txt.trim() || txt === " • " || txt === " & " || txt === ", ") {
              links.push({ text: txt, href: null });
            }
          }
        }
      }
    }

    if (links.length === 0) {
      // Fallback for plain elements with <a> tags
      const anchors = container.querySelectorAll ? Array.from(container.querySelectorAll("a[href]")) : [];
      if (anchors.length > 0) {
        for (let i = 0; i < anchors.length; i++) {
          const a = anchors[i];
          const txt = cleanText(a);
          const clean = cleanHrefPath(a.getAttribute("href"));
          if (!txt) continue;
          if (i > 0) links.push({ text: " • ", href: null });
          links.push({ text: txt, href: isBrowsePath(clean) ? clean : null });
        }
      }
    }

    // Filter out trailing duration tokens like " • 3:45" if they crept in
    while (links.length > 0) {
      const last = links[links.length - 1];
      if (!last.href && /^\s*[•·\-]?\s*\d+:\d{2}(:\d{2})?\s*$/.test(last.text)) {
        links.pop();
      } else if (!last.href && /^\s*[•·\-]\s*$/.test(last.text)) {
        links.pop();
      } else {
        break;
      }
    }

    return links;
  }

  function spaNavigateEndpoint(endpoint, fallbackUrl) {
    const app = document.querySelector("ytmusic-app");
    const prevUrl = window.location.href;
    if (app && endpoint) {
      try {
        app.dispatchEvent(
          new CustomEvent("yt-navigate", {
            bubbles: true,
            composed: true,
            detail: { endpoint },
          })
        );
      } catch (_) {}
    }
    if (fallbackUrl) {
      setTimeout(() => {
        if (window.location.href === prevUrl) {
          window.location.href = fallbackUrl;
        }
      }, 480);
    }
  }

  function endpointForUrl(url) {
    const path = cleanHrefPath(url);
    if (!path || path === "") {
      return { browseEndpoint: { browseId: "FEmusic_home" } };
    }
    if (path.startsWith("explore")) {
      return { browseEndpoint: { browseId: "FEmusic_explore" } };
    }
    if (path === "library") {
      return { browseEndpoint: { browseId: "FEmusic_library_landing" } };
    }
    if (path.startsWith("library/playlists")) {
      return { browseEndpoint: { browseId: "FEmusic_liked_playlists" } };
    }
    if (path.startsWith("library/songs")) {
      return { browseEndpoint: { browseId: "FEmusic_liked_videos" } };
    }
    if (path.startsWith("library/albums")) {
      return { browseEndpoint: { browseId: "FEmusic_liked_albums" } };
    }
    if (path.startsWith("library/artists")) {
      return { browseEndpoint: { browseId: "FEmusic_library_corpus_track_artists" } };
    }
    if (path.startsWith("playlist?")) {
      try {
        const params = new URLSearchParams(path.split("?")[1] || "");
        const list = params.get("list");
        if (list) {
          const bid = list.startsWith("VL") ? list : "VL" + list;
          return { browseEndpoint: { browseId: bid } };
        }
      } catch (_) {}
    }
    if (path.startsWith("browse/")) {
      const bid = decodeURIComponent(path.slice("browse/".length).split("?")[0]);
      if (bid) return { browseEndpoint: { browseId: bid } };
    }
    if (path.startsWith("channel/")) {
      const cid = decodeURIComponent(path.slice("channel/".length).split("?")[0]);
      if (cid) return { browseEndpoint: { browseId: cid } };
    }
    if (path.startsWith("search?")) {
      try {
        const params = new URLSearchParams(path.split("?")[1] || "");
        const q = params.get("q");
        if (q) return { searchEndpoint: { query: q } };
      } catch (_) {}
    }
    return null;
  }

  function inferRoute(urlStr) {
    try {
      const u = new URL(urlStr);
      const path = u.pathname.toLowerCase();
      if (path === "/" || path === "") return "home";
      if (
        path.startsWith("/explore") ||
        path.startsWith("/moods_and_genres") ||
        path.startsWith("/charts") ||
        path.startsWith("/new_releases")
      ) {
        return "explore";
      }
      if (path.startsWith("/search")) return "search";
      if (path === "/library" || path === "/library/") return "library";
      if (path.startsWith("/library/playlists")) return "library_playlists";
      if (path.startsWith("/library/songs")) return "library_songs";
      if (path.startsWith("/library/albums")) return "library_albums";
      if (path.startsWith("/library/artists") || path.startsWith("/library/subscriptions")) {
        return "library_artists";
      }
      if (path.startsWith("/playlist") || path.startsWith("/browse/vl")) return "playlist";
      if (path.startsWith("/browse/mpreb") || path.startsWith("/browse/ol") || path.startsWith("/album")) {
        return "album";
      }
      if (path.startsWith("/channel/") || path.startsWith("/browse/uc")) return "artist";
      return "other";
    } catch (_) {
      return "other";
    }
  }

  function isPlayerPageOpen() {
    const page = document.querySelector("ytmusic-player-page");
    return !!(page && page.hasAttribute("player-page-open_"));
  }

  function getActiveBrowseRoot(route) {
    const playerOpen = isPlayerPageOpen();

    if (route === "search") {
      const searchPages = Array.from(document.querySelectorAll("ytmusic-search-page")).filter(
        el => playerOpen || !isHiddenEl(el)
      );
      if (searchPages.length > 0) {
        return searchPages[searchPages.length - 1];
      }
      return null;
    }

    const browsePage = document.querySelector("#browse-page");
    if (browsePage && (playerOpen || !isHiddenEl(browsePage))) {
      const responses = Array.from(browsePage.querySelectorAll("ytmusic-browse-response")).filter(
        el => !el.hasAttribute("hidden") && (playerOpen || !isHiddenEl(el))
      );

      if (responses.length > 0) {
        if (route === "playlist" || route === "album") {
          for (let i = responses.length - 1; i >= 0; i--) {
            if (
              responses[i].querySelector(
                "ytmusic-two-column-browse-results-renderer, ytmusic-playlist-shelf-renderer"
              )
            ) {
              return responses[i];
            }
          }
        }
        return responses[responses.length - 1];
      }
      return browsePage;
    }

    const allResponses = Array.from(document.querySelectorAll("ytmusic-browse-response")).filter(
      el => !el.hasAttribute("hidden") && !el.closest("ytmusic-player-page") && (playerOpen || !isHiddenEl(el))
    );
    if (allResponses.length > 0) {
      return allResponses[allResponses.length - 1];
    }

    return null;
  }

  function maybeAutoHydratePlaylist(route, url) {
    if (route !== "playlist") return;
    const token = ++playlistHydrationToken;
    let steps = 0;

    function step() {
      if (token !== playlistHydrationToken) return;
      if (window.location.href !== url || inferRoute(window.location.href) !== "playlist") return;
      if (steps >= 14) return;

      const root = getActiveBrowseRoot("playlist");
      if (!root) {
        setTimeout(step, 500);
        return;
      }

      const shelves = Array.from(root.querySelectorAll("ytmusic-shelf-renderer")).filter(
        el => !isHiddenEl(el)
      );
      const hasSuggestions = shelves.some(s =>
        cleanText(s.querySelector(".title, h2")).toLowerCase().includes("suggest")
      );
      const cont = Array.from(root.querySelectorAll("ytmusic-continuation-item-renderer")).find(
        el => !isHiddenEl(el)
      );

      if (hasSuggestions && !cont) {
        sendState(true);
        return;
      }

      steps++;
      if (cont) {
        try {
          cont.scrollIntoView({ block: "end", behavior: "instant" });
        } catch (_) {}
      }
      const layout = document.querySelector("ytmusic-app-layout");
      if (layout) layout.scrollTop = layout.scrollHeight;
      window.scrollTo(0, document.documentElement.scrollHeight);

      setTimeout(() => {
        sendState(true);
        setTimeout(step, 450);
      }, 300);
    }

    setTimeout(step, 900);
  }

  function extractAccount() {
    const onYtm = window.location.hostname === "music.youtube.com";
    if (!onYtm) {
      return { is_signed_in: false, name: "Signing in…", avatar_url: null };
    }
    const signInLink = document.querySelector(
      "ytmusic-nav-bar a.sign-in-link, ytmusic-nav-bar a[href*='ServiceLogin']"
    );
    const signInVisible = signInLink && !isHiddenEl(signInLink);
    const avatarImg = document.querySelector("ytmusic-settings-button img, #avatar-btn img");
    const avatarSrc = avatarImg ? normalizeAndUpscaleUrl(avatarImg.currentSrc || avatarImg.src) : null;
    const isSignedIn = !!(avatarSrc && !signInVisible);
    return {
      is_signed_in: isSignedIn,
      name: isSignedIn ? avatarImg.alt || "YouTube Music Account" : "Guest",
      avatar_url: isSignedIn ? avatarSrc : null,
    };
  }

  function extractPlayer() {
    const bar = document.querySelector("ytmusic-player-bar");
    const video = document.querySelector("video.html5-main-video") || document.querySelector("video");
    const titleEl =
      bar &&
      (bar.querySelector(".title.ytmusic-player-bar") ||
        bar.querySelector("yt-formatted-string.title"));
    const bylineEl =
      bar &&
      (bar.querySelector(".byline.ytmusic-player-bar") ||
        bar.querySelector(".subtitle yt-formatted-string.byline") ||
        bar.querySelector(".subtitle"));

    const title = cleanText(titleEl);
    const bylineRaw = cleanText(bylineEl);
    const subtitleLinks = bylineEl ? extractTextLinks(bylineEl) : [];
    const parts = bylineRaw
      .split("•")
      .map(s => s.trim())
      .filter(Boolean);
    const artist = parts[0] || "";
    const album = parts[1] || "";

    const artwork = bar ? getImgSrc(bar) : null;
    const duration = video && Number.isFinite(video.duration) ? video.duration : 0;
    const currentTime = video && Number.isFinite(video.currentTime) ? video.currentTime : 0;
    const isPlaying = video ? !video.paused && !video.ended && video.readyState > 1 : false;

    let volume = 100;
    let isMuted = false;
    const moviePlayer = document.querySelector("#movie_player");
    if (moviePlayer && typeof moviePlayer.getVolume === "function") {
      try {
        volume = Math.round(moviePlayer.getVolume());
        isMuted = !!moviePlayer.isMuted();
      } catch (_) {}
    } else if (video) {
      volume = Math.round((video.volume || 0) * 100);
      isMuted = !!video.muted;
    }

    let likeStatus = "none";
    if (bar) {
      const likeBtn = bar.querySelector(
        "ytmusic-like-button-renderer #button-shape-like button, ytmusic-like-button-renderer .like button"
      );
      const dislikeBtn = bar.querySelector(
        "ytmusic-like-button-renderer #button-shape-dislike button, ytmusic-like-button-renderer .dislike button"
      );
      if (likeBtn && likeBtn.getAttribute("aria-pressed") === "true") {
        likeStatus = "like";
      } else if (dislikeBtn && dislikeBtn.getAttribute("aria-pressed") === "true") {
        likeStatus = "dislike";
      }
    }

    let repeatMode = "off";
    if (bar) {
      const attr = (
        bar.getAttribute("repeat-mode") ||
        bar.getAttribute("repeat-mode_") ||
        ""
      ).toUpperCase();
      if (attr === "ALL") repeatMode = "all";
      else if (attr === "ONE") repeatMode = "one";
      else {
        const repBtn = bar.querySelector(".repeat");
        const label = (
          (repBtn && (repBtn.getAttribute("aria-label") || repBtn.getAttribute("title"))) ||
          ""
        ).toLowerCase();
        if (label.includes("repeat one")) repeatMode = "one";
        else if (label.includes("repeat all")) repeatMode = "all";
      }
    }

    let isShuffled = false;
    if (bar) {
      const shufBtn = bar.querySelector(".shuffle button, .shuffle");
      if (shufBtn) {
        isShuffled = shufBtn.getAttribute("aria-pressed") === "true";
      }
    }

    const hasMedia = !!(title || duration > 0);

    return {
      has_media: hasMedia,
      title,
      artist,
      album,
      subtitle_links: subtitleLinks,
      artwork_url: artwork,
      is_playing: isPlaying,
      current_time: currentTime,
      duration,
      volume: Math.max(0, Math.min(100, volume)),
      is_muted: isMuted,
      like_status: likeStatus,
      repeat_mode: repeatMode,
      is_shuffled: isShuffled,
    };
  }

  function extractChips(root) {
    if (!root) return [];
    const chips = [];
    const seen = new Set();
    const els = root.querySelectorAll("ytmusic-chip-cloud-chip-renderer");
    for (const el of els) {
      if (isHiddenEl(el)) continue;
      const clickable = el.querySelector("a, button") || el;
      const label = cleanText(clickable);
      if (!label || seen.has(label)) continue;
      seen.add(label);
      const selected =
        el.hasAttribute("is-selected") ||
        clickable.getAttribute("aria-selected") === "true" ||
        el.classList.contains("iron-selected");
      chips.push({
        id: registerAction(clickable),
        label,
        selected,
      });
      if (chips.length >= 16) break;
    }
    return chips;
  }

  function extractHeaderActions(headerEl) {
    const actions = [];
    const seenLabels = new Set();
    const usedEls = new Set();

    function pushAction(el, label, kind, isActive = false) {
      if (!el || usedEls.has(el)) return;
      const clean = (label || "").replace(/\s+/g, " ").trim();
      if (!clean) return;
      const key = clean.toLowerCase();
      if (seenLabels.has(key)) return;
      seenLabels.add(key);
      usedEls.add(el);
      actions.push({
        id: registerAction(el),
        label: clean,
        kind,
        is_active: !!isActive,
      });
    }

    const btnNodes = headerEl.querySelectorAll(
      "ytmusic-play-button-renderer, yt-button-renderer, ytmusic-toggle-button-renderer, ytmusic-subscribe-button-renderer, ytmusic-menu-renderer, button"
    );

    for (const node of btnNodes) {
      if (isHiddenEl(node)) continue;
      const tag = node.tagName.toLowerCase();

      if (tag === "ytmusic-subscribe-button-renderer") {
        const btn = node.querySelector("button") || node;
        const txt = cleanText(btn) || btn.getAttribute("aria-label") || "Subscribe";
        const subscribed =
          node.hasAttribute("subscribed") ||
          btn.getAttribute("aria-pressed") === "true" ||
          txt.toLowerCase().startsWith("subscribed");
        pushAction(btn, txt, "toggle", subscribed);
        continue;
      }

      if (tag === "ytmusic-menu-renderer") {
        const btn =
          node.querySelector(
            "button[aria-label*='Action'], button[aria-label*='More'], yt-icon-button button, button"
          ) || node;
        pushAction(btn, "More", "menu", false);
        continue;
      }

      if (tag === "ytmusic-play-button-renderer") {
        const btn = node.querySelector("button") || node;
        pushAction(btn, "Play", "primary", false);
        continue;
      }

      if (tag === "ytmusic-toggle-button-renderer") {
        const btn = node.querySelector("button") || node;
        const txt = cleanText(btn) || btn.getAttribute("aria-label") || "";
        const lower = txt.toLowerCase();
        if (lower.includes("library") || lower.includes("save") || lower.includes("saved")) {
          const active =
            btn.getAttribute("aria-pressed") === "true" ||
            lower.includes("saved") ||
            lower.includes("remove");
          pushAction(btn, active ? "In Library" : "Save to Library", "toggle", active);
        }
        continue;
      }

      if (tag === "yt-button-renderer") {
        if (node.closest("ytmusic-subscribe-button-renderer, ytmusic-menu-renderer")) continue;
        const btn = node.querySelector("button, a") || node;
        const txt = cleanText(btn) || btn.getAttribute("aria-label") || "";
        const lower = txt.toLowerCase();
        if (!txt || lower === "more" || lower === "less" || lower.includes("read more")) continue;
        if (lower.includes("play") || lower === "shuffle") {
          pushAction(btn, txt, actions.length === 0 ? "primary" : "secondary", false);
        } else if (lower.includes("radio") || lower.includes("mix")) {
          pushAction(btn, txt, "secondary", false);
        } else if (lower.includes("edit") || lower.includes("save") || lower.includes("share")) {
          pushAction(btn, txt, "secondary", false);
        }
        continue;
      }

      if (tag === "button") {
        if (
          node.closest(
            "ytmusic-subscribe-button-renderer, ytmusic-menu-renderer, yt-button-renderer, ytmusic-toggle-button-renderer, ytmusic-play-button-renderer"
          )
        ) {
          continue;
        }
        const raw = (node.getAttribute("aria-label") || "") + " " + cleanText(node);
        const lower = raw.toLowerCase();
        if (lower.includes("action menu") || lower.includes("more actions")) {
          pushAction(node, "More", "menu", false);
        } else if (lower.includes("shuffle")) {
          pushAction(node, "Shuffle", actions.length === 0 ? "primary" : "secondary", false);
        } else if (lower.includes("radio") || lower.includes("mix")) {
          pushAction(node, "Radio", "secondary", false);
        } else if (lower.includes("play")) {
          pushAction(node, "Play", "primary", false);
        } else if (lower.includes("subscribe")) {
          const active =
            node.getAttribute("aria-pressed") === "true" || lower.includes("subscribed");
          pushAction(node, cleanText(node) || "Subscribe", "toggle", active);
        }
      }
    }

    return actions;
  }

  function extractHeaderArtwork(headerEl) {
    if (!headerEl) return null;

    // 1. Prefer the primary header cover thumbnail (skipping the tiny .strapline-thumbnail artist avatar!)
    const primaryThumbSelectors = [
      ".thumbnail ytmusic-thumbnail-renderer",
      "ytmusic-thumbnail-renderer.thumbnail",
      "#thumbnail ytmusic-thumbnail-renderer",
      ".thumbnail img",
      "#thumbnail img",
      "ytmusic-fullbleed-thumbnail-renderer",
    ];
    for (const sel of primaryThumbSelectors) {
      const node = headerEl.querySelector(sel);
      if (node && !node.closest(".strapline-thumbnail")) {
        const src = getImgSrc(node);
        if (src) return src;
      }
    }

    // 2. Check Polymer data directly on headerEl.data.thumbnail (ignoring straplineThumbnail)
    const data = headerEl.data || (headerEl.__data && headerEl.__data.data) || headerEl.__data;
    if (data) {
      const thumbData =
        data.thumbnail ||
        data.foregroundThumbnail ||
        (data.thumbnailRenderer && data.thumbnailRenderer.musicThumbnailRenderer);
      if (thumbData) {
        const src = findBestThumbnailInObject(thumbData);
        if (src) return src;
      }
    }

    // 3. Last resort: any non-strapline thumbnail inside the header
    const allThumbs = headerEl.querySelectorAll("ytmusic-thumbnail-renderer, img");
    for (const node of allThumbs) {
      if (node.closest && node.closest(".strapline-thumbnail, ytmusic-menu-renderer")) continue;
      const src = getImgSrc(node);
      if (src) return src;
    }

    return null;
  }

  function extractHeader(root) {
    if (!root) return null;
    const headerEl = Array.from(
      root.querySelectorAll(
        "ytmusic-responsive-header-renderer, ytmusic-detail-header-renderer, ytmusic-immersive-header-renderer, ytmusic-visual-header-renderer"
      )
    ).find(el => !el.hasAttribute("hidden"));
    if (!headerEl) return null;

    const title = cleanText(headerEl.querySelector("h1, .title"));
    if (!title) return null;

    const straplineEl = headerEl.querySelector(".strapline-text, .subtitle, .second-subtitle");
    const subtitle = cleanText(headerEl.querySelector(".subtitle, .second-subtitle, .strapline-text"));
    const subtitleLinks = extractTextLinks(
      headerEl.querySelector(".strapline-text") || straplineEl
    );
    const description = cleanText(
      headerEl.querySelector(".description, ytmusic-description-shelf-renderer .description")
    );
    const artwork = extractHeaderArtwork(headerEl);
    const tag = headerEl.tagName.toLowerCase();
    const isArtist = tag.includes("immersive") || tag.includes("visual");

    let kind = "COLLECTION";
    if (isArtist) kind = "ARTIST";
    else if (
      subtitle.toLowerCase().includes("album") ||
      subtitle.toLowerCase().includes("single") ||
      subtitle.toLowerCase().includes("ep")
    ) {
      kind = "ALBUM";
    } else if (
      subtitle.toLowerCase().includes("playlist") ||
      window.location.pathname.startsWith("/playlist")
    ) {
      kind = "PLAYLIST";
    }

    return {
      kind,
      title,
      subtitle,
      subtitle_links: subtitleLinks,
      description: description.slice(0, 280),
      artwork_url: artwork,
      is_circular: isArtist,
      actions: extractHeaderActions(headerEl),
    };
  }

  function findMenuButton(el) {
    if (!el || !el.querySelector) return null;
    return (
      el.querySelector(
        "ytmusic-menu-renderer button[aria-label*='Action'], ytmusic-menu-renderer button[aria-label*='More']"
      ) ||
      el.querySelector("ytmusic-menu-renderer yt-icon-button button, ytmusic-menu-renderer button") ||
      el.querySelector("button[aria-label*='Action menu'], button[aria-label*='More actions']")
    );
  }

  function parseTwoRowItem(el, allowHiddenAncestor = false) {
    if (!allowHiddenAncestor && isHiddenEl(el)) return null;
    const titleLink = el.querySelector(".title a, a.yt-simple-endpoint.title, .title");
    const title = cleanText(titleLink || el.querySelector(".title"));
    if (!title) return null;

    const subtitleEl = el.querySelector(".subtitle");
    const subtitle = cleanText(subtitleEl);
    const metaLinks = extractTextLinks(subtitleEl);
    const artwork = getImgSrc(el);
    const href = extractEndpointHref(el);
    const path = cleanHrefPath(href);
    const isNewPlaylist =
      title.toLowerCase() === "new playlist" || !!el.querySelector("[aria-label*='New playlist']");

    const linkEl =
      el.querySelector(".title a.yt-simple-endpoint[href], a.yt-simple-endpoint[href]") || titleLink;
    const playBtn = el.querySelector(
      "ytmusic-play-button-renderer, .play-button, button[aria-label*='Play']"
    );
    const menuBtn = isNewPlaylist ? null : findMenuButton(el);
    const clickTarget = linkEl || playBtn || el;

    const isCircular =
      subtitle.toLowerCase().includes("subscriber") ||
      subtitle.toLowerCase().includes("monthly audience") ||
      subtitle.toLowerCase() === "artist" ||
      !!el.querySelector("[thumbnail-crop-style='MUSIC_THUMBNAIL_CROP_CIRCLE']");

    const isExplicit = !!el.querySelector(
      "ytmusic-inline-badge-renderer[aria-label*='Explicit'], [title='Explicit']"
    );

    return {
      id: registerAction(clickTarget),
      play_id: playBtn && !isNewPlaylist ? registerAction(playBtn) : null,
      add_id: null,
      like_id: null,
      dislike_id: null,
      like_status: "none",
      menu_id: menuBtn ? registerAction(menuBtn) : null,
      title,
      subtitle,
      secondary: "",
      meta_links: metaLinks,
      duration: "",
      artwork_url: artwork,
      href,
      is_explicit: isExplicit,
      is_playing: false,
      is_circular: isCircular,
      is_new_playlist: isNewPlaylist,
      is_browse_item: isBrowsePath(path),
      badge: "",
    };
  }

  function parseListItem(el, allowHiddenAncestor = false) {
    if (!allowHiddenAncestor && isHiddenEl(el)) return null;
    const flexCols = Array.from(
      el.querySelectorAll(".flex-columns yt-formatted-string, .flex-columns .flex-column")
    );
    let title = "";
    let subtitle = "";
    let secondary = "";

    const titleCol = el.querySelector(".title-column, .title.ytmusic-responsive-list-item-renderer");
    if (titleCol) {
      title = cleanText(titleCol);
    } else if (flexCols[0]) {
      title = cleanText(flexCols[0]);
    }
    if (!title) return null;

    const secContainer = el.querySelector(".secondary-flex-columns");
    const rawSecCols = secContainer
      ? Array.from(secContainer.children).filter(c => !isHiddenEl(c))
      : [];
    const seenColText = new Set();
    const secTexts = [];
    for (const col of rawSecCols) {
      const t = cleanText(col);
      if (t && !seenColText.has(t)) {
        seenColText.add(t);
        secTexts.push(t);
      }
    }

    let metaLinks = [];
    if (secContainer) {
      metaLinks = extractTextLinks(secContainer);
    }
    if (secTexts.length > 0) {
      subtitle = secTexts[0];
      if (secTexts.length > 1) {
        secondary = secTexts[1];
      }
    } else if (flexCols.length > 1) {
      subtitle = cleanText(flexCols[1]);
      if (flexCols.length > 2) {
        const s2 = cleanText(flexCols[2]);
        if (s2 !== subtitle) secondary = s2;
      }
      if (metaLinks.length === 0) {
        metaLinks = extractTextLinks(flexCols[1]);
      }
    }

    let duration = "";
    const fixedCols = Array.from(
      el.querySelectorAll(".fixed-columns yt-formatted-string, .fixed-column")
    );
    for (const fc of fixedCols) {
      const t = cleanText(fc);
      if (/^\d+:\d{2}(:\d{2})?$/.test(t)) {
        duration = t;
        break;
      }
    }
    if (!duration && subtitle) {
      const m = subtitle.match(/(\d+:\d{2}(?::\d{2})?)$/);
      if (m) {
        duration = m[1];
        subtitle = subtitle.replace(/[•·\-]\s*\d+:\d{2}(?::\d{2})?$/, "").trim();
      }
    }

    // Only extract artwork if the row actually has a thumbnail element (album tracks intentionally have none)
    const hasThumbEl = !!el.querySelector("ytmusic-thumbnail-renderer, .left-items img");
    const artwork = hasThumbEl ? getImgSrc(el) : null;

    const playBtn = el.querySelector("ytmusic-play-button-renderer, .play-button");
    const linkEl = el.querySelector(".title-column a[href], a.yt-simple-endpoint[href]");
    const href = extractEndpointHref(el);
    const path = cleanHrefPath(href);
    const isBrowseItem = isBrowsePath(path);

    const likeBtn = el.querySelector(
      "ytmusic-like-button-renderer #button-shape-like button, ytmusic-like-button-renderer .like button"
    );
    const dislikeBtn = el.querySelector(
      "ytmusic-like-button-renderer #button-shape-dislike button, ytmusic-like-button-renderer .dislike button"
    );
    let likeStatus = "none";
    if (likeBtn && likeBtn.getAttribute("aria-pressed") === "true") likeStatus = "like";
    else if (dislikeBtn && dislikeBtn.getAttribute("aria-pressed") === "true") likeStatus = "dislike";

    const menuBtn = findMenuButton(el);

    let addBtn = null;
    const actionBtns = el.querySelectorAll("yt-button-renderer button, button");
    for (const btn of actionBtns) {
      if (btn.closest("ytmusic-like-button-renderer, ytmusic-menu-renderer")) continue;
      if (isHiddenEl(btn)) continue;
      const lbl = ((btn.getAttribute("aria-label") || "") + " " + cleanText(btn)).toLowerCase();
      if (lbl.includes("add") && !lbl.includes("address")) {
        addBtn = btn;
        break;
      }
    }

    const primaryTarget = isBrowseItem ? linkEl || el : playBtn || linkEl || el;

    const isExplicit = !!el.querySelector(
      "ytmusic-inline-badge-renderer[aria-label*='Explicit'], [title='Explicit']"
    );
    const playState =
      (playBtn && playBtn.getAttribute("state")) || el.getAttribute("play-button-state") || "";
    const isPlaying = playState === "playing" || el.classList.contains("playing");

    return {
      id: registerAction(primaryTarget),
      play_id: playBtn ? registerAction(playBtn) : null,
      add_id: addBtn ? registerAction(addBtn) : null,
      like_id: likeBtn ? registerAction(likeBtn) : null,
      dislike_id: dislikeBtn ? registerAction(dislikeBtn) : null,
      like_status: likeStatus,
      menu_id: menuBtn ? registerAction(menuBtn) : null,
      title,
      subtitle,
      secondary,
      meta_links: metaLinks,
      duration,
      artwork_url: artwork,
      href,
      is_explicit: isExplicit,
      is_playing: isPlaying,
      is_circular: false,
      is_new_playlist: false,
      is_browse_item: isBrowseItem,
      badge: "",
    };
  }

  function extractShelvesFromContainer(root, route, insidePlayerPage = false) {
    if (!root) return [];
    const shelves = [];
    const seenNodes = new Set();
    const allowHiddenAncestor = !insidePlayerPage && isPlayerPageOpen();

    const selector =
      route === "playlist" && !insidePlayerPage
        ? "ytmusic-playlist-shelf-renderer, ytmusic-shelf-renderer"
        : "ytmusic-card-shelf-renderer, ytmusic-music-card-shelf-renderer, ytmusic-carousel-shelf-renderer, ytmusic-shelf-renderer, ytmusic-playlist-shelf-renderer, ytmusic-grid-renderer";

    const shelfEls = Array.from(root.querySelectorAll(selector));

    for (const shelf of shelfEls) {
      if (shelf.hasAttribute("hidden")) continue;
      if (!allowHiddenAncestor && isHiddenEl(shelf)) continue;
      if (!insidePlayerPage && shelf.closest("ytmusic-player-page")) continue;

      let nested = false;
      for (const prev of seenNodes) {
        if (prev.contains(shelf)) {
          nested = true;
          break;
        }
      }
      if (nested) continue;
      seenNodes.add(shelf);

      const tag = shelf.tagName.toLowerCase();
      const titleEl = shelf.querySelector(
        "ytmusic-carousel-shelf-basic-header-renderer .title, ytmusic-card-shelf-header-basic-renderer .title, .header .title, h2.title, #header .title, #title"
      );
      const subtitleEl = shelf.querySelector(
        "ytmusic-carousel-shelf-basic-header-renderer .strapline, .header .strapline"
      );
      const title = cleanText(titleEl);
      const subtitle = cleanText(subtitleEl);

      if (tag === "ytmusic-card-shelf-renderer" || tag === "ytmusic-music-card-shelf-renderer") {
        const cardTitle = cleanText(
          shelf.querySelector(".main-card-content-container .title, .title a, .title")
        );
        const cardSubEl = shelf.querySelector(".main-card-content-container .subtitle, .subtitle");
        const cardSub = cleanText(cardSubEl);
        const cardArt = getImgSrc(shelf);
        const cardLink = shelf.querySelector(".title a[href], a.yt-simple-endpoint[href]");
        const href = extractEndpointHref(shelf);
        const path = cleanHrefPath(href);
        const isBrowse = isBrowsePath(path);
        const playBtn = shelf.querySelector(
          "ytmusic-play-button-renderer, button[aria-label*='Play']"
        );
        const menuBtn = findMenuButton(shelf);

        const items = [];
        if (cardTitle) {
          items.push({
            id: registerAction(isBrowse ? cardLink || shelf : playBtn || cardLink || shelf),
            play_id: playBtn ? registerAction(playBtn) : null,
            add_id: null,
            like_id: null,
            dislike_id: null,
            like_status: "none",
            menu_id: menuBtn ? registerAction(menuBtn) : null,
            title: cardTitle,
            subtitle: cardSub,
            secondary: "",
            meta_links: extractTextLinks(cardSubEl),
            duration: "",
            artwork_url: cardArt,
            href,
            is_explicit: false,
            is_playing: false,
            is_circular: false,
            is_new_playlist: false,
            is_browse_item: isBrowse,
            badge: "TOP RESULT",
          });
        }
        const subRows = Array.from(
          shelf.querySelectorAll("ytmusic-responsive-list-item-renderer")
        ).filter(el => !el.hasAttribute("hidden"));
        for (const r of subRows.slice(0, 10)) {
          const parsed = parseListItem(r, allowHiddenAncestor);
          if (parsed) items.push(parsed);
        }
        if (items.length > 0) {
          shelves.push({
            title: title || "Top result",
            subtitle: "",
            kind: "list",
            items,
          });
        }
        continue;
      }

      const twoRows = Array.from(shelf.querySelectorAll("ytmusic-two-row-item-renderer")).filter(
        el => !el.hasAttribute("hidden") && (allowHiddenAncestor || !isHiddenEl(el))
      );
      const listRows = Array.from(
        shelf.querySelectorAll("ytmusic-responsive-list-item-renderer")
      ).filter(el => !el.hasAttribute("hidden") && (allowHiddenAncestor || !isHiddenEl(el)));

      if (route === "playlist" && !insidePlayerPage) {
        if (listRows.length > 0) {
          const items = [];
          for (const el of listRows.slice(0, 500)) {
            const item = parseListItem(el, allowHiddenAncestor);
            if (item) items.push(item);
          }
          if (items.length > 0) {
            const defaultTitle =
              tag === "ytmusic-playlist-shelf-renderer" ? "Playlist Tracks" : "Suggestions";
            shelves.push({
              title: title || defaultTitle,
              subtitle,
              kind: "list",
              items,
            });
          }
        }
        continue;
      }

      if (twoRows.length > 0) {
        const items = [];
        for (const el of twoRows.slice(0, 48)) {
          const item = parseTwoRowItem(el, allowHiddenAncestor);
          if (item) items.push(item);
        }
        if (items.length > 0) {
          shelves.push({
            title: title || (route === "library_playlists" ? "Your Playlists" : "Featured"),
            subtitle,
            kind: "grid",
            items,
          });
        }
      } else if (listRows.length > 0) {
        const items = [];
        for (const el of listRows.slice(0, 120)) {
          const item = parseListItem(el, allowHiddenAncestor);
          if (item) items.push(item);
        }
        if (items.length > 0) {
          shelves.push({
            title: title || "Tracks",
            subtitle,
            kind: "list",
            items,
          });
        }
      }

      if (shelves.length >= 18) break;
    }

    return shelves;
  }

  function extractEmptyState(root) {
    if (!root) return null;
    const msgEl = Array.from(
      root.querySelectorAll("ytmusic-message-renderer, ytmusic-empty-state-renderer")
    ).find(el => !isHiddenEl(el) && !el.closest("ytmusic-player-page"));
    if (!msgEl) return null;

    const title = cleanText(msgEl.querySelector(".text, h2, yt-formatted-string.text"));
    const subtitle = cleanText(
      msgEl.querySelector(".subtext, .subtitle, yt-formatted-string.subtext")
    );
    if (!title && !subtitle) return null;
    return {
      title: title || "Nothing here yet",
      subtitle,
    };
  }

  function detectPageReady(root, isLoading, route, shelvesCount, hasEmptyMsg) {
    if (isLoading || !root) return false;
    if (root.querySelector("ytmusic-skeleton:not([hidden])")) return false;
    if (root.querySelector("tp-yt-paper-spinner-lite[active]")) return false;

    const hasStructure = !!root.querySelector(
      "ytmusic-section-list-renderer, ytmusic-single-column-browse-results-renderer, ytmusic-two-column-browse-results-renderer, ytmusic-tabbed-search-results-renderer, ytmusic-message-renderer"
    );
    if (!hasStructure) return false;

    if (route === "search" && shelvesCount === 0 && !hasEmptyMsg) {
      if (Date.now() - lastUrlChangedAt < 2200) {
        return false;
      }
    }
    return true;
  }

  function detectHasMore(root) {
    if (!root) return false;
    const conts = Array.from(root.querySelectorAll("ytmusic-continuation-item-renderer")).filter(
      el => !isHiddenEl(el) && !el.closest("ytmusic-player-page")
    );
    for (const c of conts) {
      const data = c.data || (c.__data && c.__data.data);
      if (data && (data.continuationEndpoint || data.button)) return true;
      if (c.querySelector("tp-yt-paper-spinner-lite[active], yt-button-renderer")) return true;
    }
    return false;
  }

  function extractPopupMenu() {
    const out = [];
    const popups = Array.from(document.querySelectorAll("ytmusic-menu-popup-renderer")).filter(
      p => !isHiddenEl(p)
    );
    if (popups.length === 0) return out;
    const popup = popups[popups.length - 1];
    const items = popup.querySelectorAll(
      "ytmusic-menu-navigation-item-renderer, ytmusic-menu-service-item-renderer, ytmusic-toggle-menu-service-item-renderer"
    );
    for (const item of items) {
      if (isHiddenEl(item)) continue;
      const label =
        cleanText(item.querySelector("yt-formatted-string.text, .text, a")) || cleanText(item);
      if (!label) continue;
      const target = item.querySelector("a, tp-yt-paper-item") || item;
      out.push({
        id: registerAction(target),
        label,
      });
    }
    return out;
  }

  function extractAddToPlaylist() {
    const out = [];
    const dialog = Array.from(
      document.querySelectorAll("ytmusic-add-to-playlist-renderer")
    ).find(d => !isHiddenEl(d));
    if (!dialog) return out;

    const options = dialog.querySelectorAll("ytmusic-playlist-add-to-option-renderer");
    for (const opt of options) {
      if (isHiddenEl(opt)) continue;
      const title = cleanText(opt.querySelector("#title, yt-formatted-string.title"));
      if (!title) continue;
      const subtitle = cleanText(opt.querySelector("#byline, .subtitle"));
      const target = opt.querySelector("button, #content") || opt;
      out.push({
        id: registerAction(target),
        title,
        subtitle,
      });
    }
    return out;
  }

  /**
   * Captures YouTube Music confirmation dialogs (such as "Unsubscribe from Artist?" or
   * "Delete playlist?") so GPUI can render a native confirmation modal instead of hanging.
   */
  function extractConfirmDialog() {
    const dialogs = Array.from(document.querySelectorAll("yt-confirm-dialog-renderer")).filter(
      d => !isHiddenEl(d)
    );
    if (dialogs.length === 0) return null;
    const dlg = dialogs[dialogs.length - 1];
    const confirmBtn = dlg.querySelector("#confirm-button button, #confirm-button a, #confirm-button");
    if (!confirmBtn) return null;
    const cancelBtn = dlg.querySelector("#cancel-button button, #cancel-button a, #cancel-button");
    const title = cleanText(dlg.querySelector("#title, .title"));
    const body = cleanText(dlg.querySelector("#scroller, .line-text"));
    return {
      title: title || "Confirm action",
      body,
      confirm_id: registerAction(confirmBtn),
      confirm_label: cleanText(confirmBtn) || "Confirm",
      cancel_id: cancelBtn ? registerAction(cancelBtn) : null,
      cancel_label: (cancelBtn && cleanText(cancelBtn)) || "Cancel",
    };
  }

  /**
   * Captures YouTube Music's Share modal (`yt-unified-share-panel-renderer` / `yt-copy-link-renderer`)
   * so GPUI can render a native TIDAL-styled Share popup with one-click Copy Link and share targets.
   */
  function extractShareDialog() {
    const panels = Array.from(
      document.querySelectorAll("yt-unified-share-panel-renderer, yt-copy-link-renderer")
    ).filter(p => !isHiddenEl(p));
    if (panels.length === 0) return null;

    const panel =
      panels.find(p => p.tagName.toLowerCase() === "yt-unified-share-panel-renderer") ||
      panels[panels.length - 1];

    // Check if the enclosing dialog is actually visible/opened
    const paperDialog = panel.closest && panel.closest("tp-yt-paper-dialog");
    if (paperDialog && !paperDialog.opened && paperDialog.getAttribute("aria-hidden") === "true") {
      return null;
    }

    let url = "";
    const urlInput = panel.querySelector(
      "input#share-url, yt-copy-link-renderer input, input[readonly], input[type='text']"
    );
    if (urlInput && typeof urlInput.value === "string" && urlInput.value.trim()) {
      url = urlInput.value.trim();
    }
    if (!url) {
      const copyEl =
        panel.tagName.toLowerCase() === "yt-copy-link-renderer"
          ? panel
          : panel.querySelector("yt-copy-link-renderer");
      const copyData = copyEl && (copyEl.data || (copyEl.__data && copyEl.__data.data));
      if (copyData && typeof copyData.shortUrl === "string") {
        url = copyData.shortUrl.trim();
      }
    }
    if (!url) {
      const pData = panel.data || (panel.__data && panel.__data.data);
      const findUrl = (obj, depth = 0) => {
        if (!obj || typeof obj !== "object" || depth > 5) return "";
        if (typeof obj.shortUrl === "string" && obj.shortUrl.startsWith("http")) return obj.shortUrl;
        for (const k of Object.keys(obj)) {
          if (k === "shortUrl" || k === "copyLinkRenderer" || k === "contents" || k === "bar") {
            const v = obj[k];
            if (Array.isArray(v)) {
              for (const item of v) {
                const f = findUrl(item, depth + 1);
                if (f) return f;
              }
            } else {
              const f = findUrl(v, depth + 1);
              if (f) return f;
            }
          }
        }
        return "";
      };
      url = findUrl(pData);
    }

    if (!url) return null;

    const title = cleanText(panel.querySelector("#title, h2, yt-formatted-string#title")) || "Share";
    const copyBtn = panel.querySelector(
      "#copy-button button, #copy-button yt-button-renderer button, button[aria-label*='Copy']"
    );
    const closeBtn =
      panel.querySelector("#close-button button, button[aria-label='Close'], button[aria-label='Cancel']") ||
      (paperDialog &&
        paperDialog.querySelector("#close-button button, button[aria-label='Close'], button[aria-label='Cancel']"));

    const targets = [];
    const seenLabels = new Set();
    const targetEls = panel.querySelectorAll("yt-share-target-renderer");
    for (const t of targetEls) {
      if (isHiddenEl(t)) continue;
      const btn = t.querySelector("button, a, #target") || t;
      const label =
        cleanText(t.querySelector("#title, yt-formatted-string")) ||
        (btn.getAttribute && btn.getAttribute("aria-label")) ||
        cleanText(t);
      if (!label || seenLabels.has(label.toLowerCase())) continue;
      seenLabels.add(label.toLowerCase());

      let targetUrl = null;
      const anchor = t.querySelector("a[href]");
      if (anchor && anchor.getAttribute("href")) {
        targetUrl = anchor.getAttribute("href");
      } else {
        const td = t.data || (t.__data && t.__data.data) || t.__data;
        const nav = td && (td.navigationEndpoint || td.serviceEndpoint);
        if (nav && nav.urlEndpoint && typeof nav.urlEndpoint.url === "string") {
          targetUrl = nav.urlEndpoint.url;
        }
      }

      targets.push({
        id: registerAction(btn),
        label,
        url: targetUrl,
      });
      if (targets.length >= 12) break;
    }

    return {
      title,
      url,
      targets,
      copy_id: copyBtn ? registerAction(copyBtn) : null,
      close_id: closeBtn ? registerAction(closeBtn) : null,
    };
  }

  function detectModalOpen() {
    const dialogs = document.querySelectorAll(
      "tp-yt-paper-dialog, ytmusic-dialog, ytmusic-playlist-form"
    );
    for (const d of dialogs) {
      if (isHiddenEl(d)) continue;
      if (d.tagName.toLowerCase() === "tp-yt-paper-dialog") {
        if (d.opened) return true;
      } else {
        return true;
      }
    }
    return false;
  }

  function extractSidebarPlaylists() {
    const out = [];
    const seen = new Set();
    const skip = new Set(["home", "explore", "library", "upgrade", "samples", "new playlist", "sign in"]);
    const entries = document.querySelectorAll("ytmusic-guide-renderer ytmusic-guide-entry-renderer");
    for (const entry of entries) {
      if (isHiddenEl(entry)) continue;
      const title = cleanText(entry.querySelector(".title, yt-formatted-string.title"));
      if (!title || skip.has(title.toLowerCase()) || seen.has(title)) continue;
      seen.add(title);
      const subtitle = cleanText(entry.querySelector(".subtitle, yt-formatted-string.subtitle"));
      const clickEl = entry.querySelector("tp-yt-paper-item, a") || entry;
      const href = extractEndpointHref(entry);
      out.push({
        id: registerAction(clickEl),
        title,
        subtitle,
        href,
      });
      if (out.length >= 35) break;
    }
    return out;
  }

  function extractQueue() {
    const out = [];
    const items = document.querySelectorAll("ytmusic-player-queue #contents ytmusic-player-queue-item");
    for (const el of Array.from(items).slice(0, 50)) {
      const title = cleanText(el.querySelector(".song-title"));
      if (!title) continue;
      const bylineEl = el.querySelector(".byline");
      const artist = cleanText(bylineEl);
      const artistLinks = extractTextLinks(bylineEl);
      const duration = cleanText(el.querySelector(".duration"));
      const artwork = getImgSrc(el);
      const isPlaying =
        el.hasAttribute("selected") || el.getAttribute("play-button-state") === "playing";
      const playTarget =
        el.querySelector("ytmusic-play-button-renderer, .play-button, .left-items") || el;
      out.push({
        id: registerAction(playTarget),
        title,
        artist,
        artist_links: artistLinks,
        duration,
        artwork_url: artwork,
        is_playing: isPlaying,
      });
    }
    return out;
  }

  function extractLyrics() {
    if (!lyricsRequested) {
      return { status: "idle", text: "", source: "" };
    }
    const page = document.querySelector("ytmusic-player-page");
    if (!page) return { status: "unavailable", text: "", source: "" };

    const desc = page.querySelector("ytmusic-description-shelf-renderer .description");
    const footer = page.querySelector("ytmusic-description-shelf-renderer .footer");
    const text = desc ? (desc.innerText || desc.textContent || "").trim() : "";
    if (text.length > 0) {
      return {
        status: "available",
        text,
        source: cleanText(footer),
      };
    }

    const msg = cleanText(
      page.querySelector("ytmusic-message-renderer .text, ytmusic-message-renderer .subtext")
    );
    if (msg.toLowerCase().includes("lyrics") || msg.toLowerCase().includes("not available")) {
      return { status: "unavailable", text: msg, source: "" };
    }
    return { status: "loading", text: "", source: "" };
  }

  function extractRelated() {
    if (!relatedRequested) return [];
    const page = document.querySelector("ytmusic-player-page");
    if (!page) return [];
    const tabRenderer = page.querySelector("#tab-renderer, ytmusic-section-list-renderer");
    if (!tabRenderer) return [];
    return extractShelvesFromContainer(tabRenderer, "other", true);
  }

  function collectState() {
    registry.clear();
    nextActionId = 1;

    const url = window.location.href;
    if (url !== lastTrackedUrl) {
      lastTrackedUrl = url;
      lastUrlChangedAt = Date.now();
      const newRoute = inferRoute(url);
      if (newRoute === "playlist") {
        maybeAutoHydratePlaylist(newRoute, url);
      }
    }

    const route = inferRoute(url);
    const root = getActiveBrowseRoot(route);
    const progressEl = document.querySelector("yt-page-navigation-progress");
    const isLoading = !!(
      progressEl &&
      !isHiddenEl(progressEl) &&
      Number(progressEl.getAttribute("aria-valuenow") || 0) > 0 &&
      Number(progressEl.getAttribute("aria-valuenow") || 0) < 100
    );
    const shelves = extractShelvesFromContainer(root, route, false);
    const header = extractHeader(root);
    const emptyState = shelves.length === 0 && !header ? extractEmptyState(root) : null;

    return {
      connected: window.location.hostname === "music.youtube.com",
      url,
      page_title: document.title || "YouTube Music",
      route,
      is_loading: isLoading,
      page_ready: detectPageReady(root, isLoading, route, shelves.length, !!emptyState),
      has_more: detectHasMore(root),
      modal_open: detectModalOpen(),
      player_page_open: isPlayerPageOpen(),
      can_go_back: window.history.length > 1,
      can_go_forward: true,
      empty_state: emptyState,
      confirm_dialog: extractConfirmDialog(),
      share_dialog: extractShareDialog(),
      account: extractAccount(),
      player: extractPlayer(),
      chips: extractChips(root),
      header,
      shelves,
      sidebar_playlists: extractSidebarPlaylists(),
      queue: extractQueue(),
      lyrics: extractLyrics(),
      related_shelves: extractRelated(),
      popup_menu: extractPopupMenu(),
      add_to_playlist: extractAddToPlaylist(),
    };
  }

  function sendState(force = false) {
    try {
      if (!window.ipc || typeof window.ipc.postMessage !== "function") return;
      const state = collectState();
      const json = JSON.stringify(state);
      if (force || json !== lastPayloadJson) {
        lastPayloadJson = json;
        window.ipc.postMessage(json);
      }
    } catch (err) {
      console.debug("[ytm-mask] sync error:", err);
    }
  }

  function scheduleSync(delay = 140) {
    if (syncTimer) clearTimeout(syncTimer);
    syncTimer = setTimeout(() => sendState(false), delay);
  }

  function clickEl(el) {
    if (!el) return false;
    try {
      el.scrollIntoView({ block: "nearest", inline: "nearest" });
    } catch (_) {}
    try {
      el.click();
      return true;
    } catch (_) {
      return false;
    }
  }

  function clickGuideNav(label) {
    const entries = document.querySelectorAll("ytmusic-guide-entry-renderer");
    for (const e of entries) {
      const txt = cleanText(e.querySelector(".title, yt-formatted-string")).toLowerCase();
      if (txt === label.toLowerCase()) {
        const target = e.querySelector("tp-yt-paper-item, a") || e;
        return clickEl(target);
      }
    }
    return false;
  }

  function closePlayerOverlayIfOpen() {
    const page = document.querySelector("ytmusic-player-page");
    if (page && page.hasAttribute("player-page-open_")) {
      const toggle = document.querySelector("ytmusic-player-bar .toggle-player-page-button");
      if (toggle) clickEl(toggle);
    }
  }

  function openPlayerTab(keyword) {
    const page = document.querySelector("ytmusic-player-page");
    if (!page) return false;

    const selectTab = () => {
      const tabs = page.querySelectorAll("tp-yt-paper-tab, [role='tab']");
      for (const tab of tabs) {
        if (cleanText(tab).toLowerCase().includes(keyword.toLowerCase())) {
          clickEl(tab);
          return true;
        }
      }
      return false;
    };

    if (!page.hasAttribute("player-page-open_")) {
      const toggle = document.querySelector("ytmusic-player-bar .toggle-player-page-button");
      if (toggle) clickEl(toggle);
      setTimeout(() => {
        selectTab();
        setTimeout(() => sendState(true), 450);
        setTimeout(() => sendState(true), 1200);
      }, 220);
      return true;
    }

    selectTab();
    setTimeout(() => sendState(true), 350);
    setTimeout(() => sendState(true), 1100);
    return true;
  }

  function applyVolumeToPlayer(v, muted) {
    const moviePlayer = document.querySelector("#movie_player");
    if (moviePlayer && typeof moviePlayer.setVolume === "function") {
      if (muted && typeof moviePlayer.mute === "function") {
        moviePlayer.mute();
      } else if (!muted && v > 0 && typeof moviePlayer.unMute === "function") {
        moviePlayer.unMute();
      }
      moviePlayer.setVolume(v);
    }
    const video = document.querySelector("video.html5-main-video") || document.querySelector("video");
    if (video) {
      video.volume = v / 100;
      video.muted = !!muted;
    }
    const slider = document.querySelector("ytmusic-player-bar #volume-slider");
    if (slider) {
      slider.value = muted ? 0 : v;
      slider.dispatchEvent(new Event("value-change", { bubbles: true }));
    }
  }

  // Re-apply restored volume the first time a <video> starts loading/playing so YTM doesn't reset it to 100%
  for (const evt of ["loadedmetadata", "play"]) {
    window.addEventListener(
      evt,
      e => {
        if (
          e.target &&
          e.target.tagName === "VIDEO" &&
          typeof window.__ytmDesiredVolume === "number" &&
          !window.__ytmVolumeEnforcedOnMedia
        ) {
          window.__ytmVolumeEnforcedOnMedia = true;
          applyVolumeToPlayer(window.__ytmDesiredVolume, !!window.__ytmDesiredMuted);
        }
      },
      true
    );
  }

  function setNativeInputValue(inputEl, val) {
    if (!inputEl) return;
    const proto =
      inputEl.tagName.toLowerCase() === "textarea"
        ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
    if (setter) setter.call(inputEl, val);
    else inputEl.value = val;
    inputEl.dispatchEvent(new Event("input", { bubbles: true, composed: true }));
    inputEl.dispatchEvent(new Event("change", { bubbles: true, composed: true }));
  }

  window.__ytmMask = {
    syncNow() {
      sendState(true);
    },
    togglePlay() {
      const btn = document.querySelector("ytmusic-player-bar #play-pause-button");
      if (btn) clickEl(btn);
      else {
        const v = document.querySelector("video");
        if (v) v.paused ? v.play() : v.pause();
      }
      scheduleSync(80);
    },
    next() {
      const btn = document.querySelector("ytmusic-player-bar .next-button");
      if (btn) clickEl(btn);
      scheduleSync(150);
    },
    prev() {
      const btn = document.querySelector("ytmusic-player-bar .previous-button");
      if (btn) clickEl(btn);
      scheduleSync(150);
    },
    seek(fraction) {
      const f = Math.max(0, Math.min(1, Number(fraction) || 0));
      const video = document.querySelector("video.html5-main-video") || document.querySelector("video");
      const duration = video && Number.isFinite(video.duration) ? video.duration : 0;
      if (duration <= 0) return;
      const target = f * duration;
      const moviePlayer = document.querySelector("#movie_player");
      if (moviePlayer && typeof moviePlayer.seekTo === "function") {
        moviePlayer.seekTo(target, true);
      } else if (video) {
        video.currentTime = target;
      }
      scheduleSync(80);
    },
    setVolume(vol) {
      const v = Math.max(0, Math.min(100, Math.round(Number(vol) || 0)));
      window.__ytmDesiredVolume = v;
      window.__ytmDesiredMuted = v === 0;
      applyVolumeToPlayer(v, v === 0);
      scheduleSync(80);
    },
    toggleMute() {
      const btn = document.querySelector("ytmusic-player-bar .volume");
      if (btn) clickEl(btn);
      else {
        const video = document.querySelector("video");
        if (video) video.muted = !video.muted;
      }
      setTimeout(() => {
        const video = document.querySelector("video.html5-main-video") || document.querySelector("video");
        if (video) window.__ytmDesiredMuted = !!video.muted;
        sendState(true);
      }, 90);
    },
    toggleLike() {
      const btn = document.querySelector(
        "ytmusic-player-bar ytmusic-like-button-renderer #button-shape-like button, ytmusic-player-bar ytmusic-like-button-renderer .like button"
      );
      if (btn) clickEl(btn);
      scheduleSync(150);
    },
    toggleDislike() {
      const btn = document.querySelector(
        "ytmusic-player-bar ytmusic-like-button-renderer #button-shape-dislike button, ytmusic-player-bar ytmusic-like-button-renderer .dislike button"
      );
      if (btn) clickEl(btn);
      scheduleSync(150);
    },
    toggleShuffle() {
      const btn = document.querySelector("ytmusic-player-bar .shuffle button, ytmusic-player-bar .shuffle");
      if (btn) clickEl(btn);
      scheduleSync(150);
    },
    cycleRepeat() {
      const btn = document.querySelector("ytmusic-player-bar .repeat button, ytmusic-player-bar .repeat");
      if (btn) clickEl(btn);
      scheduleSync(150);
    },
    navigateRoute(route) {
      closePlayerOverlayIfOpen();
      const map = {
        home: ["Home", "https://music.youtube.com/"],
        explore: ["Explore", "https://music.youtube.com/explore"],
        library: ["Library", "https://music.youtube.com/library"],
        library_playlists: [null, "https://music.youtube.com/library/playlists"],
        library_songs: [null, "https://music.youtube.com/library/songs"],
        library_albums: [null, "https://music.youtube.com/library/albums"],
        library_artists: [null, "https://music.youtube.com/library/artists"],
        search: [null, "https://music.youtube.com/search"],
      };
      const entry = map[route];
      if (!entry) return;
      if (entry[0] && clickGuideNav(entry[0])) {
        scheduleSync(300);
        return;
      }
      spaNavigateEndpoint(endpointForUrl(entry[1]), entry[1]);
      scheduleSync(350);
    },
    navigateUrl(url) {
      if (!url) return;
      closePlayerOverlayIfOpen();
      const full = url.startsWith("http")
        ? url
        : "https://music.youtube.com/" + url.replace(/^\/+/, "");
      spaNavigateEndpoint(endpointForUrl(full), full);
      scheduleSync(350);
    },
    playRelease(href, videoId) {
      if (videoId && /^[A-Za-z0-9_-]{11}$/.test(videoId)) {
        spaNavigateEndpoint(
          { watchEndpoint: { videoId } },
          "https://music.youtube.com/watch?v=" + encodeURIComponent(videoId)
        );
        scheduleSync(250);
        setTimeout(() => sendState(true), 800);
        return;
      }
      if (!href) return;
      closePlayerOverlayIfOpen();
      const full = href.startsWith("http")
        ? href
        : "https://music.youtube.com/" + href.replace(/^\/+/, "");
      spaNavigateEndpoint(endpointForUrl(full), full);
      let tries = 0;
      const tryClickPlay = () => {
        tries++;
        const root = getActiveBrowseRoot(inferRoute(window.location.href));
        const btn =
          root &&
          (root.querySelector(
            "ytmusic-responsive-header-renderer ytmusic-play-button-renderer, ytmusic-detail-header-renderer ytmusic-play-button-renderer, ytmusic-responsive-header-renderer button[aria-label*='Play']"
          ) ||
            root.querySelector("ytmusic-responsive-list-item-renderer ytmusic-play-button-renderer"));
        if (btn && !isHiddenEl(btn)) {
          clickEl(btn);
          setTimeout(() => sendState(true), 350);
          return;
        }
        if (tries < 18) setTimeout(tryClickPlay, 220);
      };
      setTimeout(tryClickPlay, 450);
    },
    resolveRelease(artist, title, releaseType, autoPlay) {
      const a = (artist || "").trim();
      const t = (title || "").trim();
      const query = (a + " " + t).trim();
      if (!query) return;

      closePlayerOverlayIfOpen();
      const searchUrl = "https://music.youtube.com/search?q=" + encodeURIComponent(query);
      spaNavigateEndpoint({ searchEndpoint: { query } }, searchUrl);
      scheduleSync(300);

      const norm = s =>
        (s || "")
          .toLowerCase()
          .replace(/[’‘`]/g, "'")
          .replace(/&/g, " and ")
          .replace(/[^a-z0-9]+/g, " ")
          .trim();

      const targetTitle = norm(t);
      const targetArtist = norm(a);
      const wantType = (releaseType || "").toLowerCase();
      const isAlbumLike = wantType === "album" || wantType === "ep";
      const startedAt = Date.now();

      const clickHeroPlayLater = () => {
        let tries = 0;
        const step = () => {
          tries++;
          const root = getActiveBrowseRoot(inferRoute(window.location.href));
          const btn =
            root &&
            (root.querySelector(
              "ytmusic-responsive-header-renderer ytmusic-play-button-renderer, ytmusic-detail-header-renderer ytmusic-play-button-renderer, ytmusic-responsive-header-renderer button[aria-label*='Play']"
            ) ||
              root.querySelector("ytmusic-responsive-list-item-renderer ytmusic-play-button-renderer"));
          if (btn && !isHiddenEl(btn)) {
            clickEl(btn);
            setTimeout(() => sendState(true), 350);
            return;
          }
          if (tries < 18) setTimeout(step, 220);
        };
        setTimeout(step, 450);
      };

      const tryResolve = () => {
        const root = getActiveBrowseRoot("search");
        const candidates = [];

        if (root) {
          // 1. Top Result card shelf
          const cardShelves = Array.from(
            root.querySelectorAll("ytmusic-card-shelf-renderer, ytmusic-music-card-shelf-renderer")
          ).filter(el => !isHiddenEl(el));
          for (const cs of cardShelves) {
            const cTitle = cleanText(
              cs.querySelector(".main-card-content-container .title, .title a, .title")
            );
            const cSub = cleanText(
              cs.querySelector(".main-card-content-container .subtitle, .subtitle")
            );
            const href = cleanHrefPath(extractEndpointHref(cs));
            const playBtn = cs.querySelector(
              "ytmusic-play-button-renderer, button[aria-label*='Play']"
            );
            if (cTitle) {
              candidates.push({
                title: cTitle,
                sub: cSub,
                shelf: "top result",
                href,
                playBtn,
                el: cs,
                isTop: true,
              });
            }
          }

          // 2. Shelf rows (Albums, Singles, Songs, Top result sub-items)
          const shelves = Array.from(
            root.querySelectorAll("ytmusic-shelf-renderer, ytmusic-card-shelf-renderer")
          ).filter(el => !isHiddenEl(el));
          for (const sh of shelves) {
            const shTitle = cleanText(
              sh.querySelector(".header .title, h2.title, #header .title")
            ).toLowerCase();
            const rows = Array.from(
              sh.querySelectorAll("ytmusic-responsive-list-item-renderer")
            ).filter(el => !isHiddenEl(el));
            for (const row of rows.slice(0, 8)) {
              const rTitle = cleanText(
                row.querySelector(".title-column, .title.ytmusic-responsive-list-item-renderer, .title")
              );
              const rSub = cleanText(row.querySelector(".secondary-flex-columns, .subtitle"));
              const href = cleanHrefPath(extractEndpointHref(row));
              const playBtn = row.querySelector("ytmusic-play-button-renderer, .play-button");
              if (rTitle) {
                candidates.push({
                  title: rTitle,
                  sub: rSub,
                  shelf: shTitle,
                  href,
                  playBtn,
                  el: row,
                  isTop: false,
                });
              }
            }
          }
        }

        if (candidates.length === 0) {
          if (Date.now() - startedAt < 4500) {
            setTimeout(tryResolve, 250);
          } else {
            sendState(true);
          }
          return;
        }

        let best = null;
        let bestScore = -1;

        for (const c of candidates) {
          const nt = norm(c.title);
          const ns = norm(c.sub);
          const isBrowse = isBrowsePath(c.href);
          const subLower = (c.sub || "").toLowerCase();

          let score = 0;
          if (nt === targetTitle) score += 60;
          else if (nt.includes(targetTitle) || targetTitle.includes(nt)) score += 35;
          else continue;

          if (targetArtist && (ns.includes(targetArtist) || targetArtist.includes(ns))) {
            score += 30;
          }
          if (c.isTop) score += 10;
          if (isBrowse) score += isAlbumLike ? 25 : 15;

          if (wantType === "album" && (subLower.includes("album") || c.shelf.includes("album"))) {
            score += 20;
          } else if (wantType === "ep" && (subLower.includes("ep") || subLower.includes("album"))) {
            score += 18;
          } else if (
            wantType === "single" &&
            (subLower.includes("single") || subLower.includes("song") || c.shelf.includes("song"))
          ) {
            score += 18;
          }

          if (score > bestScore) {
            bestScore = score;
            best = c;
          }
        }

        if (!best || bestScore < 60) {
          // No confident exact match — stay cleanly on the YouTube Music search results page
          sendState(true);
          return;
        }

        if (autoPlay) {
          if (best.playBtn && !isHiddenEl(best.playBtn)) {
            clickEl(best.playBtn);
            setTimeout(() => sendState(true), 350);
            return;
          }
          if (isBrowsePath(best.href)) {
            const full = "https://music.youtube.com/" + best.href;
            spaNavigateEndpoint(endpointForUrl(full), full);
            clickHeroPlayLater();
            return;
          }
          clickEl(best.el);
          setTimeout(() => sendState(true), 400);
          return;
        }

        if (isBrowsePath(best.href)) {
          const full = "https://music.youtube.com/" + best.href;
          spaNavigateEndpoint(endpointForUrl(full), full);
          scheduleSync(300);
          setTimeout(() => sendState(true), 900);
        } else {
          // For singles that resolved to a song row, check if the row has an album link in secondary columns
          const albumLink =
            best.el && best.el.querySelector && best.el.querySelector("a[href*='browse/MPRE']");
          if (albumLink) {
            const aHref = cleanHrefPath(albumLink.getAttribute("href") || "");
            if (isBrowsePath(aHref)) {
              const full = "https://music.youtube.com/" + aHref;
              spaNavigateEndpoint(endpointForUrl(full), full);
              scheduleSync(300);
              setTimeout(() => sendState(true), 900);
              return;
            }
          }
          sendState(true);
        }
      };

      setTimeout(tryResolve, 550);
    },
    search(query) {
      const q = (query || "").trim();
      if (!q) return;
      closePlayerOverlayIfOpen();
      const full = "https://music.youtube.com/search?q=" + encodeURIComponent(q);
      spaNavigateEndpoint({ searchEndpoint: { query: q } }, full);
      scheduleSync(350);
      setTimeout(() => sendState(true), 1000);
      setTimeout(() => sendState(true), 2000);
    },
    clickItem(id) {
      const el = registry.get(id);
      if (el) {
        const prevUrl = window.location.href;
        const isActionControl = !!(
          el.closest &&
          el.closest(
            "ytmusic-play-button-renderer, ytmusic-like-button-renderer, ytmusic-menu-renderer, ytmusic-subscribe-button-renderer, yt-confirm-dialog-renderer"
          )
        );
        const container =
          (el.closest &&
            el.closest("ytmusic-two-row-item-renderer, ytmusic-responsive-list-item-renderer")) ||
          el;
        const cleanHref = isActionControl ? "" : cleanHrefPath(extractEndpointHref(container));

        // If clicking a browse item from inside the player page (e.g. Related tab), close player overlay
        if (cleanHref && isBrowsePath(cleanHref) && el.closest && el.closest("ytmusic-player-page")) {
          closePlayerOverlayIfOpen();
        }

        clickEl(el);

        if (cleanHref && isBrowsePath(cleanHref)) {
          setTimeout(() => {
            if (window.location.href === prevUrl) {
              const full = "https://music.youtube.com/" + cleanHref;
              spaNavigateEndpoint(endpointForUrl(full), full);
            }
          }, 300);
        }
        scheduleSync(200);
        setTimeout(() => sendState(true), 600);
        setTimeout(() => sendState(true), 1400);
      }
    },
    openMenu(id) {
      const el = registry.get(id);
      if (el) {
        clickEl(el);
        setTimeout(() => sendState(true), 140);
        setTimeout(() => sendState(true), 360);
      }
    },
    closeMenu() {
      const shareClose = document.querySelector(
        "yt-unified-share-panel-renderer #close-button button, yt-unified-share-panel-renderer button[aria-label='Close']"
      );
      if (shareClose && !isHiddenEl(shareClose)) clickEl(shareClose);
      const openDialogs = document.querySelectorAll("tp-yt-paper-dialog[opened], tp-yt-paper-dialog");
      for (const d of openDialogs) {
        if (d.opened && typeof d.close === "function") {
          try {
            d.close();
          } catch (_) {}
        }
      }
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
      const backdrops = document.querySelectorAll("tp-yt-iron-overlay-backdrop.opened");
      for (const b of backdrops) clickEl(b);
      setTimeout(() => sendState(true), 120);
    },
    selectMenuItem(id) {
      const el = registry.get(id);
      if (el) {
        clickEl(el);
        setTimeout(() => sendState(true), 220);
        setTimeout(() => sendState(true), 650);
      }
    },
    createPlaylist(title, description, privacy) {
      const trimmedTitle = (title || "").trim();
      if (!trimmedTitle) return;

      const candidates = Array.from(
        document.querySelectorAll(
          "button[aria-label*='New playlist'], yt-button-renderer button, ytmusic-two-row-item-renderer, ytmusic-guide-entry-renderer"
        )
      );
      for (const el of candidates) {
        if (isHiddenEl(el)) continue;
        const txt = ((el.getAttribute("aria-label") || "") + " " + cleanText(el)).toLowerCase();
        if (txt.includes("new playlist")) {
          const target = el.querySelector("button, a, tp-yt-paper-item") || el;
          clickEl(target);
          break;
        }
      }

      let attempts = 0;
      function tryFill() {
        attempts++;
        const form =
          document.querySelector("ytmusic-playlist-form") ||
          Array.from(document.querySelectorAll("tp-yt-paper-dialog")).find(
            d => !isHiddenEl(d) && d.querySelector("input, textarea")
          );
        if (!form) {
          if (attempts < 25) setTimeout(tryFill, 120);
          return;
        }

        const titleInput =
          form.querySelector("#title-input input") ||
          form.querySelector("tp-yt-paper-input input") ||
          form.querySelector("input");
        const descInput =
          form.querySelector("#description-input textarea") ||
          form.querySelector("tp-yt-paper-textarea textarea") ||
          form.querySelector("textarea");

        if (titleInput) {
          titleInput.focus();
          setNativeInputValue(titleInput, trimmedTitle);
        }
        if (descInput && description) {
          setNativeInputValue(descInput, description);
        }

        const targetPriv = (privacy || "").toLowerCase();
        if (targetPriv) {
          const dropdown = form.querySelector("ytmusic-dropdown-renderer");
          if (dropdown) {
            const trigger = dropdown.querySelector("tp-yt-paper-button, button, #dropdown-trigger");
            if (trigger) clickEl(trigger);
            setTimeout(() => {
              const items = document.querySelectorAll(
                "ytmusic-dropdown-item-renderer, tp-yt-paper-item"
              );
              for (const item of items) {
                if (cleanText(item).toLowerCase().startsWith(targetPriv)) {
                  clickEl(item);
                  break;
                }
              }
              setTimeout(() => submitCreate(form), 160);
            }, 160);
            return;
          }
        }
        setTimeout(() => submitCreate(form), 160);
      }

      function submitCreate(form) {
        const btns = form.querySelectorAll("yt-button-renderer button, button");
        for (const btn of btns) {
          const txt = ((btn.getAttribute("aria-label") || "") + " " + cleanText(btn)).toLowerCase();
          if (txt.includes("create")) {
            clickEl(btn);
            setTimeout(() => sendState(true), 600);
            setTimeout(() => sendState(true), 1600);
            return;
          }
        }
      }

      setTimeout(tryFill, 180);
    },
    goBack() {
      closePlayerOverlayIfOpen();
      window.history.back();
      scheduleSync(350);
    },
    goForward() {
      closePlayerOverlayIfOpen();
      window.history.forward();
      scheduleSync(350);
    },
    refresh() {
      window.location.reload();
    },
    scrollMore() {
      window.scrollBy({ top: 1400, behavior: "smooth" });
      const scroller = document.querySelector("ytmusic-app-layout");
      if (scroller) scroller.scrollTop += 1400;
      setTimeout(() => sendState(true), 700);
    },
    requestQueue() {
      openPlayerTab("up next");
    },
    requestLyrics() {
      lyricsRequested = true;
      openPlayerTab("lyrics");
    },
    requestRelated() {
      relatedRequested = true;
      openPlayerTab("related");
    },
    closePlayerPage() {
      closePlayerOverlayIfOpen();
      setTimeout(() => sendState(true), 200);
    },
    openSignIn() {
      const link = document.querySelector("a.sign-in-link, ytmusic-nav-bar a[href*='ServiceLogin']");
      if (link) clickEl(link);
    },
  };

  function bootObservers() {
    if (!document.body) {
      window.addEventListener("DOMContentLoaded", bootObservers, { once: true });
      return;
    }

    const observer = new MutationObserver(() => scheduleSync(200));
    observer.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["aria-pressed", "state", "selected", "hidden", "opened", "repeat-mode"],
    });

    setInterval(() => sendState(false), 500);
    window.addEventListener("popstate", () => scheduleSync(250));
    window.addEventListener("yt-navigate-finish", () => {
      scheduleSync(250);
      setTimeout(() => sendState(true), 700);
      setTimeout(() => sendState(true), 1600);
    });

    setTimeout(() => sendState(true), 600);
    setTimeout(() => sendState(true), 1800);
  }

  bootObservers();
})();

use gpui::{
    Bounds, Context, FontWeight, MouseButton, MouseDownEvent, Pixels, SharedString, SharedUri,
    Window, canvas, div, img, prelude::*, px,
};

use crate::app::{DrawerTab, ScrollTarget, YtmTidalApp, upscale_artwork_url};
use crate::protocol::{LyricsStatus, PlaybackMode, ShelfKind, WebCommand};
use crate::theme::TidalTheme;
use crate::ui::{artwork_box, render_text_links, tidal_tooltip};

#[derive(Clone)]
struct DragQueueItem {
    index: usize,
    title: String,
    artist: String,
}

struct QueueDragPreview {
    title: String,
    artist: String,
}

impl Render for QueueDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2p5()
            .px_3()
            .py_2()
            .w(px(260.0))
            .rounded(px(8.0))
            .bg(TidalTheme::bg_surface_active())
            .border_1()
            .border_color(TidalTheme::accent())
            .shadow_lg()
            .child(
                div()
                    .text_xs()
                    .text_color(TidalTheme::accent())
                    .child("⋮⋮"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(TidalTheme::text_primary())
                            .truncate()
                            .child(self.title.clone()),
                    )
                    .when(!self.artist.is_empty(), |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(TidalTheme::text_secondary())
                                .truncate()
                                .child(self.artist.clone()),
                        )
                    }),
            )
    }
}

/// Main-body Now Playing stage displayed alongside Up Next / Lyrics / Related:
/// features the Song vs Video segmented toggle, large high-res album art, and track details.
pub fn render_now_playing_stage(
    app: &YtmTidalApp,
    window: &Window,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let player = &app.state.player;
    let vp = window.viewport_size();
    let vp_h: f32 = vp.height.into();
    let vp_w: f32 = vp.width.into();
    // Account for sidebar (238) + right drawer (380) + topbar (56) + player bar (90)
    let avail_w = (vp_w - 660.0).max(260.0);
    let avail_h = (vp_h - 320.0).max(240.0);
    let art_size = avail_w.min(avail_h).clamp(240.0, 460.0);

    let mode = player.playback_mode;
    let can_video = player.can_toggle_video;
    let title = if player.title.is_empty() {
        "Nothing playing".to_string()
    } else {
        player.title.clone()
    };
    let subtitle = if !player.artist.is_empty() && !player.album.is_empty() {
        format!("{} • {}", player.artist, player.album)
    } else if !player.artist.is_empty() {
        player.artist.clone()
    } else {
        String::new()
    };
    let subtitle_links = player.subtitle_links.clone();
    let upscaled_art = player.artwork_url.as_deref().map(upscale_artwork_url);

    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .flex_1()
        .h_full()
        .bg(TidalTheme::bg_main())
        .px_8()
        .py_6()
        .gap_6()
        // Song vs Video segmented toggle pill (wired to YouTube Music's ytmusic-av-toggle)
        .child(
            div()
                .flex()
                .items_center()
                .p(px(3.0))
                .rounded(px(20.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_default())
                .child(av_mode_segment(
                    "av-toggle-song",
                    "Song",
                    mode == PlaybackMode::Song,
                    true,
                    PlaybackMode::Song,
                    cx,
                ))
                .child(av_mode_segment(
                    "av-toggle-video",
                    "Video",
                    mode == PlaybackMode::Video,
                    can_video,
                    PlaybackMode::Video,
                    cx,
                )),
        )
        // Main media area: live 16:9 embedded YouTube Video viewport when Video is active,
        // or large centered upscaled album art when Song is active.
        .child({
            if mode == PlaybackMode::Video {
                let video_w = avail_w.clamp(360.0, 720.0);
                let video_h = (video_w * 9.0 / 16.0).min(avail_h.max(220.0));
                let video_w = video_h * 16.0 / 9.0;
                let slot_ref = app.video_slot_bounds.clone();

                div()
                    .id("now-playing-video-slot")
                    .relative()
                    .w(px(video_w))
                    .h(px(video_h))
                    .rounded(px(10.0))
                    .overflow_hidden()
                    .bg(TidalTheme::bg_void())
                    .border_1()
                    .border_color(TidalTheme::border_default())
                    .shadow_lg()
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .text_color(TidalTheme::text_muted())
                            .child("Loading video…"),
                    )
                    .child(
                        canvas(
                            move |bounds: Bounds<Pixels>, window: &mut Window, _cx| {
                                let scale = window.scale_factor();
                                let x: f32 = bounds.origin.x.into();
                                let y: f32 = bounds.origin.y.into();
                                let w: f32 = bounds.size.width.into();
                                let h: f32 = bounds.size.height.into();
                                if let Ok(mut guard) = slot_ref.lock() {
                                    *guard = Some((x, y, w, h, scale));
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .size_full(),
                    )
                    .into_any_element()
            } else if let Some(src) = upscaled_art {
                let zoom_url = src.clone();
                div()
                    .id("now-playing-stage-art")
                    .cursor_pointer()
                    .w(px(art_size))
                    .h(px(art_size))
                    .rounded(px(12.0))
                    .overflow_hidden()
                    .bg(TidalTheme::bg_surface())
                    .border_1()
                    .border_color(TidalTheme::border_subtle())
                    .hover(|s| s.border_color(TidalTheme::accent()))
                    .shadow_lg()
                    .tooltip(tidal_tooltip("View full artwork", None))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_artwork_lightbox(zoom_url.clone(), false, cx);
                    }))
                    .child(
                        img(SharedUri::from(src))
                            .w(px(art_size))
                            .h(px(art_size))
                            .rounded(px(12.0))
                            .object_fit(gpui::ObjectFit::Cover),
                    )
                    .into_any_element()
            } else {
                div()
                    .w(px(art_size))
                    .h(px(art_size))
                    .rounded(px(12.0))
                    .bg(TidalTheme::bg_surface())
                    .border_1()
                    .border_color(TidalTheme::border_subtle())
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_3xl()
                    .text_color(TidalTheme::text_muted())
                    .child("♫")
                    .into_any_element()
            }
        })
        // Track title + clickable artist/album links
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1p5()
                .max_w(px(art_size.max(360.0)))
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::BOLD)
                        .text_color(TidalTheme::text_primary())
                        .truncate()
                        .child(title),
                )
                .when(!subtitle.is_empty() || !subtitle_links.is_empty(), |el| {
                    el.child(render_text_links(
                        "stage-meta-link",
                        0,
                        &subtitle_links,
                        &subtitle,
                        px(13.0),
                        cx,
                    ))
                }),
        )
}

fn av_mode_segment(
    id: &'static str,
    label: &'static str,
    active: bool,
    enabled: bool,
    mode: PlaybackMode,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let base = div()
        .id(id)
        .px_5()
        .h(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(15.0))
        .bg(if active {
            TidalTheme::text_primary()
        } else {
            TidalTheme::bg_surface()
        })
        .text_xs()
        .font_weight(FontWeight::BOLD)
        .text_color(if active {
            TidalTheme::text_inverse()
        } else if enabled {
            TidalTheme::text_secondary()
        } else {
            TidalTheme::text_muted()
        });

    if enabled {
        base.cursor_pointer()
            .hover(|s| {
                if active {
                    s.bg(TidalTheme::accent())
                } else {
                    s.bg(TidalTheme::bg_surface_hover())
                        .text_color(TidalTheme::text_primary())
                }
            })
            .on_click(cx.listener(move |this, _, _, _| {
                this.controller.send(WebCommand::SetPlaybackMode(mode));
            }))
            .child(label)
            .into_any_element()
    } else {
        base.opacity(0.45)
            .tooltip(tidal_tooltip(
                "Video version unavailable for this track",
                None,
            ))
            .child(label)
            .into_any_element()
    }
}

/// Floating bottom-right Picture-in-Picture (Miniplayer) 16:9 video slot rendered inside Riptide
/// while browsing other views with Video mode active.
pub fn render_video_pip_slot(app: &YtmTidalApp) -> impl IntoElement {
    let slot_ref = app.video_slot_bounds.clone();

    div()
        .id("video-pip-slot")
        .absolute()
        .right(px(20.0))
        .bottom(px(108.0))
        .w(px(360.0))
        .h(px(203.0))
        .rounded(px(10.0))
        .overflow_hidden()
        .bg(TidalTheme::bg_void())
        .border_1()
        .border_color(TidalTheme::border_strong())
        .shadow_lg()
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(TidalTheme::text_muted())
                .child("Loading miniplayer…"),
        )
        .child(
            canvas(
                move |bounds: Bounds<Pixels>, window: &mut Window, _cx| {
                    let scale = window.scale_factor();
                    let x: f32 = bounds.origin.x.into();
                    let y: f32 = bounds.origin.y.into();
                    let w: f32 = bounds.size.width.into();
                    let h: f32 = bounds.size.height.into();
                    if let Ok(mut guard) = slot_ref.lock() {
                        *guard = Some((x, y, w, h, scale));
                    }
                },
                |_, _, _, _| {},
            )
            .size_full(),
        )
}

pub fn render_drawer(app: &YtmTidalApp, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let tab = app.drawer_tab.unwrap_or(DrawerTab::Queue);

    div()
        .flex()
        .flex_col()
        .w(px(380.0))
        .flex_shrink_0()
        .h_full()
        .bg(TidalTheme::bg_drawer())
        .border_l_1()
        .border_color(TidalTheme::border_subtle())
        // Header tabs matching YouTube Music's Up Next / Lyrics / Related panel
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .h(px(52.0))
                .border_b_1()
                .border_color(TidalTheme::border_subtle())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(drawer_tab_btn(
                            "drawer-tab-queue",
                            "UP NEXT",
                            tab == DrawerTab::Queue,
                            DrawerTab::Queue,
                            cx,
                        ))
                        .child(drawer_tab_btn(
                            "drawer-tab-lyrics",
                            "LYRICS",
                            tab == DrawerTab::Lyrics,
                            DrawerTab::Lyrics,
                            cx,
                        ))
                        .child(drawer_tab_btn(
                            "drawer-tab-related",
                            "RELATED",
                            tab == DrawerTab::Related,
                            DrawerTab::Related,
                            cx,
                        )),
                )
                .child(
                    div()
                        .id("drawer-close-btn")
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(28.0))
                        .h(px(28.0))
                        .rounded(px(6.0))
                        .text_sm()
                        .text_color(TidalTheme::text_secondary())
                        .hover(|s| {
                            s.bg(TidalTheme::bg_surface_hover())
                                .text_color(TidalTheme::text_primary())
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_drawer(cx);
                        }))
                        .child("✕"),
                ),
        )
        // Body
        .child(match tab {
            DrawerTab::Queue => render_queue_body(app, cx).into_any_element(),
            DrawerTab::Lyrics => render_lyrics_body(app, cx).into_any_element(),
            DrawerTab::Related => render_related_body(app, cx).into_any_element(),
        })
}

fn drawer_tab_btn(
    id: &'static str,
    label: &'static str,
    active: bool,
    target: DrawerTab,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .px_2p5()
        .py_1()
        .rounded(px(5.0))
        .bg(if active {
            TidalTheme::bg_surface_active()
        } else {
            TidalTheme::bg_drawer()
        })
        .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
        .text_size(px(11.0))
        .font_weight(FontWeight::BOLD)
        .text_color(if active {
            TidalTheme::accent()
        } else {
            TidalTheme::text_secondary()
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_drawer(target, cx);
        }))
        .child(label)
}

fn render_queue_body(app: &YtmTidalApp, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let queue = app.state.queue.clone();

    div()
        .id("queue-scroll")
        .track_scroll(&app.drawer_scroll.handle)
        .flex_1()
        .overflow_y_scroll()
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_h_full()
                .p_3()
                .gap_1()
                .on_scroll_wheel(cx.listener(|this, ev, _, cx| {
                    cx.stop_propagation();
                    this.handle_smooth_scroll(ScrollTarget::Drawer, ev, cx);
                }))
        .when(queue.is_empty(), |el| {
            el.child(
                div()
                    .mt_8()
                    .p_5()
                    .rounded(px(10.0))
                    .bg(TidalTheme::bg_surface())
                    .border_1()
                    .border_color(TidalTheme::border_subtle())
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(TidalTheme::text_primary())
                            .child("Play Queue is empty"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_center()
                            .text_color(TidalTheme::text_muted())
                            .child("Start playing any track or playlist, then sync the live YouTube Music queue."),
                    )
                    .child(
                        div()
                            .id("sync-queue-btn")
                            .cursor_pointer()
                            .px_3p5()
                            .py_1p5()
                            .rounded(px(6.0))
                            .bg(TidalTheme::bg_surface_active())
                            .border_1()
                            .border_color(TidalTheme::border_default())
                            .hover(|s| s.border_color(TidalTheme::accent()))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(TidalTheme::text_primary())
                            .on_click(cx.listener(|this, _, _, _| {
                                this.controller.send(WebCommand::RequestQueue);
                            }))
                            .child("Sync Queue"),
                    ),
            )
        })
        .children(queue.into_iter().enumerate().map(|(idx, item)| {
            let menu_id = item.menu_id.clone();
            let playing = item.is_playing;
            let art_group: SharedString = format!("queue-art-{idx}").into();
            let drag_payload = DragQueueItem {
                index: idx,
                title: item.title.clone(),
                artist: item.artist.clone(),
            };

            div()
                .id(("queue-item", idx))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_2()
                .rounded(px(6.0))
                .bg(if playing {
                    TidalTheme::accent_tint()
                } else {
                    TidalTheme::bg_drawer()
                })
                .border_1()
                .border_color(if playing {
                    TidalTheme::accent()
                } else {
                    TidalTheme::bg_drawer()
                })
                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                .on_drag(drag_payload, |drag, _pos, _window, cx| {
                    cx.new(|_| QueueDragPreview {
                        title: drag.title.clone(),
                        artist: drag.artist.clone(),
                    })
                })
                .drag_over::<DragQueueItem>(move |style, drag, _window, _cx| {
                    if drag.index != idx {
                        if drag.index < idx {
                            style
                                .border_b_2()
                                .border_color(TidalTheme::accent())
                                .bg(TidalTheme::bg_surface_hover())
                        } else {
                            style
                                .border_t_2()
                                .border_color(TidalTheme::accent())
                                .bg(TidalTheme::bg_surface_hover())
                        }
                    } else {
                        style.opacity(0.45)
                    }
                })
                .on_drop(cx.listener(move |this, drag: &DragQueueItem, _, cx| {
                    this.move_queue_item(drag.index, idx, cx);
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.play_queue_item(idx, cx);
                }))
                // Subtle drag handle grip
                .child(
                    div()
                        .w(px(14.0))
                        .flex_shrink_0()
                        .text_center()
                        .text_xs()
                        .text_color(TidalTheme::text_muted())
                        .child("⋮⋮"),
                )
                // Album art with on-hover Play button
                .child(
                    div()
                        .id(("queue-art-play", idx))
                        .group(art_group.clone())
                        .relative()
                        .w(px(40.0))
                        .h(px(40.0))
                        .flex_shrink_0()
                        .cursor_pointer()
                        .tooltip(tidal_tooltip("Play track", None))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.play_queue_item(idx, cx);
                        }))
                        .child(artwork_box(item.artwork_url.as_deref(), px(40.0), false))
                        .child(
                            div()
                                .absolute()
                                .inset_0()
                                .rounded(px(6.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(TidalTheme::overlay_dark())
                                .opacity(if playing { 0.85 } else { 0.0 })
                                .group_hover(art_group, |s| s.opacity(1.0))
                                .text_sm()
                                .font_weight(FontWeight::BOLD)
                                .text_color(TidalTheme::accent())
                                .child("▶"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if playing {
                                    TidalTheme::accent()
                                } else {
                                    TidalTheme::text_primary()
                                })
                                .truncate()
                                .child(item.title),
                        )
                        .child(render_text_links(
                            "queue-link",
                            idx,
                            &item.artist_links,
                            &item.artist,
                            px(11.0),
                            cx,
                        )),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0))
                        .text_color(TidalTheme::text_muted())
                        .child(item.duration),
                )
                .when_some(menu_id, |row, mid| {
                    row.child(
                        div()
                            .id(("queue-item-menu", idx))
                            .cursor_pointer()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_full()
                            .hover(|s| {
                                s.bg(TidalTheme::bg_surface_active())
                                    .text_color(TidalTheme::accent())
                            })
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .text_color(TidalTheme::text_secondary())
                            .tooltip(tidal_tooltip("More actions", None))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.open_context_menu(mid.clone(), ev.position, cx);
                                }),
                            )
                            .child("⋯"),
                    )
                })
        })),
        )
}

fn render_lyrics_body(app: &YtmTidalApp, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let lyrics = &app.state.lyrics;
    let track_title = app.state.player.title.clone();
    let track_artist = app.state.player.artist.clone();
    let subtitle_links = app.state.player.subtitle_links.clone();

    div()
        .id("lyrics-scroll")
        .track_scroll(&app.drawer_scroll.handle)
        .flex_1()
        .overflow_y_scroll()
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_h_full()
                .p_5()
                .gap_4()
                .on_scroll_wheel(cx.listener(|this, ev, _, cx| {
                    cx.stop_propagation();
                    this.handle_smooth_scroll(ScrollTarget::Drawer, ev, cx);
                }))
        .when(!track_title.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .pb_3()
                    .border_b_1()
                    .border_color(TidalTheme::border_subtle())
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(TidalTheme::text_primary())
                            .child(track_title),
                    )
                    .child(render_text_links(
                        "lyrics-artist-link",
                        0,
                        &subtitle_links,
                        &track_artist,
                        px(12.0),
                        cx,
                    )),
            )
        })
        .child(match lyrics.status {
            LyricsStatus::Available => {
                let lines: Vec<String> = lyrics.text.lines().map(|l| l.to_string()).collect();
                let source = lyrics.source.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(lines.into_iter().map(|line| {
                        div()
                            .text_sm()
                            .line_height(px(22.0))
                            .text_color(if line.trim().is_empty() {
                                TidalTheme::text_muted()
                            } else {
                                TidalTheme::text_primary()
                            })
                            .child(if line.trim().is_empty() {
                                " ".to_string()
                            } else {
                                line
                            })
                    }))
                    .when(!source.is_empty(), |el| {
                        el.child(
                            div()
                                .mt_4()
                                .pt_3()
                                .border_t_1()
                                .border_color(TidalTheme::border_subtle())
                                .text_size(px(11.0))
                                .text_color(TidalTheme::text_muted())
                                .child(source),
                        )
                    })
                    .into_any_element()
            }
            LyricsStatus::Loading => div()
                .mt_6()
                .text_sm()
                .text_color(TidalTheme::accent_soft())
                .child("Loading lyrics from YouTube Music…")
                .into_any_element(),
            LyricsStatus::Idle | LyricsStatus::Unavailable => div()
                .mt_6()
                .p_4()
                .rounded(px(10.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_subtle())
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_xs()
                        .text_color(TidalTheme::text_secondary())
                        .child(if lyrics.text.is_empty() {
                            "Lyrics are not loaded yet for the current track.".to_string()
                        } else {
                            lyrics.text.clone()
                        }),
                )
                .child(
                    div()
                        .id("refresh-lyrics-btn")
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .h(px(30.0))
                        .rounded(px(6.0))
                        .bg(TidalTheme::bg_surface_active())
                        .hover(|s| s.border_color(TidalTheme::accent()))
                        .border_1()
                        .border_color(TidalTheme::border_default())
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(TidalTheme::text_primary())
                        .on_click(cx.listener(|this, _, _, _| {
                            this.controller.send(WebCommand::RequestLyrics);
                        }))
                        .child("Fetch Lyrics"),
                )
                .into_any_element(),
        }),
        )
}

fn render_related_body(app: &YtmTidalApp, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let shelves = app.state.related_shelves.clone();

    div()
        .id("related-scroll")
        .track_scroll(&app.drawer_scroll.handle)
        .flex_1()
        .overflow_y_scroll()
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_h_full()
                .p_4()
                .gap_5()
                .on_scroll_wheel(cx.listener(|this, ev, _, cx| {
                    cx.stop_propagation();
                    this.handle_smooth_scroll(ScrollTarget::Drawer, ev, cx);
                }))
        .when(shelves.is_empty(), |el| {
            el.child(
                div()
                    .mt_8()
                    .p_5()
                    .rounded(px(10.0))
                    .bg(TidalTheme::bg_surface())
                    .border_1()
                    .border_color(TidalTheme::border_subtle())
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(TidalTheme::text_primary())
                            .child("Related content"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_center()
                            .text_color(TidalTheme::text_muted())
                            .child("Play a track to see related songs, similar albums, and artists from YouTube Music."),
                    )
                    .child(
                        div()
                            .id("sync-related-btn")
                            .cursor_pointer()
                            .px_3p5()
                            .py_1p5()
                            .rounded(px(6.0))
                            .bg(TidalTheme::bg_surface_active())
                            .border_1()
                            .border_color(TidalTheme::border_default())
                            .hover(|s| s.border_color(TidalTheme::accent()))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(TidalTheme::text_primary())
                            .on_click(cx.listener(|this, _, _, _| {
                                this.controller.send(WebCommand::RequestRelated);
                            }))
                            .child("Load Related"),
                    ),
            )
        })
        .children(shelves.into_iter().enumerate().map(|(s_idx, shelf)| {
            let title = shelf.title;
            let is_grid = shelf.kind == ShelfKind::Grid;

            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(TidalTheme::text_secondary())
                        .child(title.to_uppercase()),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(shelf.items.into_iter().enumerate().map(|(i_idx, item)| {
                            let click_id = item.id.clone();
                            let href = item.href.clone();
                            let is_browse = item.is_browse_item || is_grid;
                            let unique = s_idx * 1000 + i_idx;

                            div()
                                .id(("rel-item", unique))
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .gap_2p5()
                                .px_2()
                                .py_1p5()
                                .rounded(px(6.0))
                                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if is_browse {
                                        if let Some(ref url) = href {
                                            this.navigate_url_from_local(url.clone(), cx);
                                            return;
                                        }
                                    }
                                    this.controller
                                        .send(WebCommand::ClickItem(click_id.clone()));
                                }))
                                .child(artwork_box(
                                    item.artwork_url.as_deref(),
                                    px(38.0),
                                    item.is_circular,
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(TidalTheme::text_primary())
                                                .truncate()
                                                .child(item.title),
                                        )
                                        .child(render_text_links(
                                            "rel-meta-link",
                                            unique,
                                            &item.meta_links,
                                            &item.subtitle,
                                            px(11.0),
                                            cx,
                                        )),
                                )
                        })),
                )
        })),
        )
}

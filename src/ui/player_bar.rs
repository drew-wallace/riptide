use std::sync::atomic::Ordering;

use gpui::{Context, FontWeight, Window, div, prelude::*, px};

use crate::app::{DrawerTab, YtmTidalApp};
use crate::protocol::{LikeStatus, RepeatMode, WebCommand, format_time};
use crate::scrub_bar::ScrubBar;
use crate::theme::TidalTheme;
use crate::ui::{artwork_box, render_text_links, tidal_tooltip};

pub fn render_player_bar(
    app: &YtmTidalApp,
    window: &mut Window,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let player = &app.state.player;
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
        "Select any track from Home, Explore, or Search".to_string()
    };
    let subtitle_links = player.subtitle_links.clone();

    let progress = player.progress();
    let duration = player.duration;
    let elapsed_str = format_time(player.current_time);
    let total_str = format_time(duration);
    let is_playing = player.is_playing;
    let is_liked = player.like_status == LikeStatus::Like;
    let is_disliked = player.like_status == LikeStatus::Dislike;
    let is_shuffled = player.is_shuffled;
    let repeat_mode = player.repeat_mode;
    let is_muted = player.is_muted;
    let volume_pct = if is_muted { 0 } else { player.volume };
    let volume_frac = (volume_pct as f32) / 100.0;

    let queue_open = app.drawer_tab == Some(DrawerTab::Queue);
    let lyrics_open = app.drawer_tab == Some(DrawerTab::Lyrics);

    let app_entity_seek = cx.entity().clone();
    let app_entity_hover = cx.entity().clone();
    let app_entity_vol = cx.entity().clone();
    let app_entity_vol_hover = cx.entity().clone();
    let dragging_progress = app.dragging_progress.clone();
    let dragging_volume = app.dragging_volume.clone();
    let is_dragging_volume = dragging_volume.load(Ordering::Relaxed);
    let hover_active = app.progress_hover_active.clone();
    let vol_hover_active = app.volume_hover_active.clone();
    let show_volume_pill = is_dragging_volume || app.volume_hover.is_some();

    // Live time-preview tooltip floating directly above the cursor when hovering over the progress bar
    let hover_preview = app.progress_hover.and_then(|(frac, cursor_x)| {
        if duration > 0.0 {
            let vp_w: f32 = window.viewport_size().width.into();
            let cx_f: f32 = cursor_x.into();
            let pill_w = 56.0_f32;
            let left = (cx_f - pill_w / 2.0).clamp(8.0, (vp_w - pill_w - 8.0).max(8.0));
            let time_label = format_time(frac as f64 * duration);
            Some((left, time_label))
        } else {
            None
        }
    });

    let repeat_tooltip = match repeat_mode {
        RepeatMode::Off => "Repeat: Off (click to repeat all)",
        RepeatMode::All => "Repeat: All (click to repeat one)",
        RepeatMode::One => "Repeat: One (click to turn off)",
    };

    let mute_tooltip = if is_muted || volume_pct == 0 {
        "Unmute audio".to_string()
    } else {
        format!("Mute audio ({volume_pct}%)")
    };

    div()
        .relative()
        .flex()
        .flex_col()
        .w_full()
        .h(px(94.0))
        .flex_shrink_0()
        .bg(TidalTheme::bg_player())
        // Live hover timestamp tooltip pill floating directly above the progress bar
        .when_some(hover_preview, |el, (left, label)| {
            el.child(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(-26.0))
                    .w(px(56.0))
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .bg(TidalTheme::bg_surface_active())
                    .border_1()
                    .border_color(TidalTheme::accent())
                    .shadow_md()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(TidalTheme::text_primary())
                    .child(label),
            )
        })
        // Signature TIDAL full-width top edge progress scrubber (flush inside bar so no border clips the thumb)
        .child(
            div().id("player-progress-wrap").w_full().h(px(14.0)).child(
                ScrubBar::new(
                    "player-progress-bar",
                    progress,
                    dragging_progress,
                    move |frac, commit, _, cx| {
                        app_entity_seek.update(cx, |this, cx| {
                            this.seek_live(frac, commit, cx);
                        });
                    },
                )
                .height(px(14.0))
                .track_height(px(3.5))
                .fill_color(TidalTheme::accent())
                .show_thumb(player.has_media)
                .on_hover(hover_active, move |hover, _, cx| {
                    app_entity_hover.update(cx, |this, cx| {
                        this.set_progress_hover(hover, cx);
                    });
                }),
            ),
        )
        // Main 3-column player bar content
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .flex_1()
                .px_5()
                .pb_1()
                // LEFT: Now Playing artwork + track info + like/dislike
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3p5()
                        .w(px(370.0))
                        .min_w_0()
                        .child(artwork_box(player.artwork_url.as_deref(), px(56.0), false))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(TidalTheme::text_primary())
                                        .truncate()
                                        .child(title),
                                )
                                .child(render_text_links(
                                    "player-link",
                                    0,
                                    &subtitle_links,
                                    &subtitle,
                                    px(12.0),
                                    cx,
                                )),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .id("player-like-btn")
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(34.0))
                                        .h(px(34.0))
                                        .rounded_full()
                                        .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                        .text_lg()
                                        .text_color(if is_liked {
                                            TidalTheme::accent()
                                        } else {
                                            TidalTheme::text_secondary()
                                        })
                                        .tooltip(tidal_tooltip(
                                            if is_liked {
                                                "Remove from Liked songs"
                                            } else {
                                                "Like song"
                                            },
                                            None,
                                        ))
                                        .on_click(cx.listener(|this, _, _, _| {
                                            this.controller.send(WebCommand::ToggleLike);
                                        }))
                                        .child(if is_liked { "♥" } else { "♡" }),
                                )
                                .child(
                                    div()
                                        .id("player-dislike-btn")
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(34.0))
                                        .h(px(34.0))
                                        .rounded_full()
                                        .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                        .text_base()
                                        .text_color(if is_disliked {
                                            TidalTheme::danger()
                                        } else {
                                            TidalTheme::text_muted()
                                        })
                                        .tooltip(tidal_tooltip(
                                            if is_disliked {
                                                "Remove dislike"
                                            } else {
                                                "Dislike song"
                                            },
                                            None,
                                        ))
                                        .on_click(cx.listener(|this, _, _, _| {
                                            this.controller.send(WebCommand::ToggleDislike);
                                        }))
                                        .child("⊘"),
                                ),
                        ),
                )
                // CENTER: Tidal Transport Controls + Timestamps
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(transport_icon_btn(
                                    "btn-shuffle",
                                    "⇄",
                                    if is_shuffled {
                                        "Shuffle: On"
                                    } else {
                                        "Shuffle queue"
                                    },
                                    None,
                                    is_shuffled,
                                    WebCommand::ToggleShuffle,
                                    cx,
                                ))
                                .child(transport_icon_btn(
                                    "btn-prev",
                                    "⏮",
                                    "Previous track",
                                    None,
                                    false,
                                    WebCommand::PrevTrack,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .id("btn-play-pause")
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(44.0))
                                        .h(px(44.0))
                                        .rounded_full()
                                        .bg(TidalTheme::text_primary())
                                        .hover(|s| s.bg(TidalTheme::accent()))
                                        .text_lg()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(TidalTheme::text_inverse())
                                        .tooltip(tidal_tooltip(
                                            if is_playing { "Pause" } else { "Play" },
                                            Some("Space"),
                                        ))
                                        .on_click(cx.listener(|this, _, _, _| {
                                            this.controller.send(WebCommand::TogglePlay);
                                        }))
                                        .child(if is_playing { "❚❚" } else { "▶" }),
                                )
                                .child(transport_icon_btn(
                                    "btn-next",
                                    "⏭",
                                    "Next track",
                                    None,
                                    false,
                                    WebCommand::NextTrack,
                                    cx,
                                ))
                                .child(transport_icon_btn(
                                    "btn-repeat",
                                    match repeat_mode {
                                        RepeatMode::One => "↻1",
                                        _ => "↻",
                                    },
                                    repeat_tooltip,
                                    None,
                                    repeat_mode != RepeatMode::Off,
                                    WebCommand::CycleRepeat,
                                    cx,
                                )),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .text_size(px(11.0))
                                .text_color(TidalTheme::text_muted())
                                .child(elapsed_str)
                                .child("/")
                                .child(total_str),
                        ),
                )
                // RIGHT: Stream Pill + Lyrics + Queue + Volume
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_3()
                        .w(px(370.0))
                        .child(
                            div()
                                .id("stream-badge")
                                .px_2()
                                .py(px(3.0))
                                .rounded(px(4.0))
                                .bg(if is_playing {
                                    TidalTheme::gold_tint()
                                } else {
                                    TidalTheme::bg_surface()
                                })
                                .border_1()
                                .border_color(if is_playing {
                                    TidalTheme::gold()
                                } else {
                                    TidalTheme::border_default()
                                })
                                .text_size(px(9.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(if is_playing {
                                    TidalTheme::gold()
                                } else {
                                    TidalTheme::text_muted()
                                })
                                .tooltip(tidal_tooltip(
                                    "Playing natively via YouTube Music WebView2 stream",
                                    None,
                                ))
                                .child("YTM STREAM"),
                        )
                        .child(drawer_toggle_btn(
                            "player-lyrics-btn",
                            "Lyrics",
                            "Show or hide track lyrics",
                            lyrics_open,
                            DrawerTab::Lyrics,
                            cx,
                        ))
                        .child(drawer_toggle_btn(
                            "player-queue-btn",
                            "Queue",
                            "Show or hide Up Next / Lyrics / Related panel",
                            queue_open,
                            DrawerTab::Queue,
                            cx,
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(
                                    div()
                                        .id("btn-mute")
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(32.0))
                                        .h(px(32.0))
                                        .rounded(px(6.0))
                                        .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                        .text_base()
                                        .text_color(if is_muted {
                                            TidalTheme::danger()
                                        } else {
                                            TidalTheme::text_secondary()
                                        })
                                        .tooltip(tidal_tooltip(mute_tooltip, None))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.toggle_mute(cx);
                                        }))
                                        .child(if is_muted || volume_pct == 0 {
                                            "🔇"
                                        } else if volume_pct < 50 {
                                            "🔉"
                                        } else {
                                            "🔊"
                                        }),
                                )
                                .child(
                                    div()
                                        .id("volume-slider-wrap")
                                        .relative()
                                        .w(px(92.0))
                                        // Live updating volume percentage pill while hovering or dragging
                                        .when(show_volume_pill, |el| {
                                            let thumb_offset = (volume_frac * 92.0 - 24.0).clamp(0.0, 44.0);
                                            el.child(
                                                div()
                                                    .absolute()
                                                    .left(px(thumb_offset))
                                                    .top(px(-28.0))
                                                    .w(px(48.0))
                                                    .h(px(22.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded(px(6.0))
                                                    .bg(TidalTheme::bg_surface_active())
                                                    .border_1()
                                                    .border_color(if is_dragging_volume {
                                                        TidalTheme::accent()
                                                    } else {
                                                        TidalTheme::border_strong()
                                                    })
                                                    .shadow_md()
                                                    .text_xs()
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(TidalTheme::text_primary())
                                                    .child(format!("{volume_pct}%")),
                                            )
                                        })
                                        .child(
                                            ScrubBar::new(
                                                "player-volume-bar",
                                                volume_frac,
                                                dragging_volume,
                                                move |frac, commit, _, cx| {
                                                    app_entity_vol.update(cx, |this, cx| {
                                                        this.set_volume_live(frac, commit, cx);
                                                    });
                                                },
                                            )
                                            .height(px(20.0))
                                            .track_height(px(4.0))
                                            .fill_color(TidalTheme::text_primary())
                                            .show_thumb(true)
                                            .on_hover(vol_hover_active, move |hover, _, cx| {
                                                app_entity_vol_hover.update(cx, |this, cx| {
                                                    this.set_volume_hover(hover, cx);
                                                });
                                            }),
                                        ),
                                ),
                        ),
                ),
        )
}

fn transport_icon_btn(
    id: &'static str,
    glyph: &'static str,
    tip: &'static str,
    shortcut: Option<&'static str>,
    active: bool,
    cmd: WebCommand,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .w(px(38.0))
        .h(px(38.0))
        .rounded_full()
        .hover(|s| {
            s.bg(TidalTheme::bg_surface_hover())
                .text_color(TidalTheme::text_primary())
        })
        .text_lg()
        .text_color(if active {
            TidalTheme::accent()
        } else {
            TidalTheme::text_secondary()
        })
        .tooltip(tidal_tooltip(tip, shortcut))
        .on_click(cx.listener(move |this, _, _, _| {
            this.controller.send(cmd.clone());
        }))
        .child(glyph)
}

fn drawer_toggle_btn(
    id: &'static str,
    label: &'static str,
    tip: &'static str,
    active: bool,
    tab: DrawerTab,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .px_2p5()
        .h(px(28.0))
        .flex()
        .items_center()
        .rounded(px(6.0))
        .bg(if active {
            TidalTheme::accent_tint()
        } else {
            TidalTheme::bg_surface()
        })
        .border_1()
        .border_color(if active {
            TidalTheme::accent()
        } else {
            TidalTheme::border_subtle()
        })
        .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(if active {
            TidalTheme::accent()
        } else {
            TidalTheme::text_secondary()
        })
        .tooltip(tidal_tooltip(tip, None))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.toggle_drawer(tab, cx);
        }))
        .child(label)
}

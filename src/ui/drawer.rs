use gpui::{Context, FontWeight, div, prelude::*, px};

use crate::app::{DrawerTab, ScrollTarget, YtmTidalApp};
use crate::protocol::{LyricsStatus, ShelfKind, WebCommand};
use crate::theme::TidalTheme;
use crate::ui::{artwork_box, render_text_links};

pub fn render_drawer(app: &YtmTidalApp, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let tab = app.drawer_tab.unwrap_or(DrawerTab::Queue);

    div()
        .flex()
        .flex_col()
        .w(px(360.0))
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
            let item_id = item.id.clone();
            let playing = item.is_playing;

            div()
                .id(("queue-item", idx))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap_3()
                .px_2p5()
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
                .on_click(cx.listener(move |this, _, _, _| {
                    this.controller.send(WebCommand::ClickItem(item_id.clone()));
                }))
                .child(artwork_box(item.artwork_url.as_deref(), px(40.0), false))
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
                        .text_size(px(11.0))
                        .text_color(TidalTheme::text_muted())
                        .child(item.duration),
                )
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

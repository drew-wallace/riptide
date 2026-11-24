use gpui::{
    Context, FontWeight, MouseButton, MouseDownEvent, SharedString, Window, div, prelude::*, px,
};

use crate::app::{ScrollTarget, YtmTidalApp};
use crate::protocol::{
    ContentShelf, HeaderActionKind, LikeStatus, MediaItem, PageHeader, Route, ShelfKind, WebCommand,
};
use crate::theme::TidalTheme;
use crate::ui::{artwork_box, explicit_badge, render_text_links, tidal_tooltip};

pub fn render_content(
    app: &YtmTidalApp,
    window: &Window,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let state = &app.state;
    let has_content = !state.shelves.is_empty() || state.header.is_some();
    let has_more = state.has_more;
    let route_label = state.route.label();
    let scroll_y = (-f32::from(app.content_scroll.offset().y)).max(0.0);
    let viewport_h = f32::from(window.viewport_size().height).max(600.0);

    // Estimate Y top of each shelf so large playlists (> 30 rows) can virtualize cleanly
    let mut est_y = 24.0_f32;
    if state.header.is_none() && (has_content || state.page_ready) {
        est_y += 60.0;
    }
    if !state.chips.is_empty() {
        est_y += 58.0;
    }
    if state.header.is_some() {
        est_y += 268.0;
    }

    let shelf_elements: Vec<gpui::AnyElement> = state
        .shelves
        .iter()
        .enumerate()
        .map(|(shelf_idx, shelf)| {
            let shelf_top_y = est_y;
            let row_h = if shelf.items.iter().any(|it| it.artwork_url.is_some()) {
                58.0
            } else {
                48.0
            };
            est_y += match shelf.kind {
                ShelfKind::List => 90.0 + shelf.items.len() as f32 * row_h,
                ShelfKind::Grid => {
                    let rows = (shelf.items.len() as f32 / 5.0).ceil().max(1.0);
                    70.0 + rows * 280.0
                }
            };
            render_shelf(shelf_idx, shelf, shelf_top_y, scroll_y, viewport_h, cx)
        })
        .collect();

    div()
        .id("main-content-scroll")
        .track_scroll(&app.content_scroll.handle)
        .flex_1()
        .h_full()
        .overflow_y_scroll()
        .bg(TidalTheme::bg_main())
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_h_full()
                .px_8()
                .py_6()
                .gap_7()
                .on_scroll_wheel(cx.listener(|this, ev, _, cx| {
                    cx.stop_propagation();
                    this.handle_smooth_scroll(ScrollTarget::Content, ev, cx);
                }))
        // Clean page title without extra eyebrow
        .when(state.header.is_none() && (has_content || state.page_ready), |el| {
            el.child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .text_color(TidalTheme::text_primary())
                    .child(route_label),
            )
        })
        // Filter Chips (Moods, Search Filters, Library Tabs)
        .when(!state.chips.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .children(state.chips.iter().enumerate().map(|(idx, chip)| {
                        let id = chip.id.clone();
                        let selected = chip.selected;
                        div()
                            .id(("filter-chip", idx))
                            .cursor_pointer()
                            .px_3p5()
                            .py_1p5()
                            .rounded(px(18.0))
                            .bg(if selected {
                                TidalTheme::text_primary()
                            } else {
                                TidalTheme::bg_surface()
                            })
                            .border_1()
                            .border_color(if selected {
                                TidalTheme::text_primary()
                            } else {
                                TidalTheme::border_default()
                            })
                            .hover(|s| {
                                if selected {
                                    s.bg(TidalTheme::accent())
                                } else {
                                    s.bg(TidalTheme::bg_surface_hover())
                                        .border_color(TidalTheme::border_strong())
                                }
                            })
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if selected {
                                TidalTheme::text_inverse()
                            } else {
                                TidalTheme::text_primary()
                            })
                            .on_click(cx.listener(move |this, _, _, _| {
                                this.controller.send(WebCommand::ClickItem(id.clone()));
                            }))
                            .child(chip.label.clone())
                    })),
            )
        })
        // Detail Hero Banner (Album / Playlist / Artist with Play, Shuffle, Radio/Mix, Subscribe, More)
        .when_some(state.header.as_ref(), |el, hdr| {
            el.child(render_detail_hero(hdr.clone(), cx))
        })
        // Loading / Connecting / Legitimate Empty Collection State
        .when(!has_content, |el| el.child(render_empty_or_loading(app, cx)))
        // Content Shelves
        .children(shelf_elements)
        // Load More Footer — shown ONLY when YouTube Music actually has a continuation page
        .when(has_content && has_more, |el| {
            el.child(
                div()
                    .flex()
                    .justify_center()
                    .pt_2()
                    .pb_8()
                    .child(
                        div()
                            .id("load-more-btn")
                            .cursor_pointer()
                            .px_6()
                            .py_2()
                            .rounded(px(20.0))
                            .bg(TidalTheme::bg_surface())
                            .border_1()
                            .border_color(TidalTheme::border_default())
                            .hover(|s| {
                                s.bg(TidalTheme::bg_surface_hover())
                                    .border_color(TidalTheme::accent())
                            })
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(TidalTheme::text_primary())
                            .on_click(cx.listener(|this, _, _, _| {
                                this.controller.send(WebCommand::ScrollMore);
                            }))
                            .child("Load More"),
                    ),
            )
        }),
        )
}

fn action_icon(label: &str) -> &'static str {
    let lower = label.to_lowercase();
    if lower.contains("shuffle") {
        "⇄"
    } else if lower.contains("radio") || lower.contains("mix") {
        "◎"
    } else if lower.contains("play") {
        "▶"
    } else if lower.contains("subscribed") || lower.contains("in library") {
        "✓"
    } else if lower.contains("subscribe") || lower.contains("save") {
        "+"
    } else {
        "•"
    }
}

fn render_detail_hero(header: PageHeader, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let actions = header.actions;
    let artwork_url_for_zoom = header.artwork_url.clone();
    let title_for_zoom = header.title.clone();
    let subtitle_for_zoom = header.subtitle.clone();
    let is_circular = header.is_circular;

    let hero_artwork = if let Some(zoom_url) = artwork_url_for_zoom {
        div()
            .id("hero-artwork-zoom")
            .cursor_pointer()
            .rounded(if is_circular { px(94.0) } else { px(8.0) })
            .border_1()
            .border_color(gpui::transparent_black())
            .hover(|s| s.border_color(TidalTheme::accent()))
            .tooltip(tidal_tooltip("View full artwork", None))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_artwork_lightbox(
                    zoom_url.clone(),
                    title_for_zoom.clone(),
                    subtitle_for_zoom.clone(),
                    is_circular,
                    cx,
                );
            }))
            .child(artwork_box(
                header.artwork_url.as_deref(),
                px(188.0),
                is_circular,
            ))
            .into_any_element()
    } else {
        artwork_box(None, px(188.0), is_circular)
    };

    div()
        .flex()
        .items_center()
        .gap_7()
        .p_6()
        .rounded(px(12.0))
        .bg(TidalTheme::bg_surface())
        .border_1()
        .border_color(TidalTheme::border_subtle())
        .child(hero_artwork)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_2p5()
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(TidalTheme::accent())
                        .child(header.kind),
                )
                .child(
                    div()
                        .text_3xl()
                        .font_weight(FontWeight::BOLD)
                        .text_color(TidalTheme::text_primary())
                        .child(header.title),
                )
                .when(!header.subtitle.is_empty() || !header.subtitle_links.is_empty(), |el| {
                    el.child(render_text_links(
                        "hero-sub-link",
                        0,
                        &header.subtitle_links,
                        &header.subtitle,
                        px(14.0),
                        cx,
                    ))
                })
                .when(!header.description.is_empty(), |el| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(TidalTheme::text_muted())
                            .max_w(px(680.0))
                            .child(header.description),
                    )
                })
                .when(!actions.is_empty(), |el| {
                    el.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2p5()
                            .mt_2()
                            .children(actions.into_iter().enumerate().map(|(idx, act)| {
                                let act_id = act.id.clone();
                                match act.kind {
                                    HeaderActionKind::Primary => div()
                                        .id(("hero-act", idx))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .px_6()
                                        .h(px(40.0))
                                        .rounded(px(20.0))
                                        .bg(TidalTheme::text_primary())
                                        .hover(|s| s.bg(TidalTheme::accent()))
                                        .text_sm()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(TidalTheme::text_inverse())
                                        .on_click(cx.listener(move |this, _, _, _| {
                                            this.controller
                                                .send(WebCommand::ClickItem(act_id.clone()));
                                        }))
                                        .child(div().text_base().child(action_icon(&act.label)))
                                        .child(act.label)
                                        .into_any_element(),
                                    HeaderActionKind::Secondary => div()
                                        .id(("hero-act", idx))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .px_5()
                                        .h(px(40.0))
                                        .rounded(px(20.0))
                                        .bg(TidalTheme::bg_surface_active())
                                        .border_1()
                                        .border_color(TidalTheme::border_default())
                                        .hover(|s| {
                                            s.bg(TidalTheme::bg_surface_hover())
                                                .border_color(TidalTheme::accent())
                                        })
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(TidalTheme::text_primary())
                                        .on_click(cx.listener(move |this, _, _, _| {
                                            this.controller
                                                .send(WebCommand::ClickItem(act_id.clone()));
                                        }))
                                        .child(div().text_base().child(action_icon(&act.label)))
                                        .child(act.label)
                                        .into_any_element(),
                                    HeaderActionKind::Toggle => div()
                                        .id(("hero-act", idx))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .px_5()
                                        .h(px(40.0))
                                        .rounded(px(20.0))
                                        .bg(if act.is_active {
                                            TidalTheme::accent_tint()
                                        } else {
                                            TidalTheme::bg_surface_active()
                                        })
                                        .border_1()
                                        .border_color(if act.is_active {
                                            TidalTheme::accent()
                                        } else {
                                            TidalTheme::border_default()
                                        })
                                        .hover(|s| {
                                            s.bg(TidalTheme::bg_surface_hover())
                                                .border_color(TidalTheme::accent())
                                        })
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(if act.is_active {
                                            TidalTheme::accent()
                                        } else {
                                            TidalTheme::text_primary()
                                        })
                                        .on_click(cx.listener(move |this, _, _, _| {
                                            this.controller
                                                .send(WebCommand::ClickItem(act_id.clone()));
                                        }))
                                        .child(div().text_base().child(action_icon(&act.label)))
                                        .child(act.label)
                                        .into_any_element(),
                                    HeaderActionKind::Menu => div()
                                        .id(("hero-act", idx))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(40.0))
                                        .h(px(40.0))
                                        .rounded_full()
                                        .bg(TidalTheme::bg_surface_active())
                                        .border_1()
                                        .border_color(TidalTheme::border_default())
                                        .hover(|s| {
                                            s.bg(TidalTheme::bg_surface_hover())
                                                .border_color(TidalTheme::accent())
                                        })
                                        .text_lg()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(TidalTheme::text_primary())
                                        .tooltip(tidal_tooltip("More actions", None))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                move |this, ev: &MouseDownEvent, _, cx| {
                                                    cx.stop_propagation();
                                                    this.open_context_menu(
                                                        act_id.clone(),
                                                        ev.position,
                                                        cx,
                                                    );
                                                },
                                            ),
                                        )
                                        .child("⋯")
                                        .into_any_element(),
                                }
                            })),
                    )
                }),
        )
}

fn render_shelf(
    shelf_idx: usize,
    shelf: &ContentShelf,
    shelf_top_y: f32,
    scroll_y: f32,
    viewport_h: f32,
    cx: &mut Context<YtmTidalApp>,
) -> gpui::AnyElement {
    let title = shelf.title.clone();
    let subtitle = shelf.subtitle.clone();
    let kind = shelf.kind;

    div()
        .flex()
        .flex_col()
        .gap_3p5()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .when(!subtitle.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(TidalTheme::text_muted())
                            .child(subtitle.to_uppercase()),
                    )
                })
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::BOLD)
                        .text_color(TidalTheme::text_primary())
                        .child(title),
                ),
        )
        .child(match kind {
            ShelfKind::Grid => render_card_grid(shelf_idx, &shelf.items, cx).into_any_element(),
            ShelfKind::List => render_track_table(
                shelf_idx,
                &shelf.items,
                shelf_top_y + 72.0,
                scroll_y,
                viewport_h,
                cx,
            )
            .into_any_element(),
        })
        .into_any_element()
}

fn render_card_grid(
    shelf_idx: usize,
    items: &[MediaItem],
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .gap_5()
        .children(items.iter().enumerate().map(|(item_idx, item)| {
            let click_id = item.id.clone();
            let play_id = item.play_id.clone();
            let menu_id = item.menu_id.clone();
            let is_circular = item.is_circular;
            let is_new_playlist = item.is_new_playlist;
            let unique = shelf_idx * 1000 + item_idx;
            let card_group: SharedString = format!("card-{unique}").into();

            div()
                .id(("grid-card", unique))
                .group(card_group.clone())
                .cursor_pointer()
                .flex()
                .flex_col()
                .w(px(184.0))
                .p_2p5()
                .rounded(px(10.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_subtle())
                .hover(|s| {
                    s.bg(TidalTheme::bg_surface_hover())
                        .border_color(if is_new_playlist {
                            TidalTheme::accent()
                        } else {
                            TidalTheme::border_strong()
                        })
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if is_new_playlist {
                        this.open_new_playlist_modal(window, cx);
                    } else {
                        this.controller.send(WebCommand::ClickItem(click_id.clone()));
                    }
                }))
                // Cover art + on-hover Play & Action Menu buttons
                .child(if is_new_playlist {
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(162.0))
                        .h(px(162.0))
                        .mb_2p5()
                        .rounded(px(6.0))
                        .bg(TidalTheme::bg_surface_active())
                        .border_1()
                        .border_color(TidalTheme::border_default())
                        .text_3xl()
                        .font_weight(FontWeight::LIGHT)
                        .text_color(TidalTheme::accent())
                        .child("+")
                        .into_any_element()
                } else {
                    let group_for_play = card_group.clone();
                    let group_for_menu = card_group.clone();
                    div()
                        .relative()
                        .w(px(162.0))
                        .h(px(162.0))
                        .mb_2p5()
                        .child(artwork_box(
                            item.artwork_url.as_deref(),
                            px(162.0),
                            is_circular,
                        ))
                        // Top-right hover Action Menu (`⋯`) for albums/playlists/artists
                        .when_some(menu_id, |el, mid| {
                            el.child(
                                div()
                                    .id(("card-menu", unique))
                                    .cursor_pointer()
                                    .absolute()
                                    .right(px(8.0))
                                    .top(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(34.0))
                                    .h(px(34.0))
                                    .rounded_full()
                                    .opacity(0.0)
                                    .group_hover(group_for_menu, |s| s.opacity(1.0))
                                    .bg(TidalTheme::overlay_dark())
                                    .border_1()
                                    .border_color(TidalTheme::border_strong())
                                    .hover(|s| {
                                        s.bg(TidalTheme::bg_surface_active())
                                            .border_color(TidalTheme::accent())
                                            .text_color(TidalTheme::accent())
                                    })
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(TidalTheme::text_primary())
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
                        // Bottom-right hover Play (`▶`) button
                        .when_some(play_id, |el, pid| {
                            el.child(
                                div()
                                    .id(("card-play", unique))
                                    .cursor_pointer()
                                    .absolute()
                                    .right(px(8.0))
                                    .bottom(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .w(px(38.0))
                                    .h(px(38.0))
                                    .rounded_full()
                                    .opacity(0.0)
                                    .group_hover(group_for_play, |s| s.opacity(1.0))
                                    .bg(TidalTheme::overlay_dark())
                                    .border_1()
                                    .border_color(TidalTheme::border_strong())
                                    .hover(|s| {
                                        s.bg(TidalTheme::accent())
                                            .border_color(TidalTheme::accent())
                                            .text_color(TidalTheme::text_inverse())
                                    })
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(TidalTheme::text_primary())
                                    .tooltip(tidal_tooltip("Play immediately", None))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_click(cx.listener(move |this, _, _, _| {
                                        this.controller.send(WebCommand::ClickItem(pid.clone()));
                                    }))
                                    .child("▶"),
                            )
                        })
                        .into_any_element()
                })
                // Title row (underline on hover when card links to an album/artist/playlist)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .border_b_1()
                                .border_color(gpui::transparent_black())
                                .when(item.is_browse_item, |el| {
                                    el.hover(|s| {
                                        s.text_color(TidalTheme::accent())
                                            .border_color(TidalTheme::accent())
                                            .underline()
                                    })
                                })
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(TidalTheme::text_primary())
                                .truncate()
                                .child(item.title.clone()),
                        )
                        .when(item.is_explicit, |el| el.child(explicit_badge())),
                )
                // Subtitle (with clickable artist links where available)
                .when(!item.subtitle.is_empty(), |el| {
                    el.child(div().mt_0p5().child(render_text_links(
                        "card-sub-link",
                        unique,
                        &item.meta_links,
                        &item.subtitle,
                        px(12.0),
                        cx,
                    )))
                })
        }))
}

fn render_track_table(
    shelf_idx: usize,
    items: &[MediaItem],
    table_top_y: f32,
    scroll_y: f32,
    viewport_h: f32,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let show_artwork = items.iter().any(|it| it.artwork_url.is_some());
    let row_h = if show_artwork { 58.0_f32 } else { 48.0_f32 };
    let total = items.len();

    // Virtualize long playlists / track lists (> 30 rows) so scrolling stays 120 FPS
    let (start_row, end_row, top_spacer_h, bottom_spacer_h) = if total > 30 {
        let overscan = 460.0_f32;
        let rel_top = (scroll_y - table_top_y - overscan).max(0.0);
        let rel_bottom = (scroll_y + viewport_h - table_top_y + overscan).max(0.0);
        let s = ((rel_top / row_h).floor() as usize).min(total);
        let e = ((rel_bottom / row_h).ceil() as usize).min(total);
        let top_h = s as f32 * row_h;
        let bot_h = (total.saturating_sub(e)) as f32 * row_h;
        (s, e, top_h, bot_h)
    } else {
        (0, total, 0.0, 0.0)
    };

    let has_add_actions = items.iter().any(|it| it.add_id.is_some());
    let has_like_actions = items
        .iter()
        .any(|it| it.like_id.is_some() || it.dislike_id.is_some());
    let has_menu_actions = items.iter().any(|it| it.menu_id.is_some());
    let is_plays_col = !items.is_empty()
        && items.iter().any(|it| it.subtitle.to_lowercase().contains("play"))
        && items.iter().all(|it| {
            it.meta_links.iter().all(|l| l.href.is_none())
                && (it.subtitle.is_empty() || it.subtitle.to_lowercase().contains("play"))
        });

    div()
        .flex()
        .flex_col()
        .rounded(px(10.0))
        .bg(TidalTheme::bg_surface())
        .border_1()
        .border_color(TidalTheme::border_subtle())
        .overflow_hidden()
        // Table column header
        .child(
            div()
                .flex()
                .items_center()
                .px_4()
                .h(px(36.0))
                .border_b_1()
                .border_color(TidalTheme::border_subtle())
                .text_size(px(10.0))
                .font_weight(FontWeight::BOLD)
                .text_color(TidalTheme::text_muted())
                .child(div().w(px(34.0)).child("#"))
                .child(div().flex_1().min_w_0().child("TITLE"))
                .child(div().w(px(240.0)).child(if is_plays_col {
                    "PLAYS"
                } else {
                    "ARTIST / ALBUM"
                }))
                .when(has_add_actions, |el| {
                    el.child(div().w(px(76.0)).text_center().child("ADD"))
                })
                .when(has_like_actions || has_menu_actions, |el| {
                    el.child(div().w(px(112.0)).text_center().child("ACTIONS"))
                })
                .child(div().w(px(58.0)).text_right().child("TIME")),
        )
        .when(top_spacer_h > 0.0, |el| {
            el.child(div().h(px(top_spacer_h)).flex_shrink_0())
        })
        // Rows (virtualized slice for large lists)
        .children((start_row..end_row).map(|row_idx| {
            let item = &items[row_idx];
            let click_id = item.id.clone();
            let play_id = item.play_id.clone();
            let add_id = item.add_id.clone();
            let like_id = item.like_id.clone();
            let dislike_id = item.dislike_id.clone();
            let menu_id = item.menu_id.clone();
            let is_liked = item.like_status == LikeStatus::Like;
            let is_disliked = item.like_status == LikeStatus::Dislike;
            let playing = item.is_playing;
            let is_browse = item.is_browse_item;
            let unique = shelf_idx * 10000 + row_idx;
            let row_group: SharedString = format!("row-{unique}").into();
            let thumb_group: SharedString = format!("row-art-{unique}").into();

            let meta = if !item.secondary.is_empty()
                && !item.secondary.eq_ignore_ascii_case(&item.subtitle)
            {
                format!("{} • {}", item.subtitle, item.secondary)
            } else {
                item.subtitle.clone()
            };

            div()
                .id(("track-row", unique))
                .group(row_group.clone())
                .cursor_pointer()
                .flex()
                .items_center()
                .px_4()
                .h(if show_artwork { px(58.0) } else { px(48.0) })
                .border_b_1()
                .border_color(TidalTheme::border_subtle())
                .bg(if playing {
                    TidalTheme::accent_tint()
                } else {
                    TidalTheme::bg_surface()
                })
                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                // Clicking the ROW opens Album/Artist/Playlist page if browse item, or plays if song
                .on_click(cx.listener(move |this, _, _, _| {
                    this.controller.send(WebCommand::ClickItem(click_id.clone()));
                }))
                // Index or active playing glyph (with hover play glyph when album rows have no artwork)
                .child(
                    div()
                        .relative()
                        .w(px(34.0))
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(if playing {
                                    FontWeight::BOLD
                                } else {
                                    FontWeight::MEDIUM
                                })
                                .text_color(if playing {
                                    TidalTheme::accent()
                                } else {
                                    TidalTheme::text_muted()
                                })
                                .when(!show_artwork && !playing, |el| {
                                    el.group_hover(row_group.clone(), |s| s.opacity(0.0))
                                })
                                .child(if playing {
                                    "▶".to_string()
                                } else {
                                    format!("{}", row_idx + 1)
                                }),
                        )
                        .when(!show_artwork && !playing, |el| {
                            el.child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(TidalTheme::accent())
                                    .opacity(0.0)
                                    .group_hover(row_group, |s| s.opacity(1.0))
                                    .child("▶"),
                            )
                        }),
                )
                // Optional Artwork (omitted completely on Album tracklists where YTM shows none) + Title
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .flex_1()
                        .min_w_0()
                        .pr_4()
                        .when(show_artwork, |el| {
                            el.child(
                                div()
                                    .id(("row-art", unique))
                                    .group(thumb_group.clone())
                                    .relative()
                                    .w(px(42.0))
                                    .h(px(42.0))
                                    .flex_shrink_0()
                                    .child(artwork_box(
                                        item.artwork_url.as_deref(),
                                        px(42.0),
                                        false,
                                    ))
                                    .when_some(play_id, |el, pid| {
                                        el.tooltip(tidal_tooltip(
                                            if is_browse {
                                                "Play album / playlist"
                                            } else {
                                                "Play track"
                                            },
                                            None,
                                        ))
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(move |this, _, _, _| {
                                            this.controller
                                                .send(WebCommand::ClickItem(pid.clone()));
                                        }))
                                        .child(
                                            div()
                                                .absolute()
                                                .inset_0()
                                                .rounded(px(6.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .bg(TidalTheme::overlay_dark())
                                                .opacity(0.0)
                                                .group_hover(thumb_group, |s| s.opacity(1.0))
                                                .text_base()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(TidalTheme::accent())
                                                .child("▶"),
                                        )
                                    }),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .border_b_1()
                                        .border_color(gpui::transparent_black())
                                        .when(is_browse, |el| {
                                            el.hover(|s| {
                                                s.text_color(TidalTheme::accent())
                                                    .border_color(TidalTheme::accent())
                                                    .underline()
                                            })
                                        })
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(if playing {
                                            TidalTheme::accent()
                                        } else {
                                            TidalTheme::text_primary()
                                        })
                                        .truncate()
                                        .child(item.title.clone()),
                                )
                                .when(item.is_explicit, |el| el.child(explicit_badge()))
                                .when(!item.badge.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .px_1p5()
                                            .py(px(1.0))
                                            .rounded(px(3.0))
                                            .bg(TidalTheme::gold_tint())
                                            .text_size(px(9.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(TidalTheme::gold())
                                            .child(item.badge.clone()),
                                    )
                                }),
                        ),
                )
                // Artist / Album (individually clickable links)
                .child(div().w(px(240.0)).pr_4().overflow_hidden().child(
                    render_text_links(
                        "row-meta-link",
                        unique,
                        &item.meta_links,
                        &meta,
                        px(12.0),
                        cx,
                    ),
                ))
                // Optional "+ Add" action for suggested playlist songs
                .when(has_add_actions, |el| {
                    el.child(
                        div()
                            .w(px(76.0))
                            .flex()
                            .justify_center()
                            .when_some(add_id, |slot, aid| {
                                slot.child(
                                    div()
                                        .id(("track-add-btn", unique))
                                        .cursor_pointer()
                                        .px_2p5()
                                        .py_1()
                                        .rounded(px(14.0))
                                        .bg(TidalTheme::bg_surface_active())
                                        .border_1()
                                        .border_color(TidalTheme::border_default())
                                        .hover(|s| {
                                            s.bg(TidalTheme::accent())
                                                .border_color(TidalTheme::accent())
                                                .text_color(TidalTheme::text_inverse())
                                        })
                                        .text_size(px(11.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(TidalTheme::text_primary())
                                        .tooltip(tidal_tooltip("Add track to this playlist", None))
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(move |this, _, _, _| {
                                            this.controller
                                                .send(WebCommand::ClickItem(aid.clone()));
                                        }))
                                        .child("+ Add"),
                                )
                            }),
                    )
                })
                // Song Like, Dislike, and Action Menu (`⋯`) buttons
                .when(has_like_actions || has_menu_actions, |el| {
                    el.child(
                        div()
                            .w(px(112.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_1()
                            .when_some(like_id, |slot, lid| {
                                slot.child(
                                    div()
                                        .id(("row-like", unique))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(30.0))
                                        .h(px(30.0))
                                        .rounded_full()
                                        .hover(|s| s.bg(TidalTheme::bg_surface_active()))
                                        .text_base()
                                        .text_color(if is_liked {
                                            TidalTheme::accent()
                                        } else {
                                            TidalTheme::text_secondary()
                                        })
                                        .tooltip(tidal_tooltip(
                                            if is_liked { "Remove like" } else { "Like song" },
                                            None,
                                        ))
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(move |this, _, _, _| {
                                            this.controller
                                                .send(WebCommand::ClickItem(lid.clone()));
                                        }))
                                        .child(if is_liked { "♥" } else { "♡" }),
                                )
                            })
                            .when_some(dislike_id, |slot, did| {
                                slot.child(
                                    div()
                                        .id(("row-dislike", unique))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(30.0))
                                        .h(px(30.0))
                                        .rounded_full()
                                        .hover(|s| s.bg(TidalTheme::bg_surface_active()))
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
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(move |this, _, _, _| {
                                            this.controller
                                                .send(WebCommand::ClickItem(did.clone()));
                                        }))
                                        .child("⊘"),
                                )
                            })
                            .when_some(menu_id, |slot, mid| {
                                slot.child(
                                    div()
                                        .id(("row-menu", unique))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .w(px(30.0))
                                        .h(px(30.0))
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
                                            cx.listener(
                                                move |this, ev: &MouseDownEvent, _, cx| {
                                                    cx.stop_propagation();
                                                    this.open_context_menu(
                                                        mid.clone(),
                                                        ev.position,
                                                        cx,
                                                    );
                                                },
                                            ),
                                        )
                                        .child("⋯"),
                                )
                            }),
                    )
                })
                // Duration
                .child(
                    div()
                        .w(px(58.0))
                        .text_right()
                        .text_xs()
                        .text_color(TidalTheme::text_muted())
                        .child(if item.duration.is_empty() {
                            "—".to_string()
                        } else {
                            item.duration.clone()
                        }),
                )
        }))
        .when(bottom_spacer_h > 0.0, |el| {
            el.child(div().h(px(bottom_spacer_h)).flex_shrink_0())
        })
}

fn render_empty_or_loading(app: &YtmTidalApp, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let state = &app.state;
    let connected = state.connected;
    let is_empty_collection =
        state.empty_state.is_some() || (connected && state.page_ready && !state.is_loading);

    let (badge, heading, body) = if is_empty_collection {
        if let Some(ref empty) = state.empty_state {
            (
                "EMPTY COLLECTION",
                empty.title.clone(),
                if empty.subtitle.is_empty() {
                    state.route.empty_fallback().1.to_string()
                } else {
                    empty.subtitle.clone()
                },
            )
        } else {
            let (fallback_title, fallback_sub) = state.route.empty_fallback();
            (
                "EMPTY COLLECTION",
                fallback_title.to_string(),
                fallback_sub.to_string(),
            )
        }
    } else if connected {
        (
            "LOADING",
            format!("Loading {}…", state.route.label()),
            if app.status_message.is_empty() {
                "Fetching page data from YouTube Music…".to_string()
            } else {
                app.status_message.clone()
            },
        )
    } else {
        (
            "INITIALIZING",
            "Connecting to YouTube Music".to_string(),
            if app.status_message.is_empty() {
                "Starting the embedded WebView2 session for music.youtube.com. If Google prompts for sign-in or cookie consent, click Show WebView below.".to_string()
            } else {
                app.status_message.clone()
            },
        )
    };

    div()
        .my_12()
        .mx_auto()
        .max_w(px(520.0))
        .p_8()
        .rounded(px(14.0))
        .bg(TidalTheme::bg_surface())
        .border_1()
        .border_color(TidalTheme::border_default())
        .flex()
        .flex_col()
        .items_center()
        .gap_4()
        .child(
            div()
                .px_3()
                .py_1()
                .rounded(px(12.0))
                .bg(if is_empty_collection {
                    TidalTheme::bg_surface_active()
                } else {
                    TidalTheme::accent_tint()
                })
                .text_size(px(10.0))
                .font_weight(FontWeight::BOLD)
                .text_color(if is_empty_collection {
                    TidalTheme::text_secondary()
                } else {
                    TidalTheme::accent()
                })
                .child(badge),
        )
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::BOLD)
                .text_color(TidalTheme::text_primary())
                .child(heading),
        )
        .child(
            div()
                .text_xs()
                .text_center()
                .text_color(TidalTheme::text_secondary())
                .child(body),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .mt_2()
                .when(
                    is_empty_collection && state.route == Route::LibraryPlaylists,
                    |row| {
                        row.child(
                            div()
                                .id("empty-create-playlist")
                                .cursor_pointer()
                                .px_4()
                                .py_2()
                                .rounded(px(8.0))
                                .bg(TidalTheme::text_primary())
                                .hover(|s| s.bg(TidalTheme::accent()))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(TidalTheme::text_inverse())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_new_playlist_modal(window, cx);
                                }))
                                .child("+ New Playlist"),
                        )
                    },
                )
                .when(!is_empty_collection, |row| {
                    row.child(
                        div()
                            .id("empty-show-webview")
                            .cursor_pointer()
                            .px_4()
                            .py_2()
                            .rounded(px(8.0))
                            .bg(TidalTheme::text_primary())
                            .hover(|s| s.bg(TidalTheme::accent()))
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(TidalTheme::text_inverse())
                            .on_click(cx.listener(|this, _, _, _| {
                                this.controller.send(WebCommand::SetWebViewVisible(true));
                            }))
                            .child("Show WebView"),
                    )
                })
                .child(
                    div()
                        .id("empty-go-home")
                        .cursor_pointer()
                        .px_4()
                        .py_2()
                        .rounded(px(8.0))
                        .bg(TidalTheme::bg_surface_active())
                        .border_1()
                        .border_color(TidalTheme::border_default())
                        .hover(|s| s.border_color(TidalTheme::accent()))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(TidalTheme::text_primary())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.navigate(Route::Home, cx);
                        }))
                        .child("Explore Home"),
                )
                .child(
                    div()
                        .id("empty-reload")
                        .cursor_pointer()
                        .px_4()
                        .py_2()
                        .rounded(px(8.0))
                        .bg(TidalTheme::bg_surface_active())
                        .border_1()
                        .border_color(TidalTheme::border_default())
                        .hover(|s| s.border_color(TidalTheme::accent()))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(TidalTheme::text_primary())
                        .on_click(cx.listener(|this, _, _, _| {
                            this.controller.send(WebCommand::Refresh);
                        }))
                        .child("Reload"),
                ),
        )
}

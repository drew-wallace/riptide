use gpui::{
    Context, FontWeight, MouseButton, MouseDownEvent, SharedString, Window, div, prelude::*, px,
};

use crate::app::{
    FEED_HEADER_HEIGHT, FEED_ROW_HEIGHT, FeedDisplayRow, FeedFilter, ScrollTarget, YtmTidalApp,
};
use crate::theme::TidalTheme;
use crate::time_util::{format_relative_age, format_short_date, now_unix_secs};
use crate::ui::{artwork_box, tidal_tooltip};

pub fn render_feed_view(
    app: &YtmTidalApp,
    window: &Window,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let filter = app.feed_filter;
    let is_loading = app.feed_loading;
    let status = if !app.feed_status.is_empty() {
        app.feed_status.clone()
    } else if let Some(ts) = app.feed_last_refreshed_unix {
        format!(
            "Updated {} • {} artists",
            format_relative_age(Some(ts), now_unix_secs()),
            app.feed_scanned
        )
    } else {
        String::new()
    };
    let source_label = app.settings.feed_source.label();
    let is_signed_in = app.state.account.is_signed_in;
    let total_rows = app.feed_rows.len();

    // O(log N) viewport windowing: render ONLY the ~16–20 rows near the visible viewport
    let scroll_y = (-f32::from(app.feed_scroll.offset().y)).max(0.0);
    let viewport_h = f32::from(window.viewport_size().height).max(600.0);
    let header_chrome_h = if is_loading { 260.0 } else { 150.0 };
    let list_scroll_y = (scroll_y - header_chrome_h).max(0.0);
    let overscan = 320.0_f32;
    let vis_top = (list_scroll_y - overscan).max(0.0);
    let vis_bottom = list_scroll_y + viewport_h + overscan;

    let start_idx = if total_rows == 0 {
        0
    } else {
        let p = app.feed_row_offsets.partition_point(|&y| y < vis_top);
        p.saturating_sub(1)
    };
    let end_idx = if total_rows == 0 {
        0
    } else {
        app.feed_row_offsets
            .partition_point(|&y| y < vis_bottom)
            .min(total_rows)
    };

    let top_spacer_h = app.feed_row_offsets.get(start_idx).copied().unwrap_or(0.0);
    let rendered_end_y = if end_idx < total_rows {
        app.feed_row_offsets[end_idx]
    } else {
        app.feed_total_height
    };
    let bottom_spacer_h = (app.feed_total_height - rendered_end_y).max(0.0);

    div()
        .id("feed-scroll")
        .track_scroll(&app.feed_scroll.handle)
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
                .px_10()
                .py_7()
                .gap_6()
                .on_scroll_wheel(cx.listener(|this, ev, _, cx| {
                    cx.stop_propagation();
                    this.handle_smooth_scroll(ScrollTarget::Feed, ev, cx);
                }))
        // Header row matching TIDAL's clean Feed header
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_3xl()
                                .font_weight(FontWeight::BOLD)
                                .text_color(TidalTheme::text_primary())
                                .child("Feed"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(TidalTheme::text_muted())
                                .child(format!("Source: {source_label}")),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2p5()
                        .child(
                            div()
                                .id("feed-refresh-btn")
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .px_4()
                                .h(px(34.0))
                                .rounded(px(17.0))
                                .bg(if is_loading {
                                    TidalTheme::accent_tint()
                                } else {
                                    TidalTheme::bg_surface()
                                })
                                .border_1()
                                .border_color(if is_loading {
                                    TidalTheme::accent()
                                } else {
                                    TidalTheme::border_default()
                                })
                                .hover(|s| {
                                    s.bg(TidalTheme::bg_surface_hover())
                                        .border_color(TidalTheme::accent())
                                })
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(if is_loading {
                                    TidalTheme::accent()
                                } else {
                                    TidalTheme::text_primary()
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.refresh_feed(cx);
                                }))
                                .child(if is_loading {
                                    "↻ Refreshing…"
                                } else {
                                    "↻ Refresh Feed"
                                }),
                        )
                        .child(
                            div()
                                .id("feed-open-settings-btn")
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .px_3p5()
                                .h(px(34.0))
                                .rounded(px(17.0))
                                .bg(TidalTheme::bg_surface())
                                .border_1()
                                .border_color(TidalTheme::border_default())
                                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(TidalTheme::text_secondary())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_settings(cx);
                                }))
                                .child("⚙ Settings"),
                        ),
                ),
        )
        // Filter pills: All | Albums | Singles & EPs
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(feed_filter_pill(
                            "feed-filter-all",
                            "All",
                            filter == FeedFilter::All,
                            FeedFilter::All,
                            cx,
                        ))
                        .child(feed_filter_pill(
                            "feed-filter-albums",
                            "Albums",
                            filter == FeedFilter::Albums,
                            FeedFilter::Albums,
                            cx,
                        ))
                        .child(feed_filter_pill(
                            "feed-filter-singles",
                            "Singles & EPs",
                            filter == FeedFilter::SinglesAndEps,
                            FeedFilter::SinglesAndEps,
                            cx,
                        )),
                )
                .when(!status.is_empty(), |el| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(if is_loading {
                                TidalTheme::accent_soft()
                            } else {
                                TidalTheme::text_muted()
                            })
                            .child(status),
                    )
                }),
        )
        // Prominent loading indicator banner while refreshing Feed
        .when(is_loading, |el| {
            let pct = if app.feed_total > 0 {
                (app.feed_scanned as f32 / app.feed_total as f32).clamp(0.06, 1.0)
            } else {
                0.15
            };
            let progress_label = if app.feed_total > 0 {
                format!(
                    "{} / {} artists ({}%)",
                    app.feed_scanned,
                    app.feed_total,
                    (pct * 100.0).round() as u32
                )
            } else {
                "Connecting…".to_string()
            };

            el.child(
                div()
                    .max_w(px(780.0))
                    .p_4()
                    .rounded(px(10.0))
                    .bg(TidalTheme::bg_surface())
                    .border_1()
                    .border_color(TidalTheme::accent())
                    .flex()
                    .flex_col()
                    .gap_2p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(TidalTheme::accent())
                                            .child("↻"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(TidalTheme::text_primary())
                                            .child(if app.feed_status.is_empty() {
                                                "Refreshing Feed from YouTube Music…".to_string()
                                            } else {
                                                app.feed_status.clone()
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(TidalTheme::accent())
                                    .child(progress_label),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(4.0))
                            .rounded(px(2.0))
                            .bg(TidalTheme::bg_surface_active())
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(pct))
                                    .bg(TidalTheme::accent()),
                            ),
                    ),
            )
        })
        // Empty / Signed-out state
        .when(total_rows == 0 && !is_loading, |el| {
            el.child(
                div()
                    .my_10()
                    .p_8()
                    .max_w(px(520.0))
                    .rounded(px(14.0))
                    .bg(TidalTheme::bg_surface())
                    .border_1()
                    .border_color(TidalTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .text_color(TidalTheme::text_primary())
                            .child(if !is_signed_in {
                                "Sign in to load your Feed"
                            } else {
                                "No releases collected yet"
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(TidalTheme::text_secondary())
                            .child(if !is_signed_in {
                                "Sign in with Google so the background YouTube Music WebView can check new releases from your subscribed and collection artists."
                            } else {
                                "Click Refresh Feed to scan your artists via the background YouTube Music WebView and collect their latest albums, singles, and EPs."
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2p5()
                            .mt_1()
                            .child(
                                div()
                                    .id("feed-empty-action")
                                    .cursor_pointer()
                                    .px_4()
                                    .py_2()
                                    .rounded(px(8.0))
                                    .bg(TidalTheme::text_primary())
                                    .hover(|s| s.bg(TidalTheme::accent()))
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(TidalTheme::text_inverse())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !is_signed_in {
                                            this.controller
                                                .send(crate::protocol::WebCommand::OpenSignIn);
                                        } else {
                                            this.refresh_feed(cx);
                                        }
                                    }))
                                    .child(if !is_signed_in {
                                        "Sign In with Google"
                                    } else {
                                        "Refresh Feed Now"
                                    }),
                            ),
                    ),
            )
        })
        // Virtualized TIDAL-style chronological Feed list (only renders visible viewport slice)
        .when(total_rows > 0, |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .when(top_spacer_h > 0.0, |list| {
                        list.child(div().h(px(top_spacer_h)).flex_shrink_0())
                    })
                    .children((start_idx..end_idx).filter_map(|row_idx| {
                        match app.feed_rows[row_idx] {
                            FeedDisplayRow::Header(label) => Some(
                                div()
                                    .h(px(FEED_HEADER_HEIGHT))
                                    .flex()
                                    .items_end()
                                    .pb_2()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(TidalTheme::text_primary())
                                    .child(label)
                                    .into_any_element(),
                            ),
                            FeedDisplayRow::Release(rel_idx) => {
                                let rel = app.feed_releases.get(rel_idx)?;
                                let rel_for_click = rel.clone();
                                let rel_for_play = rel.clone();
                                let rel_for_menu = rel.clone();
                                let artist_href = rel.artist_href.clone();
                                let art_group: SharedString = format!("feed-art-{rel_idx}").into();
                                let play_tooltip = if rel.release_type.eq_ignore_ascii_case("album")
                                {
                                    "Play album"
                                } else if rel.release_type.eq_ignore_ascii_case("ep") {
                                    "Play EP"
                                } else if rel.release_type.eq_ignore_ascii_case("single") {
                                    "Play single"
                                } else {
                                    "Play release"
                                };
                                let type_prefix =
                                    if rel.release_type.eq_ignore_ascii_case("album") {
                                        "Album  •  "
                                    } else if rel.release_type.eq_ignore_ascii_case("ep") {
                                        "EP by "
                                    } else if rel.release_type.eq_ignore_ascii_case("single") {
                                        "Single by "
                                    } else {
                                        "Release by "
                                    };

                                let date_badge = rel
                                    .release_date
                                    .as_deref()
                                    .and_then(format_short_date)
                                    .or_else(|| rel.year.map(|y| y.to_string()));

                                Some(
                                    div()
                                        .id(("feed-release-row", rel_idx))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .gap_4()
                                        .h(px(FEED_ROW_HEIGHT))
                                        .px_3()
                                        .max_w(px(780.0))
                                        .rounded(px(8.0))
                                        .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.open_feed_release(&rel_for_click, cx);
                                        }))
                                        // Artwork with on-hover Play button overlay (matching track/card rows)
                                        .child(
                                            div()
                                                .id(("feed-art-play", rel_idx))
                                                .group(art_group.clone())
                                                .relative()
                                                .w(px(56.0))
                                                .h(px(56.0))
                                                .flex_shrink_0()
                                                .cursor_pointer()
                                                .tooltip(tidal_tooltip(play_tooltip, None))
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation();
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.play_feed_release(&rel_for_play, cx);
                                                }))
                                                .child(artwork_box(
                                                    rel.artwork_url.as_deref(),
                                                    px(56.0),
                                                    false,
                                                ))
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
                                                        .group_hover(art_group, |s| s.opacity(1.0))
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .w(px(30.0))
                                                                .h(px(30.0))
                                                                .rounded_full()
                                                                .bg(TidalTheme::text_primary())
                                                                .hover(|s| {
                                                                    s.bg(TidalTheme::accent())
                                                                })
                                                                .text_xs()
                                                                .font_weight(FontWeight::BOLD)
                                                                .text_color(
                                                                    TidalTheme::text_inverse(),
                                                                )
                                                                .pl(px(2.0))
                                                                .child("▶"),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .flex_1()
                                                .min_w_0()
                                                .gap_0p5()
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .child(
                                                            div()
                                                                .border_b_1()
                                                                .border_color(
                                                                    gpui::transparent_black(),
                                                                )
                                                                .hover(|s| {
                                                                    s.text_color(
                                                                        TidalTheme::accent(),
                                                                    )
                                                                    .border_color(
                                                                        TidalTheme::accent(),
                                                                    )
                                                                    .underline()
                                                                })
                                                                .text_sm()
                                                                .font_weight(FontWeight::BOLD)
                                                                .text_color(
                                                                    TidalTheme::text_primary(),
                                                                )
                                                                .truncate()
                                                                .child(rel.title.clone()),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .text_xs()
                                                        .text_color(TidalTheme::text_secondary())
                                                        .child(type_prefix)
                                                        .child(
                                                            div()
                                                                .id(("feed-artist-link", rel_idx))
                                                                .cursor_pointer()
                                                                .border_b_1()
                                                                .border_color(
                                                                    gpui::transparent_black(),
                                                                )
                                                                .hover(|s| {
                                                                    s.text_color(
                                                                        TidalTheme::accent(),
                                                                    )
                                                                    .border_color(
                                                                        TidalTheme::accent(),
                                                                    )
                                                                    .underline()
                                                                })
                                                                .on_mouse_down(
                                                                    MouseButton::Left,
                                                                    |_, _, cx| {
                                                                        cx.stop_propagation()
                                                                    },
                                                                )
                                                                .on_click(cx.listener(
                                                                    move |this, _, _, cx| {
                                                                        if !artist_href.is_empty() {
                                                                            this.navigate_url_from_local(
                                                                                artist_href.clone(),
                                                                                cx,
                                                                            );
                                                                        }
                                                                    },
                                                                ))
                                                                .child(rel.artist_name.clone()),
                                                        )
                                                        .when_some(date_badge, |el, d| {
                                                            el.child(
                                                                div()
                                                                    .text_color(
                                                                        TidalTheme::text_muted(),
                                                                    )
                                                                    .child(format!("  •  {d}")),
                                                            )
                                                        }),
                                                ),
                                        )
                                        // Far-right action menu button
                                        .child(
                                            div()
                                                .id(("feed-row-menu-btn", rel_idx))
                                                .cursor_pointer()
                                                .flex_shrink_0()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .w(px(34.0))
                                                .h(px(34.0))
                                                .rounded_full()
                                                .text_base()
                                                .text_color(TidalTheme::text_secondary())
                                                .hover(|s| {
                                                    s.bg(TidalTheme::bg_surface_active())
                                                        .text_color(TidalTheme::accent())
                                                })
                                                .tooltip(tidal_tooltip("More options", None))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(
                                                        move |this, ev: &MouseDownEvent, _, cx| {
                                                            cx.stop_propagation();
                                                            this.open_feed_item_menu(
                                                                rel_for_menu.clone(),
                                                                ev.position,
                                                                cx,
                                                            );
                                                        },
                                                    ),
                                                )
                                                .child("⋯"),
                                        )
                                        .into_any_element(),
                                )
                            }
                        }
                    }))
                    .when(bottom_spacer_h > 0.0, |list| {
                        list.child(div().h(px(bottom_spacer_h)).flex_shrink_0())
                    }),
            )
        }),
        )
}

fn feed_filter_pill(
    id: &'static str,
    label: &'static str,
    active: bool,
    target: FeedFilter,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .px_3p5()
        .py_1p5()
        .rounded(px(18.0))
        .bg(if active {
            TidalTheme::text_primary()
        } else {
            TidalTheme::bg_surface()
        })
        .border_1()
        .border_color(if active {
            TidalTheme::text_primary()
        } else {
            TidalTheme::border_default()
        })
        .hover(|s| {
            if active {
                s.bg(TidalTheme::accent())
            } else {
                s.bg(TidalTheme::bg_surface_hover())
            }
        })
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(if active {
            TidalTheme::text_inverse()
        } else {
            TidalTheme::text_primary()
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.set_feed_filter(target, cx);
        }))
        .child(label)
}

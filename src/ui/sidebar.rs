use gpui::{Context, FontWeight, SharedString, div, prelude::*, px};

use crate::app::{ActiveView, ScrollTarget, YtmTidalApp};
use crate::protocol::{Route, WebCommand};
use crate::theme::TidalTheme;
use crate::ui::{section_eyebrow, tidal_brand_mark, tidal_tooltip};

pub fn render_sidebar(app: &YtmTidalApp, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let active_route = app.state.route;
    let active_view = app.active_view;
    let feed_enabled = app.settings.feed_enabled;
    let feed_loading = app.feed_loading;
    let feed_scanned = app.feed_scanned;
    let feed_total = app.feed_total;
    let unread_feed = app.unread_feed_count();
    let playlists = app.state.sidebar_playlists.clone();
    let is_signed_in = app.state.account.is_signed_in;

    div()
        .flex()
        .flex_col()
        .w(px(238.0))
        .flex_shrink_0()
        .h_full()
        .bg(TidalTheme::bg_sidebar())
        .border_r_1()
        .border_color(TidalTheme::border_subtle())
        // Brand Header (symmetrical Y emblem + clean RIPTIDE wordmark)
        .child(
            div()
                .flex()
                .items_center()
                .gap_3p5()
                .px_5()
                .h(px(64.0))
                .border_b_1()
                .border_color(TidalTheme::border_subtle())
                .child(tidal_brand_mark())
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::BOLD)
                        .text_color(TidalTheme::text_primary())
                        .child("RIPTIDE"),
                ),
        )
        // Primary Navigation
        .child(
            div()
                .id("sidebar-scroll")
                .track_scroll(&app.sidebar_scroll.handle)
                .flex_1()
                .overflow_y_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_h_full()
                        .py_4()
                        .px_2p5()
                        .gap_5()
                        .on_scroll_wheel(cx.listener(|this, ev, _, cx| {
                            cx.stop_propagation();
                            this.handle_smooth_scroll(ScrollTarget::Sidebar, ev, cx);
                        }))
                // Discover group
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(section_eyebrow("DISCOVER"))
                        .child(nav_item(
                            "nav-home",
                            "⌂",
                            "Home",
                            active_view == ActiveView::Webapp && active_route == Route::Home,
                            Route::Home,
                            cx,
                        ))
                        // Feed directly below Home (toggled in Settings)
                        .when(feed_enabled, |el| {
                            let is_active = active_view == ActiveView::Feed;
                            el.child(
                                div()
                                    .id("nav-feed")
                                    .relative()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px_3()
                                    .h(px(38.0))
                                    .rounded(px(6.0))
                                    .cursor_pointer()
                                    .bg(if is_active {
                                        TidalTheme::bg_surface()
                                    } else {
                                        TidalTheme::bg_sidebar()
                                    })
                                    .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                    .text_sm()
                                    .font_weight(if is_active {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .text_color(if is_active {
                                        TidalTheme::accent()
                                    } else {
                                        TidalTheme::text_secondary()
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_feed(cx);
                                    }))
                                    .when(is_active, |row| {
                                        row.child(
                                            div()
                                                .absolute()
                                                .left(px(0.0))
                                                .top(px(7.0))
                                                .bottom(px(7.0))
                                                .w(px(2.5))
                                                .rounded_r(px(2.0))
                                                .bg(TidalTheme::accent()),
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_3()
                                            .child(feed_bell_icon(if is_active {
                                                TidalTheme::accent()
                                            } else {
                                                TidalTheme::text_primary()
                                            }))
                                            .child("Feed"),
                                    )
                                    .when(feed_loading, |row| {
                                        let label = if feed_total > 0 {
                                            format!("↻ {feed_scanned}/{feed_total}")
                                        } else {
                                            "↻ …".to_string()
                                        };
                                        row.child(
                                            div()
                                                .px_1p5()
                                                .py(px(1.5))
                                                .rounded(px(8.0))
                                                .bg(TidalTheme::accent_tint())
                                                .border_1()
                                                .border_color(TidalTheme::accent())
                                                .text_size(px(10.0))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(TidalTheme::accent())
                                                .child(label),
                                        )
                                    })
                                    .when(!feed_loading && unread_feed > 0, |row| {
                                        let badge = if unread_feed > 99 {
                                            "99+".to_string()
                                        } else {
                                            unread_feed.to_string()
                                        };
                                        row.child(
                                            div()
                                                .px_1p5()
                                                .min_w(px(20.0))
                                                .h(px(18.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded(px(9.0))
                                                .bg(TidalTheme::accent())
                                                .text_size(px(10.0))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(TidalTheme::text_inverse())
                                                .child(badge),
                                        )
                                    }),
                            )
                        })
                        .child(nav_item(
                            "nav-explore",
                            "◎",
                            "Explore",
                            active_view == ActiveView::Webapp && active_route == Route::Explore,
                            Route::Explore,
                            cx,
                        ))
                        .child(
                            div()
                                .id("nav-search")
                                .flex()
                                .items_center()
                                .gap_3()
                                .px_3()
                                .h(px(38.0))
                                .rounded(px(6.0))
                                .cursor_pointer()
                                .bg(if active_view == ActiveView::Webapp
                                    && active_route == Route::Search
                                {
                                    TidalTheme::bg_surface()
                                } else {
                                    TidalTheme::bg_sidebar()
                                })
                                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                .text_sm()
                                .font_weight(
                                    if active_view == ActiveView::Webapp
                                        && active_route == Route::Search
                                    {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    },
                                )
                                .text_color(
                                    if active_view == ActiveView::Webapp
                                        && active_route == Route::Search
                                    {
                                        TidalTheme::accent()
                                    } else {
                                        TidalTheme::text_secondary()
                                    },
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.focus_search(window, cx);
                                }))
                                .child(div().w(px(24.0)).text_lg().text_center().child("⌕"))
                                .child("Search"),
                        ),
                )
                // My Collection group
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(section_eyebrow("MY COLLECTION"))
                        .child(nav_item(
                            "nav-library",
                            "▤",
                            "Library",
                            active_view == ActiveView::Webapp && active_route == Route::Library,
                            Route::Library,
                            cx,
                        ))
                        .child(nav_item(
                            "nav-playlists",
                            "≡",
                            "Playlists",
                            active_view == ActiveView::Webapp
                                && active_route == Route::LibraryPlaylists,
                            Route::LibraryPlaylists,
                            cx,
                        ))
                        .child(nav_item(
                            "nav-albums",
                            "◉",
                            "Albums",
                            active_view == ActiveView::Webapp
                                && active_route == Route::LibraryAlbums,
                            Route::LibraryAlbums,
                            cx,
                        ))
                        .child(nav_item(
                            "nav-songs",
                            "♪",
                            "Tracks",
                            active_view == ActiveView::Webapp
                                && active_route == Route::LibrarySongs,
                            Route::LibrarySongs,
                            cx,
                        ))
                        .child(nav_item(
                            "nav-artists",
                            "✦",
                            "Artists",
                            active_view == ActiveView::Webapp
                                && active_route == Route::LibraryArtists,
                            Route::LibraryArtists,
                            cx,
                        )),
                )
                // User Playlists scraped live from YTM guide
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .pr_2()
                                .child(section_eyebrow("PLAYLISTS"))
                                .when(is_signed_in, |el| {
                                    el.child(
                                        div()
                                            .id("sidebar-new-playlist-btn")
                                            .cursor_pointer()
                                            .px_2()
                                            .py(px(2.0))
                                            .rounded(px(4.0))
                                            .bg(TidalTheme::bg_surface())
                                            .border_1()
                                            .border_color(TidalTheme::border_subtle())
                                            .hover(|s| {
                                                s.bg(TidalTheme::bg_surface_hover())
                                                    .border_color(TidalTheme::accent())
                                                    .text_color(TidalTheme::accent())
                                            })
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(TidalTheme::text_secondary())
                                            .tooltip(tidal_tooltip(
                                                "Create a new YouTube Music playlist",
                                                None,
                                            ))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.open_new_playlist_modal(window, cx);
                                            }))
                                            .child("+ New"),
                                    )
                                }),
                        )
                        .when(playlists.is_empty(), |el| {
                            el.child(
                                div()
                                    .mx_2()
                                    .p_3()
                                    .rounded(px(8.0))
                                    .bg(TidalTheme::bg_surface())
                                    .border_1()
                                    .border_color(TidalTheme::border_subtle())
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(TidalTheme::text_secondary())
                                            .child(if is_signed_in {
                                                "Your YouTube Music playlists will appear here."
                                            } else {
                                                "Sign in via the underlying YouTube Music webapp to sync your playlists & library."
                                            }),
                                    )
                                    .when(!is_signed_in, |card| {
                                        card.child(
                                            div()
                                                .id("sidebar-signin-btn")
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .h(px(28.0))
                                                .rounded(px(5.0))
                                                .bg(TidalTheme::text_primary())
                                                .hover(|s| s.bg(TidalTheme::accent()))
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(TidalTheme::text_inverse())
                                                .on_click(cx.listener(|this, _, _, _| {
                                                    this.controller.send(WebCommand::OpenSignIn);
                                                }))
                                                .child("Sign In with Google"),
                                        )
                                    }),
                            )
                        })
                        .children(playlists.into_iter().enumerate().map(|(idx, pl)| {
                            let click_id = pl.id.clone();
                            let href = pl.href.clone();
                            div()
                                .id(("sidebar-pl", idx))
                                .cursor_pointer()
                                .flex()
                                .flex_col()
                                .px_3()
                                .py_1p5()
                                .rounded(px(6.0))
                                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(ref url) = href {
                                        this.navigate_url_from_local(url.clone(), cx);
                                    } else {
                                        this.active_view = ActiveView::Webapp;
                                        this.controller
                                            .send(WebCommand::ClickItem(click_id.clone()));
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(TidalTheme::text_primary())
                                        .truncate()
                                        .child(pl.title),
                                )
                                .when(!pl.subtitle.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(TidalTheme::text_muted())
                                            .truncate()
                                            .child(pl.subtitle),
                                    )
                                })
                        })),
                ),
                ),
        )
        // Footer Settings button pinned cleanly at bottom of sidebar
        .child(
            div()
                .p_2p5()
                .border_t_1()
                .border_color(TidalTheme::border_subtle())
                .child({
                    let active = active_view == ActiveView::Settings;
                    div()
                        .id("nav-settings")
                        .relative()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .h(px(38.0))
                        .rounded(px(6.0))
                        .cursor_pointer()
                        .bg(if active {
                            TidalTheme::bg_surface()
                        } else {
                            TidalTheme::bg_sidebar()
                        })
                        .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                        .text_sm()
                        .font_weight(if active {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::MEDIUM
                        })
                        .text_color(if active {
                            TidalTheme::accent()
                        } else {
                            TidalTheme::text_secondary()
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_settings(cx);
                        }))
                        .when(active, |el| {
                            el.child(
                                div()
                                    .absolute()
                                    .left(px(0.0))
                                    .top(px(7.0))
                                    .bottom(px(7.0))
                                    .w(px(2.5))
                                    .rounded_r(px(2.0))
                                    .bg(TidalTheme::accent()),
                            )
                        })
                        .child(div().w(px(24.0)).text_lg().text_center().child("⚙"))
                        .child("Settings")
                }),
        )
}

fn nav_item(
    id: &'static str,
    icon: &'static str,
    label: impl Into<SharedString>,
    active: bool,
    route: Route,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .relative()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .h(px(38.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .bg(if active {
            TidalTheme::bg_surface()
        } else {
            TidalTheme::bg_sidebar()
        })
        .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
        .text_sm()
        .font_weight(if active {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::MEDIUM
        })
        .text_color(if active {
            TidalTheme::accent()
        } else {
            TidalTheme::text_secondary()
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.navigate(route, cx);
        }))
        .when(active, |el| {
            el.child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(7.0))
                    .bottom(px(7.0))
                    .w(px(2.5))
                    .rounded_r(px(2.0))
                    .bg(TidalTheme::accent()),
            )
        })
        .child(div().w(px(24.0)).text_lg().text_center().child(icon))
        .child(label.into())
}

/// Crisp monochrome outline bell icon (avoids Windows' yellow color emoji for 🔔).
fn feed_bell_icon(color: gpui::Rgba) -> impl IntoElement {
    div()
        .w(px(24.0))
        .h(px(20.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        // Bell dome
        .child(
            div()
                .w(px(11.0))
                .h(px(10.0))
                .rounded_t(px(5.5))
                .border_1()
                .border_b_0()
                .border_color(color),
        )
        // Bell rim
        .child(div().w(px(15.0)).h(px(1.5)).rounded(px(1.0)).bg(color))
        // Clapper
        .child(
            div()
                .mt(px(1.0))
                .w(px(4.0))
                .h(px(2.0))
                .rounded_b(px(2.0))
                .bg(color),
        )
}

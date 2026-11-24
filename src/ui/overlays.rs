use gpui::{
    Context, FontWeight, MouseButton, Pixels, Point, SharedUri, Window, div, img, prelude::*, px,
};

use crate::app::{ArtworkLightbox, PlaylistPrivacy, YtmTidalApp};
use crate::protocol::{ConfirmDialog, FeedRelease, ShareDialog, WebCommand};
use crate::theme::TidalTheme;

/// Renders floating GPUI overlays:
/// 1. High-res Artwork Lightbox viewer
/// 2. Native TIDAL-styled Share popup (`yt-unified-share-panel-renderer` or local Feed share)
/// 3. Native TIDAL-styled Confirmation Dialog (e.g., "Unsubscribe from Artist?")
/// 4. Native TIDAL-styled "New Playlist" dialog (headlessly drives YTM's real form)
/// 5. Native "Save to Playlist" picker (`ytmusic-add-to-playlist-renderer`)
/// 6. Feed release item Action Context Menu
/// 7. Live YouTube Music 3-dot Action Context Menu (`ytmusic-menu-popup-renderer`)
pub fn render_overlays(
    app: &YtmTidalApp,
    window: &mut Window,
    cx: &mut Context<YtmTidalApp>,
) -> Option<gpui::AnyElement> {
    if let Some(ref lb) = app.artwork_lightbox {
        return Some(render_artwork_lightbox(lb.clone(), window, cx).into_any_element());
    }

    if let Some(dlg) = app
        .local_share_dialog
        .as_ref()
        .or(app.state.share_dialog.as_ref())
    {
        return Some(render_share_dialog(dlg.clone(), app.is_share_copied(), cx).into_any_element());
    }

    if let Some(ref dlg) = app.state.confirm_dialog {
        return Some(render_confirm_dialog(dlg.clone(), cx).into_any_element());
    }

    if app.show_new_playlist_modal {
        return Some(render_new_playlist_modal(app, cx).into_any_element());
    }

    if !app.state.add_to_playlist.is_empty() {
        return Some(render_add_to_playlist_modal(app, cx).into_any_element());
    }

    if let Some((ref rel, pos)) = app.feed_item_menu {
        return Some(render_feed_item_menu(rel.clone(), pos, window, cx).into_any_element());
    }

    if app.menu_open && !app.state.popup_menu.is_empty() {
        return Some(render_context_menu(app, window, cx).into_any_element());
    }

    None
}

fn render_artwork_lightbox(
    lb: ArtworkLightbox,
    window: &mut Window,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let vp = window.viewport_size();
    let vp_w: f32 = vp.width.into();
    let vp_h: f32 = vp.height.into();
    let img_size = (vp_h - 200.0).min(vp_w - 140.0).clamp(260.0, 560.0);
    let radius = if lb.is_circular {
        px(img_size / 2.0)
    } else {
        px(12.0)
    };

    div()
        .id("artwork-lightbox-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_4()
        .bg(TidalTheme::overlay_dark())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.close_artwork_lightbox(cx);
            }),
        )
        .child(
            div()
                .id("artwork-lightbox-card")
                .flex()
                .flex_col()
                .items_center()
                .gap_3p5()
                .p_5()
                .rounded(px(16.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_strong())
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .w(px(img_size))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .flex_1()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(TidalTheme::text_primary())
                                        .truncate()
                                        .child(lb.title),
                                )
                                .when(!lb.subtitle.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_xs()
                                            .text_color(TidalTheme::text_secondary())
                                            .truncate()
                                            .child(lb.subtitle),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .id("artwork-lightbox-close")
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(32.0))
                                .h(px(32.0))
                                .rounded_full()
                                .bg(TidalTheme::bg_surface_active())
                                .hover(|s| {
                                    s.bg(TidalTheme::bg_surface_hover())
                                        .text_color(TidalTheme::accent())
                                })
                                .text_sm()
                                .text_color(TidalTheme::text_secondary())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_artwork_lightbox(cx);
                                }))
                                .child("✕"),
                        ),
                )
                .child(
                    div()
                        .w(px(img_size))
                        .h(px(img_size))
                        .rounded(radius)
                        .overflow_hidden()
                        .bg(TidalTheme::bg_void())
                        .border_1()
                        .border_color(TidalTheme::border_subtle())
                        .child(
                            img(SharedUri::from(lb.url))
                                .w(px(img_size))
                                .h(px(img_size))
                                .rounded(radius)
                                .object_fit(gpui::ObjectFit::Cover),
                        ),
                ),
        )
}

fn render_share_dialog(
    dlg: ShareDialog,
    copied: bool,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let url_for_copy = dlg.url.clone();
    let copy_id = dlg.copy_id.clone();

    div()
        .id("share-dialog-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(TidalTheme::overlay_dark())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.close_share_dialog(cx);
            }),
        )
        .child(
            div()
                .id("share-dialog-card")
                .w(px(480.0))
                .p_6()
                .rounded(px(14.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_strong())
                .shadow_lg()
                .flex()
                .flex_col()
                .gap_4()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                // Header
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::BOLD)
                                .text_color(TidalTheme::text_primary())
                                .truncate()
                                .child(dlg.title),
                        )
                        .child(
                            div()
                                .id("share-dialog-close")
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(30.0))
                                .h(px(30.0))
                                .rounded_full()
                                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                .text_base()
                                .text_color(TidalTheme::text_secondary())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_share_dialog(cx);
                                }))
                                .child("✕"),
                        ),
                )
                // Share Targets row (WhatsApp, X, Reddit, Email, YouTube, etc.)
                .when(!dlg.targets.is_empty(), |el| {
                    el.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .children(dlg.targets.into_iter().enumerate().map(|(idx, tgt)| {
                                let target_url = tgt.url.clone();
                                let target_id = tgt.id.clone();
                                div()
                                    .id(("share-target", idx))
                                    .cursor_pointer()
                                    .px_3p5()
                                    .h(px(34.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(17.0))
                                    .bg(TidalTheme::bg_surface_active())
                                    .border_1()
                                    .border_color(TidalTheme::border_default())
                                    .hover(|s| {
                                        s.bg(TidalTheme::bg_surface_hover())
                                            .border_color(TidalTheme::accent())
                                            .text_color(TidalTheme::accent())
                                    })
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(TidalTheme::text_primary())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(ref u) = target_url {
                                            cx.open_url(u);
                                            this.close_share_dialog(cx);
                                        } else if !target_id.is_empty() {
                                            this.controller
                                                .send(WebCommand::ClickItem(target_id.clone()));
                                        }
                                    }))
                                    .child(tgt.label)
                            })),
                    )
                })
                // Copyable URL row
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .p_2()
                        .rounded(px(10.0))
                        .bg(TidalTheme::bg_input())
                        .border_1()
                        .border_color(if copied {
                            TidalTheme::accent()
                        } else {
                            TidalTheme::border_default()
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .px_2()
                                .text_xs()
                                .text_color(TidalTheme::text_secondary())
                                .truncate()
                                .child(dlg.url),
                        )
                        .child(
                            div()
                                .id("share-copy-btn")
                                .cursor_pointer()
                                .flex_shrink_0()
                                .px_4()
                                .h(px(32.0))
                                .flex()
                                .items_center()
                                .rounded(px(16.0))
                                .bg(if copied {
                                    TidalTheme::accent()
                                } else {
                                    TidalTheme::text_primary()
                                })
                                .hover(|s| s.bg(TidalTheme::accent()))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(TidalTheme::text_inverse())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.copy_text_with_feedback(url_for_copy.clone(), cx);
                                    if let Some(ref cid) = copy_id {
                                        this.controller.send(WebCommand::ClickItem(cid.clone()));
                                    }
                                }))
                                .child(if copied { "✓ Copied" } else { "Copy" }),
                        ),
                ),
        )
}

fn render_feed_item_menu(
    rel: FeedRelease,
    pos: Point<Pixels>,
    window: &mut Window,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let vp = window.viewport_size();
    let vp_w: f32 = vp.width.into();
    let vp_h: f32 = vp.height.into();

    let has_artist = !rel.artist_href.is_empty();
    let row_count = if has_artist { 5 } else { 4 };
    let menu_w = 240.0_f32;
    let menu_h = row_count as f32 * 38.0 + 16.0;

    let anchor_x: f32 = pos.x.into();
    let anchor_y: f32 = pos.y.into();

    // Open leftward if close to right edge
    let left = if anchor_x + menu_w + 16.0 > vp_w {
        (anchor_x - menu_w).max(16.0)
    } else {
        anchor_x.max(16.0)
    };
    let top = anchor_y.min((vp_h - menu_h - 16.0).max(16.0)).max(16.0);

    let go_label = if rel.release_type.eq_ignore_ascii_case("album") {
        "Go to Album"
    } else if rel.release_type.eq_ignore_ascii_case("ep") {
        "Go to EP"
    } else if rel.release_type.eq_ignore_ascii_case("single") {
        "Go to Single"
    } else {
        "Go to Release"
    };

    let rel_for_play = rel.clone();
    let rel_for_open = rel.clone();
    let artist_href = rel.artist_href.clone();
    let rel_for_share = rel.clone();
    let full_link = rel.shareable_url();

    div()
        .id("feed-menu-backdrop")
        .absolute()
        .inset_0()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.close_feed_item_menu(cx);
            }),
        )
        .child(
            div()
                .id("feed-menu-popover")
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(menu_w))
                .py_1p5()
                .rounded(px(10.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_strong())
                .shadow_lg()
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(feed_menu_row(
                    "feed-act-play",
                    "▶",
                    "Play",
                    cx.listener(move |this, _, _, cx| {
                        this.play_feed_release(&rel_for_play, cx);
                    }),
                ))
                .child(feed_menu_row(
                    "feed-act-open",
                    "◉",
                    go_label,
                    cx.listener(move |this, _, _, cx| {
                        this.open_feed_release(&rel_for_open, cx);
                    }),
                ))
                .when(has_artist, |el| {
                    el.child(feed_menu_row(
                        "feed-act-artist",
                        "♪",
                        "Go to Artist",
                        cx.listener(move |this, _, _, cx| {
                            this.close_feed_item_menu(cx);
                            this.navigate_url_from_local(artist_href.clone(), cx);
                        }),
                    ))
                })
                .child(feed_menu_row(
                    "feed-act-share",
                    "↗",
                    "Share",
                    cx.listener(move |this, _, _, cx| {
                        this.open_feed_share_dialog(&rel_for_share, cx);
                    }),
                ))
                .child(feed_menu_row(
                    "feed-act-copy",
                    "⧉",
                    "Copy link",
                    cx.listener(move |this, _, _, cx| {
                        this.close_feed_item_menu(cx);
                        this.copy_text_with_feedback(full_link.clone(), cx);
                    }),
                )),
        )
}

fn feed_menu_row(
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_3()
        .px_3p5()
        .h(px(36.0))
        .hover(|s| {
            s.bg(TidalTheme::bg_surface_hover())
                .text_color(TidalTheme::accent())
        })
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(TidalTheme::text_primary())
        .on_click(on_click)
        .child(
            div()
                .w(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .text_color(TidalTheme::text_secondary())
                .child(icon),
        )
        .child(label)
}

fn render_confirm_dialog(
    dlg: ConfirmDialog,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let confirm_id = dlg.confirm_id.clone();
    let cancel_id_backdrop = dlg.cancel_id.clone();
    let cancel_id_btn = dlg.cancel_id.clone();

    div()
        .id("confirm-dialog-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(TidalTheme::overlay_dark())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, _| {
                if let Some(ref cid) = cancel_id_backdrop {
                    this.controller.send(WebCommand::ClickItem(cid.clone()));
                } else {
                    this.controller.send(WebCommand::CloseMenu);
                }
            }),
        )
        .child(
            div()
                .id("confirm-dialog-card")
                .w(px(420.0))
                .p_6()
                .rounded(px(14.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_strong())
                .shadow_lg()
                .flex()
                .flex_col()
                .gap_4()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::BOLD)
                        .text_color(TidalTheme::text_primary())
                        .child(dlg.title),
                )
                .when(!dlg.body.is_empty(), |el| {
                    el.child(
                        div()
                            .text_sm()
                            .text_color(TidalTheme::text_secondary())
                            .child(dlg.body),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2p5()
                        .pt_2()
                        .child(
                            div()
                                .id("confirm-dialog-cancel")
                                .cursor_pointer()
                                .px_4()
                                .h(px(36.0))
                                .flex()
                                .items_center()
                                .rounded(px(18.0))
                                .bg(TidalTheme::bg_surface_active())
                                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(TidalTheme::text_primary())
                                .on_click(cx.listener(move |this, _, _, _| {
                                    if let Some(ref cid) = cancel_id_btn {
                                        this.controller.send(WebCommand::ClickItem(cid.clone()));
                                    } else {
                                        this.controller.send(WebCommand::CloseMenu);
                                    }
                                }))
                                .child(dlg.cancel_label),
                        )
                        .child(
                            div()
                                .id("confirm-dialog-confirm")
                                .cursor_pointer()
                                .px_5()
                                .h(px(36.0))
                                .flex()
                                .items_center()
                                .rounded(px(18.0))
                                .bg(TidalTheme::text_primary())
                                .hover(|s| s.bg(TidalTheme::accent()))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(TidalTheme::text_inverse())
                                .on_click(cx.listener(move |this, _, _, _| {
                                    this.controller
                                        .send(WebCommand::ClickItem(confirm_id.clone()));
                                }))
                                .child(dlg.confirm_label),
                        ),
                ),
        )
}

fn render_new_playlist_modal(app: &YtmTidalApp, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let privacy = app.new_playlist_privacy;
    let can_submit = !app.new_playlist_title.read(cx).text().trim().is_empty();

    div()
        .id("new-playlist-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(TidalTheme::overlay_dark())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.close_new_playlist_modal(cx);
            }),
        )
        .child(
            div()
                .id("new-playlist-card")
                .w(px(460.0))
                .p_6()
                .rounded(px(14.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_strong())
                .shadow_lg()
                .flex()
                .flex_col()
                .gap_4()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                // Header
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::BOLD)
                                .text_color(TidalTheme::text_primary())
                                .child("New Playlist"),
                        )
                        .child(
                            div()
                                .id("new-playlist-close")
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(30.0))
                                .h(px(30.0))
                                .rounded_full()
                                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                .text_base()
                                .text_color(TidalTheme::text_secondary())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_new_playlist_modal(cx);
                                }))
                                .child("✕"),
                        ),
                )
                // Title field
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(TidalTheme::text_secondary())
                                .child("Title"),
                        )
                        .child(app.new_playlist_title.clone()),
                )
                // Description field
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(TidalTheme::text_secondary())
                                .child("Description"),
                        )
                        .child(app.new_playlist_desc.clone()),
                )
                // Privacy selector
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(TidalTheme::text_secondary())
                                .child("Privacy"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(privacy_btn(
                                    "priv-public",
                                    "Public",
                                    privacy == PlaylistPrivacy::Public,
                                    PlaylistPrivacy::Public,
                                    cx,
                                ))
                                .child(privacy_btn(
                                    "priv-unlisted",
                                    "Unlisted",
                                    privacy == PlaylistPrivacy::Unlisted,
                                    PlaylistPrivacy::Unlisted,
                                    cx,
                                ))
                                .child(privacy_btn(
                                    "priv-private",
                                    "Private",
                                    privacy == PlaylistPrivacy::Private,
                                    PlaylistPrivacy::Private,
                                    cx,
                                )),
                        ),
                )
                // Actions
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_2p5()
                        .pt_2()
                        .child(
                            div()
                                .id("new-playlist-cancel")
                                .cursor_pointer()
                                .px_4()
                                .h(px(36.0))
                                .flex()
                                .items_center()
                                .rounded(px(18.0))
                                .bg(TidalTheme::bg_surface_active())
                                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(TidalTheme::text_primary())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_new_playlist_modal(cx);
                                }))
                                .child("Cancel"),
                        )
                        .child(
                            div()
                                .id("new-playlist-submit")
                                .cursor_pointer()
                                .px_5()
                                .h(px(36.0))
                                .flex()
                                .items_center()
                                .rounded(px(18.0))
                                .bg(if can_submit {
                                    TidalTheme::text_primary()
                                } else {
                                    TidalTheme::bg_surface_active()
                                })
                                .hover(|s| {
                                    if can_submit {
                                        s.bg(TidalTheme::accent())
                                    } else {
                                        s
                                    }
                                })
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(if can_submit {
                                    TidalTheme::text_inverse()
                                } else {
                                    TidalTheme::text_muted()
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.submit_new_playlist(cx);
                                }))
                                .child("Create"),
                        ),
                ),
        )
}

fn privacy_btn(
    id: &'static str,
    label: &'static str,
    active: bool,
    target: PlaylistPrivacy,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .flex_1()
        .h(px(34.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .bg(if active {
            TidalTheme::accent_tint()
        } else {
            TidalTheme::bg_input()
        })
        .border_1()
        .border_color(if active {
            TidalTheme::accent()
        } else {
            TidalTheme::border_default()
        })
        .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(if active {
            TidalTheme::accent()
        } else {
            TidalTheme::text_secondary()
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.new_playlist_privacy = target;
            cx.notify();
        }))
        .child(label)
}

fn render_context_menu(
    app: &YtmTidalApp,
    window: &mut Window,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let items = app.state.popup_menu.clone();
    let vp = window.viewport_size();
    let vp_w: f32 = vp.width.into();
    let vp_h: f32 = vp.height.into();

    let menu_w = 248.0_f32;
    let menu_h = (items.len() as f32 * 38.0 + 16.0).min(420.0);

    let anchor_x: f32 = app.menu_position.x.into();
    let anchor_y: f32 = app.menu_position.y.into();

    let left = anchor_x.min((vp_w - menu_w - 16.0).max(16.0)).max(16.0);
    let top = anchor_y.min((vp_h - menu_h - 16.0).max(16.0)).max(16.0);

    div()
        .id("context-menu-backdrop")
        .absolute()
        .inset_0()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.close_context_menu(cx);
            }),
        )
        .child(
            div()
                .id("context-menu-popover")
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(menu_w))
                .max_h(px(420.0))
                .overflow_y_scroll()
                .py_1p5()
                .rounded(px(10.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_strong())
                .shadow_lg()
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(items.into_iter().enumerate().map(|(idx, item)| {
                    let id = item.id.clone();
                    div()
                        .id(("ctx-menu-item", idx))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .px_3p5()
                        .h(px(36.0))
                        .hover(|s| {
                            s.bg(TidalTheme::bg_surface_hover())
                                .text_color(TidalTheme::accent())
                        })
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(TidalTheme::text_primary())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.menu_open = false;
                            this.controller.send(WebCommand::SelectMenuItem(id.clone()));
                            cx.notify();
                        }))
                        .child(item.label)
                })),
        )
}

fn render_add_to_playlist_modal(
    app: &YtmTidalApp,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    let options = app.state.add_to_playlist.clone();

    div()
        .id("add-to-pl-backdrop")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(TidalTheme::overlay_dark())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.controller.send(WebCommand::CloseMenu);
                cx.notify();
            }),
        )
        .child(
            div()
                .id("add-to-pl-card")
                .w(px(380.0))
                .max_h(px(480.0))
                .p_5()
                .rounded(px(14.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_strong())
                .shadow_lg()
                .flex()
                .flex_col()
                .gap_3()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::BOLD)
                                .text_color(TidalTheme::text_primary())
                                .child("Save to Playlist"),
                        )
                        .child(
                            div()
                                .id("add-to-pl-close")
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(28.0))
                                .h(px(28.0))
                                .rounded_full()
                                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                .text_base()
                                .text_color(TidalTheme::text_secondary())
                                .on_click(cx.listener(|this, _, _, _| {
                                    this.controller.send(WebCommand::CloseMenu);
                                }))
                                .child("✕"),
                        ),
                )
                .child(
                    div()
                        .id("add-to-pl-list")
                        .flex()
                        .flex_col()
                        .gap_1()
                        .overflow_y_scroll()
                        .max_h(px(360.0))
                        .children(options.into_iter().enumerate().map(|(idx, opt)| {
                            let id = opt.id.clone();
                            div()
                                .id(("add-pl-opt", idx))
                                .cursor_pointer()
                                .flex()
                                .flex_col()
                                .px_3()
                                .py_2()
                                .rounded(px(8.0))
                                .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                                .on_click(cx.listener(move |this, _, _, _| {
                                    this.controller.send(WebCommand::SelectMenuItem(id.clone()));
                                }))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(TidalTheme::text_primary())
                                        .child(opt.title),
                                )
                                .when(!opt.subtitle.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_xs()
                                            .text_color(TidalTheme::text_muted())
                                            .child(opt.subtitle),
                                    )
                                })
                        })),
                ),
        )
}

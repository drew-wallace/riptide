use gpui::{Context, FontWeight, div, prelude::*, px};

use crate::app::YtmTidalApp;
use crate::protocol::WebCommand;
use crate::theme::TidalTheme;
use crate::ui::{artwork_box, tidal_tooltip};

pub fn render_topbar(app: &YtmTidalApp, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let connected = app.state.connected;
    let is_loading = app.state.is_loading;
    let can_back = app.can_go_back();
    let can_fwd = app.can_go_forward();
    let is_signed_in = app.state.account.is_signed_in;
    let account_name = app.state.account.name.clone();
    let avatar_url = app.state.account.avatar_url.clone();
    let webview_visible = app.webview_visible;

    div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(64.0))
        .px_6()
        .bg(TidalTheme::bg_main())
        .border_b_1()
        .border_color(TidalTheme::border_subtle())
        // Left: History navigation + Search input
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .child(history_btn(
                            "nav-back",
                            "‹",
                            "Go back",
                            Some("Alt+←"),
                            can_back,
                            WebCommand::GoBack,
                            cx,
                        ))
                        .child(history_btn(
                            "nav-fwd",
                            "›",
                            "Go forward",
                            Some("Alt+→"),
                            can_fwd,
                            WebCommand::GoForward,
                            cx,
                        ))
                        .child(history_btn(
                            "nav-refresh",
                            "↻",
                            "Reload page",
                            None,
                            true,
                            WebCommand::Refresh,
                            cx,
                        )),
                )
                .child(app.search_input.clone())
                .when(is_loading, |el| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(TidalTheme::accent_soft())
                            .child("Syncing…"),
                    )
                }),
        )
        // Right: Connection badge + Sign In / Account + Underlying WebView toggle
        .child(
            div()
                .flex()
                .items_center()
                .gap_2p5()
                // Live Webapp connection indicator
                .child(
                    div()
                        .id("connection-status-pill")
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_2p5()
                        .h(px(30.0))
                        .rounded(px(15.0))
                        .bg(TidalTheme::bg_surface())
                        .border_1()
                        .border_color(TidalTheme::border_subtle())
                        .tooltip(tidal_tooltip(
                            "Connected to embedded YouTube Music WebView2 session",
                            None,
                        ))
                        .child(
                            div()
                                .w(px(7.0))
                                .h(px(7.0))
                                .rounded_full()
                                .bg(if connected {
                                    TidalTheme::online()
                                } else {
                                    TidalTheme::warn()
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(TidalTheme::text_secondary())
                                .child(if connected {
                                    "Webapp Synced"
                                } else {
                                    "Connecting…"
                                }),
                        ),
                )
                // Show / Hide underlying WebView2 window (for Google login or inspection)
                .child(
                    div()
                        .id("toggle-webview-btn")
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_3()
                        .h(px(32.0))
                        .rounded(px(6.0))
                        .bg(if webview_visible {
                            TidalTheme::accent_tint()
                        } else {
                            TidalTheme::bg_surface()
                        })
                        .border_1()
                        .border_color(if webview_visible {
                            TidalTheme::accent()
                        } else {
                            TidalTheme::border_default()
                        })
                        .hover(|s| s.bg(TidalTheme::bg_surface_hover()))
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if webview_visible {
                            TidalTheme::accent()
                        } else {
                            TidalTheme::text_primary()
                        })
                        .tooltip(tidal_tooltip(
                            if webview_visible {
                                "Hide underlying YouTube Music browser window"
                            } else {
                                "Show underlying YouTube Music browser window"
                            },
                            Some("Ctrl+Shift+W"),
                        ))
                        .on_click(cx.listener(|this, _, _, _| {
                            this.controller.send(WebCommand::ToggleWebViewVisible);
                        }))
                        .child(if webview_visible {
                            "Hide WebView"
                        } else {
                            "Show WebView"
                        }),
                )
                // Account identity badge (non-clickable when signed in; Sign In button when signed out)
                .child(if is_signed_in {
                    div()
                        .id("account-pill")
                        .flex()
                        .items_center()
                        .gap_2()
                        .pl_1()
                        .pr_3()
                        .h(px(34.0))
                        .rounded(px(17.0))
                        .bg(TidalTheme::bg_surface())
                        .border_1()
                        .border_color(TidalTheme::border_default())
                        .flex_shrink_0()
                        .child(artwork_box(avatar_url.as_deref(), px(26.0), true))
                        .child(
                            div()
                                .whitespace_nowrap()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(TidalTheme::text_primary())
                                .child(account_name),
                        )
                        .into_any_element()
                } else {
                    div()
                        .id("topbar-signin-btn")
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .px_3p5()
                        .h(px(32.0))
                        .rounded(px(6.0))
                        .bg(TidalTheme::text_primary())
                        .hover(|s| s.bg(TidalTheme::accent()))
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(TidalTheme::text_inverse())
                        .tooltip(tidal_tooltip(
                            "Sign in with your Google account (window closes automatically after sign-in)",
                            None,
                        ))
                        .on_click(cx.listener(|this, _, _, _| {
                            this.controller.send(WebCommand::OpenSignIn);
                        }))
                        .child("Sign In")
                        .into_any_element()
                }),
        )
}

fn history_btn(
    id: &'static str,
    symbol: &'static str,
    tip: &'static str,
    shortcut: Option<&'static str>,
    enabled: bool,
    cmd: WebCommand,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .when(enabled, |el| el.cursor_pointer())
        .flex()
        .items_center()
        .justify_center()
        .w(px(34.0))
        .h(px(34.0))
        .rounded_full()
        .bg(TidalTheme::bg_surface())
        .border_1()
        .border_color(TidalTheme::border_subtle())
        .when(enabled, |el| {
            el.hover(|s| {
                s.bg(TidalTheme::bg_surface_hover())
                    .text_color(TidalTheme::text_primary())
            })
        })
        .text_xl()
        .text_color(if enabled {
            TidalTheme::text_secondary()
        } else {
            TidalTheme::border_strong()
        })
        .tooltip(tidal_tooltip(tip, shortcut))
        .on_click(cx.listener(move |this, _, _, cx| {
            if !enabled {
                return;
            }
            match cmd {
                WebCommand::GoBack => this.go_back(cx),
                WebCommand::GoForward => this.go_forward(cx),
                _ => this.controller.send(cmd.clone()),
            }
        }))
        .child(symbol)
}

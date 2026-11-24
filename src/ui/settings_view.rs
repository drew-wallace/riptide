use gpui::{Context, FontWeight, div, prelude::*, px};

use crate::app::{ScrollTarget, YtmTidalApp};
use crate::protocol::{FeedArtistSource, WebCommand};
use crate::theme::TidalTheme;
use crate::time_util::{format_12h_time, format_relative_age, now_unix_secs};

pub fn render_settings_view(app: &YtmTidalApp, cx: &mut Context<YtmTidalApp>) -> impl IntoElement {
    let s = &app.settings;
    let feed_enabled = s.feed_enabled;
    let feed_source = s.feed_source;
    let refresh_on_open = s.feed_refresh_on_open;
    let refresh_daily = s.feed_refresh_daily;
    let daily_hour = s.feed_daily_hour;
    let daily_minute = s.feed_daily_minute;
    let run_if_missed = s.feed_run_if_missed;

    let feed_loading = app.feed_loading;
    let releases_count = app.feed_releases.len();
    let artists_count = app.feed_scanned;
    let last_refreshed_label = format_relative_age(app.feed_last_refreshed_unix, now_unix_secs());
    let is_signed_in = app.state.account.is_signed_in;
    let account_name = app.state.account.name.clone();

    div()
        .id("settings-scroll")
        .track_scroll(&app.settings_scroll.handle)
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
                .gap_7()
                .on_scroll_wheel(cx.listener(|this, ev, _, cx| {
                    cx.stop_propagation();
                    this.handle_smooth_scroll(ScrollTarget::Settings, ev, cx);
                }))
        // Header
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
                        .child("Settings"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(TidalTheme::text_muted())
                        .child("Configure your native client preferences and Release Feed schedule."),
                ),
        )
        // FEED SECTION
        .child(
            div()
                .max_w(px(700.0))
                .p_6()
                .rounded(px(12.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_subtle())
                .flex()
                .flex_col()
                .gap_5()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::BOLD)
                                .text_color(TidalTheme::text_primary())
                                .child("Feed"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(TidalTheme::text_secondary())
                                .child("Discover new singles, EPs, and albums from your artists by scanning their YouTube Music pages in the background WebView."),
                        ),
                )
                // 1. Toggle Feed On/Off
                .child(toggle_row(
                    "settings-feed-toggle",
                    "Enable Feed",
                    "Show the Feed tab below Home in the sidebar.",
                    feed_enabled,
                    cx.listener(|this, _, _, cx| {
                        this.set_feed_enabled(!this.settings.feed_enabled, cx);
                    }),
                ))
                // 2. Artist Source selector
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2p5()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(TidalTheme::text_primary())
                                        .child("Artist Source"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(TidalTheme::text_muted())
                                        .child("Choose which YouTube Music artists are scanned for new releases."),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2p5()
                                .child(source_card(
                                    "src-both",
                                    "Both (Default)",
                                    "Subscribed artists & library collection",
                                    feed_source == FeedArtistSource::Both,
                                    FeedArtistSource::Both,
                                    cx,
                                ))
                                .child(source_card(
                                    "src-subscribed",
                                    "Subscribed",
                                    "Only artists you subscribed to",
                                    feed_source == FeedArtistSource::Subscribed,
                                    FeedArtistSource::Subscribed,
                                    cx,
                                ))
                                .child(source_card(
                                    "src-collection",
                                    "Collection",
                                    "Only artists in your library",
                                    feed_source == FeedArtistSource::Collection,
                                    FeedArtistSource::Collection,
                                    cx,
                                )),
                        ),
                )
                // 3. Feed Refresh Schedule
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3p5()
                        .pt_4()
                        .border_t_1()
                        .border_color(TidalTheme::border_subtle())
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::BOLD)
                                .text_color(TidalTheme::text_primary())
                                .child("Refresh Schedule"),
                        )
                        // On app open toggle (default: On)
                        .child(toggle_row(
                            "settings-refresh-on-open",
                            "Refresh on app open",
                            "Automatically refresh the Feed when the app launches and signs in.",
                            refresh_on_open,
                            cx.listener(|this, _, _, cx| {
                                this.set_feed_refresh_on_open(
                                    !this.settings.feed_refresh_on_open,
                                    cx,
                                );
                            }),
                        ))
                        // Once daily toggle (default: On)
                        .child(toggle_row(
                            "settings-refresh-daily",
                            "Refresh once daily",
                            "Automatically refresh the Feed every day at your chosen local time.",
                            refresh_daily,
                            cx.listener(|this, _, _, cx| {
                                this.set_feed_refresh_daily(!this.settings.feed_refresh_daily, cx);
                            }),
                        ))
                        // Daily time picker + Missed-schedule checkbox
                        .when(refresh_daily, |el| {
                            el.child(
                                div()
                                    .p_4()
                                    .rounded(px(10.0))
                                    .bg(TidalTheme::bg_input())
                                    .border_1()
                                    .border_color(TidalTheme::border_default())
                                    .flex()
                                    .flex_col()
                                    .gap_3p5()
                                    // Time picker row
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_0p5()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(TidalTheme::text_primary())
                                                            .child("Daily refresh time"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .text_color(TidalTheme::text_muted())
                                                            .child("Local system time (defaults to 12:00 AM)"),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(time_step_btn(
                                                        "daily-hour-dec",
                                                        "−",
                                                        cx.listener(|this, _, _, cx| {
                                                            this.step_daily_refresh_time(-30, cx);
                                                        }),
                                                    ))
                                                    .child(
                                                        div()
                                                            .w(px(92.0))
                                                            .h(px(32.0))
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .rounded(px(6.0))
                                                            .bg(TidalTheme::bg_surface())
                                                            .border_1()
                                                            .border_color(TidalTheme::accent())
                                                            .text_xs()
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(TidalTheme::accent())
                                                            .child(format_12h_time(
                                                                daily_hour,
                                                                daily_minute,
                                                            )),
                                                    )
                                                    .child(time_step_btn(
                                                        "daily-hour-inc",
                                                        "+",
                                                        cx.listener(|this, _, _, cx| {
                                                            this.step_daily_refresh_time(30, cx);
                                                        }),
                                                    )),
                                            ),
                                    )
                                    // Quick time presets
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .child(time_preset_pill(
                                                "preset-12am",
                                                "12:00 AM",
                                                daily_hour == 0 && daily_minute == 0,
                                                0,
                                                0,
                                                cx,
                                            ))
                                            .child(time_preset_pill(
                                                "preset-6am",
                                                "6:00 AM",
                                                daily_hour == 6 && daily_minute == 0,
                                                6,
                                                0,
                                                cx,
                                            ))
                                            .child(time_preset_pill(
                                                "preset-12pm",
                                                "12:00 PM",
                                                daily_hour == 12 && daily_minute == 0,
                                                12,
                                                0,
                                                cx,
                                            ))
                                            .child(time_preset_pill(
                                                "preset-6pm",
                                                "6:00 PM",
                                                daily_hour == 18 && daily_minute == 0,
                                                18,
                                                0,
                                                cx,
                                            )),
                                    )
                                    // Checkbox: run immediately if a scheduled time was missed
                                    .child(
                                        div()
                                            .id("settings-run-if-missed")
                                            .cursor_pointer()
                                            .pt_2()
                                            .border_t_1()
                                            .border_color(TidalTheme::border_subtle())
                                            .flex()
                                            .items_center()
                                            .gap_2p5()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_feed_run_if_missed(
                                                    !this.settings.feed_run_if_missed,
                                                    cx,
                                                );
                                            }))
                                            .child(
                                                div()
                                                    .w(px(18.0))
                                                    .h(px(18.0))
                                                    .rounded(px(4.0))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .bg(if run_if_missed {
                                                        TidalTheme::accent()
                                                    } else {
                                                        TidalTheme::bg_surface()
                                                    })
                                                    .border_1()
                                                    .border_color(if run_if_missed {
                                                        TidalTheme::accent()
                                                    } else {
                                                        TidalTheme::border_strong()
                                                    })
                                                    .text_xs()
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(TidalTheme::text_inverse())
                                                    .child(if run_if_missed { "✓" } else { "" }),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(TidalTheme::text_primary())
                                                    .child("Run immediately if a scheduled refresh time was missed"),
                                            ),
                                    ),
                            )
                        }),
                )
                // 4. Refresh Feed action & cache status
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .pt_3()
                        .border_t_1()
                        .border_color(TidalTheme::border_subtle())
                        .child(
                            div()
                                .text_xs()
                                .text_color(TidalTheme::text_muted())
                                .child(if feed_loading {
                                    app.feed_status.clone()
                                } else if releases_count > 0 {
                                    format!(
                                        "{releases_count} cached releases from {artists_count} artists • Last refreshed: {last_refreshed_label}"
                                    )
                                } else {
                                    format!("Last refreshed: {last_refreshed_label}")
                                }),
                        )
                        .child(
                            div()
                                .id("settings-refresh-feed")
                                .cursor_pointer()
                                .px_4()
                                .py_1p5()
                                .rounded(px(16.0))
                                .bg(TidalTheme::bg_surface_active())
                                .border_1()
                                .border_color(TidalTheme::border_default())
                                .hover(|s| {
                                    s.bg(TidalTheme::bg_surface_hover())
                                        .border_color(TidalTheme::accent())
                                })
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(TidalTheme::text_primary())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.refresh_feed(cx);
                                }))
                                .child(if feed_loading {
                                    "Refreshing…"
                                } else {
                                    "↻ Refresh Feed Now"
                                }),
                        ),
                ),
        )
        // ACCOUNT & WEBVIEW SECTION
        .child(
            div()
                .max_w(px(700.0))
                .p_6()
                .rounded(px(12.0))
                .bg(TidalTheme::bg_surface())
                .border_1()
                .border_color(TidalTheme::border_subtle())
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::BOLD)
                        .text_color(TidalTheme::text_primary())
                        .child("YouTube Music Session"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(TidalTheme::text_primary())
                                        .child(if is_signed_in {
                                            format!("Signed in as {account_name}")
                                        } else {
                                            "Not signed in".to_string()
                                        }),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(TidalTheme::text_muted())
                                        .child("Session cookies and settings are stored locally under %LOCALAPPDATA%\\riptide."),
                                ),
                        )
                        .when(!is_signed_in, |row| {
                            row.child(
                                div()
                                    .id("settings-signin")
                                    .cursor_pointer()
                                    .px_4()
                                    .py_1p5()
                                    .rounded(px(8.0))
                                    .bg(TidalTheme::text_primary())
                                    .hover(|s| s.bg(TidalTheme::accent()))
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(TidalTheme::text_inverse())
                                    .on_click(cx.listener(|this, _, _, _| {
                                        this.controller.send(WebCommand::OpenSignIn);
                                    }))
                                    .child("Sign In"),
                            )
                        }),
                ),
        ),
        )
}

fn toggle_row(
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py_2p5()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(TidalTheme::text_primary())
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(TidalTheme::text_muted())
                        .child(subtitle),
                ),
        )
        .child(
            div()
                .id(id)
                .cursor_pointer()
                .flex()
                .items_center()
                .w(px(48.0))
                .h(px(26.0))
                .p(px(3.0))
                .rounded(px(13.0))
                .bg(if enabled {
                    TidalTheme::accent()
                } else {
                    TidalTheme::bg_surface_active()
                })
                .border_1()
                .border_color(if enabled {
                    TidalTheme::accent()
                } else {
                    TidalTheme::border_strong()
                })
                .justify_end()
                .when(!enabled, |el| el.justify_start())
                .on_click(on_click)
                .child(
                    div()
                        .w(px(18.0))
                        .h(px(18.0))
                        .rounded_full()
                        .bg(if enabled {
                            TidalTheme::text_inverse()
                        } else {
                            TidalTheme::text_secondary()
                        }),
                ),
        )
}

fn time_step_btn(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .w(px(32.0))
        .h(px(32.0))
        .rounded(px(6.0))
        .bg(TidalTheme::bg_surface())
        .border_1()
        .border_color(TidalTheme::border_default())
        .hover(|s| {
            s.bg(TidalTheme::bg_surface_hover())
                .border_color(TidalTheme::accent())
                .text_color(TidalTheme::accent())
        })
        .text_base()
        .font_weight(FontWeight::BOLD)
        .text_color(TidalTheme::text_primary())
        .on_click(on_click)
        .child(label)
}

fn time_preset_pill(
    id: &'static str,
    label: &'static str,
    active: bool,
    hour: u8,
    minute: u8,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .px_2p5()
        .py_1()
        .rounded(px(12.0))
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
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(if active {
            TidalTheme::accent()
        } else {
            TidalTheme::text_secondary()
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            this.set_daily_refresh_time(hour, minute, cx);
        }))
        .child(label)
}

fn source_card(
    id: &'static str,
    title: &'static str,
    desc: &'static str,
    active: bool,
    target: FeedArtistSource,
    cx: &mut Context<YtmTidalApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .flex_1()
        .p_3p5()
        .rounded(px(10.0))
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
        .flex()
        .flex_col()
        .gap_1()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.set_feed_source(target, cx);
        }))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(if active {
                    TidalTheme::accent()
                } else {
                    TidalTheme::text_primary()
                })
                .child(title),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(TidalTheme::text_muted())
                .child(desc),
        )
}

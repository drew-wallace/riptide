pub mod content;
pub mod drawer;
pub mod feed_view;
pub mod overlays;
pub mod player_bar;
pub mod settings_view;
pub mod sidebar;
pub mod topbar;

use gpui::{
    AnyElement, AnyView, App, Context, FontWeight, MouseButton, Pixels, SharedString, SharedUri,
    Window, div, img, prelude::*, px,
};

use crate::app::YtmTidalApp;
use crate::protocol::TextLink;
use crate::theme::TidalTheme;

/// Sleek TIDAL-styled hover tooltip view for controls and buttons.
pub struct TidalTooltip {
    label: SharedString,
    shortcut: Option<SharedString>,
}

impl TidalTooltip {
    pub fn new(label: impl Into<SharedString>, shortcut: Option<&'static str>) -> Self {
        Self {
            label: label.into(),
            shortcut: shortcut.map(SharedString::from),
        }
    }
}

impl Render for TidalTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_2p5()
            .py_1()
            .rounded(px(6.0))
            .bg(TidalTheme::bg_surface_active())
            .border_1()
            .border_color(TidalTheme::border_strong())
            .shadow_md()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(TidalTheme::text_primary())
                    .child(self.label.clone()),
            )
            .when_some(self.shortcut.clone(), |el, key| {
                el.child(
                    div()
                        .px_1p5()
                        .py(px(1.0))
                        .rounded(px(4.0))
                        .bg(TidalTheme::bg_sidebar())
                        .border_1()
                        .border_color(TidalTheme::border_default())
                        .text_size(px(10.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(TidalTheme::accent())
                        .child(key),
                )
            })
    }
}

pub fn tidal_tooltip(
    label: impl Into<SharedString>,
    shortcut: Option<&'static str>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label: SharedString = label.into();
    move |_window, cx| cx.new(|_| TidalTooltip::new(label.clone(), shortcut)).into()
}

/// Renders inline artist/album/playlist metadata links with unmistakable underline on hover.
pub fn render_text_links(
    id_prefix: &'static str,
    unique: usize,
    links: &[TextLink],
    fallback: &str,
    font_size: Pixels,
    cx: &mut Context<YtmTidalApp>,
) -> AnyElement {
    let has_clickable = links.iter().any(|l| l.href.is_some());
    if !has_clickable {
        return div()
            .text_size(font_size)
            .text_color(TidalTheme::text_secondary())
            .truncate()
            .child(fallback.to_string())
            .into_any_element();
    }

    div()
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_size(font_size)
        .text_color(TidalTheme::text_secondary())
        .children(links.iter().enumerate().map(|(i, seg)| {
            if let Some(ref href) = seg.href {
                let target = href.clone();
                div()
                    .id((id_prefix, unique * 64 + i))
                    .cursor_pointer()
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(gpui::transparent_black())
                    .hover(|s| {
                        s.text_color(TidalTheme::accent())
                            .border_color(TidalTheme::accent())
                            .underline()
                    })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.navigate_url_from_local(target.clone(), cx);
                    }))
                    .child(seg.text.clone())
                    .into_any_element()
            } else {
                div()
                    .flex_shrink_0()
                    .text_color(TidalTheme::text_muted())
                    .child(seg.text.clone())
                    .into_any_element()
            }
        }))
        .into_any_element()
}

/// Minimalist 4-square geometric Y emblem with exact integer pixel symmetry:
/// Top row: 6px + 10px gap + 6px = 22px; center 6px square sits at exactly x=8..14 (8px on each side).
pub fn tidal_brand_mark() -> impl IntoElement {
    let dot = |color: gpui::Rgba| {
        div()
            .w(px(6.0))
            .h(px(6.0))
            .rounded(px(1.5))
            .bg(color)
    };

    div()
        .flex()
        .flex_col()
        .items_center()
        .w(px(22.0))
        .gap(px(2.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w(px(22.0))
                .child(dot(TidalTheme::text_primary()))
                .child(dot(TidalTheme::text_primary())),
        )
        .child(dot(TidalTheme::accent()))
        .child(dot(TidalTheme::text_primary()))
}

/// Renders remote album/playlist/artist artwork with GPUI's native `img()` loader
/// and a sleek obsidian placeholder when artwork is unavailable.
pub fn artwork_box(url: Option<&str>, size_px: Pixels, circular: bool) -> AnyElement {
    let radius = if circular {
        size_px / 2.0
    } else {
        px(6.0)
    };

    let base = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .w(size_px)
        .h(size_px)
        .rounded(radius)
        .overflow_hidden()
        .bg(TidalTheme::bg_surface_active())
        .border_1()
        .border_color(TidalTheme::border_subtle());

    if let Some(src) = url.filter(|u| !u.is_empty()) {
        base.child(
            img(SharedUri::from(src.to_string()))
                .w(size_px)
                .h(size_px)
                .rounded(radius)
                .object_fit(gpui::ObjectFit::Cover),
        )
        .into_any_element()
    } else {
        base.child(
            div()
                .text_color(TidalTheme::text_muted())
                .text_lg()
                .child("♫"),
        )
        .into_any_element()
    }
}

pub fn explicit_badge() -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .px(px(4.0))
        .h(px(16.0))
        .rounded(px(3.0))
        .bg(TidalTheme::bg_surface_active())
        .border_1()
        .border_color(TidalTheme::border_default())
        .text_color(TidalTheme::text_secondary())
        .text_size(px(9.0))
        .font_weight(FontWeight::BOLD)
        .child("E")
}

pub fn section_eyebrow(label: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_size(px(10.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TidalTheme::text_muted())
        .px_3()
        .mb_1()
        .child(label.into())
}

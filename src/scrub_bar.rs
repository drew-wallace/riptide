use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, ElementId, GlobalElementId, Hitbox, HitboxBehavior,
    Hsla, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Style,
    Window, fill, point, prelude::*, px, relative, size,
};

use crate::theme::TidalTheme;

/// Interactive horizontal progress / volume scrubber supporting click-to-jump,
/// smooth click-and-drag scrubbing, and optional live hover-position reporting.
pub struct ScrubBar {
    id: ElementId,
    value: f32,
    height: Pixels,
    track_height: Pixels,
    fill_color: Hsla,
    track_color: Hsla,
    show_thumb: bool,
    dragging: Arc<AtomicBool>,
    hover_active: Option<Arc<AtomicBool>>,
    on_hover: Option<Rc<dyn Fn(Option<(f32, Pixels)>, &mut Window, &mut App)>>,
    on_change: Rc<dyn Fn(f32, bool, &mut Window, &mut App)>,
}

impl ScrubBar {
    pub fn new(
        id: impl Into<ElementId>,
        value: f32,
        dragging: Arc<AtomicBool>,
        on_change: impl Fn(f32, bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            value: value.clamp(0.0, 1.0),
            height: px(12.0),
            track_height: px(3.0),
            fill_color: TidalTheme::accent().into(),
            track_color: TidalTheme::border_default().into(),
            show_thumb: false,
            dragging,
            hover_active: None,
            on_hover: None,
            on_change: Rc::new(on_change),
        }
    }

    pub fn height(mut self, height: Pixels) -> Self {
        self.height = height;
        self
    }

    pub fn track_height(mut self, height: Pixels) -> Self {
        self.track_height = height;
        self
    }

    pub fn fill_color(mut self, color: impl Into<Hsla>) -> Self {
        self.fill_color = color.into();
        self
    }

    pub fn show_thumb(mut self, show: bool) -> Self {
        self.show_thumb = show;
        self
    }

    pub fn on_hover(
        mut self,
        hover_active: Arc<AtomicBool>,
        handler: impl Fn(Option<(f32, Pixels)>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.hover_active = Some(hover_active);
        self.on_hover = Some(Rc::new(handler));
        self
    }
}

fn fraction_for_x(bounds: Bounds<Pixels>, x: Pixels) -> Option<f32> {
    let width: f32 = bounds.size.width.into();
    if width > 0.0 {
        let dx: f32 = (x - bounds.left()).into();
        Some((dx / width).clamp(0.0, 1.0))
    } else {
        None
    }
}

impl IntoElement for ScrubBar {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ScrubBar {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = self.height.into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        _cx: &mut App,
    ) {
        window.set_cursor_style(CursorStyle::PointingHand, hitbox);

        let is_dragging = self.dragging.load(Ordering::Relaxed);
        let is_hovered = self
            .hover_active
            .as_ref()
            .is_some_and(|h| h.load(Ordering::Relaxed));
        let active_track_h = if is_dragging || is_hovered {
            self.track_height + px(2.0)
        } else {
            self.track_height
        };

        let track_y = bounds.top() + (bounds.size.height - active_track_h) / 2.0;
        let track_bounds = Bounds::new(
            point(bounds.left(), track_y),
            size(bounds.size.width, active_track_h),
        );

        let fill_width = bounds.size.width * self.value;
        let fill_bounds = Bounds::new(
            point(bounds.left(), track_y),
            size(fill_width, active_track_h),
        );

        let mut track_quad = fill(track_bounds, self.track_color);
        track_quad.corner_radii = gpui::Corners::all(active_track_h / 2.0);
        window.paint_quad(track_quad);

        if fill_width > px(0.0) {
            let mut fill_quad = fill(fill_bounds, self.fill_color);
            fill_quad.corner_radii = gpui::Corners::all(active_track_h / 2.0);
            window.paint_quad(fill_quad);
        }

        if self.show_thumb || is_dragging || is_hovered {
            let thumb_size = if is_dragging { px(12.0) } else { px(10.0) };
            let thumb_x = (bounds.left() + fill_width - thumb_size / 2.0)
                .clamp(bounds.left(), bounds.right() - thumb_size);
            let thumb_y = bounds.top() + (bounds.size.height - thumb_size) / 2.0;
            let mut quad = fill(
                Bounds::new(point(thumb_x, thumb_y), size(thumb_size, thumb_size)),
                if is_dragging {
                    TidalTheme::accent()
                } else {
                    TidalTheme::text_primary()
                },
            );
            quad.corner_radii = gpui::Corners::all(thumb_size / 2.0);
            window.paint_quad(quad);
        }

        // MouseDown starts drag + jumps immediately
        {
            let dragging = self.dragging.clone();
            let on_change = self.on_change.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.button == MouseButton::Left
                    && bounds.contains(&event.position)
                {
                    dragging.store(true, Ordering::Relaxed);
                    if let Some(frac) = fraction_for_x(bounds, event.position.x) {
                        on_change(frac, false, window, cx);
                    }
                }
            });
        }

        // MouseMove drives continuous dragging AND live hover-time preview
        {
            let dragging = self.dragging.clone();
            let on_change = self.on_change.clone();
            let hover_active = self.hover_active.clone();
            let on_hover = self.on_hover.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let is_drag = dragging.load(Ordering::Relaxed);
                if is_drag {
                    if event.pressed_button == Some(MouseButton::Left) {
                        if let Some(frac) = fraction_for_x(bounds, event.position.x) {
                            on_change(frac, false, window, cx);
                            if let (Some(h_flag), Some(h_cb)) = (&hover_active, &on_hover) {
                                h_flag.store(true, Ordering::Relaxed);
                                h_cb(Some((frac, event.position.x)), window, cx);
                            }
                        }
                    } else {
                        dragging.store(false, Ordering::Relaxed);
                    }
                    return;
                }

                if let (Some(h_flag), Some(h_cb)) = (&hover_active, &on_hover) {
                    if bounds.contains(&event.position) {
                        if let Some(frac) = fraction_for_x(bounds, event.position.x) {
                            h_flag.store(true, Ordering::Relaxed);
                            h_cb(Some((frac, event.position.x)), window, cx);
                        }
                    } else if h_flag.swap(false, Ordering::Relaxed) {
                        h_cb(None, window, cx);
                    }
                }
            });
        }

        // MouseUp commits final value and ends drag
        {
            let dragging = self.dragging.clone();
            let on_change = self.on_change.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && event.button == MouseButton::Left
                    && dragging.swap(false, Ordering::Relaxed)
                {
                    if let Some(frac) = fraction_for_x(bounds, event.position.x) {
                        on_change(frac, true, window, cx);
                    }
                }
            });
        }
    }
}

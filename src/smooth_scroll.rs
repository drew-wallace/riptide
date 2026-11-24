use std::time::Instant;

use gpui::{Pixels, Point, ScrollDelta, ScrollHandle, ScrollWheelEvent, point, px};

/// Wraps a GPUI `ScrollHandle` with VSync-driven, frame-rate-independent smooth scrolling
/// while preserving instant programmatic scroll jumps (e.g., page navigation & history restore).
pub struct SmoothScroller {
    pub handle: ScrollHandle,
    current_y: f32,
    target_y: f32,
    animating: bool,
    decay_rate: f32,
    last_frame: Option<Instant>,
}

impl SmoothScroller {
    pub fn new() -> Self {
        Self {
            handle: ScrollHandle::new(),
            current_y: 0.0,
            target_y: 0.0,
            animating: false,
            decay_rate: 12.5,
            last_frame: None,
        }
    }

    pub fn offset(&self) -> Point<Pixels> {
        self.handle.offset()
    }

    pub fn max_offset_y(&self) -> f32 {
        self.handle.max_offset().height.into()
    }

    pub fn is_animating(&self) -> bool {
        self.animating
    }

    /// Immediately jumps to `offset` and cancels any in-flight wheel momentum.
    pub fn set_immediate(&mut self, offset: Point<Pixels>) {
        self.animating = false;
        self.last_frame = None;
        let y: f32 = offset.y.into();
        self.current_y = y;
        self.target_y = y;
        self.handle.set_offset(offset);
    }

    /// Accumulates wheel delta and returns `true` when animation should be driven on next frame.
    pub fn on_wheel(&mut self, event: &ScrollWheelEvent) -> bool {
        let (delta_y, rate): (f32, f32) = match event.delta {
            // Standard mouse wheel (Windows sends +/-3.0 lines per notch by default):
            // ~114 px per notch with a crisp, silky ~180ms deceleration glide.
            ScrollDelta::Lines(p) => (p.y * 38.0, 16.5),
            // High-precision touchpad / pixel-wheel: ultra-responsive with light smoothing.
            ScrollDelta::Pixels(p) => (f32::from(p.y) * 1.1, 26.0),
        };
        if delta_y.abs() < 0.01 {
            return false;
        }

        let live_y: f32 = self.handle.offset().y.into();
        if !self.animating {
            self.current_y = live_y;
            self.target_y = live_y;
            self.last_frame = Some(Instant::now());
        } else if (live_y - self.current_y).abs() > 2.0 {
            // Keep in sync if layout clamping shifted the underlying handle
            self.current_y = live_y;
        }

        self.decay_rate = rate;
        let max_y = self.max_offset_y().max(0.0);
        let next_target = (self.target_y + delta_y).clamp(-max_y, 0.0);

        if (next_target - self.current_y).abs() < 0.15 {
            self.target_y = next_target;
            return false;
        }

        self.target_y = next_target;
        self.animating = true;
        true
    }

    /// Advances smooth interpolation using exact elapsed frame delta time (`dt`).
    /// Call from `render()` synchronized with `window.request_animation_frame()`.
    /// Returns `true` while still moving.
    pub fn step(&mut self) -> bool {
        if !self.animating {
            return false;
        }

        let now = Instant::now();
        let dt = self
            .last_frame
            .map(|prev| now.duration_since(prev).as_secs_f32())
            .unwrap_or(1.0 / 120.0)
            .clamp(0.001, 0.05);
        self.last_frame = Some(now);

        let max_y = self.max_offset_y().max(0.0);
        self.target_y = self.target_y.clamp(-max_y, 0.0);

        let diff = self.target_y - self.current_y;
        if diff.abs() < 0.15 {
            self.current_y = self.target_y;
            self.handle.set_offset(point(px(0.0), px(self.target_y)));
            self.animating = false;
            self.last_frame = None;
            false
        } else {
            // Frame-rate-independent exponential smoothing: 1 - e^(-k * dt)
            let alpha = 1.0 - (-self.decay_rate * dt).exp();
            self.current_y += diff * alpha;
            self.handle.set_offset(point(px(0.0), px(self.current_y)));
            true
        }
    }
}

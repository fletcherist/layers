use std::time::{Duration, Instant};

use crate::gpu::InstanceRaw;

// ---------------------------------------------------------------------------
// Toast notification system
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Error,
    #[allow(dead_code)]
    Info,
    #[allow(dead_code)]
    Success,
}

pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub created_at: Instant,
    pub duration: Duration,
    pub id: Option<String>,
    pub persistent: bool,
}

pub struct ToastManager {
    pub toasts: Vec<Toast>,
}

use crate::gpu::TextEntry;

const TOAST_WIDTH: f32 = 320.0;
const TOAST_HEIGHT: f32 = 52.0;
const TOAST_MARGIN: f32 = 16.0;
const TOAST_GAP: f32 = 8.0;
const TOAST_BORDER_RADIUS: f32 = 8.0;
const ACCENT_BAR_WIDTH: f32 = 4.0;
const FADE_OUT_SECS: f32 = 0.5;
const DEFAULT_DURATION: Duration = Duration::from_secs(3);

impl ToastManager {
    pub fn new() -> Self {
        Self { toasts: Vec::new() }
    }

    pub fn push(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.toasts.push(Toast {
            message: message.into(),
            kind,
            created_at: Instant::now(),
            duration: DEFAULT_DURATION,
            id: None,
            persistent: false,
        });
    }

    /// Push a persistent toast that stays until explicitly dismissed via `dismiss_by_id`.
    /// If a toast with the same `id` already exists, it is replaced (message updated).
    pub fn push_persistent(&mut self, id: impl Into<String>, message: impl Into<String>, kind: ToastKind) {
        let id = id.into();
        self.toasts.retain(|t| t.id.as_deref() != Some(&id));
        self.toasts.push(Toast {
            message: message.into(),
            kind,
            created_at: Instant::now(),
            duration: DEFAULT_DURATION,
            id: Some(id),
            persistent: true,
        });
    }

    /// Dismiss a persistent toast by its id. Returns `true` if one was removed.
    pub fn dismiss_by_id(&mut self, id: &str) -> bool {
        let before = self.toasts.len();
        self.toasts.retain(|t| t.id.as_deref() != Some(id));
        self.toasts.len() != before
    }

    /// Remove expired toasts. Returns `true` if any were removed.
    pub fn tick(&mut self) -> bool {
        let before = self.toasts.len();
        self.toasts.retain(|t| t.persistent || t.created_at.elapsed() < t.duration);
        self.toasts.len() != before
    }

    pub fn has_active(&self) -> bool {
        !self.toasts.is_empty()
    }

    /// Build overlay rectangle instances for all active toasts.
    pub fn build_instances(&self, screen_w: f32, screen_h: f32, scale: f32) -> Vec<InstanceRaw> {
        let mut out = Vec::new();
        let w = TOAST_WIDTH * scale;
        let h = TOAST_HEIGHT * scale;
        let margin = TOAST_MARGIN * scale;
        let gap = TOAST_GAP * scale;
        let accent_w = ACCENT_BAR_WIDTH * scale;
        let radius = TOAST_BORDER_RADIUS * scale;

        for (i, toast) in self.toasts.iter().enumerate() {
            let alpha = toast_alpha(toast);
            let x = screen_w - w - margin;
            let y = screen_h - margin - h - (h + gap) * i as f32;

            let (bg_color, accent_color) = match toast.kind {
                ToastKind::Error => (
                    [0.25, 0.10, 0.10, 0.92 * alpha],
                    [0.9, 0.3, 0.3, 1.0 * alpha],
                ),
                ToastKind::Info => (
                    [0.12, 0.14, 0.20, 0.92 * alpha],
                    [0.4, 0.6, 1.0, 1.0 * alpha],
                ),
                ToastKind::Success => (
                    [0.10, 0.22, 0.12, 0.92 * alpha],
                    [0.3, 0.85, 0.4, 1.0 * alpha],
                ),
            };

            // Background
            out.push(InstanceRaw {
                position: [x, y],
                size: [w, h],
                color: bg_color,
                border_radius: radius,
            });

            // Left accent bar
            out.push(InstanceRaw {
                position: [x, y],
                size: [accent_w, h],
                color: accent_color,
                border_radius: radius,
            });
        }

        out
    }

    /// Build text descriptors for all active toasts.
    pub fn build_text_entries(&self, screen_w: f32, screen_h: f32, scale: f32) -> Vec<TextEntry> {
        let mut out = Vec::new();
        let w = TOAST_WIDTH * scale;
        let h = TOAST_HEIGHT * scale;
        let margin = TOAST_MARGIN * scale;
        let gap = TOAST_GAP * scale;
        let accent_w = ACCENT_BAR_WIDTH * scale;

        let font_size = 12.0 * scale;
        let line_height = 16.0 * scale;

        for (i, toast) in self.toasts.iter().enumerate() {
            let alpha = toast_alpha(toast);
            let x = screen_w - w - margin + accent_w + 8.0 * scale;
            let y = screen_h - margin - h - (h + gap) * i as f32;
            let text_y = y + (h - line_height) * 0.5;

            let a = (255.0 * alpha) as u8;
            out.push(TextEntry {
                text: toast.message.clone(),
                x,
                y: text_y,
                font_size,
                line_height,
                max_width: w - accent_w - 16.0 * scale,
                color: [230, 230, 235, a],
                weight: 400,
                bounds: None,
                center: false,
            });
        }

        out
    }
}

fn toast_alpha(toast: &Toast) -> f32 {
    if toast.persistent {
        return 1.0;
    }
    let elapsed = toast.created_at.elapsed().as_secs_f32();
    let total = toast.duration.as_secs_f32();
    let remaining = total - elapsed;
    if remaining <= 0.0 {
        0.0
    } else if remaining < FADE_OUT_SECS {
        remaining / FADE_OUT_SECS
    } else {
        1.0
    }
}

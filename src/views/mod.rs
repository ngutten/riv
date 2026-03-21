pub mod grid;
pub mod list;
pub mod preview;
pub mod single;

use crate::state::{AppState, ZoomMode};

pub const ROW_HEIGHT: f32 = 20.0;
pub const THUMB_PADDING: f32 = 8.0;
pub const GRID_LABEL_HEIGHT: f32 = 18.0;

pub fn thumb_size(state: &AppState) -> f32 {
    state.thumb_size as f32
}

pub fn compute_display_size(state: &AppState, tex_size: egui::Vec2, avail: egui::Vec2) -> egui::Vec2 {
    match state.zoom_mode {
        ZoomMode::FitWindow => {
            let scale = (avail.x / tex_size.x).min(avail.y / tex_size.y).min(1.0);
            tex_size * scale
        }
        ZoomMode::FitWidth => {
            let scale = avail.x / tex_size.x;
            tex_size * scale
        }
        ZoomMode::FitHeight => {
            let scale = avail.y / tex_size.y;
            tex_size * scale
        }
        ZoomMode::Original => tex_size,
        ZoomMode::Custom => tex_size * state.zoom_level,
    }
}

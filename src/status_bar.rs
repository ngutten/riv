use crate::cache::TextureCache;
use crate::dir::DirState;
use crate::state::{AppState, ViewMode};

pub fn draw(ui: &mut egui::Ui, state: &AppState, dir: &DirState, cache: &TextureCache) {
    let height = 22.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );

    ui.painter().rect_filled(
        rect,
        0.0,
        egui::Color32::from_gray(40),
    );

    let abs_idx = state.resolved_index();

    let multi_count = state.multi_select_count();
    let multi_prefix = if multi_count > 0 {
        format!("{} selected | ", multi_count)
    } else {
        String::new()
    };

    // Show status message if recent (< 3s)
    let status_msg = state.status_message.as_ref().and_then(|(msg, when)| {
        if when.elapsed().as_secs_f32() < 3.0 {
            Some(msg.as_str())
        } else {
            None
        }
    });

    let left_text = if let Some(msg) = status_msg {
        format!(" {}", msg)
    } else { match state.view_mode {
        ViewMode::List => {
            if let Some(entry) = dir.entries.get(abs_idx) {
                if entry.is_dir {
                    format!(" {}{}/", multi_prefix, entry.name)
                } else {
                    format!(" {}{}", multi_prefix, entry.name)
                }
            } else {
                " (empty)".to_string()
            }
        }
        ViewMode::Grid => {
            if let Some(entry) = dir.entries.get(abs_idx) {
                format!(" {}{}", multi_prefix, entry.name)
            } else {
                " (empty)".to_string()
            }
        }
        ViewMode::Single => {
            if let Some(entry) = dir.entries.get(abs_idx) {
                format!(" {} | {}", entry.name, state.zoom_mode.label())
            } else {
                String::new()
            }
        }
    } };

    let pending = cache.pending_full.len() + cache.pending_thumb.len();
    let loading_str = if pending > 0 {
        format!("(loading {}...) ", pending)
    } else {
        String::new()
    };

    let right_text = {
        let mode_str = match state.view_mode {
            ViewMode::List => "LIST",
            ViewMode::Grid => "GRID",
            ViewMode::Single => "VIEW",
        };
        let sort_indicator = if state.view_mode != ViewMode::Single {
            let arrow = if state.sort_ascending { "\u{25b2}" } else { "\u{25bc}" };
            format!("{} {} | ", state.sort_field.label(), arrow)
        } else {
            String::new()
        };
        let visible = state.visible_count(dir);
        let pos = if visible == 0 {
            "0/0".to_string()
        } else {
            format!("{}/{}", state.selected_index + 1, visible)
        };
        let counts = if state.filter_active {
            format!("{} matches", visible)
        } else {
            format!("{} images, {} dirs", dir.image_count, dir.dir_count)
        };
        format!("{}{}{} | {} | {} ", loading_str, sort_indicator, counts, pos, mode_str)
    };

    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        &left_text,
        egui::FontId::monospace(12.0),
        egui::Color32::from_gray(200),
    );

    ui.painter().text(
        rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        &right_text,
        egui::FontId::monospace(12.0),
        egui::Color32::from_gray(180),
    );
}

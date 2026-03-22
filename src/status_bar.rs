use crate::cache::TextureCache;
use crate::dir::DirState;
use crate::state::{AppState, CompareSide, ViewMode};

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
        ViewMode::Compare => {
            if let Some(ref cmp) = state.compare {
                let left_name = dir.entries.get(cmp.left_index)
                    .map(|e| e.name.as_str()).unwrap_or("?");
                let right_name = dir.entries.get(cmp.right_index)
                    .map(|e| e.name.as_str()).unwrap_or("?");
                let side = match cmp.active_side {
                    CompareSide::Left => "L",
                    CompareSide::Right => "R",
                };
                let lock = if cmp.locked { " [LOCKED]" } else { "" };
                format!(" Compare: {} | {} (active: {}){}", left_name, right_name, side, lock)
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
            ViewMode::Compare => "COMPARE",
        };
        let sort_indicator = if matches!(state.view_mode, ViewMode::List | ViewMode::Grid) {
            let arrow = if state.sort_ascending { "\u{25b2}" } else { "\u{25bc}" };
            format!("{} {} | ", state.sort_field.label(), arrow)
        } else {
            String::new()
        };
        let dims_str = {
            let idx = if state.view_mode == ViewMode::Compare {
                state.compare.as_ref().map(|c| match c.active_side {
                    CompareSide::Left => c.left_index,
                    CompareSide::Right => c.right_index,
                }).unwrap_or(abs_idx)
            } else {
                abs_idx
            };
            dir.entries.get(idx)
                .and_then(|e| cache.image_dimensions.get(&e.path))
                .map(|(w, h)| format!("{}x{} | ", w, h))
                .unwrap_or_default()
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
        format!("{}{}{}{} | {} | {} ", loading_str, dims_str, sort_indicator, counts, pos, mode_str)
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

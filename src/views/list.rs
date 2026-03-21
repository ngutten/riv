use crate::dir::{format_date, format_size, DirState};
use crate::state::{AppState, EditAction};
use crate::views::ROW_HEIGHT;

fn context_menu_items(ui: &mut egui::Ui, state: &mut AppState, is_dir: bool) {
    let multi = state.multi_select_count() > 0;
    let single_disabled = multi || is_dir;

    if ui.add_enabled(!single_disabled, egui::Button::new("Rename")).clicked() {
        state.pending_edit_action = Some(EditAction::Rename);
        ui.close();
    }
    if ui.add_enabled(!is_dir, egui::Button::new("Copy to...")).clicked() {
        state.pending_edit_action = Some(EditAction::CopyTo);
        ui.close();
    }
    if ui.add_enabled(!single_disabled, egui::Button::new("View EXIF")).clicked() {
        state.pending_edit_action = Some(EditAction::ViewExif);
        ui.close();
    }
    if ui.add_enabled(!single_disabled, egui::Button::new("Rotate Left")).clicked() {
        state.pending_edit_action = Some(EditAction::RotateLeft);
        ui.close();
    }
    if ui.add_enabled(!single_disabled, egui::Button::new("Rotate Right")).clicked() {
        state.pending_edit_action = Some(EditAction::RotateRight);
        ui.close();
    }
    ui.separator();
    if ui.add_enabled(!is_dir, egui::Button::new("Open in GIMP")).clicked() {
        state.pending_edit_action = Some(EditAction::OpenInGimp);
        ui.close();
    }
    if ui.add_enabled(!is_dir, egui::Button::new("Open in Krita")).clicked() {
        state.pending_edit_action = Some(EditAction::OpenInKrita);
        ui.close();
    }
    ui.separator();
    if ui.add_enabled(!is_dir, egui::Button::new("Delete")).clicked() {
        state.pending_edit_action = Some(EditAction::Delete);
        ui.close();
    }
    if ui.button("Copy Path").clicked() {
        state.pending_edit_action = Some(EditAction::CopyPath);
        ui.close();
    }
}

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, dir: &DirState) {
    let visible_count = state.visible_count(dir);
    if visible_count == 0 {
        ui.centered_and_justified(|ui| {
            if state.filter_active {
                ui.label("No matches");
            } else {
                ui.label("No images found in this directory");
            }
        });
        return;
    }

    let mut scroll = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);

    if state.scroll_to_selected {
        let y = state.selected_index as f32 * ROW_HEIGHT;
        scroll = scroll.vertical_scroll_offset(
            (y - ui.available_height() / 2.0 + ROW_HEIGHT / 2.0).max(0.0),
        );
        state.scroll_to_selected = false;
    }

    scroll.show_rows(ui, ROW_HEIGHT, visible_count, |ui, row_range| {
        for visible_idx in row_range {
            let abs_idx = if state.filter_active {
                state.filtered_indices[visible_idx]
            } else {
                visible_idx
            };
            let entry = &dir.entries[abs_idx];
            let selected = visible_idx == state.selected_index;
            let multi_sel = state.is_multi_selected(abs_idx);

            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), egui::Sense::click());

            let painter = ui.painter_at(rect);

            if selected {
                painter.rect_filled(
                    rect,
                    0.0,
                    ui.visuals().selection.bg_fill,
                );
            } else if multi_sel {
                painter.rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(60, 80, 120, 80),
                );
            }

            let text_color = if selected {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().text_color()
            };

            let name_text = if entry.is_dir {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            };

            let galley = painter.layout_no_wrap(
                name_text,
                egui::FontId::monospace(13.0),
                text_color,
            );
            painter.galley(
                rect.left_center() + egui::vec2(4.0, -galley.size().y / 2.0),
                galley,
                text_color,
            );

            if !entry.is_dir {
                let size_str = format_size(entry.size);
                let size_galley = painter.layout_no_wrap(
                    size_str,
                    egui::FontId::monospace(12.0),
                    ui.visuals().weak_text_color(),
                );
                let size_x = rect.right() - size_galley.size().x - 8.0;
                painter.galley(
                    egui::pos2(
                        size_x,
                        rect.center().y - size_galley.size().y / 2.0,
                    ),
                    size_galley,
                    ui.visuals().weak_text_color(),
                );

                let date_str = format_date(entry.modified);
                if !date_str.is_empty() {
                    let date_galley = painter.layout_no_wrap(
                        date_str,
                        egui::FontId::monospace(12.0),
                        ui.visuals().weak_text_color(),
                    );
                    painter.galley(
                        egui::pos2(
                            size_x - date_galley.size().x - 12.0,
                            rect.center().y - date_galley.size().y / 2.0,
                        ),
                        date_galley,
                        ui.visuals().weak_text_color(),
                    );
                }
            }

            // Click handling with multi-select
            if response.clicked() {
                let modifiers = ui.input(|i| i.modifiers);
                if modifiers.command {
                    state.toggle_select(abs_idx);
                } else if modifiers.shift {
                    let anchor = state.last_click_index.unwrap_or(abs_idx);
                    state.select_range(anchor, abs_idx);
                    state.selected_index = visible_idx;
                } else {
                    state.clear_multi_select();
                    state.selected_index = visible_idx;
                    state.preview_focused = false;
                    state.reset_zoom();
                }
                state.last_click_index = Some(abs_idx);
            }

            if response.double_clicked() {
                state.clear_multi_select();
                state.selected_index = visible_idx;
                if entry.is_dir {
                    state.double_click_enter = Some(abs_idx);
                }
            }

            // Context menu
            response.context_menu(|ui| {
                context_menu_items(ui, state, entry.is_dir);
            });
        }
    });
}

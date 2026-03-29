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
    if ui.add_enabled(!single_disabled, egui::Button::new("Metadata")).clicked() {
        state.pending_edit_action = Some(EditAction::ViewMetadata);
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
    if ui.add_enabled(!is_dir, egui::Button::new("Compare...")).clicked() {
        state.pending_edit_action = Some(EditAction::Compare);
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
    // During filter, show old-style unified list (filter searches everything)
    if state.filter_active {
        draw_filtered(ui, state, dir);
        return;
    }

    // Separate dirs and images
    let dir_indices: Vec<usize> = dir.entries.iter().enumerate()
        .filter(|(_, e)| e.is_dir)
        .map(|(i, _)| i)
        .collect();
    let image_indices: Vec<usize> = dir.entries.iter().enumerate()
        .filter(|(_, e)| !e.is_dir)
        .map(|(i, _)| i)
        .collect();

    // Directory panel at top
    if !dir_indices.is_empty() {
        draw_dir_panel(ui, state, &dir_indices, dir);
        ui.add(egui::Separator::default().spacing(2.0));
    }

    // Image panel
    if image_indices.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label("No images found in this directory");
        });
        return;
    }

    draw_image_panel(ui, state, dir, &image_indices);
}

fn draw_dir_panel(
    ui: &mut egui::Ui,
    state: &mut AppState,
    dir_indices: &[usize],
    dir: &DirState,
) {
    let spacing_y = ui.spacing().item_spacing.y;
    let row_pitch = ROW_HEIGHT + spacing_y;
    let max_visible = 8;
    let visible_rows = dir_indices.len().min(max_visible);
    let panel_height = visible_rows as f32 * row_pitch;

    // Cap at 30% of available height
    let panel_height = panel_height.min(ui.available_height() * 0.3);

    let dir_count = dir_indices.len();

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), panel_height),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            let scroll = egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_salt("dir_panel");

            scroll.show_rows(ui, ROW_HEIGHT, dir_count, |ui, row_range| {
                for row in row_range {
                    let abs_idx = dir_indices[row];
                    let entry = &dir.entries[abs_idx];

                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                        egui::Sense::click(),
                    );

                    let painter = ui.painter_at(rect);

                    // Hover highlight
                    if response.hovered() {
                        painter.rect_filled(
                            rect,
                            0.0,
                            egui::Color32::from_rgba_premultiplied(60, 80, 120, 50),
                        );
                    }

                    let text_color = ui.visuals().weak_text_color();
                    let name = format!("{}/", entry.name);
                    let galley = painter.layout_no_wrap(
                        name,
                        egui::FontId::monospace(13.0),
                        text_color,
                    );
                    painter.galley(
                        rect.left_center() + egui::vec2(4.0, -galley.size().y / 2.0),
                        galley,
                        text_color,
                    );

                    // Single click enters directory
                    if response.clicked() || response.double_clicked() {
                        state.double_click_enter = Some(abs_idx);
                    }
                }
            });
        },
    );
}

fn draw_image_panel(
    ui: &mut egui::Ui,
    state: &mut AppState,
    dir: &DirState,
    image_indices: &[usize],
) {
    let image_count = image_indices.len();
    let spacing_y = ui.spacing().item_spacing.y;
    let row_pitch = ROW_HEIGHT + spacing_y;

    // Find selected image's position in image_indices
    let selected_img_pos = image_indices.iter()
        .position(|&abs| abs == state.selected_index);

    let mut scroll = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .id_salt("image_panel");

    if state.scroll_to_selected {
        if let Some(img_pos) = selected_img_pos {
            let y = img_pos as f32 * row_pitch;
            scroll = scroll.vertical_scroll_offset(
                (y - ui.available_height() / 2.0 + row_pitch / 2.0).max(0.0),
            );
        }
        state.scroll_to_selected = false;
    }

    scroll.show_rows(ui, ROW_HEIGHT, image_count, |ui, row_range| {
        for img_idx in row_range {
            let abs_idx = image_indices[img_idx];
            let entry = &dir.entries[abs_idx];
            let selected = abs_idx == state.selected_index;
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

            let galley = painter.layout_no_wrap(
                entry.name.clone(),
                egui::FontId::monospace(13.0),
                text_color,
            );
            painter.galley(
                rect.left_center() + egui::vec2(4.0, -galley.size().y / 2.0),
                galley,
                text_color,
            );

            // File size
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

            // Date
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

            // Click handling with multi-select
            if response.clicked() {
                let modifiers = ui.input(|i| i.modifiers);
                if modifiers.command {
                    state.toggle_select(abs_idx);
                } else if modifiers.shift {
                    let anchor = state.last_click_index.unwrap_or(abs_idx);
                    state.select_range(anchor, abs_idx);
                    state.selected_index = abs_idx;
                } else {
                    state.clear_multi_select();
                    state.selected_index = abs_idx;
                    state.preview_focused = false;
                    state.reset_zoom();
                }
                state.last_click_index = Some(abs_idx);
            }

            if response.double_clicked() {
                state.clear_multi_select();
                state.selected_index = abs_idx;
                // Double-click an image = enter single view
                state.view_mode = crate::state::ViewMode::Single;
                state.previous_browse_mode = crate::state::BrowseMode::List;
                state.reset_zoom();
            }

            // Context menu
            response.context_menu(|ui| {
                context_menu_items(ui, state, false);
            });
        }
    });
}

/// Filtered mode: shows all matching entries (dirs + images) in a unified list.
/// Used when the filter bar is active.
fn draw_filtered(ui: &mut egui::Ui, state: &mut AppState, dir: &DirState) {
    let visible_count = state.visible_count(dir);
    if visible_count == 0 {
        ui.centered_and_justified(|ui| {
            ui.label("No matches");
        });
        return;
    }

    let spacing_y = ui.spacing().item_spacing.y;
    let row_pitch = ROW_HEIGHT + spacing_y;

    let mut scroll = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);

    if state.scroll_to_selected {
        let y = state.selected_index as f32 * row_pitch;
        scroll = scroll.vertical_scroll_offset(
            (y - ui.available_height() / 2.0 + row_pitch / 2.0).max(0.0),
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

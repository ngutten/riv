use crate::cache::TextureCache;
use crate::decode::DecodePipeline;
use crate::dir::DirState;
use crate::state::{AppState, EditAction};
use crate::views::{GRID_LABEL_HEIGHT, THUMB_PADDING, thumb_size};

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

pub fn draw(
    ui: &mut egui::Ui,
    state: &mut AppState,
    dir: &DirState,
    cache: &mut TextureCache,
    pipeline: &DecodePipeline,
) {
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

    let avail_width = ui.available_width();
    let ts = thumb_size(state);
    let cell_width = ts + THUMB_PADDING;
    let columns = ((avail_width / cell_width).floor() as usize).max(1);
    let row_height = ts + GRID_LABEL_HEIGHT + THUMB_PADDING * 2.0;
    let total_rows = (visible_count + columns - 1) / columns;

    let spacing_y = ui.spacing().item_spacing.y;
    let row_pitch = row_height + spacing_y;

    let mut scroll = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);

    if state.scroll_to_selected {
        let selected_row = state.selected_index / columns;
        let y = selected_row as f32 * row_pitch;
        scroll = scroll.vertical_scroll_offset(
            (y - ui.available_height() / 2.0 + row_pitch / 2.0).max(0.0),
        );
        state.scroll_to_selected = false;
    }

    scroll.show_rows(ui, row_height, total_rows, |ui, row_range| {
        for row in row_range {
            ui.horizontal(|ui| {
                for col in 0..columns {
                    let visible_idx = row * columns + col;
                    if visible_idx >= visible_count {
                        break;
                    }
                    let abs_idx = if state.filter_active {
                        state.filtered_indices[visible_idx]
                    } else {
                        visible_idx
                    };
                    let entry = &dir.entries[abs_idx];
                    let selected = visible_idx == state.selected_index;
                    let multi_sel = state.is_multi_selected(abs_idx);

                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(cell_width, row_height),
                        egui::Sense::click(),
                    );

                    if selected {
                        ui.painter().rect_filled(
                            rect,
                            4.0,
                            ui.visuals().selection.bg_fill,
                        );
                    } else if multi_sel {
                        ui.painter().rect_filled(
                            rect,
                            4.0,
                            egui::Color32::from_rgba_premultiplied(60, 80, 120, 80),
                        );
                    }

                    // Draw thumbnail or placeholder
                    let thumb_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(THUMB_PADDING, THUMB_PADDING),
                        egui::vec2(ts, ts),
                    );

                    if entry.is_dir {
                        let stroke = egui::Stroke::new(1.0, ui.visuals().weak_text_color());
                        ui.painter().rect_stroke(
                            thumb_rect.shrink(20.0),
                            4.0,
                            stroke,
                            egui::StrokeKind::Outside,
                        );
                        ui.painter().text(
                            thumb_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "DIR",
                            egui::FontId::proportional(14.0),
                            ui.visuals().weak_text_color(),
                        );
                    } else if let Some(tex) = cache.get_thumb(&entry.path) {
                        let tex_size = tex.size_vec2();
                        let scale = (ts / tex_size.x).min(ts / tex_size.y);
                        let display_size = tex_size * scale;
                        let centered = egui::Rect::from_center_size(
                            thumb_rect.center(),
                            display_size,
                        );
                        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                        ui.painter().image(tex.id(), centered, uv, egui::Color32::WHITE);
                    } else {
                        // Request thumbnail decode
                        if !cache.is_pending(&entry.path, true) {
                            cache.mark_pending(entry.path.clone(), true);
                            pipeline.request(
                                entry.path.clone(),
                                true,
                                cache.generation,
                                entry.zip_source.clone(),
                            );
                        }
                        // Placeholder
                        let stroke = egui::Stroke::new(1.0, ui.visuals().weak_text_color());
                        ui.painter().rect_stroke(
                            thumb_rect.shrink(2.0),
                            2.0,
                            stroke,
                            egui::StrokeKind::Outside,
                        );
                    }

                    // Filename label
                    let label_text = if entry.name.len() > 18 {
                        format!("{}...", &entry.name[..15])
                    } else {
                        entry.name.clone()
                    };
                    let label_pos = egui::pos2(
                        rect.min.x + THUMB_PADDING,
                        rect.max.y - GRID_LABEL_HEIGHT - THUMB_PADDING / 2.0,
                    );
                    ui.painter().text(
                        label_pos,
                        egui::Align2::LEFT_TOP,
                        label_text,
                        egui::FontId::proportional(11.0),
                        if selected {
                            ui.visuals().strong_text_color()
                        } else {
                            ui.visuals().text_color()
                        },
                    );

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
    });

    // Store columns for input navigation
    ui.memory_mut(|mem| {
        mem.data.insert_temp(egui::Id::new("grid_columns"), columns);
    });
}

use crate::dir::DirState;
use crate::state::{AppState, BrowseMode, EditAction, ViewMode, ZoomMode};

pub struct InputAction {
    pub enter_directory: Option<usize>,
    pub go_up: bool,
    pub quit: bool,
    pub focus_path_bar: bool,
    pub sort_changed: bool,
    pub start_filter: bool,
}

pub fn handle_input(
    ctx: &egui::Context,
    state: &mut AppState,
    dir: &DirState,
) -> InputAction {
    let mut action = InputAction {
        enter_directory: None,
        go_up: false,
        quit: false,
        focus_path_bar: false,
        sort_changed: false,
        start_filter: false,
    };

    // When a dialog is open, only handle Escape to close it
    if state.dialog.is_some() {
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                state.dialog = None;
            }
        });
        return action;
    }

    // When path bar is focused, only handle Escape (consume it so TextEdit doesn't get it)
    if state.path_bar_focused {
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                state.path_bar_focused = false;
                state.sync_path_bar();
            }
        });
        return action;
    }

    // When filter is active, handle filter-specific keys
    if state.filter_active {
        let total = state.visible_count(dir);
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                state.clear_filter();
                state.scroll_to_selected = true;
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                let abs = state.resolved_index();
                state.clear_filter();
                if let Some(entry) = dir.entries.get(abs) {
                    if entry.is_dir {
                        action.enter_directory = Some(abs);
                    } else {
                        state.previous_browse_mode = if state.view_mode == ViewMode::List {
                            BrowseMode::List
                        } else {
                            BrowseMode::Grid
                        };
                        state.view_mode = ViewMode::Single;
                        state.reset_zoom();
                    }
                }
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                if total > 0 && state.selected_index + 1 < total {
                    state.selected_index += 1;
                    state.scroll_to_selected = true;
                    state.reset_zoom();
                }
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                if state.selected_index > 0 {
                    state.selected_index -= 1;
                    state.scroll_to_selected = true;
                    state.reset_zoom();
                }
            }
        });
        return action;
    }

    // Read grid columns OUTSIDE the ctx.input() closure to avoid nested RwLock acquisition.
    // epaint's debug RwLock uses std::sync::RwLock which doesn't support recursive reads on Linux.
    let grid_columns: usize = if state.view_mode == ViewMode::Grid {
        ctx.memory(|mem| mem.data.get_temp(egui::Id::new("grid_columns")).unwrap_or(4))
    } else {
        4
    };

    let total = state.visible_count(dir);
    if total == 0 && state.view_mode != ViewMode::Single {
        // Only handle quit and go_up in empty dirs
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Q) {
                action.quit = true;
            }
            if i.key_pressed(egui::Key::Backspace) {
                action.go_up = true;
            }
            if i.modifiers.command && i.key_pressed(egui::Key::L) {
                action.focus_path_bar = true;
            }
            if i.key_pressed(egui::Key::Slash) {
                action.start_filter = true;
            }
        });
        return action;
    }

    ctx.input(|i| {
        if i.key_pressed(egui::Key::Q) && state.view_mode != ViewMode::Single {
            action.quit = true;
            return;
        }

        // Ctrl+L focuses path bar; / starts filter (in browse modes)
        if state.view_mode != ViewMode::Single {
            if i.modifiers.command && i.key_pressed(egui::Key::L) {
                action.focus_path_bar = true;
                return;
            }
            if i.key_pressed(egui::Key::Slash) {
                action.start_filter = true;
                return;
            }
        }

        match state.view_mode {
            ViewMode::List | ViewMode::Grid if state.preview_focused => {
                // Preview panel is focused — image navigation + zoom
                let pointer_in_preview = state.preview_rect
                    .and_then(|r| i.pointer.latest_pos().map(|p| r.contains(p)))
                    .unwrap_or(false);

                if i.key_pressed(egui::Key::Escape) {
                    state.preview_focused = false;
                } else if i.key_pressed(egui::Key::ArrowRight)
                    || i.key_pressed(egui::Key::L)
                    || i.key_pressed(egui::Key::Space)
                {
                    navigate_single(state, dir, 1);
                    state.scroll_to_selected = true;
                } else if i.key_pressed(egui::Key::ArrowLeft)
                    || i.key_pressed(egui::Key::H)
                {
                    navigate_single(state, dir, -1);
                    state.scroll_to_selected = true;
                } else if i.key_pressed(egui::Key::Enter) {
                    // Enter fullscreen single view
                    let abs = state.resolved_index();
                    if let Some(entry) = dir.entries.get(abs) {
                        if !entry.is_dir {
                            state.previous_browse_mode = if state.view_mode == ViewMode::List {
                                BrowseMode::List
                            } else {
                                BrowseMode::Grid
                            };
                            state.view_mode = ViewMode::Single;
                            state.reset_zoom();
                        }
                    }
                } else if i.key_pressed(egui::Key::I) {
                    state.show_info_overlay = !state.show_info_overlay;
                } else if i.key_pressed(egui::Key::E) {
                    state.show_exif_overlay = !state.show_exif_overlay;
                } else if i.key_pressed(egui::Key::G) && !i.modifiers.shift {
                    state.pending_edit_action = Some(EditAction::OpenInGimp);
                } else if i.key_pressed(egui::Key::Y) {
                    state.pending_edit_action = Some(EditAction::CopyPath);
                } else if i.key_pressed(egui::Key::Delete) {
                    state.pending_edit_action = Some(EditAction::Delete);
                } else if i.key_pressed(egui::Key::F) {
                    state.zoom_mode = state.zoom_mode.cycle();
                    state.pan_offset = egui::Vec2::ZERO;
                } else if i.key_pressed(egui::Key::Num0) {
                    state.reset_zoom();
                } else {
                    let scroll_delta = i.smooth_scroll_delta.y;
                    if scroll_delta != 0.0 && pointer_in_preview {
                        let factor = if scroll_delta > 0.0 { 1.1 } else { 1.0 / 1.1 };
                        state.zoom_mode = ZoomMode::Custom;
                        state.zoom_level *= factor;
                        state.zoom_level = state.zoom_level.clamp(0.05, 50.0);
                    }
                    if i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus) {
                        state.zoom_mode = ZoomMode::Custom;
                        state.zoom_level *= 1.25;
                        state.zoom_level = state.zoom_level.clamp(0.05, 50.0);
                    }
                    if i.key_pressed(egui::Key::Minus) {
                        state.zoom_mode = ZoomMode::Custom;
                        state.zoom_level /= 1.25;
                        state.zoom_level = state.zoom_level.clamp(0.05, 50.0);
                    }
                    // Pan via drag
                    if i.pointer.any_down() {
                        if let Some(pos) = i.pointer.latest_pos() {
                            if let Some(last) = state.last_drag_pos {
                                let delta = pos - last;
                                state.pan_offset += delta;
                            }
                            state.last_drag_pos = Some(pos);
                            state.is_dragging = true;
                        }
                    } else {
                        state.is_dragging = false;
                        state.last_drag_pos = None;
                    }
                }
                // Tab still switches list/grid even when preview focused
                if i.key_pressed(egui::Key::Tab) {
                    if state.view_mode == ViewMode::List {
                        state.view_mode = ViewMode::Grid;
                    } else {
                        state.view_mode = ViewMode::List;
                    }
                    state.scroll_to_selected = true;
                }
            }
            ViewMode::List => {
                let prev_index = state.selected_index;
                if i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J) {
                    if state.selected_index + 1 < total {
                        state.selected_index += 1;
                    }
                }
                if i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K) {
                    if state.selected_index > 0 {
                        state.selected_index -= 1;
                    }
                }
                if i.key_pressed(egui::Key::Space) {
                    if state.selected_index + 1 < total {
                        state.selected_index += 1;
                    }
                }
                if i.key_pressed(egui::Key::Home) {
                    state.selected_index = 0;
                }
                if i.key_pressed(egui::Key::End) {
                    state.selected_index = total.saturating_sub(1);
                }
                if i.key_pressed(egui::Key::PageDown) {
                    state.selected_index = (state.selected_index + 30).min(total - 1);
                }
                if i.key_pressed(egui::Key::PageUp) {
                    state.selected_index = state.selected_index.saturating_sub(30);
                }
                // Mouse wheel scrolling
                let scroll_delta = i.smooth_scroll_delta.y;
                if scroll_delta != 0.0 {
                    let pointer_in_preview = state.preview_rect
                        .and_then(|r| i.pointer.latest_pos().map(|p| r.contains(p)))
                        .unwrap_or(false);
                    if !pointer_in_preview {
                        let lines = (scroll_delta / 30.0).round() as i32;
                        let new_idx = (state.selected_index as i32 - lines)
                            .clamp(0, total as i32 - 1) as usize;
                        if new_idx != state.selected_index {
                            state.selected_index = new_idx;
                            state.scroll_to_selected = true;
                            state.reset_zoom();
                            state.clear_multi_select();
                        }
                    }
                }
                if i.key_pressed(egui::Key::Enter) {
                    let abs = state.resolved_index();
                    if let Some(entry) = dir.entries.get(abs) {
                        if entry.is_dir {
                            action.enter_directory = Some(abs);
                        } else {
                            state.previous_browse_mode = BrowseMode::List;
                            state.view_mode = ViewMode::Single;
                            state.reset_zoom();
                        }
                    }
                }
                if i.key_pressed(egui::Key::Tab) {
                    state.view_mode = ViewMode::Grid;
                    state.scroll_to_selected = true;
                }
                if i.key_pressed(egui::Key::Backspace) {
                    action.go_up = true;
                }
                // Sort keybinds
                if i.key_pressed(egui::Key::S) {
                    if i.modifiers.shift {
                        state.sort_ascending = !state.sort_ascending;
                    } else {
                        state.sort_field = state.sort_field.cycle();
                    }
                    action.sort_changed = true;
                }
                // Zoom keys for preview panel
                if i.key_pressed(egui::Key::F) {
                    state.zoom_mode = state.zoom_mode.cycle();
                    state.pan_offset = egui::Vec2::ZERO;
                }
                if i.key_pressed(egui::Key::Num0) {
                    state.reset_zoom();
                }
                if i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus) {
                    state.zoom_mode = ZoomMode::Custom;
                    state.zoom_level *= 1.25;
                    state.zoom_level = state.zoom_level.clamp(0.05, 50.0);
                }
                if i.key_pressed(egui::Key::Minus) {
                    state.zoom_mode = ZoomMode::Custom;
                    state.zoom_level /= 1.25;
                    state.zoom_level = state.zoom_level.clamp(0.05, 50.0);
                }
                // Edit action shortcuts
                if i.key_pressed(egui::Key::E) {
                    state.show_exif_overlay = !state.show_exif_overlay;
                }
                if i.key_pressed(egui::Key::G) && !i.modifiers.shift {
                    state.pending_edit_action = Some(EditAction::OpenInGimp);
                }
                if i.key_pressed(egui::Key::Y) {
                    state.pending_edit_action = Some(EditAction::CopyPath);
                }
                if i.key_pressed(egui::Key::Delete) {
                    state.pending_edit_action = Some(EditAction::Delete);
                }
                if i.key_pressed(egui::Key::F2) {
                    state.pending_edit_action = Some(EditAction::Rename);
                }
                // Reset preview zoom and auto-scroll when selection changes
                if state.selected_index != prev_index {
                    state.reset_zoom();
                    state.scroll_to_selected = true;
                    state.clear_multi_select();
                }
            }
            ViewMode::Grid => {
                let prev_index = state.selected_index;
                let columns = grid_columns;

                if i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J) {
                    let next = state.selected_index + columns;
                    if next < total {
                        state.selected_index = next;
                    }
                }
                if i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K) {
                    if state.selected_index >= columns {
                        state.selected_index -= columns;
                    }
                }
                if i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::L) {
                    if state.selected_index + 1 < total {
                        state.selected_index += 1;
                    }
                }
                if i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::H) {
                    if state.selected_index > 0 {
                        state.selected_index -= 1;
                    }
                }
                if i.key_pressed(egui::Key::Space) {
                    if state.selected_index + 1 < total {
                        state.selected_index += 1;
                    }
                }
                if i.key_pressed(egui::Key::Home) {
                    state.selected_index = 0;
                }
                if i.key_pressed(egui::Key::End) {
                    state.selected_index = total.saturating_sub(1);
                }
                if i.key_pressed(egui::Key::PageDown) {
                    state.selected_index = (state.selected_index + columns * 5).min(total - 1);
                }
                if i.key_pressed(egui::Key::PageUp) {
                    state.selected_index = state.selected_index.saturating_sub(columns * 5);
                }
                // Mouse wheel scrolling in grid
                let scroll_delta = i.smooth_scroll_delta.y;
                if scroll_delta != 0.0 {
                    let pointer_in_preview = state.preview_rect
                        .and_then(|r| i.pointer.latest_pos().map(|p| r.contains(p)))
                        .unwrap_or(false);
                    if !pointer_in_preview {
                        let rows = (scroll_delta / 50.0).round() as i32;
                        let delta = rows * columns as i32;
                        let new_idx = (state.selected_index as i32 - delta)
                            .clamp(0, total as i32 - 1) as usize;
                        if new_idx != state.selected_index {
                            state.selected_index = new_idx;
                            state.scroll_to_selected = true;
                            state.reset_zoom();
                            state.clear_multi_select();
                        }
                    }
                }
                if i.key_pressed(egui::Key::Enter) {
                    let abs = state.resolved_index();
                    if let Some(entry) = dir.entries.get(abs) {
                        if entry.is_dir {
                            action.enter_directory = Some(abs);
                        } else {
                            state.previous_browse_mode = BrowseMode::Grid;
                            state.view_mode = ViewMode::Single;
                            state.reset_zoom();
                        }
                    }
                }
                if i.key_pressed(egui::Key::Tab) {
                    state.view_mode = ViewMode::List;
                    state.scroll_to_selected = true;
                }
                if i.key_pressed(egui::Key::Backspace) {
                    action.go_up = true;
                }
                // Sort keybinds
                if i.key_pressed(egui::Key::S) {
                    if i.modifiers.shift {
                        state.sort_ascending = !state.sort_ascending;
                    } else {
                        state.sort_field = state.sort_field.cycle();
                    }
                    action.sort_changed = true;
                }
                // Zoom keys for preview panel
                if i.key_pressed(egui::Key::F) {
                    state.zoom_mode = state.zoom_mode.cycle();
                    state.pan_offset = egui::Vec2::ZERO;
                }
                if i.key_pressed(egui::Key::Num0) {
                    state.reset_zoom();
                }
                if i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus) {
                    state.zoom_mode = ZoomMode::Custom;
                    state.zoom_level *= 1.25;
                    state.zoom_level = state.zoom_level.clamp(0.05, 50.0);
                }
                if i.key_pressed(egui::Key::Minus) {
                    state.zoom_mode = ZoomMode::Custom;
                    state.zoom_level /= 1.25;
                    state.zoom_level = state.zoom_level.clamp(0.05, 50.0);
                }
                // Edit action shortcuts
                if i.key_pressed(egui::Key::E) {
                    state.show_exif_overlay = !state.show_exif_overlay;
                }
                if i.key_pressed(egui::Key::G) && !i.modifiers.shift {
                    state.pending_edit_action = Some(EditAction::OpenInGimp);
                }
                if i.key_pressed(egui::Key::Y) {
                    state.pending_edit_action = Some(EditAction::CopyPath);
                }
                if i.key_pressed(egui::Key::Delete) {
                    state.pending_edit_action = Some(EditAction::Delete);
                }
                if i.key_pressed(egui::Key::F2) {
                    state.pending_edit_action = Some(EditAction::Rename);
                }
                // Reset preview zoom and auto-scroll when selection changes
                if state.selected_index != prev_index {
                    state.reset_zoom();
                    state.scroll_to_selected = true;
                    state.clear_multi_select();
                }
            }
            ViewMode::Single => {
                if i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Q) {
                    state.view_mode = match state.previous_browse_mode {
                        BrowseMode::List => ViewMode::List,
                        BrowseMode::Grid => ViewMode::Grid,
                    };
                    state.scroll_to_selected = true;
                }
                if i.key_pressed(egui::Key::I) {
                    state.show_info_overlay = !state.show_info_overlay;
                }
                // Edit action shortcuts
                if i.key_pressed(egui::Key::E) {
                    state.show_exif_overlay = !state.show_exif_overlay;
                }
                if i.key_pressed(egui::Key::G) && !i.modifiers.shift {
                    state.pending_edit_action = Some(EditAction::OpenInGimp);
                }
                if i.key_pressed(egui::Key::Y) {
                    state.pending_edit_action = Some(EditAction::CopyPath);
                }
                if i.key_pressed(egui::Key::Delete) {
                    state.pending_edit_action = Some(EditAction::Delete);
                }
                // Space toggles play/pause for animated GIFs, otherwise navigates
                if i.key_pressed(egui::Key::Space) {
                    // Check if current image is a GIF
                    let abs = state.resolved_index();
                    let is_gif = dir.entries.get(abs)
                        .map(|e| {
                            e.path.extension()
                                .and_then(|ext| ext.to_str())
                                .map(|ext| ext.eq_ignore_ascii_case("gif"))
                                .unwrap_or(false)
                        })
                        .unwrap_or(false);
                    if is_gif {
                        state.animation_playing = !state.animation_playing;
                    } else {
                        navigate_single(state, dir, 1);
                    }
                } else if i.key_pressed(egui::Key::ArrowRight)
                    || i.key_pressed(egui::Key::L)
                {
                    navigate_single(state, dir, 1);
                }
                if i.key_pressed(egui::Key::ArrowLeft)
                    || i.key_pressed(egui::Key::H)
                {
                    navigate_single(state, dir, -1);
                }
                if i.key_pressed(egui::Key::Home) {
                    // Go to first image
                    for (idx, e) in dir.entries.iter().enumerate() {
                        if !e.is_dir {
                            state.selected_index = idx;
                            state.reset_zoom();
                            break;
                        }
                    }
                }
                if i.key_pressed(egui::Key::End) {
                    // Go to last image
                    for (idx, e) in dir.entries.iter().enumerate().rev() {
                        if !e.is_dir {
                            state.selected_index = idx;
                            state.reset_zoom();
                            break;
                        }
                    }
                }
                // Zoom
                if i.key_pressed(egui::Key::F) {
                    state.zoom_mode = state.zoom_mode.cycle();
                    state.pan_offset = egui::Vec2::ZERO;
                }
                if i.key_pressed(egui::Key::Num0) {
                    state.reset_zoom();
                }
                let scroll_delta = i.smooth_scroll_delta.y;
                if scroll_delta != 0.0 {
                    let factor = if scroll_delta > 0.0 { 1.1 } else { 1.0 / 1.1 };
                    state.zoom_mode = ZoomMode::Custom;
                    if state.zoom_level == 1.0 {
                        // Initialize from current effective size
                    }
                    state.zoom_level *= factor;
                    state.zoom_level = state.zoom_level.clamp(0.05, 50.0);
                }
                if i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus) {
                    state.zoom_mode = ZoomMode::Custom;
                    state.zoom_level *= 1.25;
                    state.zoom_level = state.zoom_level.clamp(0.05, 50.0);
                }
                if i.key_pressed(egui::Key::Minus) {
                    state.zoom_mode = ZoomMode::Custom;
                    state.zoom_level /= 1.25;
                    state.zoom_level = state.zoom_level.clamp(0.05, 50.0);
                }
                // Pan via drag
                if i.pointer.any_down() {
                    if let Some(pos) = i.pointer.latest_pos() {
                        if let Some(last) = state.last_drag_pos {
                            let delta = pos - last;
                            state.pan_offset += delta;
                        }
                        state.last_drag_pos = Some(pos);
                        state.is_dragging = true;
                    }
                } else {
                    state.is_dragging = false;
                    state.last_drag_pos = None;
                }
            }
        }
    });

    action
}

fn navigate_single(state: &mut AppState, dir: &DirState, direction: i32) {
    let mut idx = state.selected_index as i32;
    loop {
        idx += direction;
        if idx < 0 || idx >= dir.entries.len() as i32 {
            return; // No more images in this direction
        }
        if !dir.entries[idx as usize].is_dir {
            state.selected_index = idx as usize;
            state.reset_zoom();
            // Reset animation state when navigating
            state.animation_frame = 0;
            state.animation_elapsed = 0.0;
            state.animation_playing = true;
            return;
        }
    }
}

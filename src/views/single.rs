use crate::cache::TextureCache;
use crate::decode::DecodePipeline;
use crate::dir::{format_size, DirState};
use crate::state::AppState;
use crate::views::compute_display_size;

pub fn draw(
    ui: &mut egui::Ui,
    state: &mut AppState,
    dir: &DirState,
    cache: &mut TextureCache,
    pipeline: &DecodePipeline,
) {
    if state.selected_index >= dir.entries.len() {
        return;
    }
    let entry = &dir.entries[state.selected_index];
    if entry.is_dir {
        return;
    }

    let avail = ui.available_size();
    let dt = ui.input(|i| i.unstable_dt);

    // Check for animated GIF
    let is_animated = cache.get_animated(&entry.path).is_some();

    if is_animated {
        let anim = cache.get_animated(&entry.path).unwrap();
        let frame_count = anim.frames.len();
        if frame_count > 0 {
            let frame_idx = state.animation_frame.min(frame_count - 1);
            let tex = &anim.frames[frame_idx];
            let tex_size = tex.size_vec2();
            let display_size = compute_display_size(state, tex_size, avail);

            let (alloc_rect, _) = ui.allocate_exact_size(avail, egui::Sense::click_and_drag());
            let center = alloc_rect.center() + state.pan_offset;
            let rect = egui::Rect::from_center_size(center, display_size);
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);

            // Advance animation
            if state.animation_playing && frame_count > 1 {
                state.animation_elapsed += dt;
                let delay = anim.delays[frame_idx].as_secs_f32();
                if state.animation_elapsed >= delay {
                    state.animation_elapsed -= delay;
                    state.animation_frame = (frame_idx + 1) % frame_count;
                }
                ui.ctx().request_repaint();
            }

            // Info overlay
            if state.show_info_overlay {
                let dims = cache.image_dimensions.get(&entry.path);
                draw_info_overlay(ui, alloc_rect, entry, dims, state, Some((frame_idx, frame_count)));
            }
        }
    } else if let Some(tex) = cache.get_full(&entry.path) {
        let tex_size = tex.size_vec2();
        let display_size = compute_display_size(state, tex_size, avail);

        let (alloc_rect, _) = ui.allocate_exact_size(avail, egui::Sense::click_and_drag());
        let center = alloc_rect.center() + state.pan_offset;
        let rect = egui::Rect::from_center_size(center, display_size);
        let painter = ui.painter();
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(tex.id(), rect, uv, egui::Color32::WHITE);

        // Info overlay
        if state.show_info_overlay {
            let dims = cache.image_dimensions.get(&entry.path);
            draw_info_overlay(ui, alloc_rect, entry, dims, state, None);
        }
    } else {
        // Request full decode
        if !cache.is_pending(&entry.path, false) {
            cache.mark_pending(entry.path.clone(), false);
            pipeline.request(entry.path.clone(), false, cache.generation, entry.zip_source.clone());
        }
        ui.centered_and_justified(|ui| {
            ui.spinner();
        });
    }
}

fn draw_info_overlay(
    ui: &mut egui::Ui,
    alloc_rect: egui::Rect,
    entry: &crate::dir::DirEntry,
    dims: Option<&(u32, u32)>,
    state: &AppState,
    anim_info: Option<(usize, usize)>,
) {
    let painter = ui.painter();

    let mut lines = Vec::new();
    lines.push(entry.name.clone());

    if let Some((w, h)) = dims {
        lines.push(format!("{}x{}", w, h));
    }

    lines.push(format_size(entry.size));

    if let Some(ext) = entry.path.extension().and_then(|e| e.to_str()) {
        lines.push(ext.to_uppercase());
    }

    lines.push(format!("Zoom: {}", state.zoom_mode.label()));

    if let Some((frame, total)) = anim_info {
        let status = if state.animation_playing { "Playing" } else { "Paused" };
        lines.push(format!("Frame {}/{} ({})", frame + 1, total, status));
    }

    let font = egui::FontId::monospace(12.0);
    let text_color = egui::Color32::from_gray(220);
    let line_height = 16.0;
    let padding = 8.0;
    let panel_width = 220.0;
    let panel_height = lines.len() as f32 * line_height + padding * 2.0;

    let panel_rect = egui::Rect::from_min_size(
        alloc_rect.min + egui::vec2(10.0, 10.0),
        egui::vec2(panel_width, panel_height),
    );

    painter.rect_filled(panel_rect, 4.0, egui::Color32::from_black_alpha(180));

    for (i, line) in lines.iter().enumerate() {
        let pos = panel_rect.min + egui::vec2(padding, padding + i as f32 * line_height);
        painter.text(pos, egui::Align2::LEFT_TOP, line, font.clone(), text_color);
    }
}

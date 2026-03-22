use crate::cache::TextureCache;
use crate::decode::DecodePipeline;
use crate::dir::DirState;
use crate::state::{CompareSide, CompareState, ZoomPan};
use crate::views::compute_display_size_with;

fn draw_side(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    index: usize,
    zoom: &ZoomPan,
    dir: &DirState,
    cache: &mut TextureCache,
    pipeline: &DecodePipeline,
    is_active: bool,
) {
    let painter = ui.painter_at(rect);

    // Active side border
    if is_active {
        let accent = egui::Color32::from_rgb(80, 130, 200);
        painter.rect_stroke(rect, 0.0, egui::Stroke::new(2.0, accent), egui::StrokeKind::Inside);
    }

    let entry = match dir.entries.get(index) {
        Some(e) if !e.is_dir => e,
        _ => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No image",
                egui::FontId::proportional(14.0),
                egui::Color32::from_gray(120),
            );
            return;
        }
    };

    // Label at bottom
    let label_height = 20.0;
    let image_rect = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.max.x, rect.max.y - label_height),
    );
    let label_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x, rect.max.y - label_height),
        rect.max,
    );
    painter.rect_filled(label_rect, 0.0, egui::Color32::from_black_alpha(160));
    painter.text(
        label_rect.center(),
        egui::Align2::CENTER_CENTER,
        &entry.name,
        egui::FontId::monospace(11.0),
        egui::Color32::from_gray(200),
    );

    // Draw image
    if let Some(tex) = cache.get_full(&entry.path) {
        let tex_size = tex.size_vec2();
        let avail = image_rect.size();
        let display_size = compute_display_size_with(zoom.zoom_mode, zoom.zoom_level, tex_size, avail);
        let center = image_rect.center() + zoom.pan_offset;
        let img_rect = egui::Rect::from_center_size(center, display_size);
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(tex.id(), img_rect, uv, egui::Color32::WHITE);
    } else {
        // Request decode
        if !cache.is_pending(&entry.path, false) {
            cache.mark_pending(entry.path.clone(), false);
            pipeline.request(entry.path.clone(), false, cache.generation, entry.zip_source.clone());
        }
        painter.text(
            image_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Loading...",
            egui::FontId::proportional(14.0),
            egui::Color32::from_gray(120),
        );
    }
}

pub fn draw(
    ui: &mut egui::Ui,
    compare: &CompareState,
    dir: &DirState,
    cache: &mut TextureCache,
    pipeline: &DecodePipeline,
) {
    let avail = ui.available_size();
    let (rect, _response) = ui.allocate_exact_size(avail, egui::Sense::click_and_drag());

    let sep_w = 2.0;
    let half_w = ((rect.width() - sep_w) / 2.0).floor();

    let left_rect = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(half_w, rect.height()),
    );
    let right_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + half_w + sep_w, rect.min.y),
        egui::vec2(rect.width() - half_w - sep_w, rect.height()),
    );

    // Separator
    let sep_x = rect.min.x + half_w + sep_w / 2.0;
    ui.painter().vline(
        sep_x,
        rect.y_range(),
        egui::Stroke::new(sep_w, egui::Color32::from_gray(60)),
    );

    draw_side(
        ui,
        left_rect,
        compare.left_index,
        &compare.left_zoom,
        dir,
        cache,
        pipeline,
        compare.active_side == CompareSide::Left,
    );
    draw_side(
        ui,
        right_rect,
        compare.right_index,
        &compare.right_zoom,
        dir,
        cache,
        pipeline,
        compare.active_side == CompareSide::Right,
    );
}

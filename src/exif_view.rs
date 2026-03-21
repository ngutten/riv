use std::path::Path;

#[derive(Debug, Clone)]
pub struct ExifData {
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub exposure: Option<String>,
    pub iso: Option<String>,
    pub aperture: Option<String>,
    pub focal_length: Option<String>,
    pub date: Option<String>,
    pub gps: Option<String>,
}

pub fn read_exif(path: &Path) -> Option<ExifData> {
    let file = std::fs::File::open(path).ok()?;
    let mut buf = std::io::BufReader::new(file);
    let reader = exif::Reader::new().read_from_container(&mut buf).ok()?;

    let get = |tag: exif::Tag| -> Option<String> {
        reader
            .get_field(tag, exif::In::PRIMARY)
            .map(|f| f.display_value().with_unit(&reader).to_string())
    };

    let gps = extract_gps(&reader);

    Some(ExifData {
        camera: get(exif::Tag::Model),
        lens: get(exif::Tag::LensModel),
        exposure: get(exif::Tag::ExposureTime),
        iso: get(exif::Tag::PhotographicSensitivity),
        aperture: get(exif::Tag::FNumber),
        focal_length: get(exif::Tag::FocalLength),
        date: get(exif::Tag::DateTimeOriginal),
        gps,
    })
}

fn extract_gps(reader: &exif::Exif) -> Option<String> {
    let lat = reader.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)?;
    let lat_ref = reader.get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY)?;
    let lon = reader.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY)?;
    let lon_ref = reader.get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY)?;
    Some(format!(
        "{} {} / {} {}",
        lat.display_value(),
        lat_ref.display_value(),
        lon.display_value(),
        lon_ref.display_value(),
    ))
}

pub fn draw_exif_overlay(ui: &mut egui::Ui, alloc_rect: egui::Rect, exif: Option<&ExifData>) {
    let painter = ui.painter();
    let font = egui::FontId::monospace(12.0);
    let text_color = egui::Color32::from_gray(220);
    let label_color = egui::Color32::from_gray(150);
    let line_height = 16.0;
    let padding = 8.0;
    let panel_width = 260.0;

    let mut lines: Vec<(&str, String)> = Vec::new();

    match exif {
        Some(data) => {
            if let Some(ref v) = data.camera {
                lines.push(("Camera", v.clone()));
            }
            if let Some(ref v) = data.lens {
                lines.push(("Lens", v.clone()));
            }
            if let Some(ref v) = data.exposure {
                lines.push(("Exposure", v.clone()));
            }
            if let Some(ref v) = data.aperture {
                lines.push(("Aperture", v.clone()));
            }
            if let Some(ref v) = data.iso {
                lines.push(("ISO", v.clone()));
            }
            if let Some(ref v) = data.focal_length {
                lines.push(("Focal", v.clone()));
            }
            if let Some(ref v) = data.date {
                lines.push(("Date", v.clone()));
            }
            if let Some(ref v) = data.gps {
                lines.push(("GPS", v.clone()));
            }
            if lines.is_empty() {
                lines.push(("", "No EXIF data".to_string()));
            }
        }
        None => {
            lines.push(("", "No EXIF data".to_string()));
        }
    }

    let panel_height = lines.len() as f32 * line_height + padding * 2.0;
    let panel_rect = egui::Rect::from_min_size(
        egui::pos2(
            alloc_rect.right() - panel_width - 10.0,
            alloc_rect.min.y + 10.0,
        ),
        egui::vec2(panel_width, panel_height),
    );

    painter.rect_filled(panel_rect, 4.0, egui::Color32::from_black_alpha(180));

    for (i, (label, value)) in lines.iter().enumerate() {
        let y = panel_rect.min.y + padding + i as f32 * line_height;
        if !label.is_empty() {
            painter.text(
                egui::pos2(panel_rect.min.x + padding, y),
                egui::Align2::LEFT_TOP,
                format!("{}:", label),
                font.clone(),
                label_color,
            );
            painter.text(
                egui::pos2(panel_rect.min.x + padding + 80.0, y),
                egui::Align2::LEFT_TOP,
                value,
                font.clone(),
                text_color,
            );
        } else {
            painter.text(
                egui::pos2(panel_rect.min.x + padding, y),
                egui::Align2::LEFT_TOP,
                value,
                font.clone(),
                text_color,
            );
        }
    }
}

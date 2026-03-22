use std::io::BufReader;
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

#[derive(Debug, Clone)]
pub struct TextChunk {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct ImageMetadata {
    pub exif: Option<ExifData>,
    pub text_chunks: Vec<TextChunk>,
}

pub fn read_metadata(path: &Path) -> Option<ImageMetadata> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "png" => {
            let chunks = read_png_text_chunks(path).unwrap_or_default();
            if chunks.is_empty() {
                return None;
            }
            Some(ImageMetadata {
                exif: None,
                text_chunks: chunks,
            })
        }
        "jpg" | "jpeg" | "tif" | "tiff" => {
            let exif = read_exif(path);
            if exif.is_none() {
                return None;
            }
            Some(ImageMetadata {
                exif,
                text_chunks: Vec::new(),
            })
        }
        _ => {
            // Try EXIF for other formats (some WebP, HEIF have EXIF)
            let exif = read_exif(path);
            if exif.is_none() {
                return None;
            }
            Some(ImageMetadata {
                exif,
                text_chunks: Vec::new(),
            })
        }
    }
}

fn read_exif(path: &Path) -> Option<ExifData> {
    let file = std::fs::File::open(path).ok()?;
    let mut buf = BufReader::new(file);
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

fn read_png_text_chunks(path: &Path) -> Option<Vec<TextChunk>> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let reader = decoder.read_info().ok()?;
    let info = reader.info();

    let mut chunks = Vec::new();

    for text in &info.uncompressed_latin1_text {
        chunks.push(TextChunk {
            key: text.keyword.clone(),
            value: text.text.clone(),
        });
    }

    for text in &info.compressed_latin1_text {
        if let Ok(decompressed) = text.get_text() {
            chunks.push(TextChunk {
                key: text.keyword.clone(),
                value: decompressed,
            });
        }
    }

    for text in &info.utf8_text {
        if let Ok(decompressed) = text.get_text() {
            chunks.push(TextChunk {
                key: text.keyword.clone(),
                value: decompressed,
            });
        }
    }

    Some(chunks)
}

pub fn draw_metadata_overlay(ui: &mut egui::Ui, alloc_rect: egui::Rect, meta: Option<&ImageMetadata>) {
    let painter = ui.painter();
    let font = egui::FontId::monospace(12.0);
    let text_color = egui::Color32::from_gray(220);
    let label_color = egui::Color32::from_gray(150);
    let line_height = 16.0;
    let padding = 8.0;
    let panel_width = 360.0;

    let mut lines: Vec<(&str, String)> = Vec::new();

    match meta {
        Some(data) => {
            if let Some(ref exif) = data.exif {
                if let Some(ref v) = exif.camera {
                    lines.push(("Camera", v.clone()));
                }
                if let Some(ref v) = exif.lens {
                    lines.push(("Lens", v.clone()));
                }
                if let Some(ref v) = exif.exposure {
                    lines.push(("Exposure", v.clone()));
                }
                if let Some(ref v) = exif.aperture {
                    lines.push(("Aperture", v.clone()));
                }
                if let Some(ref v) = exif.iso {
                    lines.push(("ISO", v.clone()));
                }
                if let Some(ref v) = exif.focal_length {
                    lines.push(("Focal", v.clone()));
                }
                if let Some(ref v) = exif.date {
                    lines.push(("Date", v.clone()));
                }
                if let Some(ref v) = exif.gps {
                    lines.push(("GPS", v.clone()));
                }
            }
            for chunk in &data.text_chunks {
                let truncated = if chunk.value.len() > 200 {
                    format!("{}...", &chunk.value[..200])
                } else {
                    chunk.value.clone()
                };
                // Replace newlines with spaces for inline display
                let oneline = truncated.replace('\n', " ");
                lines.push((&chunk.key, oneline));
            }
            if lines.is_empty() {
                lines.push(("", "No metadata".to_string()));
            }
        }
        None => {
            lines.push(("", "No metadata".to_string()));
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

pub fn draw_metadata_popup(ctx: &egui::Context, meta: Option<&ImageMetadata>, open: &mut bool) {
    let mut should_close = false;
    egui::Window::new("Metadata")
        .open(open)
        .resizable(true)
        .default_width(500.0)
        .default_height(400.0)
        .show(ctx, |ui| {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                should_close = true;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                match meta {
                    Some(data) => {
                        if let Some(ref exif) = data.exif {
                            ui.heading("EXIF");
                            egui::Grid::new("exif_grid")
                                .num_columns(2)
                                .spacing([12.0, 4.0])
                                .show(ui, |ui| {
                                    let fields: &[(&str, &Option<String>)] = &[
                                        ("Camera", &exif.camera),
                                        ("Lens", &exif.lens),
                                        ("Exposure", &exif.exposure),
                                        ("Aperture", &exif.aperture),
                                        ("ISO", &exif.iso),
                                        ("Focal Length", &exif.focal_length),
                                        ("Date", &exif.date),
                                        ("GPS", &exif.gps),
                                    ];
                                    for (label, value) in fields {
                                        if let Some(v) = value {
                                            ui.label(*label);
                                            ui.label(v);
                                            ui.end_row();
                                        }
                                    }
                                });
                            ui.add_space(8.0);
                        }
                        if !data.text_chunks.is_empty() {
                            ui.heading("PNG Metadata");
                            for chunk in &data.text_chunks {
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(&chunk.key).strong());
                                let mut text = chunk.value.clone();
                                ui.add(
                                    egui::TextEdit::multiline(&mut text)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(chunk.value.lines().count().max(2).min(12))
                                        .font(egui::FontId::monospace(11.0)),
                                );
                            }
                        }
                        if data.exif.is_none() && data.text_chunks.is_empty() {
                            ui.label("No metadata found.");
                        }
                    }
                    None => {
                        ui.label("No metadata found.");
                    }
                }
            });
        });
    if should_close {
        *open = false;
    }
}

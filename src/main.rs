mod app;
mod cache;
mod config;
mod decode;
mod dir;
mod exif_view;
mod input;
mod state;
mod status_bar;
mod views;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "riv", about = "Fast, minimal image viewer")]
struct Cli {
    /// Directory or image file to open
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Initial view mode
    #[arg(long, value_parser = ["list", "grid"])]
    view: Option<String>,

    /// Sort field
    #[arg(long, value_parser = ["name", "size", "date"])]
    sort: Option<String>,

    /// Sort in descending order
    #[arg(long)]
    sort_desc: bool,

    /// Thumbnail size in pixels
    #[arg(long, default_value = "160")]
    thumb_size: u32,
}

fn generate_icon() -> egui::IconData {
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            let cx = x as f32 - 15.5;
            let cy = y as f32 - 15.5;
            let dist = (cx * cx + cy * cy).sqrt();

            if dist < 14.0 {
                // Blue-purple gradient circle
                let t = dist / 14.0;
                rgba[idx] = (60.0 + 100.0 * t) as u8;     // R
                rgba[idx + 1] = (100.0 + 50.0 * (1.0 - t)) as u8; // G
                rgba[idx + 2] = (200.0 + 30.0 * (1.0 - t)) as u8; // B
                rgba[idx + 3] = 255;                         // A

                // Draw "R" letter in white
                let in_r = (x >= 10 && x <= 12 && y >= 8 && y <= 24) // vertical bar
                    || (x >= 12 && x <= 20 && y >= 8 && y <= 10)     // top bar
                    || (x >= 12 && x <= 20 && y >= 14 && y <= 16)    // middle bar
                    || (x >= 20 && x <= 22 && y >= 10 && y <= 14)    // top right
                    || (x >= 16 && x <= 22 && y >= 16 && y <= 24     // diagonal
                        && (x as i32 - 14) >= (y as i32 - 16) / 2
                        && (x as i32 - 14) <= (y as i32 - 16) / 2 + 2);

                if in_r {
                    rgba[idx] = 255;
                    rgba[idx + 1] = 255;
                    rgba[idx + 2] = 255;
                    rgba[idx + 3] = 255;
                }
            } else if dist < 15.0 {
                // Anti-aliased edge
                let alpha = ((15.0 - dist) * 255.0) as u8;
                rgba[idx] = 80;
                rgba[idx + 1] = 120;
                rgba[idx + 2] = 210;
                rgba[idx + 3] = alpha;
            }
        }
    }

    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

fn main() {
    let cli = Cli::parse();
    let config = config::Config::load();

    let path = cli.path.canonicalize().unwrap_or_else(|e| {
        eprintln!("riv: cannot open '{}': {}", cli.path.display(), e);
        std::process::exit(1);
    });

    let icon = generate_icon();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("riv")
            .with_inner_size([1024.0, 768.0])
            .with_icon(icon),
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            // Minimize frame latency to reduce resize artifacts (hall-of-mirrors).
            // This is a platform limitation — winit lacks _NET_WM_SYNC_REQUEST on X11
            // so the WM composites stale pixels during resize. Lower latency helps.
            desired_maximum_frame_latency: Some(1),
            present_mode: wgpu::PresentMode::AutoNoVsync,
            ..Default::default()
        },
        ..Default::default()
    };

    let cli_view = cli.view;
    let cli_sort = cli.sort;
    let cli_sort_desc = cli.sort_desc;
    let cli_thumb_size = cli.thumb_size;

    eframe::run_native(
        "riv",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::RivApp::new(
                cc,
                path,
                cli_view.as_deref(),
                cli_sort.as_deref(),
                cli_sort_desc,
                cli_thumb_size,
                config,
            )))
        }),
    )
    .expect("Failed to launch eframe");
}

use crossbeam_channel::{Receiver, Sender};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use crate::dir::ZipSource;
use crate::state::DecodedFrame;

#[derive(Debug)]
pub struct DecodeRequest {
    pub path: PathBuf,
    pub is_thumbnail: bool,
    pub generation: u64,
    pub zip_source: Option<ZipSource>,
    pub thumb_size: u32,
}

pub struct DecodeResult {
    pub path: PathBuf,
    pub is_thumbnail: bool,
    pub generation: u64,
    pub data: Option<DecodedImage>,
    pub animated: Option<AnimatedResult>,
}

pub struct DecodedImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct AnimatedResult {
    pub frames: Vec<DecodedFrame>,
    pub delays: Vec<Duration>,
}

/// LIFO work queue — workers pop from the end so the most recently
/// requested image (the one the user is looking at) gets decoded first.
struct WorkQueue {
    stack: Mutex<Vec<DecodeRequest>>,
    condvar: Condvar,
}

impl WorkQueue {
    fn new() -> Self {
        Self {
            stack: Mutex::new(Vec::new()),
            condvar: Condvar::new(),
        }
    }

    fn push(&self, req: DecodeRequest) {
        self.stack.lock().unwrap().push(req);
        self.condvar.notify_one();
    }

    /// Block until a request is available, then pop the newest one.
    fn pop(&self) -> DecodeRequest {
        let mut stack = self.stack.lock().unwrap();
        loop {
            if let Some(req) = stack.pop() {
                return req;
            }
            stack = self.condvar.wait(stack).unwrap();
        }
    }

    /// Drain all requests whose generation is older than `gen`.
    fn drain_stale(&self, gen: u64) {
        let mut stack = self.stack.lock().unwrap();
        stack.retain(|r| r.generation >= gen);
    }
}

pub struct DecodePipeline {
    pub result_rx: Receiver<DecodeResult>,
    queue: Arc<WorkQueue>,
    current_generation: Arc<AtomicU64>,
    pub thumb_size: u32,
}

impl DecodePipeline {
    pub fn new(thumb_size: u32) -> Self {
        let queue = Arc::new(WorkQueue::new());
        let current_generation = Arc::new(AtomicU64::new(0));
        let (result_tx, result_rx) = crossbeam_channel::unbounded::<DecodeResult>();

        let num_workers = thread::available_parallelism()
            .map(|n| n.get().clamp(4, 8))
            .unwrap_or(4);

        for _ in 0..num_workers {
            let q = Arc::clone(&queue);
            let gen = Arc::clone(&current_generation);
            let tx = result_tx.clone();
            thread::spawn(move || {
                worker_loop(q, gen, tx);
            });
        }

        Self {
            result_rx,
            queue,
            current_generation,
            thumb_size,
        }
    }

    pub fn request(
        &self,
        path: PathBuf,
        is_thumbnail: bool,
        generation: u64,
        zip_source: Option<ZipSource>,
    ) {
        self.queue.push(DecodeRequest {
            path,
            is_thumbnail,
            generation,
            zip_source,
            thumb_size: self.thumb_size,
        });
    }

    /// Update the current generation and drain all stale requests from the
    /// queue so workers don't waste time on images from a previous directory.
    pub fn set_generation(&self, gen: u64) {
        self.current_generation.store(gen, Ordering::Relaxed);
        self.queue.drain_stale(gen);
    }
}

fn worker_loop(queue: Arc<WorkQueue>, generation: Arc<AtomicU64>, tx: Sender<DecodeResult>) {
    loop {
        let req = queue.pop();

        // Skip stale requests before doing expensive I/O
        if req.generation < generation.load(Ordering::Relaxed) {
            continue;
        }

        let ext = req.path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Animated formats (full decode only, not thumbnail, not from zip)
        if !req.is_thumbnail && req.zip_source.is_none() {
            let anim = match ext.as_str() {
                "gif" => decode_animated_gif(&req.path),
                "webp" => decode_animated_webp(&req.path),
                _ => None,
            };
            if let Some(anim) = anim {
                let _ = tx.send(DecodeResult {
                    path: req.path,
                    is_thumbnail: false,
                    generation: req.generation,
                    data: None,
                    animated: Some(anim),
                });
                continue;
            }
            // Fall through to static decode if animated decode fails
        }

        // Format-specific decoders
        let data = if let Some(ref zip_source) = req.zip_source {
            decode_from_zip(zip_source, req.is_thumbnail, req.thumb_size)
        } else {
            match ext.as_str() {
                "svg" | "svgz" => decode_svg(&req.path, req.is_thumbnail, req.thumb_size),
                "pdf" => decode_pdf(&req.path, req.is_thumbnail, req.thumb_size),
                "cr2" | "cr3" | "nef" | "arw" | "orf" | "rw2" | "dng" | "raf" | "pef" | "srw" =>
                    decode_raw(&req.path, req.is_thumbnail, req.thumb_size),
                #[cfg(feature = "heif")]
                "heif" | "heic" => decode_heif(&req.path, req.is_thumbnail, req.thumb_size),
                _ => decode_image(&req.path, req.is_thumbnail, req.thumb_size),
            }
        };
        let _ = tx.send(DecodeResult {
            path: req.path,
            is_thumbnail: req.is_thumbnail,
            generation: req.generation,
            data,
            animated: None,
        });
    }
}

fn is_jpeg(path: &PathBuf) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg"))
        .unwrap_or(false)
}

fn is_jpeg_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg")
}

fn decode_image(path: &PathBuf, is_thumbnail: bool, thumb_size: u32) -> Option<DecodedImage> {
    if is_jpeg(path) {
        if let Some(img) = decode_jpeg(path, is_thumbnail, thumb_size) {
            return Some(img);
        }
        // Fall through to image crate on turbojpeg failure
    }
    decode_generic(path, is_thumbnail, thumb_size)
}

fn decode_jpeg(path: &PathBuf, is_thumbnail: bool, thumb_size: u32) -> Option<DecodedImage> {
    let jpeg_data = std::fs::read(path).ok()?;
    decode_jpeg_bytes(&jpeg_data, is_thumbnail, thumb_size)
}

fn decode_jpeg_bytes(jpeg_data: &[u8], is_thumbnail: bool, thumb_size: u32) -> Option<DecodedImage> {
    let mut decompressor = turbojpeg::Decompressor::new().ok()?;
    let header = decompressor.read_header(jpeg_data).ok()?;
    let width = header.width;
    let height = header.height;

    let mut pixels = vec![0u8; 4 * width * height];
    let image = turbojpeg::Image {
        pixels: pixels.as_mut_slice(),
        width,
        pitch: 4 * width,
        height,
        format: turbojpeg::PixelFormat::RGBA,
    };
    decompressor.decompress(jpeg_data, image).ok()?;

    if is_thumbnail {
        let rgba = image::RgbaImage::from_raw(width as u32, height as u32, pixels)?;
        let thumb = image::DynamicImage::ImageRgba8(rgba).thumbnail(thumb_size, thumb_size);
        let rgba = thumb.to_rgba8();
        let (w, h) = rgba.dimensions();
        Some(DecodedImage {
            rgba: rgba.into_raw(),
            width: w,
            height: h,
        })
    } else {
        Some(DecodedImage {
            rgba: pixels,
            width: width as u32,
            height: height as u32,
        })
    }
}

fn decode_generic(path: &Path, is_thumbnail: bool, thumb_size: u32) -> Option<DecodedImage> {
    let img = image::open(path).ok()?;
    let img = if is_thumbnail {
        img.thumbnail(thumb_size, thumb_size)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(DecodedImage {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

fn decode_from_zip(zip_source: &ZipSource, is_thumbnail: bool, thumb_size: u32) -> Option<DecodedImage> {
    let file = std::fs::File::open(&zip_source.archive_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(&zip_source.entry_name).ok()?;

    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes).ok()?;
    drop(entry);

    if is_jpeg_name(&zip_source.entry_name) {
        if let Some(img) = decode_jpeg_bytes(&bytes, is_thumbnail, thumb_size) {
            return Some(img);
        }
    }

    decode_bytes(&bytes, is_thumbnail, thumb_size)
}

fn decode_bytes(bytes: &[u8], is_thumbnail: bool, thumb_size: u32) -> Option<DecodedImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if is_thumbnail {
        img.thumbnail(thumb_size, thumb_size)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(DecodedImage {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

fn decode_animated_gif(path: &PathBuf) -> Option<AnimatedResult> {
    use image::codecs::gif::GifDecoder;
    use image::AnimationDecoder;
    use std::io::BufReader;

    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let decoder = GifDecoder::new(reader).ok()?;
    let frames_iter = decoder.into_frames();

    let mut frames = Vec::new();
    let mut delays = Vec::new();

    for frame_result in frames_iter {
        let frame: image::Frame = frame_result.ok()?;
        let (numer, denom) = frame.delay().numer_denom_ms();
        let delay_ms = if denom == 0 { 100 } else { numer / denom };
        // GIFs with 0 delay default to 100ms
        let delay_ms = if delay_ms == 0 { 100 } else { delay_ms };
        delays.push(Duration::from_millis(delay_ms as u64));

        let rgba = frame.into_buffer();
        let (w, h) = rgba.dimensions();
        frames.push(DecodedFrame {
            rgba: rgba.into_raw(),
            width: w,
            height: h,
        });
    }

    if frames.is_empty() {
        return None;
    }

    Some(AnimatedResult { frames, delays })
}

fn decode_animated_webp(path: &Path) -> Option<AnimatedResult> {
    use image::codecs::webp::WebPDecoder;
    use image::AnimationDecoder;
    use std::io::BufReader;

    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let decoder = WebPDecoder::new(reader).ok()?;
    if !decoder.has_animation() {
        return None;
    }
    let frames_iter = decoder.into_frames();

    let mut frames = Vec::new();
    let mut delays = Vec::new();

    for frame_result in frames_iter {
        let frame: image::Frame = frame_result.ok()?;
        let (numer, denom) = frame.delay().numer_denom_ms();
        let delay_ms = if denom == 0 { 100 } else { numer / denom };
        let delay_ms = if delay_ms == 0 { 100 } else { delay_ms };
        delays.push(Duration::from_millis(delay_ms as u64));

        let rgba = frame.into_buffer();
        let (w, h) = rgba.dimensions();
        frames.push(DecodedFrame {
            rgba: rgba.into_raw(),
            width: w,
            height: h,
        });
    }

    if frames.is_empty() {
        return None;
    }

    Some(AnimatedResult { frames, delays })
}

fn decode_svg(path: &Path, is_thumbnail: bool, thumb_size: u32) -> Option<DecodedImage> {
    let data = std::fs::read(path).ok()?;
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&data, &opt).ok()?;
    let size = tree.size();

    let max_dim = if is_thumbnail {
        thumb_size as f32
    } else {
        4096.0
    };

    let (w, h) = scale_to_fit(size.width(), size.height(), max_dim);
    let w = w.max(1.0) as u32;
    let h = h.max(1.0) as u32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    let scale = w as f32 / size.width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // Convert premultiplied RGBA to straight RGBA for egui
    let rgba = unpremultiply_alpha(pixmap.data());

    Some(DecodedImage {
        rgba,
        width: w,
        height: h,
    })
}

fn scale_to_fit(w: f32, h: f32, max_dim: f32) -> (f32, f32) {
    if w <= max_dim && h <= max_dim {
        return (w, h);
    }
    let scale = (max_dim / w).min(max_dim / h);
    (w * scale, h * scale)
}

fn unpremultiply_alpha(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks_exact(4) {
        let a = chunk[3] as u32;
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let r = ((chunk[0] as u32 * 255 + a / 2) / a).min(255) as u8;
            let g = ((chunk[1] as u32 * 255 + a / 2) / a).min(255) as u8;
            let b = ((chunk[2] as u32 * 255 + a / 2) / a).min(255) as u8;
            out.extend_from_slice(&[r, g, b, chunk[3]]);
        }
    }
    out
}

fn decode_pdf(path: &Path, is_thumbnail: bool, thumb_size: u32) -> Option<DecodedImage> {
    let dpi = if is_thumbnail { 36 } else { 150 };
    let tmp = std::env::temp_dir().join(format!("riv_pdf_{}.png", std::process::id()));
    let status = std::process::Command::new("mutool")
        .args(["draw", "-r", &dpi.to_string(), "-o"])
        .arg(&tmp)
        .arg(path)
        .arg("1") // first page only
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let result = decode_generic(&tmp, is_thumbnail, thumb_size);
    let _ = std::fs::remove_file(&tmp);
    result
}

fn decode_raw(path: &Path, is_thumbnail: bool, thumb_size: u32) -> Option<DecodedImage> {
    // dcraw_emu writes output next to the input file, so use a unique temp copy approach
    // Try dcraw_emu first, fall back to dcraw
    let result = decode_raw_with("dcraw_emu", path, is_thumbnail, thumb_size);
    if result.is_some() {
        return result;
    }
    decode_raw_with("dcraw", path, is_thumbnail, thumb_size)
}

fn decode_raw_with(
    tool: &str,
    path: &Path,
    is_thumbnail: bool,
    thumb_size: u32,
) -> Option<DecodedImage> {
    // dcraw_emu -T writes a .tiff next to the input file
    // dcraw_emu -e -T extracts embedded thumbnail
    let args: Vec<&str> = if is_thumbnail {
        vec!["-e", "-T"]
    } else {
        vec!["-T", "-w"]
    };
    let status = std::process::Command::new(tool)
        .args(&args)
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    // Output file is input path with .tiff extension (or .thumb.jpg for -e)
    let output = if is_thumbnail {
        // -e extracts embedded thumb: original.thumb.jpg
        let thumb_path = path.with_extension("thumb.jpg");
        if thumb_path.exists() {
            thumb_path
        } else {
            path.with_extension("tiff")
        }
    } else {
        path.with_extension("tiff")
    };
    let result = decode_generic(&output, is_thumbnail, thumb_size);
    let _ = std::fs::remove_file(&output);
    result
}

#[cfg(feature = "heif")]
fn decode_heif(path: &Path, is_thumbnail: bool, thumb_size: u32) -> Option<DecodedImage> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

    let lib_heif = LibHeif::new();
    let ctx = HeifContext::read_from_file(path.to_str()?).ok()?;
    let handle = ctx.primary_image_handle().ok()?;
    let image = lib_heif
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .ok()?;
    let planes = image.planes();
    let interleaved = planes.interleaved?;
    let w = handle.width();
    let h = handle.height();

    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let row_start = (y as usize) * interleaved.stride;
        rgba.extend_from_slice(&interleaved.data[row_start..row_start + (w as usize * 4)]);
    }

    if is_thumbnail {
        let img = image::RgbaImage::from_raw(w, h, rgba)?;
        let thumb = image::DynamicImage::ImageRgba8(img).thumbnail(thumb_size, thumb_size);
        let rgba = thumb.to_rgba8();
        let (tw, th) = rgba.dimensions();
        Some(DecodedImage {
            rgba: rgba.into_raw(),
            width: tw,
            height: th,
        })
    } else {
        Some(DecodedImage {
            rgba,
            width: w,
            height: h,
        })
    }
}

/// Check if an external tool is available in PATH.
pub fn tool_available(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

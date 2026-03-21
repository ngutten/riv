# riv — Rust Image Viewer

A fast, minimal image viewer built for browsing large directories (80k+ files) without choking. No GTK, no GNOME, no glib. Optimized for the workflow: open a directory, browse images, zoom/pan, navigate subdirectories, send to external tools.

## Design Philosophy

- **Directory-first**: `riv .` or `riv /path/to/dir` opens that directory immediately. No launchers, no recent files, no import step.
- **Virtualized everything**: Never load what isn't visible. A directory with 80k files should open as fast as one with 80.
- **Simple state model**: The app is always in one of two modes — grid view or single-image view. That's it.
- **No database**: The filesystem is the database. No sidecar files, no thumbnail caches on disk, no metadata databases.
- **Keyboard-driven with mouse support**: Efficient navigation without requiring either input exclusively.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────┐
│                   UI Thread                      │
│  ┌──────────┐  ┌──────────┐  ┌───────────────┐  │
│  │ Grid View│  │Single    │  │ Input Handler │  │
│  │ (egui)   │  │Image View│  │ (keys/mouse)  │  │
│  └────┬─────┘  └────┬─────┘  └───────────────┘  │
│       │              │                           │
│  ┌────▼──────────────▼─────┐                     │
│  │    Texture Cache (LRU)  │                     │
│  │   GPU textures for      │                     │
│  │   visible + prefetch    │                     │
│  └────────────┬────────────┘                     │
├───────────────┼─────────────────────────────────┤
│               │        Worker Threads            │
│  ┌────────────▼────────────┐                     │
│  │   Decode Thread Pool    │                     │
│  │  (image decode → RGBA)  │                     │
│  └────────────┬────────────┘                     │
│  ┌────────────▼────────────┐                     │
│  │  Thumbnail Generator    │                     │
│  │  (resized RGBA for grid)│                     │
│  └─────────────────────────┘                     │
└─────────────────────────────────────────────────┘
```

### Key Data Structures

```rust
/// The entire app state
struct App {
    dir: DirState,
    view: ViewMode,
    cache: TextureCache,
    decode_tx: Sender<DecodeRequest>,
    decoded_rx: Receiver<DecodeResult>,
    config: Config,
}

/// Current directory listing — just paths, nothing loaded
struct DirState {
    root: PathBuf,
    entries: Vec<DirEntry>,     // sorted, filtered to supported formats
    subdirs: Vec<PathBuf>,      // for navigation
    selected: usize,            // cursor position
}

struct DirEntry {
    path: PathBuf,
    file_size: u64,             // from readdir, no extra stat needed
    // No image data here — that's in the cache
}

enum ViewMode {
    Grid {
        scroll_offset: f32,
        columns: usize,         // computed from window width / thumb size
        thumb_size: u32,        // configurable
    },
    Single {
        index: usize,
        zoom: ZoomMode,
        pan: Vec2,
    },
}

enum ZoomMode {
    FitWidth,
    FitHeight,
    FitWindow,      // fit whichever dimension is tighter
    Original,       // 1:1 pixels
    Custom(f32),    // arbitrary zoom factor
}
```

### Texture Cache

```rust
struct TextureCache {
    /// Full-res textures for single view, keyed by index
    full: LruCache<usize, TextureHandle>,
    /// Thumbnail textures for grid view, keyed by index
    thumbs: LruCache<usize, TextureHandle>,
    /// Track in-flight decode requests to avoid duplicates
    pending: HashSet<(usize, bool)>,  // (index, is_thumbnail)
}
```

The LRU cache sizes should be tuned but reasonable defaults:
- `full`: 10-20 images (these are big, GPU memory matters)
- `thumbs`: 500-1000 thumbnails (small, can hold many)

### Async Decode Pipeline

The UI thread never decodes images. Instead:

1. UI determines which indices are visible (grid viewport or single view + prefetch)
2. For each visible index not in cache and not pending, send a `DecodeRequest`
3. Worker threads decode → produce RGBA pixel buffer
4. UI thread polls `decoded_rx` each frame, uploads to GPU texture

```rust
struct DecodeRequest {
    index: usize,
    path: PathBuf,
    thumbnail: bool,        // if true, resize to thumb_size during decode
    priority: u32,          // lower = higher priority (visible > prefetch)
}

struct DecodeResult {
    index: usize,
    thumbnail: bool,
    pixels: RgbaImage,      // or an error
}
```

Worker pool: use `rayon` or a small custom threadpool. 4-8 threads is reasonable.
Priority queue for requests so visible items decode before prefetch items.

---

## Features — Phased Implementation

### Phase 1: Core Viewer (MVP)

Get a usable image viewer running. Target: one focused session.

- [ ] **Directory loading**: `readdir`, filter by extension, sort by name
- [ ] **Grid view**: virtualized thumbnail grid, scroll with mouse wheel / keyboard
- [ ] **Single image view**: click or Enter to open, Escape to return to grid
- [ ] **Zoom modes**: fit-window (default), fit-width, fit-height, original, scroll-to-zoom
- [ ] **Pan**: click-drag when zoomed past window bounds
- [ ] **Navigation**: arrow keys / PgUp / PgDn in both views, Home/End
- [ ] **Async decode**: background decoding with texture cache
- [ ] **Format support**: PNG, JPEG, WebP, BMP, GIF (static), TIFF via `image` crate
- [ ] **Status bar**: filename, dimensions, file size, index/total, zoom level

### Phase 2: Directory Navigation & Metadata

- [ ] **Subdirectory navigation**: show subdirs in grid (with folder icon or label), Enter to descend, Backspace/Alt-Left to go up
- [ ] **EXIF display**: toggle panel showing metadata (use `kamadak-exif` or `rexif` crate)
- [ ] **Sort options**: name, date modified, file size (keybinds to toggle)
- [ ] **File info overlay**: toggle detailed file info on current image

### Phase 3: External Tools & Actions

- [ ] **Open in external app**: configurable keybinds, e.g. `g` → open in GIMP, `r` → rotate with `jpegtran` or ImageMagick
- [ ] **Config file**: `~/.config/riv/config.toml` for keybinds, external tools, default zoom, thumb size
- [ ] **Basic file ops**: delete (move to trash via `trash-rs` crate), copy path to clipboard

### Phase 4: Extended Format Support

- [ ] **PDF preview**: render first page via `mupdf-rs` or shell out to `mutool draw`
- [ ] **RAW formats**: `rawloader` crate or shell out to `dcraw`/`darktable-cli`
- [ ] **AVIF/HEIF**: `libheif-rs` or `ravif`
- [ ] **SVG**: `resvg` crate (renders to raster)
- [ ] **Animated GIF/WebP**: frame-by-frame playback

### Phase 5: Performance Polish

- [ ] **Parallel directory enumeration**: for network mounts or very slow filesystems
- [ ] **Smarter prefetch**: predict scroll direction, prefetch in that direction
- [ ] **Memory pressure handling**: reduce cache sizes when system memory is low
- [ ] **Optional persistent thumbnail cache**: opt-in disk cache for repeat visits to huge directories

---

## Crate Dependencies (Initial)

```toml
[dependencies]
eframe = "0.29"                # egui + wgpu windowing
egui = "0.29"
image = "0.25"                 # core image decoding
lru = "0.12"                   # LRU cache
crossbeam-channel = "0.5"      # fast MPMC channels
rayon = "0.10"                 # thread pool (or use std threads)
notify = "7"                   # filesystem watcher (optional, for live reload)
clap = { version = "4", features = ["derive"] }  # CLI args
directories = "5"              # XDG config paths
toml = "0.8"                   # config parsing
trash = "5"                    # trash support

# Phase 2+
kamadak-exif = "0.5"           # EXIF reading

# Phase 4
resvg = "0.44"                 # SVG rendering
# mupdf or shell out for PDF
# rawloader for RAW
```

---

## Key Implementation Notes

### Virtualized Grid Rendering

The critical performance insight. In egui's `update()`:

```
total_rows = ceil(num_entries / columns)
first_visible_row = floor(scroll_offset / (thumb_size + padding))
last_visible_row = first_visible_row + ceil(viewport_height / (thumb_size + padding))
visible_indices = range(first_visible_row * columns, last_visible_row * columns)
    .filter(|i| i < num_entries)
```

Only request thumbnails for `visible_indices` plus a prefetch band of ±2 rows.
Render placeholder rectangles (gray with filename) for entries whose thumbnails
haven't decoded yet. This means opening an 80k-file directory is instant — you
just see placeholders that fill in progressively.

egui's `ScrollArea` supports setting virtual content height without rendering
all rows: set `total_content_height = total_rows * (thumb_size + padding)`.

### Image Decode Strategy

For thumbnails: decode at reduced resolution when possible. The `image` crate's
JPEG decoder supports `scale_denom` for fast 1/2, 1/4, 1/8 decoding — use this
to avoid decoding a 4000x3000 JPEG just to display it at 256x256. For other
formats, decode full then resize with `image::imageops::resize` using
`FilterType::Triangle` (fast, decent quality).

For single view: always decode at full resolution. The texture upload is the
bottleneck — for a 4000x3000 RGBA image that's ~48MB of pixel data going to
the GPU. This is fine for one image; just don't queue too many simultaneously.

### Keyboard Map (Defaults)

```
Grid View:
  Arrow keys      Navigate selection
  Enter / Space   Open selected image
  PgUp / PgDn     Scroll by page
  Home / End       Jump to first / last
  Backspace        Go to parent directory
  /                Filter/search filename (stretch goal)
  q                Quit
  s                Cycle sort mode

Single View:
  Escape           Return to grid
  Left / Right     Previous / next image
  PgUp / PgDn      Jump 10 images
  f                 Cycle zoom: fit-window → fit-width → fit-height → original
  + / -            Zoom in / out
  Scroll wheel     Zoom in / out (at cursor position)
  Click + drag     Pan (when zoomed)
  0                 Reset zoom to fit-window
  i                 Toggle info overlay
  e                 Toggle EXIF panel
  g                 Open in GIMP (configurable)
  r                 Rotate (configurable external tool)
  Delete            Move to trash
  y                 Copy file path to clipboard
```

### Why egui + eframe

- **Immediate mode**: no widget tree to manage, just compute what's visible and draw it
- **wgpu backend**: GPU-accelerated, textures upload directly, smooth zoom/pan
- **No GTK/glib/Qt dependency**: just wgpu + winit underneath
- **Small API surface**: you'll use `ScrollArea`, `Image`, `Window`, `SidePanel` and that's most of it
- **Good enough for tooling**: not the prettiest, but functional and fast

### Platform Considerations

- egui/eframe targets X11 and Wayland on Linux via winit
- wgpu will use Vulkan by default, falls back to GL
- No Wayland-specific issues expected for a simple viewer
- Clipboard access: `arboard` crate works on X11/Wayland

---

## Project Structure

```
riv/
├── Cargo.toml
├── src/
│   ├── main.rs           # CLI parsing, eframe launch
│   ├── app.rs            # App struct, egui update loop
│   ├── dir.rs            # Directory listing, sorting, filtering
│   ├── cache.rs          # TextureCache + LRU management
│   ├── decode.rs         # Worker thread pool, decode pipeline
│   ├── grid.rs           # Grid view rendering (virtualized)
│   ├── single.rs         # Single image view + zoom/pan
│   ├── metadata.rs       # EXIF/file info display
│   ├── config.rs         # Config file parsing + defaults
│   ├── keys.rs           # Input handling / keybind dispatch
│   └── external.rs       # External tool launching
└── config.example.toml   # Example configuration
```

This is more files than strictly necessary — you could start with everything in
`main.rs` + `app.rs` and extract modules as they grow. The structure above is
the "once it's working" layout, not the "first afternoon" layout.

---

## Getting Started

```bash
cargo init riv
cd riv
# Add dependencies to Cargo.toml
cargo run -- /path/to/images
```

Start with Phase 1. The first milestone is: `cargo run -- .` in a directory of
PNGs shows a scrollable grid of gray placeholder rectangles with filenames, that
progressively fill in with thumbnails. Click one, see it full-size with
fit-to-window zoom. That's maybe 500-800 lines of Rust to get working.

---

## Open Questions / Future Directions

- **Multi-window or tabs?** Probably not for v1. Single window, single directory.
- **Image comparison mode?** Side-by-side of two images could be useful for SD output (comparing generations). Nice-to-have, not core.
- **Color management?** `lcms2` via `lcms2-rs` if you care about ICC profiles. Probably not critical for your use case.
- **GPU decode?** For very large images, GPU JPEG decode via compute shaders is possible with wgpu. Overkill unless you hit a wall.
- **Tiling/montage view?** Show N images at once in a grid at larger size (2x2, 3x3). Could be nice for comparing SD generations. Different from the thumbnail grid — this would be a "comparison grid" at near-full resolution.

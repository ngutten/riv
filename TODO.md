# riv — TODO

## MVP (Done)
- [x] Project skeleton + window launch
- [x] Directory scanning with image format filtering
- [x] Dense text-only file list (default view) with filename + size
- [x] Keyboard navigation (arrow keys, j/k, Home/End, PgUp/PgDn)
- [x] Selection highlight + status bar
- [x] Async decode pipeline (4 worker threads, crossbeam channels)
- [x] LRU texture cache (15 full, 500 thumbs)
- [x] Single image view (Enter to open, Escape to return)
- [x] Zoom modes: FitWindow, FitWidth, FitHeight, Original, Custom
- [x] Zoom controls: f cycle, +/- keys, scroll wheel, 0 reset
- [x] Click-drag pan in single view
- [x] Left/Right navigation between images in single view
- [x] Thumbnail grid view (Tab to toggle List/Grid)
- [x] Subdirectory navigation (Enter to descend, Backspace to go up)
- [x] Prefetch next/prev images in single view
- [x] q to quit

## Post-MVP (Done)
- [x] Window icon
- [x] Filename search/filter in list view
- [x] Sort options (name, size, date)
- [x] Image info overlay (dimensions, format, zoom level) — `i` key toggle
- [x] Animated GIF playback — Space to play/pause in single view
- [x] Drag-and-drop file/directory opening
- [x] Command-line flags for initial view mode, sort order (`--view`, `--sort`, `--sort-desc`)
- [x] Mouse wheel scrolling in list/grid views
- [x] Configurable thumbnail size (`--thumb-size`)
- [x] Status bar showing decode progress
- [x] Menu bar: View (List/Grid, Zoom Mode, Image Info, Sort), Edit, Go

## Future
- [ ] Image comparison / montage view
- [ ] Edit actions (rename, copy, EXIF, rotate, open-in-app)

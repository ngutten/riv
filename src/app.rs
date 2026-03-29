use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::cache::{AnimatedTextures, TextureCache};
use crate::config::Config;
use crate::decode::DecodePipeline;
use crate::dir::{self, DirState, ZipContext};
use crate::metadata::{self, ImageMetadata};
use crate::input;
use crate::state::{AppState, BrowseMode, CompareState, DialogState, EditAction, ImageFilter, SortField, ViewMode, ZoomMode};
use crate::status_bar;
use crate::views;

pub struct RivApp {
    state: AppState,
    dir: DirState,
    cache: TextureCache,
    pipeline: DecodePipeline,
    config: Config,
    metadata_cache: HashMap<PathBuf, Option<ImageMetadata>>,
    pub has_mutool: bool,
    pub has_dcraw: bool,
    first_frame: bool,
}

impl RivApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        path: PathBuf,
        cli_view: Option<&str>,
        cli_sort: Option<&str>,
        cli_sort_desc: bool,
        thumb_size: u32,
        config: Config,
    ) -> Self {
        let dir_path = if path.is_file() {
            path.parent().unwrap_or(&path).to_path_buf()
        } else {
            path.clone()
        };

        let mut state = AppState::new(dir_path.clone());
        state.thumb_size = thumb_size;

        // Apply config defaults (CLI flags override below)
        if let Some(ref view) = config.defaults.view {
            match view.as_str() {
                "grid" => state.view_mode = ViewMode::Grid,
                _ => state.view_mode = ViewMode::List,
            }
        }
        if let Some(ref sort) = config.defaults.sort {
            state.sort_field = match sort.as_str() {
                "size" => SortField::Size,
                "date" => SortField::Date,
                _ => SortField::Name,
            };
        }
        if let Some(desc) = config.defaults.sort_desc {
            if desc {
                state.sort_ascending = false;
            }
        }
        if let Some(ts) = config.defaults.thumb_size {
            state.thumb_size = ts;
        }

        // Apply CLI flags (override config)
        if let Some(view) = cli_view {
            match view {
                "grid" => state.view_mode = ViewMode::Grid,
                _ => state.view_mode = ViewMode::List,
            }
        }
        if let Some(sort) = cli_sort {
            state.sort_field = match sort {
                "size" => SortField::Size,
                "date" => SortField::Date,
                _ => SortField::Name,
            };
        }
        if cli_sort_desc {
            state.sort_ascending = false;
        }

        let mut dir = DirState::scan(&dir_path);
        dir.sort(state.sort_field, state.sort_ascending);

        // If opened with a file, select it
        if path.is_file() {
            if let Some(idx) = dir.entries.iter().position(|e| e.path == path) {
                state.selected_index = idx;
                state.view_mode = ViewMode::Single;
            }
        }

        let has_mutool = crate::decode::tool_available("mutool");
        let has_dcraw = crate::decode::tool_available("dcraw_emu")
            || crate::decode::tool_available("dcraw");

        Self {
            state,
            dir,
            cache: TextureCache::new(),
            pipeline: DecodePipeline::new(thumb_size),
            config,
            metadata_cache: HashMap::new(),
            has_mutool,
            has_dcraw,
            first_frame: true,
        }
    }

    fn poll_decode_results(&mut self, ctx: &egui::Context) {
        let mut got_results = false;
        while let Ok(result) = self.pipeline.result_rx.try_recv() {
            if result.generation != self.cache.generation {
                continue; // Stale result from old directory
            }

            // Handle animated GIF results
            if let Some(anim) = result.animated {
                let mut frames = Vec::with_capacity(anim.frames.len());
                for (i, frame) in anim.frames.iter().enumerate() {
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [frame.width as usize, frame.height as usize],
                        &frame.rgba,
                    );
                    let handle = ctx.load_texture(
                        format!("{}#frame{}", result.path.to_string_lossy(), i),
                        color_image,
                        self.state.image_filter.texture_options(),
                    );
                    frames.push(handle);
                }
                // Store dimensions from first frame
                if let Some(first) = anim.frames.first() {
                    self.cache.image_dimensions.insert(
                        result.path.clone(),
                        (first.width, first.height),
                    );
                }
                self.cache.insert_animated(
                    result.path,
                    AnimatedTextures {
                        frames,
                        delays: anim.delays,
                    },
                );
                got_results = true;
                continue;
            }

            if let Some(decoded) = result.data {
                // Store dimensions
                self.cache.image_dimensions.insert(
                    result.path.clone(),
                    (decoded.width, decoded.height),
                );

                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [decoded.width as usize, decoded.height as usize],
                    &decoded.rgba,
                );
                let tex_opts = if result.is_thumbnail {
                    egui::TextureOptions::LINEAR
                } else {
                    self.state.image_filter.texture_options()
                };
                let handle = ctx.load_texture(
                    result.path.to_string_lossy(),
                    color_image,
                    tex_opts,
                );
                if result.is_thumbnail {
                    self.cache.insert_thumb(result.path, handle);
                } else {
                    self.cache.insert_full(result.path, handle);
                }
                got_results = true;
            } else {
                // Decode failed — show hint if external tool is missing
                let ext = result.path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                match ext.as_str() {
                    "pdf" if !self.has_mutool => {
                        self.state.status_message = Some((
                            "Install mupdf-tools for PDF support".to_string(),
                            std::time::Instant::now(),
                        ));
                    }
                    "cr2" | "cr3" | "nef" | "arw" | "orf" | "rw2" | "dng" | "raf" | "pef" | "srw"
                        if !self.has_dcraw =>
                    {
                        self.state.status_message = Some((
                            "Install libraw-bin for RAW photo support".to_string(),
                            std::time::Instant::now(),
                        ));
                    }
                    _ => {}
                }
                // Remove from pending
                if result.is_thumbnail {
                    self.cache.pending_thumb.remove(&result.path);
                } else {
                    self.cache.pending_full.remove(&result.path);
                }
            }
        }
        if got_results {
            ctx.request_repaint();
        }
    }

    fn prefetch(&mut self) {
        let idx = self.state.resolved_index();
        match self.state.view_mode {
            ViewMode::Single => {
                // Prefetch next and previous image
                for &delta in &[-1i32, 1, -2, 2] {
                    let target = idx as i32 + delta;
                    if target >= 0 && (target as usize) < self.dir.entries.len() {
                        let entry = &self.dir.entries[target as usize];
                        if !entry.is_dir
                            && self.cache.get_full(&entry.path).is_none()
                            && self.cache.get_animated(&entry.path).is_none()
                            && !self.cache.is_pending(&entry.path, false)
                        {
                            self.cache.mark_pending(entry.path.clone(), false);
                            self.pipeline.request(
                                entry.path.clone(),
                                false,
                                self.cache.generation,
                                entry.zip_source.clone(),
                            );
                        }
                    }
                }
            }
            ViewMode::Grid | ViewMode::List => {
                // Prefetch selected image + neighbors for preview panel
                for &delta in &[0i32, -1, 1, -2, 2] {
                    let target = idx as i32 + delta;
                    if target >= 0 && (target as usize) < self.dir.entries.len() {
                        let entry = &self.dir.entries[target as usize];
                        if !entry.is_dir
                            && self.cache.get_full(&entry.path).is_none()
                            && self.cache.get_animated(&entry.path).is_none()
                            && !self.cache.is_pending(&entry.path, false)
                        {
                            self.cache.mark_pending(entry.path.clone(), false);
                            self.pipeline.request(
                                entry.path.clone(),
                                false,
                                self.cache.generation,
                                entry.zip_source.clone(),
                            );
                        }
                    }
                }
            }
            ViewMode::Compare => {
                // Prefetch both visible images + ±1 neighbors
                if let Some(ref cmp) = self.state.compare {
                    let indices = [cmp.left_index, cmp.right_index];
                    for &base in &indices {
                        for &delta in &[0i32, -1, 1] {
                            let target = base as i32 + delta;
                            if target >= 0 && (target as usize) < self.dir.entries.len() {
                                let entry = &self.dir.entries[target as usize];
                                if !entry.is_dir
                                    && self.cache.get_full(&entry.path).is_none()
                                    && self.cache.get_animated(&entry.path).is_none()
                                    && !self.cache.is_pending(&entry.path, false)
                                {
                                    self.cache.mark_pending(entry.path.clone(), false);
                                    self.pipeline.request(
                                        entry.path.clone(),
                                        false,
                                        self.cache.generation,
                                        entry.zip_source.clone(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn navigate_to_dir(&mut self, path: PathBuf) {
        self.state.current_dir = path;
        self.dir = DirState::scan(&self.state.current_dir);
        self.dir.sort(self.state.sort_field, self.state.sort_ascending);
        self.state.selected_index = self.dir.entries.iter()
            .position(|e| !e.is_dir)
            .unwrap_or(0);
        self.state.clear_filter();
        self.cache.clear_all();
        self.pipeline.set_generation(self.cache.generation);
        self.state.sync_path_bar();
        self.state.preview_focused = false;
    }

    fn navigate_to_zip(&mut self, archive_path: PathBuf, inner_prefix: String) {
        let ctx = ZipContext {
            archive_path: archive_path.clone(),
            inner_prefix: inner_prefix.clone(),
        };
        self.dir = DirState::scan_zip(&ctx);
        self.dir.sort(self.state.sort_field, self.state.sort_ascending);
        // Show synthetic path in path bar: "archive.zip > subdir/"
        if inner_prefix.is_empty() {
            self.state.current_dir = archive_path;
        } else {
            self.state.current_dir =
                PathBuf::from(format!("{}!{}", archive_path.display(), inner_prefix));
        }
        self.state.selected_index = self.dir.entries.iter()
            .position(|e| !e.is_dir)
            .unwrap_or(0);
        self.state.clear_filter();
        self.cache.clear_all();
        self.pipeline.set_generation(self.cache.generation);
        self.state.sync_path_bar();
        self.state.preview_focused = false;
    }

    fn handle_enter_directory(&mut self, idx: usize) {
        let entry = &self.dir.entries[idx];

        // ".." always means go up (critical for zip archives where
        // appending ".." to the inner prefix creates an invalid path)
        if entry.name == ".." {
            self.handle_go_up();
            return;
        }

        let path = entry.path.clone();

        if dir::is_zip(&path) && path.exists() {
            // Entering a real .zip file on disk
            self.navigate_to_zip(path, String::new());
        } else if let Some(ref zip_ctx) = self.dir.zip_ctx.clone() {
            // We're inside a zip and navigating to a subdirectory within it
            // Extract the subdirectory name from the entry
            let new_prefix = format!("{}{}/", zip_ctx.inner_prefix, entry.name);
            self.navigate_to_zip(zip_ctx.archive_path.clone(), new_prefix);
        } else {
            self.navigate_to_dir(path);
        }
    }

    fn handle_go_up(&mut self) {
        if let Some(ref zip_ctx) = self.dir.zip_ctx.clone() {
            if zip_ctx.inner_prefix.is_empty() {
                // At zip root — go back to the real parent directory
                let parent = zip_ctx
                    .archive_path
                    .parent()
                    .unwrap_or(std::path::Path::new("/"))
                    .to_path_buf();
                self.navigate_to_dir(parent);
            } else {
                // Strip the last path component from the prefix
                let trimmed = zip_ctx.inner_prefix.trim_end_matches('/');
                let new_prefix = match trimmed.rfind('/') {
                    Some(pos) => trimmed[..=pos].to_string(),
                    None => String::new(),
                };
                self.navigate_to_zip(zip_ctx.archive_path.clone(), new_prefix);
            }
        } else if let Some(parent) = self.state.current_dir.parent() {
            let parent = parent.to_path_buf();
            self.navigate_to_dir(parent);
        }
    }

    fn recompute_filter(&mut self) {
        if !self.state.filter_active {
            return;
        }
        let query = self.state.filter_text.to_lowercase();
        self.state.filtered_indices = (0..self.dir.entries.len())
            .filter(|&i| {
                let name = &self.dir.entries[i].name;
                name.to_lowercase().contains(&query)
            })
            .collect();
        // Clamp selected_index
        let count = self.state.filtered_indices.len();
        if count > 0 {
            if self.state.selected_index >= count {
                self.state.selected_index = count - 1;
            }
        } else {
            self.state.selected_index = 0;
        }
    }

    fn refresh_directory(&mut self) {
        let current_name = self.dir.entries.get(self.state.resolved_index())
            .map(|e| e.name.clone());
        self.dir = DirState::scan(&self.state.current_dir);
        self.dir.sort(self.state.sort_field, self.state.sort_ascending);
        // Restore selection by name
        if let Some(name) = current_name {
            if let Some(pos) = self.dir.entries.iter().position(|e| e.name == name) {
                self.state.selected_index = pos;
            }
        }
        self.state.clear_multi_select();
        self.cache.clear_all();
        self.pipeline.set_generation(self.cache.generation);
        self.metadata_cache.clear();
    }

    fn open_in_tool(&mut self, name: &str, path: &std::path::Path) {
        let (cmd, extra_args) = self.config.tool_command(name);
        let mut command = std::process::Command::new(&cmd);
        for arg in &extra_args {
            command.arg(arg);
        }
        command.arg(path);
        match command.spawn() {
            Ok(_) => {
                self.state.status_message = Some((
                    format!("Opened in {}", name),
                    Instant::now(),
                ));
            }
            Err(e) => {
                self.state.status_message = Some((
                    format!("Failed to open {}: {}", name, e),
                    Instant::now(),
                ));
            }
        }
    }

    fn rotate_image(&mut self, path: &std::path::Path, clockwise: bool) {
        match image::open(path) {
            Ok(img) => {
                let rotated = if clockwise {
                    img.rotate90()
                } else {
                    img.rotate270()
                };
                if let Err(e) = rotated.save(path) {
                    self.state.status_message = Some((
                        format!("Failed to save: {}", e),
                        Instant::now(),
                    ));
                    return;
                }
                // Evict from all caches
                let path_buf = path.to_path_buf();
                self.cache.full.pop(&path_buf);
                self.cache.thumbs.pop(&path_buf);
                self.cache.animated.pop(&path_buf);
                self.cache.image_dimensions.remove(&path_buf);
                self.cache.pending_full.remove(&path_buf);
                self.cache.pending_thumb.remove(&path_buf);
                self.metadata_cache.remove(&path_buf);
                self.state.status_message = Some((
                    format!("Rotated {}", if clockwise { "right" } else { "left" }),
                    Instant::now(),
                ));
            }
            Err(e) => {
                self.state.status_message = Some((
                    format!("Failed to open image: {}", e),
                    Instant::now(),
                ));
            }
        }
    }

    fn copy_path_to_clipboard(&mut self, path: &std::path::Path) {
        let path_str = path.to_string_lossy().to_string();
        match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&path_str)) {
            Ok(_) => {
                self.state.status_message = Some((
                    format!("Copied: {}", path_str),
                    Instant::now(),
                ));
            }
            Err(e) => {
                self.state.status_message = Some((
                    format!("Clipboard error: {}", e),
                    Instant::now(),
                ));
            }
        }
    }

    fn get_selected_paths(&self) -> Vec<PathBuf> {
        if self.state.multi_select_count() > 0 {
            self.state.multi_selected.iter()
                .filter_map(|&i| self.dir.entries.get(i))
                .filter(|e| !e.is_dir)
                .map(|e| e.path.clone())
                .collect()
        } else {
            let abs = self.state.resolved_index();
            if let Some(entry) = self.dir.entries.get(abs) {
                if !entry.is_dir {
                    return vec![entry.path.clone()];
                }
            }
            Vec::new()
        }
    }

    fn dispatch_edit_action(&mut self, action: EditAction) {
        let abs_idx = self.state.resolved_index();
        let entry = match self.dir.entries.get(abs_idx) {
            Some(e) => e,
            None => return,
        };
        let is_zip_source = entry.zip_source.is_some();
        let path = entry.path.clone();
        let name = entry.name.clone();

        match action {
            EditAction::Rename => {
                if is_zip_source { return; }
                self.state.dialog = Some(DialogState::Rename {
                    original_path: path,
                    new_name: name,
                });
            }
            EditAction::CopyTo => {
                let paths = self.get_selected_paths();
                if paths.is_empty() { return; }
                let default_dest = self.state.current_dir.to_string_lossy().to_string();
                self.state.dialog = Some(DialogState::CopyTo {
                    source_paths: paths,
                    dest_path: default_dest,
                });
            }
            EditAction::ViewMetadata => {
                self.state.show_metadata_popup = !self.state.show_metadata_popup;
            }
            EditAction::RotateLeft => {
                if is_zip_source { return; }
                self.rotate_image(&path, false);
            }
            EditAction::RotateRight => {
                if is_zip_source { return; }
                self.rotate_image(&path, true);
            }
            EditAction::OpenInGimp => {
                if is_zip_source { return; }
                self.open_in_tool("gimp", &path);
            }
            EditAction::OpenInKrita => {
                if is_zip_source { return; }
                self.open_in_tool("krita", &path);
            }
            EditAction::Delete => {
                if is_zip_source { return; }
                let paths = self.get_selected_paths();
                if paths.is_empty() { return; }
                let names: Vec<String> = paths.iter()
                    .filter_map(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .collect();
                self.state.dialog = Some(DialogState::DeleteConfirm { paths, names });
            }
            EditAction::CopyPath => {
                self.copy_path_to_clipboard(&path);
            }
            EditAction::Compare => {
                // Multi-select exactly 2 → compare those
                if self.state.multi_select_count() == 2 {
                    let indices: Vec<usize> = self.state.multi_selected.iter().copied().collect();
                    let left = indices[0];
                    let right = indices[1];
                    if !self.dir.entries[left].is_dir && !self.dir.entries[right].is_dir {
                        self.state.previous_browse_mode = if self.state.view_mode == ViewMode::List {
                            BrowseMode::List
                        } else {
                            BrowseMode::Grid
                        };
                        self.state.compare = Some(CompareState::new(left, right));
                        self.state.view_mode = ViewMode::Compare;
                    }
                } else {
                    // Compare current with next image
                    let mut next = None;
                    for idx in (abs_idx + 1)..self.dir.entries.len() {
                        if !self.dir.entries[idx].is_dir {
                            next = Some(idx);
                            break;
                        }
                    }
                    if let Some(right) = next {
                        self.state.previous_browse_mode = if self.state.view_mode == ViewMode::List {
                            BrowseMode::List
                        } else {
                            BrowseMode::Grid
                        };
                        self.state.compare = Some(CompareState::new(abs_idx, right));
                        self.state.view_mode = ViewMode::Compare;
                    }
                }
            }
        }
    }

    fn draw_dialogs(&mut self, ctx: &egui::Context) {
        let dialog = match self.state.dialog.take() {
            Some(d) => d,
            None => return,
        };

        match dialog {
            DialogState::Rename { original_path, mut new_name } => {
                let mut submitted = false;
                let mut cancelled = false;
                egui::Window::new("Rename")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Rename: {}", original_path.file_name().unwrap_or_default().to_string_lossy()));
                        let response = ui.text_edit_singleline(&mut new_name);
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            submitted = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Rename").clicked() {
                                submitted = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancelled = true;
                            }
                        });
                    });

                if submitted {
                    let new_path = original_path.parent().unwrap_or(std::path::Path::new(".")).join(&new_name);
                    match std::fs::rename(&original_path, &new_path) {
                        Ok(_) => {
                            self.state.status_message = Some((
                                format!("Renamed to {}", new_name),
                                Instant::now(),
                            ));
                            self.refresh_directory();
                            // Try to re-select the renamed file
                            if let Some(pos) = self.dir.entries.iter().position(|e| e.name == new_name) {
                                self.state.selected_index = pos;
                            }
                        }
                        Err(e) => {
                            self.state.status_message = Some((
                                format!("Rename failed: {}", e),
                                Instant::now(),
                            ));
                        }
                    }
                } else if !cancelled {
                    self.state.dialog = Some(DialogState::Rename { original_path, new_name });
                }
            }
            DialogState::CopyTo { source_paths, mut dest_path } => {
                let mut submitted = false;
                let mut cancelled = false;
                egui::Window::new("Copy To")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Copy {} file(s) to:", source_paths.len()));
                        let response = ui.text_edit_singleline(&mut dest_path);
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            submitted = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Copy").clicked() {
                                submitted = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancelled = true;
                            }
                        });
                    });

                if submitted {
                    let dest = std::path::PathBuf::from(&dest_path);
                    if dest.is_dir() {
                        let mut ok = 0;
                        let mut fail = 0;
                        for src in &source_paths {
                            let fname = src.file_name().unwrap_or_default();
                            let target = dest.join(fname);
                            // For zip-sourced files, read from zip
                            if let Some(entry) = self.dir.entries.iter().find(|e| e.path == *src) {
                                if let Some(ref zs) = entry.zip_source {
                                    if let Ok(file) = std::fs::File::open(&zs.archive_path) {
                                        if let Ok(mut archive) = zip::ZipArchive::new(file) {
                                            if let Ok(mut zf) = archive.by_name(&zs.entry_name) {
                                                if let Ok(mut out) = std::fs::File::create(&target) {
                                                    if std::io::copy(&mut zf, &mut out).is_ok() {
                                                        ok += 1;
                                                        continue;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    fail += 1;
                                    continue;
                                }
                            }
                            match std::fs::copy(src, &target) {
                                Ok(_) => ok += 1,
                                Err(_) => fail += 1,
                            }
                        }
                        let msg = if fail == 0 {
                            format!("Copied {} file(s)", ok)
                        } else {
                            format!("Copied {}, {} failed", ok, fail)
                        };
                        self.state.status_message = Some((msg, Instant::now()));
                    } else {
                        self.state.status_message = Some((
                            "Destination is not a directory".to_string(),
                            Instant::now(),
                        ));
                    }
                } else if !cancelled {
                    self.state.dialog = Some(DialogState::CopyTo { source_paths, dest_path });
                }
            }
            DialogState::DeleteConfirm { paths, names } => {
                let mut confirmed = false;
                let mut cancelled = false;
                egui::Window::new("Delete")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label("Move to trash?");
                        let display: String = if names.len() <= 5 {
                            names.join(", ")
                        } else {
                            format!("{}, ... ({} files)", names[..3].join(", "), names.len())
                        };
                        ui.label(&display);
                        ui.horizontal(|ui| {
                            if ui.button("Delete").clicked() {
                                confirmed = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancelled = true;
                            }
                        });
                    });

                if confirmed {
                    let mut ok = 0;
                    let mut fail = 0;
                    for p in &paths {
                        match trash::delete(p) {
                            Ok(_) => ok += 1,
                            Err(_) => fail += 1,
                        }
                    }
                    let msg = if fail == 0 {
                        format!("Deleted {} file(s)", ok)
                    } else {
                        format!("Deleted {}, {} failed", ok, fail)
                    };
                    self.state.status_message = Some((msg, Instant::now()));
                    self.refresh_directory();
                } else if !cancelled {
                    self.state.dialog = Some(DialogState::DeleteConfirm { paths, names });
                }
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }

        // Use the first dropped file/dir
        if let Some(dropped_path) = dropped[0].path.as_ref() {
            let path = dropped_path.clone();
            if path.is_dir() {
                self.navigate_to_dir(path);
            } else if dir::is_zip(&path) && path.is_file() {
                self.navigate_to_zip(path, String::new());
            } else if path.is_file() {
                // Navigate to parent dir, select the file
                if let Some(parent) = path.parent() {
                    let parent = parent.to_path_buf();
                    self.navigate_to_dir(parent);
                    if let Some(idx) = self.dir.entries.iter().position(|e| e.path == path) {
                        self.state.selected_index = idx;
                    }
                }
            }
        }
    }
}

impl eframe::App for RivApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Fully opaque dark background — reduces visible artifacts during resize
        [0.05, 0.05, 0.05, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Maximize on first frame (more reliable than builder hint on some WMs)
        if self.first_frame {
            self.first_frame = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }

        // Always request repaint so the surface stays current during window resize
        ctx.request_repaint();

        // Handle drag-and-drop
        self.handle_dropped_files(ctx);

        self.poll_decode_results(ctx);

        // Handle input before rendering (avoids egui consuming keys)
        let action = input::handle_input(ctx, &mut self.state, &self.dir);

        if action.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if action.sort_changed {
            // Remember current filename, re-sort, restore selection
            let current_name = self.dir.entries.get(self.state.resolved_index())
                .map(|e| e.name.clone());
            self.dir.sort(self.state.sort_field, self.state.sort_ascending);
            if let Some(name) = current_name {
                if let Some(pos) = self.dir.entries.iter().position(|e| e.name == name) {
                    if self.state.filter_active {
                        self.recompute_filter();
                        // Find the visible index for this absolute position
                        if let Some(vis) = self.state.filtered_indices.iter().position(|&i| i == pos) {
                            self.state.selected_index = vis;
                        }
                    } else {
                        self.state.selected_index = pos;
                    }
                }
            }
            self.state.scroll_to_selected = true;
        }

        if let Some(idx) = action.enter_directory.or(self.state.double_click_enter.take()) {
            self.handle_enter_directory(idx);
        }

        if action.go_up {
            self.handle_go_up();
        }

        if action.focus_path_bar {
            self.state.path_bar_focused = true;
            self.state.sync_path_bar();
        }

        if action.start_filter {
            self.state.filter_active = true;
            self.state.filter_text.clear();
            self.state.filtered_indices = (0..self.dir.entries.len()).collect();
            self.state.selected_index = 0;
            self.state.scroll_to_selected = true;
        }

        // Recompute filter each frame while active
        if self.state.filter_active {
            self.recompute_filter();
        }

        self.prefetch();

        // Set window title
        let title = format!(
            "riv - {}",
            self.state.current_dir.file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // Menu bar (only in browse modes)
        if self.state.view_mode != ViewMode::Single && self.state.view_mode != ViewMode::Compare {
            egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("View", |ui| {
                        // View mode switching
                        let is_list = self.state.view_mode == ViewMode::List;
                        let is_grid = self.state.view_mode == ViewMode::Grid;
                        if ui.radio(is_list, "List View").clicked() {
                            self.state.view_mode = ViewMode::List;
                            self.state.scroll_to_selected = true;
                            ui.close();
                        }
                        if ui.radio(is_grid, "Grid View").clicked() {
                            self.state.view_mode = ViewMode::Grid;
                            self.state.scroll_to_selected = true;
                            ui.close();
                        }

                        ui.separator();

                        // Zoom Mode submenu
                        ui.menu_button("Zoom Mode", |ui| {
                            for mode in [ZoomMode::FitWindow, ZoomMode::FitWidth, ZoomMode::FitHeight, ZoomMode::Original] {
                                if ui.radio_value(&mut self.state.zoom_mode, mode, mode.label()).clicked() {
                                    self.state.default_zoom_mode = mode;
                                    self.state.zoom_level = 1.0;
                                    self.state.pan_offset = egui::Vec2::ZERO;
                                    ui.close();
                                }
                            }
                        });

                        // Interpolation submenu
                        ui.menu_button("Interpolation", |ui| {
                            for filter in [ImageFilter::Nearest, ImageFilter::Linear] {
                                let prev = self.state.image_filter;
                                if ui.radio_value(&mut self.state.image_filter, filter, filter.label()).clicked() {
                                    if prev != self.state.image_filter {
                                        // Clear full image + animated caches to re-upload with new filter
                                        self.cache.clear_full();
                                        self.cache.clear_animated();
                                    }
                                    ui.close();
                                }
                            }
                        });

                        // Image Info toggle
                        let info_label = if self.state.show_info_overlay { "Image Info  [on]" } else { "Image Info" };
                        if ui.button(info_label).clicked() {
                            self.state.show_info_overlay = !self.state.show_info_overlay;
                            ui.close();
                        }

                        ui.separator();

                        // Sort submenu
                        ui.menu_button("Sort", |ui| {
                            for field in [SortField::Name, SortField::Size, SortField::Date] {
                                if ui.radio_value(&mut self.state.sort_field, field, field.label()).clicked() {
                                    let current_name = self.dir.entries.get(self.state.resolved_index())
                                        .map(|e| e.name.clone());
                                    self.dir.sort(self.state.sort_field, self.state.sort_ascending);
                                    if let Some(name) = current_name {
                                        if let Some(pos) = self.dir.entries.iter().position(|e| e.name == name) {
                                            self.state.selected_index = pos;
                                        }
                                    }
                                    self.state.scroll_to_selected = true;
                                    ui.close();
                                }
                            }
                            ui.separator();
                            let asc_label = if self.state.sort_ascending { "Ascending  [\u{25b2}]" } else { "Ascending" };
                            let desc_label = if !self.state.sort_ascending { "Descending [\u{25bc}]" } else { "Descending" };
                            if ui.radio(self.state.sort_ascending, asc_label).clicked() {
                                self.state.sort_ascending = true;
                                let current_name = self.dir.entries.get(self.state.resolved_index())
                                    .map(|e| e.name.clone());
                                self.dir.sort(self.state.sort_field, self.state.sort_ascending);
                                if let Some(name) = current_name {
                                    if let Some(pos) = self.dir.entries.iter().position(|e| e.name == name) {
                                        self.state.selected_index = pos;
                                    }
                                }
                                self.state.scroll_to_selected = true;
                                ui.close();
                            }
                            if ui.radio(!self.state.sort_ascending, desc_label).clicked() {
                                self.state.sort_ascending = false;
                                let current_name = self.dir.entries.get(self.state.resolved_index())
                                    .map(|e| e.name.clone());
                                self.dir.sort(self.state.sort_field, self.state.sort_ascending);
                                if let Some(name) = current_name {
                                    if let Some(pos) = self.dir.entries.iter().position(|e| e.name == name) {
                                        self.state.selected_index = pos;
                                    }
                                }
                                self.state.scroll_to_selected = true;
                                ui.close();
                            }
                        });
                    });

                    ui.menu_button("Edit", |ui| {
                        let abs_idx = self.state.resolved_index();
                        let has_selection = abs_idx < self.dir.entries.len();
                        let is_dir = has_selection && self.dir.entries[abs_idx].is_dir;
                        let multi = self.state.multi_select_count() > 0;
                        let single_disabled = !has_selection || is_dir || multi;
                        let file_disabled = !has_selection || is_dir;

                        if ui.add_enabled(!single_disabled, egui::Button::new("Rename")).clicked() {
                            self.state.pending_edit_action = Some(EditAction::Rename);
                            ui.close();
                        }
                        if ui.add_enabled(!file_disabled, egui::Button::new("Copy to...")).clicked() {
                            self.state.pending_edit_action = Some(EditAction::CopyTo);
                            ui.close();
                        }
                        if ui.add_enabled(!single_disabled, egui::Button::new("Metadata")).clicked() {
                            self.state.pending_edit_action = Some(EditAction::ViewMetadata);
                            ui.close();
                        }
                        ui.separator();
                        if ui.add_enabled(!single_disabled, egui::Button::new("Rotate Left")).clicked() {
                            self.state.pending_edit_action = Some(EditAction::RotateLeft);
                            ui.close();
                        }
                        if ui.add_enabled(!single_disabled, egui::Button::new("Rotate Right")).clicked() {
                            self.state.pending_edit_action = Some(EditAction::RotateRight);
                            ui.close();
                        }
                        ui.separator();
                        if ui.add_enabled(!file_disabled, egui::Button::new("Open in GIMP")).clicked() {
                            self.state.pending_edit_action = Some(EditAction::OpenInGimp);
                            ui.close();
                        }
                        if ui.add_enabled(!file_disabled, egui::Button::new("Open in Krita")).clicked() {
                            self.state.pending_edit_action = Some(EditAction::OpenInKrita);
                            ui.close();
                        }
                        ui.separator();
                        if ui.add_enabled(!file_disabled, egui::Button::new("Delete")).clicked() {
                            self.state.pending_edit_action = Some(EditAction::Delete);
                            ui.close();
                        }
                        if ui.add_enabled(has_selection, egui::Button::new("Copy Path")).clicked() {
                            self.state.pending_edit_action = Some(EditAction::CopyPath);
                            ui.close();
                        }
                    });

                    ui.menu_button("Go", |ui| {
                        if ui.button("Parent Directory").clicked() {
                            self.handle_go_up();
                            ui.close();
                        }
                        let abs_idx = self.state.resolved_index();
                        let can_enter = abs_idx < self.dir.entries.len()
                            && self.dir.entries[abs_idx].is_dir;
                        if ui.add_enabled(can_enter, egui::Button::new("Enter Directory")).clicked() {
                            self.handle_enter_directory(abs_idx);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Next Image").clicked() {
                            let total = self.state.visible_count(&self.dir);
                            if self.state.selected_index + 1 < total {
                                self.state.selected_index += 1;
                                self.state.scroll_to_selected = true;
                                self.state.reset_zoom();
                            }
                            ui.close();
                        }
                        if ui.button("Previous Image").clicked() {
                            if self.state.selected_index > 0 {
                                self.state.selected_index -= 1;
                                self.state.scroll_to_selected = true;
                                self.state.reset_zoom();
                            }
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("First").clicked() {
                            self.state.selected_index = 0;
                            self.state.scroll_to_selected = true;
                            self.state.reset_zoom();
                            ui.close();
                        }
                        let total = self.state.visible_count(&self.dir);
                        if ui.button("Last").clicked() {
                            self.state.selected_index = total.saturating_sub(1);
                            self.state.scroll_to_selected = true;
                            self.state.reset_zoom();
                            ui.close();
                        }
                    });
                });
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let avail = ui.available_size();
            let status_height = 22.0;
            let path_bar_height = 24.0;
            let is_browse = matches!(self.state.view_mode, ViewMode::List | ViewMode::Grid);
            let filter_bar_height = if self.state.filter_active && is_browse {
                24.0
            } else {
                0.0
            };

            // Path bar
            if is_browse {
                let path_bar_rect = ui.allocate_exact_size(
                    egui::vec2(avail.x, path_bar_height),
                    egui::Sense::hover(),
                ).0;

                let mut path_bar_ui = ui.new_child(
                    egui::UiBuilder::new().max_rect(path_bar_rect),
                );

                path_bar_ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let id = egui::Id::new("path_bar_edit");
                    let response = ui.add_sized(
                        egui::vec2(ui.available_width(), path_bar_height),
                        egui::TextEdit::singleline(&mut self.state.path_bar_text)
                            .id(id)
                            .font(egui::FontId::monospace(13.0))
                            .margin(egui::Margin::symmetric(4, 2)),
                    );

                    // Handle lost focus first — Enter submits in singleline
                    // TextEdit, or user clicked away. Escape is handled in
                    // input.rs before the TextEdit consumes it.
                    if response.lost_focus() {
                        self.state.path_bar_focused = false;
                        let typed_path = std::path::PathBuf::from(&self.state.path_bar_text);
                        if typed_path.is_dir() {
                            let canonical = typed_path
                                .canonicalize()
                                .unwrap_or(typed_path);
                            self.navigate_to_dir(canonical);
                        } else if dir::is_zip(&typed_path) && typed_path.is_file() {
                            self.navigate_to_zip(typed_path, String::new());
                        } else {
                            self.state.sync_path_bar();
                        }
                    } else {
                        // Only request focus if we didn't just lose it
                        if self.state.path_bar_focused && !response.has_focus() {
                            response.request_focus();
                        }
                        self.state.path_bar_focused = response.has_focus();
                    }
                });

                // Subtle separator
                ui.add(egui::Separator::default().spacing(0.0));
            }

            let avail = ui.available_size();
            let content_height = avail.y - status_height - filter_bar_height;

            // Content area
            let content_rect = ui.allocate_exact_size(
                egui::vec2(avail.x, content_height),
                egui::Sense::hover(),
            ).0;

            let mut content_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(content_rect)
            );

            match self.state.view_mode {
                ViewMode::List | ViewMode::Grid => {
                    let half_w = (content_rect.width() / 2.0).floor();
                    let sep_w = 1.0;

                    let left_rect = egui::Rect::from_min_size(
                        content_rect.min,
                        egui::vec2(half_w - sep_w, content_rect.height()),
                    );
                    let right_rect = egui::Rect::from_min_size(
                        egui::pos2(content_rect.min.x + half_w + sep_w, content_rect.min.y),
                        egui::vec2(content_rect.width() - half_w - sep_w, content_rect.height()),
                    );

                    self.state.preview_rect = Some(right_rect);

                    // Vertical separator
                    let sep_x = content_rect.min.x + half_w;
                    content_ui.painter().vline(
                        sep_x,
                        content_rect.y_range(),
                        egui::Stroke::new(sep_w, content_ui.visuals().widgets.noninteractive.bg_stroke.color),
                    );

                    // Focus border on hovered pane
                    if let Some(pos) = content_ui.input(|i| i.pointer.latest_pos()) {
                        let accent = egui::Color32::from_rgb(80, 130, 200);
                        let border_stroke = egui::Stroke::new(1.0, accent);
                        if left_rect.contains(pos) {
                            content_ui.painter().rect_stroke(left_rect, 0.0, border_stroke, egui::StrokeKind::Inside);
                        } else if right_rect.contains(pos) {
                            content_ui.painter().rect_stroke(right_rect, 0.0, border_stroke, egui::StrokeKind::Inside);
                        }
                    }

                    // Left panel: list or grid
                    let mut left_ui = content_ui.new_child(
                        egui::UiBuilder::new().max_rect(left_rect),
                    );

                    if self.state.view_mode == ViewMode::List {
                        views::list::draw(&mut left_ui, &mut self.state, &self.dir);
                    } else {
                        views::grid::draw(
                            &mut left_ui,
                            &mut self.state,
                            &self.dir,
                            &mut self.cache,
                            &self.pipeline,
                        );
                    }

                    // Right panel: preview
                    let mut right_ui = content_ui.new_child(
                        egui::UiBuilder::new().max_rect(right_rect),
                    );
                    let preview_response = views::preview::draw(
                        &mut right_ui,
                        &mut self.state,
                        &self.dir,
                        &mut self.cache,
                        &self.pipeline,
                    );

                    // Focus handling (list/grid items handle unfocusing via their click handlers)
                    if preview_response.clicked() {
                        self.state.preview_focused = true;
                    }
                }
                ViewMode::Single => {
                    views::single::draw(
                        &mut content_ui,
                        &mut self.state,
                        &self.dir,
                        &mut self.cache,
                        &self.pipeline,
                    );
                }
                ViewMode::Compare => {
                    if let Some(ref compare) = self.state.compare {
                        views::compare::draw(
                            &mut content_ui,
                            compare,
                            &self.dir,
                            &mut self.cache,
                            &self.pipeline,
                        );
                    }
                }
            }

            // Filter bar
            if self.state.filter_active && is_browse {
                let filter_rect = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), filter_bar_height),
                    egui::Sense::hover(),
                ).0;

                let mut filter_ui = ui.new_child(
                    egui::UiBuilder::new().max_rect(filter_rect),
                );

                filter_ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label("/");
                    let id = egui::Id::new("filter_bar_edit");
                    let response = ui.add_sized(
                        egui::vec2(ui.available_width(), filter_bar_height),
                        egui::TextEdit::singleline(&mut self.state.filter_text)
                            .id(id)
                            .font(egui::FontId::monospace(13.0))
                            .margin(egui::Margin::symmetric(4, 2)),
                    );
                    if !response.has_focus() {
                        response.request_focus();
                    }
                });
            }

            // Dispatch pending edit action
            if let Some(action) = self.state.pending_edit_action.take() {
                self.dispatch_edit_action(action);
            }

            // Metadata inline overlay (e key)
            if self.state.show_metadata_overlay {
                let abs_idx = self.state.resolved_index();
                if let Some(entry) = self.dir.entries.get(abs_idx) {
                    if !entry.is_dir && entry.zip_source.is_none() {
                        let path = entry.path.clone();
                        if !self.metadata_cache.contains_key(&path) {
                            let data = metadata::read_metadata(&path);
                            self.metadata_cache.insert(path.clone(), data);
                        }
                        let meta = self.metadata_cache.get(&path).and_then(|o| o.as_ref());
                        metadata::draw_metadata_overlay(ui, content_rect, meta);
                    }
                }
            }

            // Status bar
            status_bar::draw(ui, &self.state, &self.dir, &self.cache);
        });

        // Metadata popup window
        if self.state.show_metadata_popup {
            let abs_idx = self.state.resolved_index();
            let meta = if let Some(entry) = self.dir.entries.get(abs_idx) {
                if !entry.is_dir && entry.zip_source.is_none() {
                    let path = entry.path.clone();
                    if !self.metadata_cache.contains_key(&path) {
                        let data = metadata::read_metadata(&path);
                        self.metadata_cache.insert(path.clone(), data);
                    }
                    self.metadata_cache.get(&path).and_then(|o| o.as_ref()).cloned()
                } else {
                    None
                }
            } else {
                None
            };
            metadata::draw_metadata_popup(ctx, meta.as_ref(), &mut self.state.show_metadata_popup);
        }

        // Expire old status messages
        if let Some((_, when)) = &self.state.status_message {
            if when.elapsed().as_secs_f32() >= 3.0 {
                self.state.status_message = None;
            }
        }

        // Draw modal dialogs
        self.draw_dialogs(ctx);
    }
}

use crate::dir::DirState;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Name,
    Size,
    Date,
}

impl SortField {
    pub fn cycle(self) -> Self {
        match self {
            SortField::Name => SortField::Size,
            SortField::Size => SortField::Date,
            SortField::Date => SortField::Name,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortField::Name => "Name",
            SortField::Size => "Size",
            SortField::Date => "Date",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Grid,
    Single,
    Compare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseMode {
    List,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomMode {
    FitWindow,
    FitWidth,
    FitHeight,
    Original,
    Custom,
}

impl ZoomMode {
    pub fn cycle(self) -> Self {
        match self {
            ZoomMode::FitWindow => ZoomMode::FitWidth,
            ZoomMode::FitWidth => ZoomMode::FitHeight,
            ZoomMode::FitHeight => ZoomMode::Original,
            ZoomMode::Original => ZoomMode::FitWindow,
            ZoomMode::Custom => ZoomMode::FitWindow,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ZoomMode::FitWindow => "Fit Window",
            ZoomMode::FitWidth => "Fit Width",
            ZoomMode::FitHeight => "Fit Height",
            ZoomMode::Original => "Original (1:1)",
            ZoomMode::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFilter {
    Nearest,
    Linear,
}

impl ImageFilter {
    pub fn label(self) -> &'static str {
        match self {
            ImageFilter::Nearest => "Nearest (Sharp)",
            ImageFilter::Linear => "Bilinear (Smooth)",
        }
    }

    pub fn texture_options(self) -> egui::TextureOptions {
        match self {
            ImageFilter::Nearest => egui::TextureOptions::NEAREST,
            ImageFilter::Linear => egui::TextureOptions::LINEAR,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditAction {
    Rename,
    CopyTo,
    ViewMetadata,
    RotateLeft,
    RotateRight,
    OpenInGimp,
    OpenInKrita,
    Delete,
    CopyPath,
    Compare,
}

#[derive(Debug, Clone)]
pub enum DialogState {
    Rename { original_path: PathBuf, new_name: String },
    CopyTo { source_paths: Vec<PathBuf>, dest_path: String },
    DeleteConfirm { paths: Vec<PathBuf>, names: Vec<String> },
}

pub struct DecodedFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareSide {
    Left,
    Right,
}

pub struct ZoomPan {
    pub zoom_mode: ZoomMode,
    pub zoom_level: f32,
    pub pan_offset: egui::Vec2,
    pub is_dragging: bool,
    pub last_drag_pos: Option<egui::Pos2>,
}

impl ZoomPan {
    pub fn new() -> Self {
        Self {
            zoom_mode: ZoomMode::FitWindow,
            zoom_level: 1.0,
            pan_offset: egui::Vec2::ZERO,
            is_dragging: false,
            last_drag_pos: None,
        }
    }

    pub fn reset(&mut self) {
        self.zoom_mode = ZoomMode::FitWindow;
        self.zoom_level = 1.0;
        self.pan_offset = egui::Vec2::ZERO;
    }
}

pub struct CompareState {
    pub left_index: usize,
    pub right_index: usize,
    pub active_side: CompareSide,
    pub left_zoom: ZoomPan,
    pub right_zoom: ZoomPan,
    pub locked: bool,
}

impl CompareState {
    pub fn new(left: usize, right: usize) -> Self {
        Self {
            left_index: left,
            right_index: right,
            active_side: CompareSide::Left,
            left_zoom: ZoomPan::new(),
            right_zoom: ZoomPan::new(),
            locked: true,
        }
    }

    pub fn active_zoom(&mut self) -> &mut ZoomPan {
        match self.active_side {
            CompareSide::Left => &mut self.left_zoom,
            CompareSide::Right => &mut self.right_zoom,
        }
    }
}

pub struct AppState {
    pub current_dir: PathBuf,
    pub view_mode: ViewMode,
    pub previous_browse_mode: BrowseMode,
    pub selected_index: usize,
    pub zoom_mode: ZoomMode,
    pub zoom_level: f32,
    pub pan_offset: egui::Vec2,
    pub is_dragging: bool,
    pub last_drag_pos: Option<egui::Pos2>,
    pub scroll_to_selected: bool,
    pub path_bar_focused: bool,
    pub path_bar_text: String,
    pub preview_focused: bool,
    pub double_click_enter: Option<usize>,
    pub sort_field: SortField,
    pub sort_ascending: bool,
    pub filter_active: bool,
    pub filter_text: String,
    pub filtered_indices: Vec<usize>,
    pub default_zoom_mode: ZoomMode,
    pub preview_rect: Option<egui::Rect>,
    pub pending_edit_action: Option<EditAction>,
    pub multi_selected: BTreeSet<usize>,
    pub last_click_index: Option<usize>,
    // Image info overlay
    pub show_info_overlay: bool,
    // Metadata overlay (inline)
    pub show_metadata_overlay: bool,
    // Metadata popup window
    pub show_metadata_popup: bool,
    // Modal dialog
    pub dialog: Option<DialogState>,
    // Transient status message
    pub status_message: Option<(String, Instant)>,
    // Animated GIF playback
    pub animation_frame: usize,
    pub animation_elapsed: f32,
    pub animation_playing: bool,
    // Configurable thumbnail size
    pub thumb_size: u32,
    // Compare mode state
    pub compare: Option<CompareState>,
    // Image interpolation filter
    pub image_filter: ImageFilter,
}

impl AppState {
    pub fn new(dir: PathBuf) -> Self {
        let path_bar_text = dir.to_string_lossy().into_owned();
        Self {
            current_dir: dir,
            view_mode: ViewMode::List,
            previous_browse_mode: BrowseMode::List,
            selected_index: 0,
            zoom_mode: ZoomMode::FitWindow,
            zoom_level: 1.0,
            pan_offset: egui::Vec2::ZERO,
            is_dragging: false,
            last_drag_pos: None,
            scroll_to_selected: false,
            path_bar_focused: false,
            path_bar_text,
            preview_focused: false,
            double_click_enter: None,
            sort_field: SortField::Name,
            sort_ascending: true,
            filter_active: false,
            filter_text: String::new(),
            filtered_indices: Vec::new(),
            default_zoom_mode: ZoomMode::FitWindow,
            preview_rect: None,
            pending_edit_action: None,
            multi_selected: BTreeSet::new(),
            last_click_index: None,
            show_info_overlay: false,
            show_metadata_overlay: false,
            show_metadata_popup: false,
            dialog: None,
            status_message: None,
            animation_frame: 0,
            animation_elapsed: 0.0,
            animation_playing: true,
            thumb_size: 160,
            compare: None,
            image_filter: ImageFilter::Nearest,
        }
    }

    pub fn sync_path_bar(&mut self) {
        self.path_bar_text = self.current_dir.to_string_lossy().into_owned();
    }

    pub fn reset_zoom(&mut self) {
        self.zoom_mode = self.default_zoom_mode;
        self.zoom_level = 1.0;
        self.pan_offset = egui::Vec2::ZERO;
    }

    pub fn is_multi_selected(&self, abs: usize) -> bool {
        self.multi_selected.contains(&abs)
    }

    pub fn toggle_select(&mut self, abs: usize) {
        if !self.multi_selected.remove(&abs) {
            self.multi_selected.insert(abs);
        }
    }

    pub fn select_range(&mut self, from: usize, to: usize) {
        let (lo, hi) = if from <= to { (from, to) } else { (to, from) };
        for i in lo..=hi {
            self.multi_selected.insert(i);
        }
    }

    pub fn clear_multi_select(&mut self) {
        self.multi_selected.clear();
    }

    pub fn multi_select_count(&self) -> usize {
        self.multi_selected.len()
    }

    /// Convert visible-space index to absolute dir.entries index.
    pub fn resolved_index(&self) -> usize {
        if self.filter_active && !self.filtered_indices.is_empty() {
            self.filtered_indices[self.selected_index.min(self.filtered_indices.len() - 1)]
        } else {
            self.selected_index
        }
    }

    /// Number of visible entries (filtered or total).
    pub fn visible_count(&self, dir: &DirState) -> usize {
        if self.filter_active {
            self.filtered_indices.len()
        } else {
            dir.entries.len()
        }
    }

    pub fn clear_filter(&mut self) {
        if self.filter_active {
            let abs = self.resolved_index();
            self.filter_active = false;
            self.filter_text.clear();
            self.filtered_indices.clear();
            self.selected_index = abs;
        }
    }
}

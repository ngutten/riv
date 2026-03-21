use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::state::SortField;

const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tiff", "tif",
    "avif", "svg", "svgz",
];

const PDF_EXTENSIONS: &[&str] = &["pdf"];

const RAW_EXTENSIONS: &[&str] = &[
    "cr2", "cr3", "nef", "arw", "orf", "rw2", "dng", "raf", "pef", "srw",
];

#[cfg(feature = "heif")]
const HEIF_EXTENSIONS: &[&str] = &["heif", "heic"];

#[derive(Clone, Debug)]
pub struct ZipContext {
    pub archive_path: PathBuf,
    pub inner_prefix: String, // "" for root, "subdir/" for subdirectory
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub zip_source: Option<ZipSource>,
}

#[derive(Debug, Clone)]
pub struct ZipSource {
    pub archive_path: PathBuf,
    pub entry_name: String,
}

pub struct DirState {
    pub entries: Vec<DirEntry>,
    pub image_count: usize,
    pub dir_count: usize,
    pub zip_ctx: Option<ZipContext>,
}

impl DirState {
    pub fn scan(dir: &Path) -> Self {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();

                // Skip hidden files
                if name.starts_with('.') {
                    continue;
                }

                let meta = entry.metadata().ok();
                let modified = meta.as_ref().and_then(|m| m.modified().ok());

                if path.is_dir() {
                    dirs.push(DirEntry {
                        path,
                        name,
                        is_dir: true,
                        size: 0,
                        modified,
                        zip_source: None,
                    });
                } else if is_zip(&path) {
                    dirs.push(DirEntry {
                        path,
                        name,
                        is_dir: true,
                        size: 0,
                        modified,
                        zip_source: None,
                    });
                } else if is_image(&path) {
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    files.push(DirEntry {
                        path,
                        name,
                        is_dir: false,
                        size,
                        modified,
                        zip_source: None,
                    });
                }
            }
        }

        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let dir_count = dirs.len();
        let image_count = files.len();

        let mut entries = Vec::with_capacity(1 + dirs.len() + files.len());

        // Prepend ".." entry if directory has a parent
        if let Some(parent) = dir.parent() {
            entries.push(DirEntry {
                path: parent.to_path_buf(),
                name: "..".to_string(),
                is_dir: true,
                size: 0,
                modified: None,
                zip_source: None,
            });
        }

        entries.append(&mut dirs);
        entries.append(&mut files);

        DirState {
            entries,
            image_count,
            dir_count,
            zip_ctx: None,
        }
    }

    pub fn scan_zip(ctx: &ZipContext) -> Self {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        if let Ok(file) = std::fs::File::open(&ctx.archive_path) {
            if let Ok(mut archive) = zip::ZipArchive::new(file) {
                let mut seen_dirs = HashSet::new();

                for i in 0..archive.len() {
                    let entry = match archive.by_index(i) {
                        Ok(e) => e,
                        Err(_) => continue,
                    };
                    let entry_name = entry.name().to_string();

                    // Only consider entries under the current prefix
                    if !entry_name.starts_with(&ctx.inner_prefix) {
                        continue;
                    }

                    let relative = &entry_name[ctx.inner_prefix.len()..];
                    if relative.is_empty() {
                        continue;
                    }

                    if let Some(slash_pos) = relative.find('/') {
                        // This is a subdirectory entry
                        let dir_name = &relative[..slash_pos];
                        if !dir_name.is_empty() && seen_dirs.insert(dir_name.to_string()) {
                            let synthetic_path = PathBuf::from(format!(
                                "{}!{}{}",
                                ctx.archive_path.display(),
                                ctx.inner_prefix,
                                dir_name
                            ));
                            dirs.push(DirEntry {
                                path: synthetic_path,
                                name: dir_name.to_string(),
                                is_dir: true,
                                size: 0,
                                modified: None,
                                zip_source: None,
                            });
                        }
                    } else {
                        // This is a file at the current level
                        let file_path = Path::new(relative);
                        if is_image(file_path) {
                            let synthetic_path = PathBuf::from(format!(
                                "{}!{}",
                                ctx.archive_path.display(),
                                entry_name
                            ));
                            files.push(DirEntry {
                                path: synthetic_path,
                                name: relative.to_string(),
                                is_dir: false,
                                size: entry.size(),
                                modified: None,
                                zip_source: Some(ZipSource {
                                    archive_path: ctx.archive_path.clone(),
                                    entry_name,
                                }),
                            });
                        }
                    }
                }
            }
        }

        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let dir_count = dirs.len();
        let image_count = files.len();

        let mut entries = Vec::with_capacity(1 + dirs.len() + files.len());

        // ".." entry — goes up within zip or back to real filesystem
        entries.push(DirEntry {
            path: if ctx.inner_prefix.is_empty() {
                // At zip root — go back to the real parent directory
                ctx.archive_path
                    .parent()
                    .unwrap_or(Path::new("/"))
                    .to_path_buf()
            } else {
                // Inside a subdirectory within the zip — synthetic path
                PathBuf::from(format!(
                    "{}!{}",
                    ctx.archive_path.display(),
                    ctx.inner_prefix.trim_end_matches('/')
                ))
            },
            name: "..".to_string(),
            is_dir: true,
            size: 0,
            modified: None,
            zip_source: None,
        });

        entries.append(&mut dirs);
        entries.append(&mut files);

        DirState {
            entries,
            image_count,
            dir_count,
            zip_ctx: Some(ctx.clone()),
        }
    }

    pub fn sort(&mut self, field: SortField, ascending: bool) {
        // Find where dirs end and files begin (skip ".." at index 0 if present)
        let start = if self.entries.first().map(|e| e.name == "..").unwrap_or(false) {
            1
        } else {
            0
        };
        let file_start = self.entries[start..].iter().position(|e| !e.is_dir).map(|p| p + start).unwrap_or(self.entries.len());

        let cmp = |a: &DirEntry, b: &DirEntry| {
            let ord = match field {
                SortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortField::Size => a.size.cmp(&b.size),
                SortField::Date => a.modified.cmp(&b.modified),
            };
            if ascending { ord } else { ord.reverse() }
        };

        self.entries[start..file_start].sort_by(|a, b| cmp(a, b));
        self.entries[file_start..].sort_by(|a, b| cmp(a, b));
    }

    pub fn image_entries(&self) -> impl Iterator<Item = (usize, &DirEntry)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.is_dir)
    }
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_lowercase();
            let s = lower.as_str();
            IMAGE_EXTENSIONS.contains(&s)
                || PDF_EXTENSIONS.contains(&s)
                || RAW_EXTENSIONS.contains(&s)
                || {
                    #[cfg(feature = "heif")]
                    { HEIF_EXTENSIONS.contains(&s) }
                    #[cfg(not(feature = "heif"))]
                    { false }
                }
        })
        .unwrap_or(false)
}

pub fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase() == "zip")
        .unwrap_or(false)
}

pub fn format_date(t: Option<SystemTime>) -> String {
    let t = match t {
        Some(t) => t,
        None => return String::new(),
    };
    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Manual conversion from epoch seconds to YYYY-MM-DD HH:MM
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hh = time_of_day / 3600;
    let mm = (time_of_day % 3600) / 60;

    // Days since epoch to y/m/d (civil from days algorithm)
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, m, d, hh, mm)
}

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

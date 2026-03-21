use egui::TextureHandle;
use lru::LruCache;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

pub struct AnimatedTextures {
    pub frames: Vec<TextureHandle>,
    pub delays: Vec<std::time::Duration>,
}

pub struct TextureCache {
    pub full: LruCache<PathBuf, TextureHandle>,
    pub thumbs: LruCache<PathBuf, TextureHandle>,
    pub pending_full: HashSet<PathBuf>,
    pub pending_thumb: HashSet<PathBuf>,
    pub generation: u64,
    pub image_dimensions: HashMap<PathBuf, (u32, u32)>,
    pub animated: LruCache<PathBuf, AnimatedTextures>,
}

impl TextureCache {
    pub fn new() -> Self {
        Self {
            full: LruCache::new(NonZeroUsize::new(15).unwrap()),
            thumbs: LruCache::new(NonZeroUsize::new(500).unwrap()),
            pending_full: HashSet::new(),
            pending_thumb: HashSet::new(),
            generation: 0,
            image_dimensions: HashMap::new(),
            animated: LruCache::new(NonZeroUsize::new(5).unwrap()),
        }
    }

    pub fn get_full(&mut self, path: &PathBuf) -> Option<&TextureHandle> {
        self.full.get(path)
    }

    pub fn get_thumb(&mut self, path: &PathBuf) -> Option<&TextureHandle> {
        self.thumbs.get(path)
    }

    pub fn get_animated(&mut self, path: &PathBuf) -> Option<&AnimatedTextures> {
        self.animated.get(path)
    }

    pub fn insert_full(&mut self, path: PathBuf, handle: TextureHandle) {
        self.pending_full.remove(&path);
        self.full.put(path, handle);
    }

    pub fn insert_thumb(&mut self, path: PathBuf, handle: TextureHandle) {
        self.pending_thumb.remove(&path);
        self.thumbs.put(path, handle);
    }

    pub fn insert_animated(&mut self, path: PathBuf, anim: AnimatedTextures) {
        self.pending_full.remove(&path);
        self.animated.put(path, anim);
    }

    pub fn is_pending(&self, path: &Path, is_thumbnail: bool) -> bool {
        if is_thumbnail {
            self.pending_thumb.contains(path)
        } else {
            self.pending_full.contains(path)
        }
    }

    pub fn mark_pending(&mut self, path: PathBuf, is_thumbnail: bool) {
        if is_thumbnail {
            self.pending_thumb.insert(path);
        } else {
            self.pending_full.insert(path);
        }
    }

    pub fn clear_all(&mut self) {
        self.full.clear();
        self.thumbs.clear();
        self.pending_full.clear();
        self.pending_thumb.clear();
        self.image_dimensions.clear();
        self.animated.clear();
        self.generation += 1;
    }
}

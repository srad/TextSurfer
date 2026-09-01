use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::{MAX_PREPARED_IMAGE_BYTES, MAX_PREPARED_IMAGES, PreparedImageKey};

#[derive(Default)]
pub(in crate::vga) struct PreparedImageCache {
    entries: HashMap<PreparedImageKey, Arc<[u8]>>,
    order: VecDeque<PreparedImageKey>,
    bytes: usize,
}

impl PreparedImageCache {
    pub(in crate::vga) fn get(&mut self, key: PreparedImageKey) -> Option<Arc<[u8]>> {
        let rgba = self.entries.get(&key).cloned()?;
        self.order.retain(|candidate| *candidate != key);
        self.order.push_back(key);
        Some(rgba)
    }

    pub(in crate::vga) fn insert(&mut self, key: PreparedImageKey, rgba: Arc<[u8]>) -> bool {
        if rgba.len() > MAX_PREPARED_IMAGE_BYTES {
            return false;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.len());
            self.order.retain(|candidate| *candidate != key);
        }
        while self.entries.len() >= MAX_PREPARED_IMAGES
            || self.bytes.saturating_add(rgba.len()) > MAX_PREPARED_IMAGE_BYTES
        {
            let Some(oldest) = self.order.pop_front() else {
                return false;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(removed.len());
            }
        }
        self.bytes = self.bytes.saturating_add(rgba.len());
        self.entries.insert(key, rgba);
        self.order.push_back(key);
        true
    }

    pub(in crate::vga) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.bytes = 0;
    }
}

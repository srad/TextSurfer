mod cache;
mod prepare;
mod worker;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::core::image::{DecodedImage, ImageAssetId};

pub use prepare::RasterImagePreparer;
pub use worker::{ImagePreparationPool, InlineImagePreparationQueue};

pub(in crate::vga) use cache::PreparedImageCache;

pub const MAX_PREPARED_IMAGE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PREPARED_IMAGES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PreparedImageKey {
    pub generation: u64,
    pub surface_epoch: u64,
    pub asset_id: ImageAssetId,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct ImagePreparationRequest {
    pub key: PreparedImageKey,
    pub image: DecodedImage,
}

#[derive(Clone, Debug)]
pub struct ImagePreparationResult {
    pub key: PreparedImageKey,
    pub rgba: Option<Arc<[u8]>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImagePreparationSubmitted {
    Queued,
    Duplicate,
    Refused,
    Closed,
}

#[derive(Clone, Debug)]
pub enum ImagePreparationPoll {
    Ready(ImagePreparationResult),
    Empty,
    Disconnected,
}

pub trait ImagePreparer: Send + Sync {
    fn prepare(&self, request: ImagePreparationRequest) -> ImagePreparationResult;
}

pub trait ImagePreparationQueue: Send + Sync {
    fn activate(&self, generation: u64, surface_epoch: u64);
    fn submit(&self, request: ImagePreparationRequest) -> ImagePreparationSubmitted;
    fn poll(&self) -> ImagePreparationPoll;
    fn signal(&self) -> Arc<ImagePreparationSignal>;
    fn shutdown(&self);
}

pub struct ImagePreparationSignal {
    pending: std::sync::atomic::AtomicUsize,
    ready: std::sync::atomic::AtomicBool,
}

impl ImagePreparationSignal {
    pub fn pending(&self) -> bool {
        self.pending.load(std::sync::atomic::Ordering::Acquire) > 0
    }

    pub fn ready(&self) -> bool {
        self.ready.load(std::sync::atomic::Ordering::Acquire)
    }
}

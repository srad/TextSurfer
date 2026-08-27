#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};
use image::{DynamicImage, ImageDecoder as _, ImageError, ImageFormat, ImageReader, Limits};

use crate::core::image::{DecodedImage, ImageDecodeError, ImageDecodeRequest, ImageDecoder};

pub const MAX_IMAGE_AXIS: u32 = 8192;
pub const MAX_IMAGE_PIXELS: u64 = 8_388_608;
pub const MAX_IMAGE_RGBA_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_DECODER_ALLOC: u64 = 64 * 1024 * 1024;

pub struct RasterImageDecoder;

#[derive(Clone, Debug)]
pub struct ImageDecodeJob {
    pub tab_id: u64,
    pub generation: u64,
    pub request: ImageDecodeRequest,
}

#[derive(Clone, Debug)]
pub struct ImageDecodePayload {
    pub tab_id: u64,
    pub generation: u64,
    pub asset_id: crate::core::image::ImageAssetId,
    pub revision: u64,
    pub result: Result<DecodedImage, ImageDecodeError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSubmitted {
    Queued,
    Duplicate,
    Refused,
    Closed,
}

#[derive(Clone, Debug)]
pub enum ImageDecodePoll {
    Ready(ImageDecodePayload),
    Empty,
    Disconnected,
}

pub trait ImageDecodeQueue: Send + Sync {
    fn submit(&self, job: ImageDecodeJob) -> ImageSubmitted;
    fn poll(&self) -> ImageDecodePoll;
    fn cancel(&self, tab_id: u64, generation: u64);
    fn shutdown(&self);
}

type JobKey = (u64, u64, crate::core::image::ImageAssetId, u64);

struct PoolChannels {
    jobs: Sender<ImageDecodeJob>,
    results: Receiver<ImageDecodePayload>,
}

pub struct ImageDecodePool {
    decoder: Arc<dyn ImageDecoder>,
    channels: Mutex<Option<PoolChannels>>,
    pending: Arc<Mutex<HashSet<JobKey>>>,
    canceled: Arc<Mutex<HashSet<(u64, u64)>>>,
    closed: AtomicBool,
}

impl ImageDecodePool {
    pub fn new(decoder: Arc<dyn ImageDecoder>) -> Self {
        Self {
            decoder,
            channels: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashSet::new())),
            canceled: Arc::new(Mutex::new(HashSet::new())),
            closed: AtomicBool::new(false),
        }
    }

    fn start(&self) {
        let mut channels = self.channels.lock().expect("image pool channels");
        if channels.is_some() {
            return;
        }
        let (jobs_tx, jobs_rx) = crossbeam_channel::bounded(128);
        let (results_tx, results_rx) = crossbeam_channel::bounded(128);
        let decoder = self.decoder.clone();
        let pending = self.pending.clone();
        let canceled = self.canceled.clone();
        std::thread::spawn(move || run_worker(decoder, jobs_rx, results_tx, pending, canceled));
        *channels = Some(PoolChannels {
            jobs: jobs_tx,
            results: results_rx,
        });
    }
}

impl ImageDecodeQueue for ImageDecodePool {
    fn submit(&self, job: ImageDecodeJob) -> ImageSubmitted {
        if self.closed.load(Ordering::Acquire) {
            return ImageSubmitted::Closed;
        }
        self.start();
        let jobs = self
            .channels
            .lock()
            .expect("image pool channels")
            .as_ref()
            .map(|channels| channels.jobs.clone());
        let Some(jobs) = jobs else {
            return ImageSubmitted::Closed;
        };
        let key = job_key(&job);
        let mut pending = self.pending.lock().expect("image pool pending");
        if !pending.insert(key) {
            return ImageSubmitted::Duplicate;
        }
        if pending.len() > 128 {
            pending.remove(&key);
            return ImageSubmitted::Refused;
        }
        match jobs.try_send(job) {
            Ok(()) => ImageSubmitted::Queued,
            Err(TrySendError::Full(_)) => {
                pending.remove(&key);
                ImageSubmitted::Refused
            }
            Err(TrySendError::Disconnected(_)) => {
                pending.remove(&key);
                ImageSubmitted::Closed
            }
        }
    }

    fn poll(&self) -> ImageDecodePoll {
        let results = self
            .channels
            .lock()
            .expect("image pool channels")
            .as_ref()
            .map(|channels| channels.results.clone());
        let Some(results) = results else {
            return ImageDecodePoll::Empty;
        };
        loop {
            match results.try_recv() {
                Ok(payload) => {
                    let key = (
                        payload.tab_id,
                        payload.generation,
                        payload.asset_id,
                        payload.revision,
                    );
                    let canceled = self
                        .canceled
                        .lock()
                        .expect("image pool canceled")
                        .contains(&(payload.tab_id, payload.generation));
                    finish_key(&self.pending, &self.canceled, key);
                    if canceled {
                        continue;
                    }
                    return ImageDecodePoll::Ready(payload);
                }
                Err(TryRecvError::Empty) => return ImageDecodePoll::Empty,
                Err(TryRecvError::Disconnected) => return ImageDecodePoll::Disconnected,
            }
        }
    }

    fn cancel(&self, tab_id: u64, generation: u64) {
        if self
            .pending
            .lock()
            .expect("image pool pending")
            .iter()
            .any(|key| key.0 == tab_id && key.1 == generation)
        {
            self.canceled
                .lock()
                .expect("image pool canceled")
                .insert((tab_id, generation));
        }
    }

    fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.channels.lock().expect("image pool channels").take();
    }
}

fn job_key(job: &ImageDecodeJob) -> JobKey {
    (
        job.tab_id,
        job.generation,
        job.request.asset_id,
        job.request.revision,
    )
}

fn run_worker(
    decoder: Arc<dyn ImageDecoder>,
    jobs: Receiver<ImageDecodeJob>,
    results: Sender<ImageDecodePayload>,
    pending: Arc<Mutex<HashSet<JobKey>>>,
    canceled: Arc<Mutex<HashSet<(u64, u64)>>>,
) {
    while let Ok(job) = jobs.recv() {
        let key = job_key(&job);
        if canceled
            .lock()
            .expect("image pool canceled")
            .contains(&(job.tab_id, job.generation))
        {
            finish_key(&pending, &canceled, key);
            continue;
        }
        let payload = ImageDecodePayload {
            tab_id: job.tab_id,
            generation: job.generation,
            asset_id: job.request.asset_id,
            revision: job.request.revision,
            result: decoder.decode(job.request),
        };
        if results.send(payload).is_err() {
            break;
        }
    }
}

fn finish_key(
    pending: &Mutex<HashSet<JobKey>>,
    canceled: &Mutex<HashSet<(u64, u64)>>,
    key: JobKey,
) {
    let generation = (key.0, key.1);
    let mut pending = pending.lock().expect("image pool pending");
    pending.remove(&key);
    let remains = pending
        .iter()
        .any(|candidate| (candidate.0, candidate.1) == generation);
    drop(pending);
    if !remains {
        canceled
            .lock()
            .expect("image pool canceled")
            .remove(&generation);
    }
}

impl ImageDecoder for RasterImageDecoder {
    fn decode(&self, request: ImageDecodeRequest) -> Result<DecodedImage, ImageDecodeError> {
        let mut reader = ImageReader::new(Cursor::new(request.bytes))
            .with_guessed_format()
            .map_err(|_| ImageDecodeError::Invalid)?;
        match reader.format() {
            Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif) => {}
            Some(_) => return Err(ImageDecodeError::UnsupportedFormat),
            None => return Err(ImageDecodeError::UnknownFormat),
        }
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_AXIS);
        limits.max_image_height = Some(MAX_IMAGE_AXIS);
        limits.max_alloc = Some(MAX_DECODER_ALLOC);
        reader.limits(limits);
        let decoder = reader.into_decoder().map_err(map_decode_error)?;
        let (width, height) = decoder.dimensions();
        validate_dimensions(width, height)?;
        let rgba = DynamicImage::from_decoder(decoder)
            .map_err(map_decode_error)?
            .into_rgba8()
            .into_raw();
        if u64::try_from(rgba.len()).map_or(true, |len| len > MAX_IMAGE_RGBA_BYTES) {
            return Err(ImageDecodeError::Limit);
        }
        let image = DecodedImage {
            asset_id: request.asset_id,
            revision: request.revision,
            width,
            height,
            rgba: Arc::from(rgba),
        };
        tracing::debug!(
            asset_id = image.asset_id.0,
            revision = image.revision,
            width = image.width,
            height = image.height,
            "image decoded"
        );
        Ok(image)
    }
}

fn map_decode_error(error: ImageError) -> ImageDecodeError {
    if matches!(error, ImageError::Limits(_)) {
        ImageDecodeError::Limit
    } else {
        ImageDecodeError::Invalid
    }
}

pub(crate) fn validate_dimensions(width: u32, height: u32) -> Result<u64, ImageDecodeError> {
    if width > MAX_IMAGE_AXIS || height > MAX_IMAGE_AXIS {
        return Err(ImageDecodeError::Limit);
    }
    let pixels = u64::from(width) * u64::from(height);
    let rgba_bytes = pixels.checked_mul(4).ok_or(ImageDecodeError::Limit)?;
    if pixels > MAX_IMAGE_PIXELS || rgba_bytes > MAX_IMAGE_RGBA_BYTES {
        return Err(ImageDecodeError::Limit);
    }
    Ok(rgba_bytes)
}

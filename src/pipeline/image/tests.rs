use std::io::Cursor;
use std::sync::Arc;

use image::codecs::gif::GifEncoder;
use image::{DynamicImage, Frame, ImageFormat, Rgba, RgbaImage};

use super::*;
use crate::core::image::{ImageAssetId, ImageDecodeError, ImageDecodeRequest, ImageDecoder};

fn encoded(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
    let pixels = RgbaImage::from_pixel(width, height, Rgba([12, 34, 56, 255]));
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(pixels)
        .write_to(&mut output, format)
        .unwrap();
    output.into_inner()
}

fn request(bytes: Vec<u8>) -> ImageDecodeRequest {
    ImageDecodeRequest {
        asset_id: ImageAssetId(7),
        revision: 3,
        bytes: Arc::from(bytes),
    }
}

#[test]
fn enabled_raster_formats_decode_by_signature() {
    for format in [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::WebP,
        ImageFormat::Gif,
    ] {
        let decoded = RasterImageDecoder
            .decode(request(encoded(format, 2, 3)))
            .unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 3));
        assert_eq!(decoded.asset_id, ImageAssetId(7));
        assert_eq!(decoded.revision, 3);
        assert_eq!(decoded.rgba.len(), 24);
    }
}

#[test]
fn animated_gif_uses_its_first_frame() {
    let mut output = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut output);
        encoder
            .encode_frame(Frame::new(RgbaImage::from_pixel(
                1,
                1,
                Rgba([255, 0, 0, 255]),
            )))
            .unwrap();
        encoder
            .encode_frame(Frame::new(RgbaImage::from_pixel(
                1,
                1,
                Rgba([0, 0, 255, 255]),
            )))
            .unwrap();
    }
    let decoded = RasterImageDecoder.decode(request(output)).unwrap();
    assert_eq!(&*decoded.rgba, &[255, 0, 0, 255]);
}

#[test]
fn malformed_and_unsupported_bytes_are_rejected() {
    assert!(matches!(
        RasterImageDecoder.decode(request(b"not an image".to_vec())),
        Err(ImageDecodeError::Invalid)
    ));
}

#[test]
fn every_per_image_limit_is_strict() {
    assert_eq!(
        validate_dimensions(MAX_IMAGE_AXIS + 1, 1),
        Err(ImageDecodeError::Limit)
    );
    assert_eq!(
        validate_dimensions(4096, 2049),
        Err(ImageDecodeError::Limit)
    );
    assert_eq!(validate_dimensions(4096, 2048), Ok(MAX_IMAGE_RGBA_BYTES));
}

#[test]
fn worker_loop_preserves_routing_and_reports_decode_failures() {
    let (jobs_tx, jobs_rx) = crossbeam_channel::bounded(2);
    let (results_tx, results_rx) = crossbeam_channel::bounded(2);
    let pending = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
    let canceled = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
    let job = ImageDecodeJob {
        tab_id: 4,
        generation: 9,
        request: request(b"broken".to_vec()),
    };
    pending.lock().unwrap().insert(job_key(&job));
    jobs_tx.send(job).unwrap();
    drop(jobs_tx);
    run_worker(
        Arc::new(RasterImageDecoder),
        jobs_rx,
        results_tx,
        pending,
        canceled,
    );
    let payload = results_rx.try_recv().unwrap();
    assert_eq!((payload.tab_id, payload.generation), (4, 9));
    assert_eq!(payload.asset_id, ImageAssetId(7));
    assert_eq!(payload.revision, 3);
    assert!(matches!(payload.result, Err(ImageDecodeError::Invalid)));
}

#[test]
fn canceled_worker_jobs_are_dropped_without_a_result() {
    let (jobs_tx, jobs_rx) = crossbeam_channel::bounded(1);
    let (results_tx, results_rx) = crossbeam_channel::bounded(1);
    let pending = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
    let canceled = Arc::new(std::sync::Mutex::new(std::collections::HashSet::from([(
        4, 9,
    )])));
    let job = ImageDecodeJob {
        tab_id: 4,
        generation: 9,
        request: request(encoded(ImageFormat::Png, 1, 1)),
    };
    let key = job_key(&job);
    pending.lock().unwrap().insert(key);
    jobs_tx.send(job).unwrap();
    drop(jobs_tx);
    run_worker(
        Arc::new(RasterImageDecoder),
        jobs_rx,
        results_tx,
        pending.clone(),
        canceled,
    );
    assert!(results_rx.try_recv().is_err());
    assert!(!pending.lock().unwrap().contains(&key));
}

#[test]
fn pool_coalesces_duplicate_jobs_and_stays_closed_after_shutdown() {
    let pool = ImageDecodePool::new(Arc::new(RasterImageDecoder));
    let job = ImageDecodeJob {
        tab_id: 2,
        generation: 5,
        request: request(encoded(ImageFormat::Png, 1, 1)),
    };
    assert_eq!(pool.submit(job.clone()), ImageSubmitted::Queued);
    assert_eq!(pool.submit(job.clone()), ImageSubmitted::Duplicate);
    pool.shutdown();
    assert_eq!(pool.submit(job), ImageSubmitted::Closed);
}

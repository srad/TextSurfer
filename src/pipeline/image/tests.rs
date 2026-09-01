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
fn unknown_unsupported_and_malformed_images_are_distinct() {
    assert_eq!(
        RasterImageDecoder.decode(request(b"not an image".to_vec())),
        Err(ImageDecodeError::UnknownFormat)
    );
    assert_eq!(
        RasterImageDecoder.decode(request(b"BMunsupported bitmap".to_vec())),
        Err(ImageDecodeError::UnsupportedFormat)
    );
    let mut malformed_png = encoded(ImageFormat::Png, 1, 1);
    malformed_png.truncate(24);
    assert_eq!(
        RasterImageDecoder.decode(request(malformed_png)),
        Err(ImageDecodeError::Invalid)
    );
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
fn svg_dimensions_view_box_and_paths_decode() {
    let decoded = RasterImageDecoder
        .decode(request(
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="6" height="4" viewBox="0 0 3 2"><path fill="#ff0000" d="M0 0h3v2H0z"/></svg>"##
                .to_vec(),
        ))
        .unwrap();
    assert_eq!((decoded.width, decoded.height), (6, 4));
    assert_eq!(decoded.rgba.len(), 6 * 4 * 4);
    assert_eq!(&decoded.rgba[..4], &[255, 0, 0, 255]);
}

#[test]
fn svg_alpha_is_straight_rgba() {
    let decoded = RasterImageDecoder
        .decode(request(
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><path fill="#804020" fill-opacity="0.5" d="M0 0h1v1H0z"/></svg>"##
                .to_vec(),
        ))
        .unwrap();
    assert_eq!(&*decoded.rgba, &[128, 64, 32, 128]);
}

#[test]
fn svg_inputs_remain_bounded_and_external_references_are_inert() {
    assert!(
        RasterImageDecoder
            .decode(request(b"<svg".to_vec()))
            .is_err()
    );
    assert_eq!(
        RasterImageDecoder.decode(request(
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="8193" height="1"><path d="M0 0h1v1H0z"/></svg>"#
                .to_vec(),
        )),
        Err(ImageDecodeError::Limit)
    );
    let directory = tempfile::tempdir().unwrap();
    let external = directory.path().join("external.png");
    std::fs::write(&external, encoded(ImageFormat::Png, 1, 1)).unwrap();
    let external_url = url::Url::from_file_path(external).unwrap();
    let decoded = RasterImageDecoder
        .decode(request(
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><image width="1" height="1" href="{external_url}"/></svg>"#
            )
            .into_bytes(),
        ))
        .unwrap();
    assert_eq!(&*decoded.rgba, &[0, 0, 0, 0]);
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
    assert!(matches!(
        payload.result,
        Err(ImageDecodeError::UnknownFormat)
    ));
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

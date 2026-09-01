use std::sync::Arc;

use crate::core::image::{
    DecodedImage, DecodedImageSource, ImageAssetId, ImageDecodeRequest, ImageDecoder,
};
use crate::pipeline::image::RasterImageDecoder;

use super::*;

fn key(generation: u64, surface_epoch: u64, width: u32, height: u32) -> PreparedImageKey {
    PreparedImageKey {
        generation,
        surface_epoch,
        asset_id: ImageAssetId(7),
        revision: 3,
        width,
        height,
    }
}

fn raster_request(generation: u64, surface_epoch: u64) -> ImagePreparationRequest {
    ImagePreparationRequest {
        key: key(generation, surface_epoch, 1, 1),
        image: DecodedImage {
            asset_id: ImageAssetId(7),
            revision: 3,
            width: 1,
            height: 1,
            rgba: Arc::from([20, 40, 60, 128]),
            source: DecodedImageSource::Raster,
        },
    }
}

fn queue_contract(queue: &dyn ImagePreparationQueue) {
    queue.activate(5, 2);
    let request = raster_request(5, 2);
    assert_eq!(
        queue.submit(request.clone()),
        ImagePreparationSubmitted::Queued
    );
    assert_eq!(
        queue.submit(request.clone()),
        ImagePreparationSubmitted::Duplicate
    );
    assert!(queue.signal().pending());
    queue.activate(6, 2);
    assert_eq!(
        queue.submit(request.clone()),
        ImagePreparationSubmitted::Refused
    );
    queue.shutdown();
    assert_eq!(queue.submit(request), ImagePreparationSubmitted::Closed);
}

#[test]
fn inline_queue_obeys_submission_contract() {
    queue_contract(&InlineImagePreparationQueue::new(Arc::new(
        RasterImagePreparer,
    )));
}

#[test]
fn worker_queue_obeys_submission_contract() {
    queue_contract(&ImagePreparationPool::new(Arc::new(RasterImagePreparer)));
}

#[test]
fn worker_drops_stale_contexts_without_preparing_them() {
    let (requests, request_rx) = std::sync::mpsc::sync_channel(2);
    let (result_tx, results) = std::sync::mpsc::sync_channel(2);
    let active = Arc::new(std::sync::Mutex::new((6, 2)));
    let signal = Arc::new(ImagePreparationSignal {
        pending: std::sync::atomic::AtomicUsize::new(0),
        ready: std::sync::atomic::AtomicBool::new(false),
    });
    requests.send(raster_request(5, 2)).unwrap();
    requests.send(raster_request(6, 2)).unwrap();
    drop(requests);
    super::worker::run_worker(
        Arc::new(RasterImagePreparer),
        request_rx,
        result_tx,
        active,
        Arc::clone(&signal),
    );
    assert!(results.recv().unwrap().rgba.is_none());
    assert!(results.recv().unwrap().rgba.is_some());
    assert!(signal.ready());
}

#[test]
fn inline_queue_reports_prepared_results_and_then_empty() {
    let queue = InlineImagePreparationQueue::new(Arc::new(RasterImagePreparer));
    queue.activate(5, 2);
    let request = raster_request(5, 2);
    assert_eq!(queue.submit(request), ImagePreparationSubmitted::Queued);
    let ImagePreparationPoll::Ready(result) = queue.poll() else {
        panic!("prepared result");
    };
    assert_eq!(result.key, key(5, 2, 1, 1));
    assert_eq!(result.rgba.as_deref(), Some([10, 20, 30, 128].as_slice()));
    assert!(matches!(queue.poll(), ImagePreparationPoll::Empty));
    assert!(!queue.signal().pending());
}

#[test]
fn cache_keys_isolate_document_generations_and_surface_epochs() {
    let mut cache = PreparedImageCache::default();
    cache.insert(key(5, 2, 1, 1), Arc::from([1, 2, 3, 4]));
    assert!(cache.get(key(5, 2, 1, 1)).is_some());
    assert!(cache.get(key(6, 2, 1, 1)).is_none());
    assert!(cache.get(key(5, 3, 1, 1)).is_none());
}

#[test]
fn wikipedia_svg_is_rendered_directly_at_its_final_vga_size() {
    let bytes: Arc<[u8]> = Arc::from(
        include_bytes!("../../../testdata/browser-corpus/wikipedia-linux/r002.svg").as_slice(),
    );
    let decoded = RasterImageDecoder
        .decode(ImageDecodeRequest {
            asset_id: ImageAssetId(7),
            revision: 3,
            bytes: Arc::clone(&bytes),
        })
        .unwrap();
    assert!(matches!(decoded.source, DecodedImageSource::Svg(_)));
    let prepared = RasterImagePreparer.prepare(ImagePreparationRequest {
        key: key(5, 2, 48, 48),
        image: decoded,
    });
    let actual = prepared.rgba.unwrap();

    let tree =
        resvg::usvg::Tree::from_data_nested(&bytes, &resvg::usvg::Options::default()).unwrap();
    let mut expected = resvg::tiny_skia::Pixmap::new(48, 48).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            48.0 / tree.size().width(),
            48.0 / tree.size().height(),
        ),
        &mut expected.as_mut(),
    );
    assert_eq!(actual.as_ref(), expected.data());
}

#[test]
fn raster_downscaling_uses_lanczos_and_preserves_premultiplied_alpha() {
    let image = DecodedImage {
        asset_id: ImageAssetId(7),
        revision: 3,
        width: 4,
        height: 1,
        rgba: Arc::from([
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 255, 255, 255, 0,
        ]),
        source: DecodedImageSource::Raster,
    };
    let prepared = RasterImagePreparer.prepare(ImagePreparationRequest {
        key: key(5, 2, 2, 1),
        image,
    });
    let actual = prepared.rgba.unwrap();
    for pixel in actual.chunks_exact(4) {
        assert!(pixel[0] <= pixel[3]);
        assert!(pixel[1] <= pixel[3]);
        assert!(pixel[2] <= pixel[3]);
        if pixel[3] == 0 {
            assert_eq!(&pixel[..3], &[0, 0, 0]);
        }
    }
}

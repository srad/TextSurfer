use std::sync::Arc;

use crate::core::image::{DecodedImage, DecodedImageSource};

use super::{
    ImagePreparationRequest, ImagePreparationResult, ImagePreparer, MAX_PREPARED_IMAGE_BYTES,
};

pub struct RasterImagePreparer;

impl ImagePreparer for RasterImagePreparer {
    fn prepare(&self, request: ImagePreparationRequest) -> ImagePreparationResult {
        let rgba = prepare_image_pixels(&request.image, request.key.width, request.key.height);
        ImagePreparationResult {
            key: request.key,
            rgba,
        }
    }
}

fn prepare_image_pixels(image: &DecodedImage, width: u32, height: u32) -> Option<Arc<[u8]>> {
    let destination_bytes = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(4)?;
    if destination_bytes > MAX_PREPARED_IMAGE_BYTES || width == 0 || height == 0 {
        return None;
    }
    if let DecodedImageSource::Svg(bytes) = &image.source
        && let Some(rgba) = prepare_svg(bytes, width, height)
    {
        return Some(Arc::from(rgba));
    }
    prepare_raster(image, width, height).map(Arc::from)
}

fn prepare_svg(bytes: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let tree = resvg::usvg::Tree::from_data_nested(bytes, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            width as f32 / tree.size().width(),
            height as f32 / tree.size().height(),
        ),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.take();
    clamp_premultiplied(&mut rgba);
    Some(rgba)
}

fn prepare_raster(image: &DecodedImage, width: u32, height: u32) -> Option<Vec<u8>> {
    let expected = usize::try_from(image.width)
        .ok()?
        .checked_mul(usize::try_from(image.height).ok()?)?
        .checked_mul(4)?;
    if image.rgba.len() != expected {
        return None;
    }
    let mut premultiplied = image.rgba.as_ref().to_vec();
    for pixel in premultiplied.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    if width == image.width && height == image.height {
        return Some(premultiplied);
    }
    let source = image::RgbaImage::from_raw(image.width, image.height, premultiplied)?;
    let filter = if width < image.width || height < image.height {
        image::imageops::FilterType::Lanczos3
    } else {
        image::imageops::FilterType::CatmullRom
    };
    let mut rgba = image::imageops::resize(&source, width, height, filter).into_raw();
    clamp_premultiplied(&mut rgba);
    Some(rgba)
}

fn clamp_premultiplied(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = pixel[3];
        if alpha == 0 {
            pixel[..3].fill(0);
        } else {
            for channel in &mut pixel[..3] {
                *channel = (*channel).min(alpha);
            }
        }
    }
}

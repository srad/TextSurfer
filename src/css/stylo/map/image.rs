use std::sync::Arc;

use style::properties::ComputedValues;
use style::values::computed::LengthPercentage;
use style::values::computed::image::Image;
use style::values::computed::length_percentage::Unpacked;
use style::values::generics::NonNegative;
use style::values::generics::background::GenericBackgroundSize;
use style::values::generics::length::GenericLengthPercentageOrAuto;
use style::values::specified::background::{BackgroundRepeat, BackgroundRepeatKeyword};

use crate::core::style::{
    CssImageCoordinate, CssImageDimension, CssImageKind, CssImageLayer, CssImageRepeat,
    CssImageSize, LengthAxis, StyleStore,
};

use super::length::Lengths;

pub(super) fn backgrounds(
    values: &ComputedValues,
    lengths: &Lengths,
    store: &mut StyleStore,
) -> crate::core::style::CssImageLayers {
    let background = values.get_background();
    let layers = url_layers(
        &background.background_image.0,
        &background.background_position_x.0,
        &background.background_position_y.0,
        &background.background_size.0,
        &background.background_repeat.0,
        CssImageKind::Background,
        lengths,
    );
    store.images.insert(layers)
}

pub(super) fn masks(
    values: &ComputedValues,
    lengths: &Lengths,
    store: &mut StyleStore,
) -> crate::core::style::CssImageLayers {
    let svg = values.get_svg();
    let layers = url_layers(
        &svg.mask_image.0,
        &svg.mask_position_x.0,
        &svg.mask_position_y.0,
        &svg.mask_size.0,
        &svg.mask_repeat.0,
        CssImageKind::Mask,
        lengths,
    );
    store.images.insert(layers)
}

fn url_layers(
    images: &[Image],
    positions_x: &[LengthPercentage],
    positions_y: &[LengthPercentage],
    sizes: &[style::values::computed::BackgroundSize],
    repeats: &[BackgroundRepeat],
    kind: CssImageKind,
    lengths: &Lengths,
) -> Vec<CssImageLayer> {
    images
        .iter()
        .enumerate()
        .filter_map(|(index, image)| {
            let Image::Url(url) = image else {
                return None;
            };
            let url = url.url()?;
            let repeat = repeats.get(index % repeats.len().max(1));
            Some(CssImageLayer {
                url: Arc::from(url.as_str()),
                kind,
                position_x: positions_x
                    .get(index % positions_x.len().max(1))
                    .map_or(CssImageCoordinate::Center, |value| {
                        coordinate(value, LengthAxis::Horizontal, lengths)
                    }),
                position_y: positions_y
                    .get(index % positions_y.len().max(1))
                    .map_or(CssImageCoordinate::Center, |value| {
                        coordinate(value, LengthAxis::Vertical, lengths)
                    }),
                size: sizes
                    .get(index % sizes.len().max(1))
                    .map_or(CssImageSize::Auto, |size| image_size(size, lengths)),
                repeat_x: repeat.map_or(CssImageRepeat::Repeat, |repeat| image_repeat(repeat.0)),
                repeat_y: repeat.map_or(CssImageRepeat::Repeat, |repeat| image_repeat(repeat.1)),
            })
        })
        .collect()
}

fn coordinate(value: &LengthPercentage, axis: LengthAxis, lengths: &Lengths) -> CssImageCoordinate {
    match value.unpack() {
        Unpacked::Length(length) => {
            CssImageCoordinate::Cells(lengths.signed_cells(length.px(), axis))
        }
        Unpacked::Percentage(percentage) => {
            CssImageCoordinate::Percent((percentage.0 * 10_000.0).round() as i32)
        }
        Unpacked::Calc(_) => CssImageCoordinate::Center,
    }
}

fn image_size(value: &style::values::computed::BackgroundSize, lengths: &Lengths) -> CssImageSize {
    match value {
        GenericBackgroundSize::ExplicitSize { width, height } => CssImageSize::Explicit {
            width: dimension(width, LengthAxis::Horizontal, lengths),
            height: dimension(height, LengthAxis::Vertical, lengths),
        },
        GenericBackgroundSize::Cover => CssImageSize::Cover,
        GenericBackgroundSize::Contain => CssImageSize::Contain,
    }
}

fn dimension(
    value: &GenericLengthPercentageOrAuto<NonNegative<LengthPercentage>>,
    axis: LengthAxis,
    lengths: &Lengths,
) -> CssImageDimension {
    let GenericLengthPercentageOrAuto::LengthPercentage(NonNegative(value)) = value else {
        return CssImageDimension::Auto;
    };
    match value.unpack() {
        Unpacked::Length(length) => CssImageDimension::Cells(lengths.cells(length.px(), axis)),
        Unpacked::Percentage(percentage) => {
            CssImageDimension::Percent((percentage.0 * 10_000.0).round().max(0.0) as u32)
        }
        Unpacked::Calc(_) => CssImageDimension::Auto,
    }
}

fn image_repeat(value: BackgroundRepeatKeyword) -> CssImageRepeat {
    match value {
        BackgroundRepeatKeyword::NoRepeat => CssImageRepeat::NoRepeat,
        BackgroundRepeatKeyword::Repeat
        | BackgroundRepeatKeyword::Space
        | BackgroundRepeatKeyword::Round => CssImageRepeat::Repeat,
    }
}

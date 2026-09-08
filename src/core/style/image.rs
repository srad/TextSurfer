use std::sync::Arc;

use im::Vector;

const MAX_CSS_IMAGE_LAYERS: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CssImageLayers(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssImageKind {
    Background,
    Mask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssImageRepeat {
    Repeat,
    NoRepeat,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssImageCoordinate {
    Cells(isize),
    Percent(i32),
    #[default]
    Center,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssImageDimension {
    Cells(usize),
    Percent(u32),
    #[default]
    Auto,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssImageSize {
    Explicit {
        width: CssImageDimension,
        height: CssImageDimension,
    },
    Cover,
    Contain,
    #[default]
    Auto,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CssImageLayer {
    pub url: Arc<str>,
    pub kind: CssImageKind,
    pub position_x: CssImageCoordinate,
    pub position_y: CssImageCoordinate,
    pub size: CssImageSize,
    pub repeat_x: CssImageRepeat,
    pub repeat_y: CssImageRepeat,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CssImageStore {
    layers: Vector<Vector<CssImageLayer>>,
}

impl CssImageStore {
    pub(crate) fn insert(&mut self, layers: Vec<CssImageLayer>) -> CssImageLayers {
        if layers.is_empty() {
            return CssImageLayers::default();
        }
        let layers: Vector<_> = layers.into_iter().take(MAX_CSS_IMAGE_LAYERS).collect();
        if let Some(index) = self.layers.iter().position(|stored| stored == &layers) {
            return CssImageLayers(u32::try_from(index + 1).unwrap_or(u32::MAX));
        }
        let Some(handle) = self
            .layers
            .len()
            .checked_add(1)
            .and_then(|value| u32::try_from(value).ok())
        else {
            return CssImageLayers::default();
        };
        self.layers.push_back(layers);
        CssImageLayers(handle)
    }

    pub(crate) fn get(&self, handle: CssImageLayers) -> Option<&Vector<CssImageLayer>> {
        let index = usize::try_from(handle.0).ok()?.checked_sub(1)?;
        self.layers.get(index)
    }
}

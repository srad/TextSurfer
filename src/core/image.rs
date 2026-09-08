use std::collections::HashMap;
use std::sync::Arc;

use crate::core::dom::NodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageAssetId(pub u64);

#[derive(Clone, Debug)]
pub struct ImageDecodeRequest {
    pub asset_id: ImageAssetId,
    pub revision: u64,
    pub source: ImageDecodeSource,
}

#[derive(Clone, Debug)]
pub enum ImageDecodeSource {
    Bytes(Arc<[u8]>),
    DataUrl(Arc<str>),
}

#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub asset_id: ImageAssetId,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
    pub source: DecodedImageSource,
}

#[derive(Clone, Debug)]
pub enum DecodedImageSource {
    Raster,
    Svg(Arc<[u8]>),
}

impl PartialEq for DecodedImage {
    fn eq(&self, other: &Self) -> bool {
        self.asset_id == other.asset_id
            && self.revision == other.revision
            && self.width == other.width
            && self.height == other.height
            && Arc::ptr_eq(&self.rgba, &other.rgba)
            && match (&self.source, &other.source) {
                (DecodedImageSource::Raster, DecodedImageSource::Raster) => true,
                (DecodedImageSource::Svg(left), DecodedImageSource::Svg(right)) => {
                    Arc::ptr_eq(left, right)
                }
                _ => false,
            }
    }
}

impl Eq for DecodedImage {}

#[derive(Clone, Debug, Default)]
pub struct ImageResources {
    by_node: HashMap<NodeId, DecodedImage>,
    by_source: HashMap<String, DecodedImage>,
}

impl ImageResources {
    pub fn insert(&mut self, node: NodeId, image: DecodedImage) {
        self.by_node.insert(node, image);
    }

    pub fn get(&self, node: NodeId) -> Option<&DecodedImage> {
        self.by_node.get(&node)
    }

    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &DecodedImage)> {
        self.by_node.iter().map(|(node, image)| (*node, image))
    }

    pub fn insert_source(&mut self, source: String, image: DecodedImage) {
        self.by_source.insert(source, image);
    }

    pub fn get_source(&self, source: &str) -> Option<&DecodedImage> {
        self.by_source.get(source)
    }

    pub fn assets(&self) -> impl Iterator<Item = &DecodedImage> {
        self.by_node.values().chain(self.by_source.values())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageDecodeError {
    UnknownFormat,
    UnsupportedFormat,
    Invalid,
    Limit,
    Unavailable,
}

pub trait ImageDecoder: Send + Sync {
    fn decode(&self, request: ImageDecodeRequest) -> Result<DecodedImage, ImageDecodeError>;
}

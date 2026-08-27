use std::collections::HashMap;
use std::sync::Arc;

use crate::core::dom::NodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageAssetId(pub u64);

#[derive(Clone, Debug)]
pub struct ImageDecodeRequest {
    pub asset_id: ImageAssetId,
    pub revision: u64,
    pub bytes: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub asset_id: ImageAssetId,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

impl PartialEq for DecodedImage {
    fn eq(&self, other: &Self) -> bool {
        self.asset_id == other.asset_id
            && self.revision == other.revision
            && self.width == other.width
            && self.height == other.height
            && Arc::ptr_eq(&self.rgba, &other.rgba)
    }
}

impl Eq for DecodedImage {}

#[derive(Clone, Debug, Default)]
pub struct ImageResources {
    by_node: HashMap<NodeId, DecodedImage>,
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

use std::sync::Arc;

use crate::core::dom::{ElementNs, Node};
use crate::core::image::{DecodedImage, ImageAssetId, ImageDecodeError, ImageDecodeRequest};
use crate::net::{FetchError, FetchResponse, MAX_BODY_BYTES, ResourceId};

use super::discovery::attr_value;
use super::resource_url::normalized_url;
use super::{
    FetchCommand, ImageEntry, ImageState, MAX_IMAGE_DECODED_BYTES, MAX_IMAGE_FETCH_BYTES,
    MAX_IMAGE_URLS, PageLoad,
};

impl PageLoad {
    pub(super) fn image_resources(&self) -> crate::core::image::ImageResources {
        let mut resources = crate::core::image::ImageResources::default();
        for (node, index) in &self.image_node_index {
            if let ImageState::Ready(image) = &self.images[*index].state {
                resources.insert(*node, image.clone());
            }
        }
        resources
    }

    pub(super) fn discover_document_images(&mut self, effective_base: &url::Url) {
        let document = self.document.borrow();
        let mut discovered = Vec::new();
        let mut failed = 0usize;
        let mut stack: Vec<_> = document.roots().iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
                continue;
            };
            if *ns == ElementNs::Html
                && name == "img"
                && let Some(src) = attr_value(attrs, "src").filter(|src| !src.trim().is_empty())
            {
                match effective_base.join(src.trim()) {
                    Ok(mut url) if self.scheme_allowed(&url) => {
                        url.set_fragment(None);
                        discovered.push((id, url));
                    }
                    _ => failed = failed.saturating_add(1),
                }
            }
            if name != "template" {
                stack.extend(document.children(id).into_iter().rev());
            }
        }
        drop(document);
        self.failed_images = self.failed_images.saturating_add(failed);
        for (node, url) in discovered {
            let key = normalized_url(&url);
            if let Some(index) = self.image_url_cache.get(&key).copied() {
                self.image_node_index.insert(node, index);
                continue;
            }
            if self.images.len() >= MAX_IMAGE_URLS {
                self.failed_images = self.failed_images.saturating_add(1);
                continue;
            }
            let resource_id = ResourceId(self.next_resource_id);
            self.next_resource_id += 1;
            let asset_id = ImageAssetId(self.next_image_asset_id);
            self.next_image_asset_id += 1;
            let index = self.images.len();
            self.images.push(ImageEntry {
                asset_id,
                revision: 1,
                state: ImageState::Fetching,
            });
            self.image_fetch_index.insert(resource_id, index);
            self.image_asset_index.insert(asset_id, index);
            self.image_node_index.insert(node, index);
            self.image_url_cache.insert(key, index);
            self.commands.push(FetchCommand { resource_id, url });
        }
    }

    pub(super) fn deliver_image_fetch(
        &mut self,
        resource_id: ResourceId,
        result: Result<FetchResponse, FetchError>,
    ) -> bool {
        let Some(index) = self.image_fetch_index.get(&resource_id).copied() else {
            return false;
        };
        if !matches!(self.images[index].state, ImageState::Fetching) {
            return false;
        }
        let Ok(response) = result else {
            self.fail_image(index);
            return true;
        };
        let bytes = response.body.len();
        if !response.is_success()
            || !self.scheme_allowed(&response.final_url)
            || bytes > MAX_BODY_BYTES
            || self
                .image_raw_bytes
                .checked_add(bytes)
                .is_none_or(|total| total > MAX_IMAGE_FETCH_BYTES)
        {
            self.fail_image(index);
            return true;
        }
        self.image_raw_bytes += bytes;
        let final_key = normalized_url(&response.final_url);
        self.image_url_cache.entry(final_key).or_insert(index);
        self.images[index].state = ImageState::Decoding;
        self.image_decode_commands.push(ImageDecodeRequest {
            asset_id: self.images[index].asset_id,
            revision: self.images[index].revision,
            bytes: Arc::from(response.body),
        });
        true
    }

    pub fn deliver_image_decode(
        &mut self,
        asset_id: ImageAssetId,
        revision: u64,
        result: Result<DecodedImage, ImageDecodeError>,
    ) -> bool {
        let Some(index) = self.image_asset_index.get(&asset_id).copied() else {
            return false;
        };
        let ImageState::Decoding = self.images[index].state else {
            return false;
        };
        if revision != self.images[index].revision {
            return false;
        }
        let Ok(image) = result else {
            self.fail_image(index);
            return true;
        };
        if image.asset_id != asset_id || image.revision != revision {
            return false;
        }
        let expected = crate::pipeline::image::validate_dimensions(image.width, image.height).ok();
        let bytes = image.rgba.len();
        if expected != u64::try_from(bytes).ok()
            || self
                .image_decoded_bytes
                .checked_add(bytes)
                .is_none_or(|total| total > MAX_IMAGE_DECODED_BYTES)
        {
            self.fail_image(index);
            return true;
        }
        self.image_decoded_bytes += bytes;
        self.images[index].state = ImageState::Ready(image);
        self.dirty = true;
        true
    }

    pub fn fail_pending_image_decodes(&mut self) -> bool {
        let mut changed = false;
        for index in 0..self.images.len() {
            if matches!(self.images[index].state, ImageState::Decoding) {
                self.fail_image(index);
                changed = true;
            }
        }
        changed
    }

    pub fn fail_pending_image_fetches(&mut self) -> bool {
        let mut changed = false;
        for index in 0..self.images.len() {
            if matches!(self.images[index].state, ImageState::Fetching) {
                self.fail_image(index);
                changed = true;
            }
        }
        changed
    }

    fn fail_image(&mut self, index: usize) {
        self.images[index].state = ImageState::Failed;
        self.failed_images = self.failed_images.saturating_add(1);
    }
}

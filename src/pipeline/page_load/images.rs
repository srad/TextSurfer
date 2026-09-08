use std::sync::Arc;

use crate::core::dom::{ElementNs, Node};
use crate::core::image::{
    DecodedImage, ImageAssetId, ImageDecodeError, ImageDecodeRequest, ImageDecodeSource,
};
use crate::net::http::diagnostic_url;
use crate::net::{FetchError, FetchResponse, MAX_BODY_BYTES, ResourceId};

use super::super::render::RenderCause;
use super::discovery::attr_value;
use super::resource_url::normalized_url;
use super::{
    FetchCommand, ImageEntry, ImageFailure, ImageState, MAX_IMAGE_DECODED_BYTES,
    MAX_IMAGE_FETCH_BYTES, MAX_IMAGE_URLS, PageLoad, RenderInvalidation,
};

impl PageLoad {
    pub(super) fn image_resources(&self) -> crate::core::image::ImageResources {
        let mut resources = crate::core::image::ImageResources::default();
        for (node, index) in &self.image_node_index {
            if let ImageState::Ready(image) = &self.images[*index].state {
                resources.insert(*node, image.clone());
            }
        }
        for image in &self.images {
            if let ImageState::Ready(decoded) = &image.state {
                resources.insert_source(normalized_url(&image.requested), decoded.clone());
            }
        }
        resources
    }

    pub(super) fn discover_document_images(&mut self, effective_base: &url::Url) {
        let document = self.document.borrow();
        let mut discovered = Vec::new();
        let mut invalid_addresses = 0usize;
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
                    Ok(mut url) if self.image_scheme_allowed(&url) => {
                        url.set_fragment(None);
                        discovered.push((id, url));
                    }
                    _ => invalid_addresses = invalid_addresses.saturating_add(1),
                }
            }
            if name != "template" {
                stack.extend(document.children(id).into_iter().rev());
            }
        }
        drop(document);
        self.image_failures.invalid_address = self
            .image_failures
            .invalid_address
            .saturating_add(invalid_addresses);
        if invalid_addresses > 0 {
            tracing::warn!(count = invalid_addresses, "image addresses rejected");
        }
        for (node, url) in discovered {
            let data = url.scheme() == "data";
            let key = normalized_url(&url);
            if let Some(index) = self.image_url_cache.get(&key).copied() {
                self.image_node_index.insert(node, index);
                continue;
            }
            if self.images.len() >= MAX_IMAGE_URLS {
                self.image_failures.resource_limit =
                    self.image_failures.resource_limit.saturating_add(1);
                tracing::warn!(url = %diagnostic_url(&url), "image URL limit reached");
                continue;
            }
            if data
                && self
                    .image_raw_bytes
                    .checked_add(url.as_str().len())
                    .is_none_or(|total| total > MAX_IMAGE_FETCH_BYTES)
            {
                self.image_failures.resource_limit =
                    self.image_failures.resource_limit.saturating_add(1);
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
                requested: url.clone(),
                state: if data {
                    ImageState::Decoding
                } else {
                    ImageState::Fetching
                },
            });
            self.image_asset_index.insert(asset_id, index);
            self.image_node_index.insert(node, index);
            self.image_url_cache.insert(key, index);
            if data {
                self.image_raw_bytes = self.image_raw_bytes.saturating_add(url.as_str().len());
                self.image_decode_commands.push(ImageDecodeRequest {
                    asset_id,
                    revision: 1,
                    source: ImageDecodeSource::DataUrl(Arc::from(url.as_str())),
                });
            } else {
                self.image_fetch_index.insert(resource_id, index);
                self.commands.push(FetchCommand { resource_id, url });
            }
        }
    }

    pub(super) fn discover_css_images(&mut self, styles: &crate::core::style::StyleTree) {
        let sources: Vec<_> = styles.image_sources().map(str::to_owned).collect();
        for source in sources {
            let Ok(mut url) = url::Url::parse(&source) else {
                self.image_failures.invalid_address =
                    self.image_failures.invalid_address.saturating_add(1);
                continue;
            };
            url.set_fragment(None);
            if !self.image_scheme_allowed(&url) {
                self.image_failures.invalid_address =
                    self.image_failures.invalid_address.saturating_add(1);
                continue;
            }
            let key = normalized_url(&url);
            if self.image_url_cache.contains_key(&key) {
                continue;
            }
            if self.images.len() >= MAX_IMAGE_URLS
                || self
                    .image_raw_bytes
                    .checked_add(url.as_str().len())
                    .is_none_or(|total| total > MAX_IMAGE_FETCH_BYTES)
            {
                self.image_failures.resource_limit =
                    self.image_failures.resource_limit.saturating_add(1);
                continue;
            }
            let resource_id = ResourceId(self.next_resource_id);
            self.next_resource_id = self.next_resource_id.saturating_add(1);
            let asset_id = ImageAssetId(self.next_image_asset_id);
            self.next_image_asset_id = self.next_image_asset_id.saturating_add(1);
            let data = url.scheme() == "data";
            let index = self.images.len();
            self.images.push(ImageEntry {
                asset_id,
                revision: 1,
                requested: url.clone(),
                state: if data {
                    ImageState::Decoding
                } else {
                    ImageState::Fetching
                },
            });
            self.image_asset_index.insert(asset_id, index);
            self.image_url_cache.insert(key, index);
            if data {
                self.image_raw_bytes += url.as_str().len();
                self.image_decode_commands.push(ImageDecodeRequest {
                    asset_id,
                    revision: 1,
                    source: ImageDecodeSource::DataUrl(Arc::from(url.as_str())),
                });
            } else {
                self.image_fetch_index.insert(resource_id, index);
                self.commands.push(FetchCommand { resource_id, url });
            }
        }
    }

    fn image_scheme_allowed(&self, url: &url::Url) -> bool {
        url.scheme() == "data" || self.scheme_allowed(url)
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
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                let reason = match error {
                    FetchError::Network(_) => "network",
                    FetchError::HttpStatus(_) => "http_status",
                    FetchError::UnsupportedScheme(_) => "unsupported_scheme",
                    FetchError::UnsupportedMethod(_) => "unsupported_method",
                    FetchError::BodyTooLarge { .. } => "body_too_large",
                };
                tracing::warn!(
                    url = %diagnostic_url(&self.images[index].requested),
                    reason,
                    "image fetch failed"
                );
                self.fail_image(index, ImageFailure::FetchFailure);
                return true;
            }
        };
        let bytes = response.body.len();
        if !response.is_success() {
            let reason = if response.status == 429 {
                ImageFailure::RateLimited
            } else {
                ImageFailure::FetchFailure
            };
            tracing::warn!(
                url = %diagnostic_url(&response.final_url),
                status = response.status,
                content_type = ?response.content_type,
                "image response rejected"
            );
            self.fail_image(index, reason);
            return true;
        }
        if !self.scheme_allowed(&response.final_url) {
            self.fail_image(index, ImageFailure::InvalidAddress);
            return true;
        }
        if bytes > MAX_BODY_BYTES
            || self
                .image_raw_bytes
                .checked_add(bytes)
                .is_none_or(|total| total > MAX_IMAGE_FETCH_BYTES)
        {
            self.fail_image(index, ImageFailure::ResourceLimit);
            return true;
        }
        self.image_raw_bytes += bytes;
        let final_key = normalized_url(&response.final_url);
        self.image_url_cache.entry(final_key).or_insert(index);
        self.images[index].state = ImageState::Decoding;
        self.image_decode_commands.push(ImageDecodeRequest {
            asset_id: self.images[index].asset_id,
            revision: self.images[index].revision,
            source: ImageDecodeSource::Bytes(Arc::from(response.body)),
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
        let image = match result {
            Ok(image) => image,
            Err(error) => {
                let reason = match error {
                    ImageDecodeError::UnknownFormat => ImageFailure::UnknownFormat,
                    ImageDecodeError::UnsupportedFormat => ImageFailure::UnsupportedFormat,
                    ImageDecodeError::Invalid => ImageFailure::InvalidData,
                    ImageDecodeError::Limit => ImageFailure::ResourceLimit,
                    ImageDecodeError::Unavailable => ImageFailure::Unavailable,
                };
                self.fail_image(index, reason);
                return true;
            }
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
            self.fail_image(index, ImageFailure::ResourceLimit);
            return true;
        }
        self.image_decoded_bytes += bytes;
        let width = image.width;
        let height = image.height;
        self.images[index].state = ImageState::Ready(image);
        let ready_images = self
            .images
            .iter()
            .filter(|image| matches!(image.state, ImageState::Ready(_)))
            .count();
        let pending_images = self
            .images
            .iter()
            .filter(|image| matches!(image.state, ImageState::Fetching | ImageState::Decoding))
            .count();
        tracing::trace!(
            target: "textsurfer::perf",
            asset_id = asset_id.0,
            revision,
            width,
            height,
            ready_images,
            pending_images,
            decoded_bytes = bytes,
            "image decode invalidated layout"
        );
        self.invalidate_soft(RenderInvalidation::LAYOUT, RenderCause::Image);
        true
    }

    pub fn fail_pending_image_decodes(&mut self) -> bool {
        let mut changed = false;
        for index in 0..self.images.len() {
            if matches!(self.images[index].state, ImageState::Decoding) {
                self.fail_image(index, ImageFailure::Unavailable);
                changed = true;
            }
        }
        changed
    }

    pub fn fail_pending_image_fetches(&mut self) -> bool {
        let mut changed = false;
        for index in 0..self.images.len() {
            if matches!(self.images[index].state, ImageState::Fetching) {
                self.fail_image(index, ImageFailure::Unavailable);
                changed = true;
            }
        }
        changed
    }

    fn fail_image(&mut self, index: usize, reason: ImageFailure) {
        if matches!(self.images[index].state, ImageState::Failed) {
            return;
        }
        self.images[index].state = ImageState::Failed;
        let count = match reason {
            ImageFailure::InvalidAddress => &mut self.image_failures.invalid_address,
            ImageFailure::RateLimited => &mut self.image_failures.rate_limited,
            ImageFailure::FetchFailure => &mut self.image_failures.fetch_failure,
            ImageFailure::UnknownFormat => &mut self.image_failures.unknown_format,
            ImageFailure::UnsupportedFormat => &mut self.image_failures.unsupported_format,
            ImageFailure::InvalidData => &mut self.image_failures.invalid_data,
            ImageFailure::ResourceLimit => &mut self.image_failures.resource_limit,
            ImageFailure::Unavailable => &mut self.image_failures.unavailable,
        };
        *count = count.saturating_add(1);
        tracing::warn!(
            url = %diagnostic_url(&self.images[index].requested),
            reason = ?reason,
            "image failed"
        );
    }
}

mod delivery;
mod discovery;
mod images;
mod refresh;
mod resource_url;
mod settle;
mod sheets;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::time::Duration;

use encoding_rs::Encoding;
use url::Url;

use crate::core::dom::{AttrNs, Node, NodeId, SharedDocument};
use crate::core::geom::Size;
use crate::core::image::{DecodedImage, ImageAssetId, ImageDecodeRequest};
use crate::core::style::{Palette, RenderContext};
use crate::css::{ColorScheme, DynamicState, MediaContext, MediaQueryList, StateDeps, StyleSheet};
use crate::html::{Html5everParser, HtmlParser};
use crate::net::{FetchResponse, ResourceId};

use super::render::RenderedPage;

pub const STYLESHEET_DEADLINE: Duration = Duration::from_secs(5);
pub const MAX_EXTERNAL_OCCURRENCES: usize = 64;
pub const MAX_IMPORT_DEPTH: usize = 8;
pub const MAX_EXTERNAL_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_IMAGE_URLS: usize = 128;
pub const MAX_IMAGE_FETCH_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_IMAGE_DECODED_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_DECLARATIVE_REFRESHES: u8 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchCommand {
    pub resource_id: ResourceId,
    pub url: Url,
}

#[derive(Clone, Copy, Debug)]
pub struct PageLoadOptions {
    pub render: RenderContext,
    pub palette: Palette,
    pub scripting: bool,
    pub color_scheme: ColorScheme,
    pub started: Duration,
}

enum RootSource {
    Inline {
        sheet: StyleSheet,
        queries: MediaQueryList,
        imports: Vec<usize>,
    },
    External(usize),
}

struct Occurrence {
    fetch_id: ResourceId,
    queries: MediaQueryList,
    depth: usize,
    ancestors: Vec<String>,
    environment: &'static Encoding,
    sheet: Option<StyleSheet>,
    imports: Vec<usize>,
    failed: bool,
}

enum FetchState {
    Pending,
    Ready(FetchResponse),
    Failed,
}

struct FetchEntry {
    requested: Url,
    state: FetchState,
    occurrences: Vec<usize>,
}

enum ImageState {
    Fetching,
    Decoding,
    Ready(DecodedImage),
    Failed,
}

struct ImageEntry {
    asset_id: ImageAssetId,
    revision: u64,
    state: ImageState,
}

pub struct PageLoad {
    document: SharedDocument,
    document_url: Url,
    effective_base: Url,
    immediate_refresh: Option<Url>,
    html_encoding: &'static Encoding,
    parse_errors: usize,
    roots: Vec<RootSource>,
    occurrences: Vec<Occurrence>,
    fetches: Vec<FetchEntry>,
    fetch_index: HashMap<ResourceId, usize>,
    url_cache: HashMap<String, ResourceId>,
    decoded_cache: HashMap<(ResourceId, &'static str), StyleSheet>,
    commands: Vec<FetchCommand>,
    image_decode_commands: Vec<ImageDecodeRequest>,
    images: Vec<ImageEntry>,
    image_fetch_index: HashMap<ResourceId, usize>,
    image_asset_index: HashMap<ImageAssetId, usize>,
    image_node_index: HashMap<NodeId, usize>,
    image_url_cache: HashMap<String, usize>,
    next_resource_id: u64,
    next_image_asset_id: u64,
    raw_bytes: usize,
    decoded_bytes: usize,
    failed_resources: usize,
    image_raw_bytes: usize,
    image_decoded_bytes: usize,
    failed_images: usize,
    external_disabled: bool,
    cancel_requested: bool,
    media: MediaContext,
    palette: Palette,
    deadline: Duration,
    first_painted: bool,
    final_painted: bool,
    dirty: bool,
    state_deps: StateDeps,
}

impl PageLoad {
    pub fn new(
        source: &str,
        document_url: Url,
        html_encoding: &'static Encoding,
        options: PageLoadOptions,
    ) -> Self {
        let PageLoadOptions {
            render,
            palette,
            scripting,
            color_scheme,
            started,
        } = options;
        let outcome = Html5everParser::new(scripting).parse_document(source);
        let effective_base = outcome
            .base_href
            .as_deref()
            .and_then(|base| document_url.join(base).ok())
            .unwrap_or_else(|| document_url.clone());
        let immediate_refresh =
            refresh::immediate_refresh(&outcome.document, &document_url, &effective_base);
        let mut load = Self {
            document: outcome.document,
            document_url,
            effective_base: effective_base.clone(),
            immediate_refresh,
            html_encoding,
            parse_errors: outcome.parse_errors,
            roots: Vec::new(),
            occurrences: Vec::new(),
            fetches: Vec::new(),
            fetch_index: HashMap::new(),
            url_cache: HashMap::new(),
            decoded_cache: HashMap::new(),
            commands: Vec::new(),
            image_decode_commands: Vec::new(),
            images: Vec::new(),
            image_fetch_index: HashMap::new(),
            image_asset_index: HashMap::new(),
            image_node_index: HashMap::new(),
            image_url_cache: HashMap::new(),
            next_resource_id: 1,
            next_image_asset_id: 1,
            raw_bytes: 0,
            decoded_bytes: 0,
            failed_resources: 0,
            image_raw_bytes: 0,
            image_decoded_bytes: 0,
            failed_images: 0,
            external_disabled: false,
            cancel_requested: false,
            media: MediaContext::screen()
                .with_palette(palette)
                .with_scripting(scripting)
                .with_color_scheme(color_scheme)
                .with_render_context(render),
            palette,
            deadline: started.saturating_add(STYLESHEET_DEADLINE),
            first_painted: false,
            final_painted: false,
            dirty: true,
            state_deps: StateDeps::default(),
        };
        load.discover_document_sources(&effective_base);
        load.discover_document_images(&effective_base);
        load.process_materializations();
        load
    }

    pub fn take_commands(&mut self) -> Vec<FetchCommand> {
        std::mem::take(&mut self.commands)
    }

    pub fn take_cancel_requested(&mut self) -> bool {
        std::mem::take(&mut self.cancel_requested)
    }

    pub fn take_image_decode_commands(&mut self) -> Vec<ImageDecodeRequest> {
        std::mem::take(&mut self.image_decode_commands)
    }

    pub fn resize(&mut self, viewport: Size) -> Option<RenderedPage> {
        if self.media.viewport == viewport {
            return None;
        }
        self.set_viewport(viewport);
        if self.first_painted {
            self.final_painted = self.applicable_graph_settled();
            self.dirty = false;
            Some(self.render_page())
        } else {
            None
        }
    }

    pub fn set_viewport(&mut self, viewport: Size) {
        if self.media.viewport != viewport {
            self.media = self.media.with_viewport(viewport);
            self.dirty = true;
        }
    }

    pub fn set_dynamic_state(&mut self, state: DynamicState) -> Option<RenderedPage> {
        if self.media.state == state {
            return None;
        }
        let previous = self.media.state;
        self.media = self.media.with_state(state);
        if !self.first_painted || !self.restyle_needed(previous, state) {
            return None;
        }
        Some(self.render_page())
    }

    fn restyle_needed(&self, previous: DynamicState, next: DynamicState) -> bool {
        self.state_deps.hover && previous.hover != next.hover
            || self.state_deps.focus && previous.focus != next.focus
            || self.state_deps.active && previous.active != next.active
            || self.hovered_link(previous.hover) != self.hovered_link(next.hover)
    }

    fn hovered_link(&self, mut node: Option<NodeId>) -> Option<NodeId> {
        let document = self.document.borrow();
        while let Some(id) = node {
            if matches!(
                document.node(id),
                Some(Node::Element { name, attrs, .. })
                    if name == "a"
                        && attrs.iter().any(|attr| attr.ns == AttrNs::None && attr.name == "href")
            ) {
                return Some(id);
            }
            node = document.parent(id);
        }
        None
    }

    pub fn parse_errors(&self) -> usize {
        self.parse_errors
    }

    /// The URL relative references in this document resolve against: the document URL,
    /// or what its first `<base href>` made of it.
    pub fn base_url(&self) -> &Url {
        &self.effective_base
    }

    pub fn immediate_refresh(&self) -> Option<&Url> {
        self.immediate_refresh.as_ref()
    }

    pub fn has_painted(&self) -> bool {
        self.first_painted
    }

    pub fn is_settled(&self) -> bool {
        let styles_settled = self.external_disabled
            || self
                .fetches
                .iter()
                .all(|fetch| !matches!(fetch.state, FetchState::Pending));
        styles_settled
            && self
                .images
                .iter()
                .all(|image| matches!(image.state, ImageState::Ready(_) | ImageState::Failed))
    }

    pub fn applicable_is_settled(&self) -> bool {
        self.applicable_graph_settled()
    }

    pub fn external_occurrences(&self) -> usize {
        self.occurrences.len()
    }

    pub fn failed_resources(&self) -> usize {
        self.failed_resources
    }

    pub fn failed_images(&self) -> usize {
        self.failed_images
    }

    pub fn decoded_image(&self, node: NodeId) -> Option<&DecodedImage> {
        let index = self.image_node_index.get(&node)?;
        match &self.images[*index].state {
            ImageState::Ready(image) => Some(image),
            _ => None,
        }
    }

    pub fn external_disabled(&self) -> bool {
        self.external_disabled
    }
}

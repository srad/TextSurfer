mod delivery;
mod discovery;
mod images;
mod pending;
mod refresh;
mod resource_url;
mod settle;
mod sheets;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use encoding_rs::Encoding;
use url::Url;

use crate::core::dom::{NodeId, SharedDocument};
use crate::core::form::{
    FormError, FormMutationError, FormState, FormSubmission, build_submission,
    build_submission_for_form, form_owner,
};
use crate::core::geom::Size;
use crate::core::image::{DecodedImage, ImageAssetId, ImageDecodeRequest};
use crate::core::style::{Palette, RenderContext};
use crate::css::{ColorScheme, DynamicState, MediaContext};
use crate::html::{Html5everParser, HtmlParser, ParseOutcome};
use crate::net::{FetchResponse, ResourceId};

use super::render::{RenderCause, RenderCauses, RenderedPage};
use crate::core::style::StyleTree;
use crate::layout::BoxTree;

pub use pending::PendingPageLoad;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RenderInvalidation {
    cascade: bool,
    layout: bool,
}

impl RenderInvalidation {
    const PAINT: Self = Self {
        cascade: false,
        layout: false,
    };
    const RESTYLE: Self = Self {
        cascade: true,
        layout: false,
    };
    const LAYOUT: Self = Self {
        cascade: false,
        layout: true,
    };
    const STYLE: Self = Self {
        cascade: true,
        layout: true,
    };

    fn union(self, other: Self) -> Self {
        Self {
            cascade: self.cascade || other.cascade,
            layout: self.layout || other.layout,
        }
    }
}

pub const STYLESHEET_DEADLINE: Duration = Duration::from_millis(250);
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
        source: String,
        media: String,
        imports: Vec<usize>,
    },
    External {
        occurrence: usize,
        media: String,
    },
}

struct Occurrence {
    fetch_id: ResourceId,
    media: String,
    depth: usize,
    ancestors: Vec<String>,
    environment: &'static Encoding,
    source: Option<Arc<str>>,
    base_url: Url,
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
    requested: Url,
    state: ImageState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageFailureSummary {
    pub invalid_address: usize,
    pub rate_limited: usize,
    pub fetch_failure: usize,
    pub unknown_format: usize,
    pub unsupported_format: usize,
    pub invalid_data: usize,
    pub resource_limit: usize,
    pub unavailable: usize,
}

impl ImageFailureSummary {
    pub fn total(self) -> usize {
        self.invalid_address
            .saturating_add(self.rate_limited)
            .saturating_add(self.fetch_failure)
            .saturating_add(self.unknown_format)
            .saturating_add(self.unsupported_format)
            .saturating_add(self.invalid_data)
            .saturating_add(self.resource_limit)
            .saturating_add(self.unavailable)
    }
}

#[derive(Clone, Copy, Debug)]
enum ImageFailure {
    InvalidAddress,
    RateLimited,
    FetchFailure,
    UnknownFormat,
    UnsupportedFormat,
    InvalidData,
    ResourceLimit,
    Unavailable,
}

pub struct PageLoad {
    document: SharedDocument,
    forms: FormState,
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
    decoded_cache: HashMap<(ResourceId, &'static str), Arc<str>>,
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
    image_failures: ImageFailureSummary,
    external_disabled: bool,
    cancel_requested: bool,
    media: MediaContext,
    palette: Palette,
    deadline: Duration,
    first_painted: bool,
    final_painted: bool,
    invalidation: Option<RenderInvalidation>,
    render_causes: RenderCauses,
    cached_styles: Option<Arc<StyleTree>>,
    cached_layout: Option<BoxTree>,
    cached_layout_styles: Option<Arc<StyleTree>>,
    cached_css_warnings: usize,
    deferred_rendering: bool,
    render_epoch: u64,
    hard_epoch: u64,
    style_revision: u64,
    last_published_epoch: u64,
    direct_renders: Option<super::render::BlockingRenderQueue>,
}

impl PageLoad {
    fn invalidate(&mut self, level: RenderInvalidation, cause: RenderCause) {
        self.hard_epoch = self.hard_epoch.wrapping_add(1);
        self.invalidate_soft(level, cause);
    }

    fn invalidate_soft(&mut self, level: RenderInvalidation, cause: RenderCause) {
        let coalesced = self.invalidation.is_some();
        if !coalesced {
            self.render_epoch = self.render_epoch.wrapping_add(1);
        }
        self.invalidation = Some(
            self.invalidation
                .map_or(level, |current| current.union(level)),
        );
        self.render_causes.insert(cause);
        tracing::trace!(
            target: "textsurfer::perf",
            epoch = self.render_epoch,
            hard_epoch = self.hard_epoch,
            invalidation = ?self.invalidation,
            cause = ?cause,
            causes = %self.render_causes,
            coalesced,
            "render invalidated"
        );
    }

    fn is_dirty(&self) -> bool {
        self.invalidation.is_some()
    }

    pub fn defer_rendering(&mut self) {
        self.deferred_rendering = true;
    }

    pub fn render_epoch(&self) -> u64 {
        self.render_epoch
    }

    pub fn hard_epoch(&self) -> u64 {
        self.hard_epoch
    }

    pub fn has_render_work(&self) -> bool {
        self.first_painted && self.is_dirty()
    }

    fn render_or_defer(&mut self) -> Option<RenderedPage> {
        (!self.deferred_rendering).then(|| self.render_page())
    }

    pub fn new(
        source: &str,
        document_url: Url,
        html_encoding: &'static Encoding,
        options: PageLoadOptions,
    ) -> Self {
        let outcome = Html5everParser::new(options.scripting).parse_document(source);
        Self::from_outcome(outcome, document_url, html_encoding, options)
    }

    pub(crate) fn from_outcome(
        outcome: ParseOutcome,
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
        let effective_base = outcome
            .base_href
            .as_deref()
            .and_then(|base| document_url.join(base).ok())
            .unwrap_or_else(|| document_url.clone());
        let immediate_refresh =
            refresh::immediate_refresh(&outcome.document, &document_url, &effective_base);
        let mut load = Self {
            document: outcome.document,
            forms: FormState::default(),
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
            image_failures: ImageFailureSummary::default(),
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
            invalidation: Some(RenderInvalidation::STYLE),
            render_causes: RenderCauses::one(RenderCause::Initial),
            cached_styles: None,
            cached_layout: None,
            cached_layout_styles: None,
            cached_css_warnings: 0,
            deferred_rendering: false,
            render_epoch: 1,
            hard_epoch: 1,
            style_revision: 1,
            last_published_epoch: 0,
            direct_renders: None,
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
            self.render_or_defer()
        } else {
            None
        }
    }

    pub fn set_viewport(&mut self, viewport: Size) {
        if self.media.viewport != viewport {
            self.style_revision = self.style_revision.wrapping_add(1);
            self.media = self.media.with_viewport(viewport);
            self.invalidate(RenderInvalidation::STYLE, RenderCause::Viewport);
        }
    }

    pub fn set_dynamic_state(&mut self, state: DynamicState) -> Option<RenderedPage> {
        if self.media.state == state {
            return None;
        }
        self.media = self.media.with_state(state);
        if !self.first_painted {
            return None;
        }
        self.invalidate(RenderInvalidation::RESTYLE, RenderCause::DynamicState);
        self.render_or_defer()
    }

    pub fn parse_errors(&self) -> usize {
        self.parse_errors
    }

    /// The URL relative references in this document resolve against: the document URL,
    /// or what its first `<base href>` made of it.
    pub fn base_url(&self) -> &Url {
        &self.effective_base
    }

    pub fn form_state(&self) -> &FormState {
        &self.forms
    }

    pub fn set_form_text(&mut self, node: NodeId, value: String) -> Result<(), FormMutationError> {
        self.forms.set_text(&self.document.borrow(), node, value)
    }

    pub fn set_form_checked(
        &mut self,
        node: NodeId,
        checked: bool,
    ) -> Result<Option<RenderedPage>, FormMutationError> {
        self.forms
            .set_checked(&self.document.borrow(), node, checked)?;
        Ok(self.render_after_form_mutation())
    }

    pub fn select_form_option(
        &mut self,
        node: NodeId,
        index: usize,
    ) -> Result<Option<RenderedPage>, FormMutationError> {
        self.forms.select(&self.document.borrow(), node, index)?;
        Ok(self.render_after_form_mutation())
    }

    pub fn reset_form(&mut self, form: NodeId) -> Option<RenderedPage> {
        self.forms.reset_form(&self.document.borrow(), form);
        self.render_after_form_mutation()
    }

    pub fn form_submission(&self, submitter: NodeId) -> Result<FormSubmission, FormError> {
        build_submission(
            &self.document.borrow(),
            &self.forms,
            Some(submitter),
            &self.document_url,
            &self.effective_base,
        )
    }

    pub fn implicit_form_submission(&self, control: NodeId) -> Result<FormSubmission, FormError> {
        let document = self.document.borrow();
        let form = form_owner(&document, control).ok_or(FormError::NoFormOwner)?;
        build_submission_for_form(
            &document,
            &self.forms,
            form,
            None,
            &self.document_url,
            &self.effective_base,
        )
    }

    fn render_after_form_mutation(&mut self) -> Option<RenderedPage> {
        self.invalidate(RenderInvalidation::STYLE, RenderCause::FormState);
        if !self.first_painted {
            return None;
        }
        self.render_or_defer()
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

    pub fn resource_progress(&self) -> (usize, usize) {
        let completed_fetches = self
            .fetches
            .iter()
            .filter(|fetch| !matches!(fetch.state, FetchState::Pending))
            .count();
        let completed_images = self
            .images
            .iter()
            .filter(|image| matches!(image.state, ImageState::Ready(_) | ImageState::Failed))
            .count();
        (
            completed_fetches + completed_images,
            self.fetches.len() + self.images.len(),
        )
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
        self.image_failures.total()
    }

    pub fn image_failures(&self) -> ImageFailureSummary {
        self.image_failures
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

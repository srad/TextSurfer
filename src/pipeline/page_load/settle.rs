use std::time::Duration;

use crate::core::style::Palette;
use crate::css::ColorScheme;

use super::super::render::{RenderCause, RenderedPage};
use super::{FetchState, PageLoad, RenderInvalidation, RootSource};

impl PageLoad {
    pub fn set_color_context(&mut self, palette: Palette, color_scheme: ColorScheme) -> bool {
        if self.palette == palette && self.media.color_scheme == color_scheme {
            return false;
        }
        self.palette = palette;
        self.style_revision = self.style_revision.wrapping_add(1);
        self.media = self
            .media
            .with_palette(palette)
            .with_color_scheme(color_scheme);
        self.invalidate(RenderInvalidation::STYLE, RenderCause::Theme);
        true
    }

    pub fn recolor(&mut self, palette: Palette, color_scheme: ColorScheme) -> Option<RenderedPage> {
        if !self.set_color_context(palette, color_scheme) || !self.first_painted {
            return None;
        }
        self.final_painted = self.applicable_graph_settled();
        self.render_or_defer()
    }

    pub fn render_if_ready(&mut self, now: Duration) -> Option<RenderedPage> {
        let settled = self.applicable_graph_settled();
        if !self.first_painted {
            if (!settled || !self.images_settled()) && now < self.deadline {
                return None;
            }
            self.first_painted = true;
            self.final_painted = settled;
            return self.render_or_defer();
        }
        if !self.final_painted && settled && self.is_dirty() {
            self.final_painted = true;
            self.render_causes.insert(RenderCause::ResourceSettlement);
            return self.render_or_defer();
        }
        None
    }

    pub fn force_render(&mut self) -> RenderedPage {
        while self.scripts_runnable_at(Duration::ZERO) {
            let _ = self.advance_scripts(Duration::ZERO);
        }
        self.first_painted = true;
        self.final_painted = self.applicable_graph_settled();
        if self.invalidation.is_none() {
            self.invalidation = Some(RenderInvalidation::PAINT);
            self.render_causes.insert(RenderCause::Forced);
        }
        self.render_page()
    }

    pub fn render_after_image(&mut self) -> Option<RenderedPage> {
        if !self.first_painted || !self.is_dirty() {
            return None;
        }
        self.render_or_defer()
    }

    pub(super) fn applicable_graph_settled(&self) -> bool {
        let styles_settled = self.external_disabled
            || self.roots.iter().all(|root| match root {
                RootSource::Inline { media, imports, .. } => {
                    !self.media_matches(media)
                        || imports
                            .iter()
                            .all(|occurrence| self.occurrence_settled(*occurrence, true))
                }
                RootSource::External { occurrence, media } => {
                    !self.media_matches(media) || self.occurrence_settled(*occurrence, true)
                }
            });
        styles_settled && !self.scripts_pending()
    }

    fn occurrence_settled(&self, occurrence_id: usize, inherited_match: bool) -> bool {
        let occurrence = &self.occurrences[occurrence_id];
        if occurrence.failed || !inherited_match || !self.media_matches(&occurrence.media) {
            return true;
        }
        let Some(fetch_index) = self.fetch_index.get(&occurrence.fetch_id).copied() else {
            return true;
        };
        if matches!(self.fetches[fetch_index].state, FetchState::Pending) {
            return false;
        }
        occurrence
            .imports
            .iter()
            .all(|child| self.occurrence_settled(*child, true))
    }

    fn media_matches(&self, source: &str) -> bool {
        crate::css::stylo::media_matches(source, self.media, self.document.borrow().quirks_mode())
    }
}

use std::sync::Arc;

use super::super::render::{
    BlockingRenderQueue, RenderJob, RenderKey, RenderResult, RenderTree, RenderedPage, StyleInput,
    StyleSessionId, StyleSource,
};
use super::{PageLoad, RenderInvalidation, RootSource};

impl PageLoad {
    pub(super) fn render_page(&mut self) -> RenderedPage {
        let key = RenderKey {
            tab_id: 0,
            generation: 0,
            epoch: self.render_epoch,
            hard_epoch: self.hard_epoch,
        };
        let result = self
            .render_job_blocking(key)
            .expect("a direct render has pending work");
        self.apply_render_result(result)
            .expect("a direct render keeps its epoch")
    }

    pub(crate) fn render_job_blocking(&mut self, key: RenderKey) -> Option<RenderResult> {
        let job = self.take_render_job(key)?;
        self.direct_renders
            .get_or_insert_with(BlockingRenderQueue::new)
            .render(job)
    }

    pub fn take_render_job(&mut self, key: RenderKey) -> Option<RenderJob> {
        if !self.has_render_work()
            || key.epoch != self.render_epoch
            || key.hard_epoch != self.hard_epoch
        {
            return None;
        }
        let invalidation = self
            .invalidation
            .take()
            .unwrap_or(RenderInvalidation::PAINT);
        let causes = std::mem::take(&mut self.render_causes);
        let had_cached_styles = self.cached_styles.is_some();
        let had_cached_layout = self.cached_layout.is_some();
        let (styles, previous_styles, css_warnings) =
            if invalidation.cascade || self.cached_styles.is_none() {
                let previous_styles = self.cached_styles.take();
                if invalidation.layout {
                    self.cached_layout = None;
                    self.cached_layout_styles = None;
                }
                (None, previous_styles, 0)
            } else {
                (self.cached_styles.clone(), None, self.cached_css_warnings)
            };
        let images = self.image_resources();
        let style_input = self.style_input(key);
        let layout = (!invalidation.layout)
            .then(|| self.cached_layout.take())
            .flatten();
        let layout_styles = layout
            .as_ref()
            .and_then(|_| self.cached_layout_styles.take());
        tracing::trace!(
            target: "textsurfer::perf",
            tab_id = key.tab_id,
            generation = key.generation,
            epoch = key.epoch,
            hard_epoch = key.hard_epoch,
            invalidation = ?invalidation,
            causes = %causes,
            causes_bits = causes.bits(),
            had_cached_styles,
            had_cached_layout,
            reused_styles = styles.is_some(),
            reused_layout = layout.is_some(),
            images = images.iter().count(),
            "render job prepared"
        );
        Some(RenderJob {
            key,
            causes,
            document: RenderTree::from_document(&self.document.borrow()),
            styles,
            previous_styles,
            style_input,
            forms: self.forms.clone(),
            images,
            media: self.media,
            palette: self.palette,
            layout,
            layout_styles,
            css_warnings,
        })
    }

    pub fn apply_render_result(&mut self, result: RenderResult) -> Option<RenderedPage> {
        if result.key.hard_epoch != self.hard_epoch {
            if let Some(pending) = self.invalidation {
                if !pending.layout {
                    self.cached_layout = Some(result.layout);
                    self.cached_layout_styles = Some(result.styles.clone());
                }
                if !pending.cascade {
                    self.cached_styles = Some(result.styles);
                }
            }
            tracing::trace!(
                target: "textsurfer::perf",
                epoch = result.key.epoch,
                result_hard_epoch = result.key.hard_epoch,
                current_hard_epoch = self.hard_epoch,
                causes = %result.causes,
                "render result rejected after hard invalidation"
            );
            return None;
        }
        if result.key.epoch <= self.last_published_epoch {
            tracing::trace!(
                target: "textsurfer::perf",
                epoch = result.key.epoch,
                last_published_epoch = self.last_published_epoch,
                causes = %result.causes,
                "render result rejected after newer publication"
            );
            return None;
        }
        let soft_stale = result.key.epoch != self.render_epoch;
        if result.key.epoch == self.render_epoch {
            self.cached_layout = Some(result.layout);
            self.cached_styles = Some(result.styles.clone());
            self.cached_layout_styles = Some(result.styles.clone());
            self.cached_css_warnings = result.css_warnings;
        } else {
            match self.invalidation {
                Some(pending) if !pending.layout && !pending.cascade => {
                    self.cached_layout = Some(result.layout);
                    self.cached_styles = Some(result.styles.clone());
                    self.cached_layout_styles = Some(result.styles.clone());
                }
                Some(pending) if pending.layout && !pending.cascade => {
                    self.cached_layout = None;
                    self.cached_layout_styles = None;
                    self.cached_styles = Some(result.styles.clone());
                }
                Some(pending) if !pending.layout && pending.cascade => {
                    self.cached_layout = Some(result.layout);
                    self.cached_layout_styles = Some(result.styles.clone());
                    self.cached_styles = None;
                }
                Some(_) | None => {
                    self.cached_layout = None;
                    self.cached_layout_styles = None;
                    self.cached_styles = None;
                }
            }
        }
        self.last_published_epoch = result.key.epoch;
        tracing::trace!(
            target: "textsurfer::perf",
            epoch = result.key.epoch,
            current_epoch = self.render_epoch,
            hard_epoch = result.key.hard_epoch,
            causes = %result.causes,
            soft_stale,
            pending_invalidation = ?self.invalidation,
            "render result published"
        );
        Some(RenderedPage {
            document: self.document.clone(),
            styles: result.styles,
            painted: result.painted,
            painted_changed: result.painted_changed,
            parse_errors: self.parse_errors,
            css_warnings: self.cached_css_warnings,
        })
    }

    fn style_input(&self, key: RenderKey) -> StyleInput {
        let roots = self
            .roots
            .iter()
            .filter_map(|root| match root {
                RootSource::Inline {
                    source,
                    media,
                    imports,
                    ..
                } => Some(StyleSource {
                    source: Some(Arc::from(source.as_str())),
                    base_url: self.effective_base.clone(),
                    media: Arc::from(media.as_str()),
                    imports: if self.external_disabled {
                        Arc::from([])
                    } else {
                        imports
                            .iter()
                            .map(|occurrence| self.style_source(*occurrence, None))
                            .collect::<Vec<_>>()
                            .into()
                    },
                }),
                RootSource::External { occurrence, media } if !self.external_disabled => {
                    Some(self.style_source(*occurrence, Some(media)))
                }
                RootSource::External { .. } => None,
            })
            .collect::<Vec<_>>()
            .into();
        StyleInput {
            session: StyleSessionId {
                tab_id: key.tab_id,
                generation: key.generation,
                revision: self.style_revision,
            },
            roots,
        }
    }

    fn style_source(&self, occurrence_id: usize, media: Option<&str>) -> StyleSource {
        let occurrence = &self.occurrences[occurrence_id];
        StyleSource {
            source: (!occurrence.failed)
                .then(|| occurrence.source.clone())
                .flatten(),
            base_url: occurrence.base_url.clone(),
            media: Arc::from(media.unwrap_or(&occurrence.media)),
            imports: occurrence
                .imports
                .iter()
                .map(|child| self.style_source(*child, None))
                .collect::<Vec<_>>()
                .into(),
        }
    }
}

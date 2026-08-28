use super::super::render::{RenderJob, RenderKey, RenderResult, RenderTree, RenderedPage};
use super::{PageLoad, RenderInvalidation, RootSource};
use crate::css::{CssRule, MediaQueryList, MediaRule, StateDeps, StyleSheet};

impl PageLoad {
    pub(super) fn render_page(&mut self) -> RenderedPage {
        let key = RenderKey {
            tab_id: 0,
            generation: 0,
            epoch: self.render_epoch,
            hard_epoch: self.hard_epoch,
        };
        let result = self
            .take_render_job(key)
            .expect("a direct render has pending work")
            .execute();
        self.apply_render_result(result)
            .expect("a direct render keeps its epoch")
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
        let (styles, previous_styles, cascade_state, sheets, css_warnings) =
            if invalidation.cascade || self.cached_styles.is_none() {
                let sheets = self.ordered_sheets();
                self.state_deps = sheets.iter().fold(StateDeps::default(), |deps, sheet| {
                    deps.union(sheet.state_deps)
                });
                self.state_effects = crate::css::StateEffects::for_sheets(&sheets, self.media);
                let css_warnings = sheets.iter().map(|sheet| sheet.diagnostics.total()).sum();
                let previous_styles = self.cached_styles.take();
                let cascade_state = self.cached_cascade_state.take();
                if invalidation.layout {
                    self.cached_layout = None;
                    self.cached_layout_styles = None;
                }
                (None, previous_styles, cascade_state, sheets, css_warnings)
            } else {
                (
                    self.cached_styles.clone(),
                    None,
                    self.cached_cascade_state.clone(),
                    Vec::new(),
                    self.cached_css_warnings,
                )
            };
        let images = self.image_resources();
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
            stylesheets = sheets.len(),
            images = images.iter().count(),
            "render job prepared"
        );
        Some(RenderJob {
            key,
            causes,
            document: RenderTree::from_document(&self.document.borrow()),
            styles,
            previous_styles,
            cascade_state,
            sheets,
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
                    self.cached_cascade_state = result.cascade_state;
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
            self.cached_cascade_state = result.cascade_state.clone();
            self.cached_layout_styles = Some(result.styles.clone());
            self.cached_css_warnings = result.css_warnings;
        } else {
            match self.invalidation {
                Some(pending) if !pending.layout && !pending.cascade => {
                    self.cached_layout = Some(result.layout);
                    self.cached_styles = Some(result.styles.clone());
                    self.cached_cascade_state = result.cascade_state.clone();
                    self.cached_layout_styles = Some(result.styles.clone());
                }
                Some(pending) if pending.layout && !pending.cascade => {
                    self.cached_layout = None;
                    self.cached_layout_styles = None;
                    self.cached_styles = Some(result.styles.clone());
                    self.cached_cascade_state = result.cascade_state.clone();
                }
                Some(pending) if !pending.layout && pending.cascade => {
                    self.cached_layout = Some(result.layout);
                    self.cached_layout_styles = Some(result.styles.clone());
                    self.cached_styles = None;
                    self.cached_cascade_state = None;
                }
                Some(_) | None => {
                    self.cached_layout = None;
                    self.cached_layout_styles = None;
                    self.cached_styles = None;
                    self.cached_cascade_state = None;
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

    fn ordered_sheets(&self) -> Vec<StyleSheet> {
        let mut sheets = Vec::new();
        for root in &self.roots {
            match root {
                RootSource::Inline {
                    sheet,
                    queries,
                    imports,
                } => {
                    if !self.external_disabled {
                        for occurrence in imports {
                            self.flatten_occurrence(
                                *occurrence,
                                std::slice::from_ref(queries),
                                &mut sheets,
                            );
                        }
                    }
                    sheets.push(sheet_without_imports(sheet, std::slice::from_ref(queries)));
                }
                RootSource::External(occurrence) if !self.external_disabled => {
                    self.flatten_occurrence(*occurrence, &[], &mut sheets);
                }
                RootSource::External(_) => {}
            }
        }
        sheets
    }

    fn flatten_occurrence(
        &self,
        occurrence_id: usize,
        inherited: &[MediaQueryList],
        output: &mut Vec<StyleSheet>,
    ) {
        let occurrence = &self.occurrences[occurrence_id];
        if occurrence.failed {
            return;
        }
        let mut chain = inherited.to_vec();
        chain.push(occurrence.queries.clone());
        for child in &occurrence.imports {
            self.flatten_occurrence(*child, &chain, output);
        }
        if let Some(sheet) = &occurrence.sheet {
            output.push(sheet_without_imports(sheet, &chain));
        }
    }
}

fn sheet_without_imports(sheet: &StyleSheet, queries: &[MediaQueryList]) -> StyleSheet {
    let mut rules: Vec<_> = sheet
        .rules
        .iter()
        .filter(|rule| !matches!(rule, CssRule::Import(_)))
        .cloned()
        .collect();
    for query in queries.iter().rev() {
        if !matches!(query, MediaQueryList::Always) {
            rules = vec![CssRule::Media(MediaRule {
                queries: query.clone(),
                rules,
            })];
        }
    }
    StyleSheet {
        rules,
        diagnostics: sheet.diagnostics.clone(),
        state_deps: sheet.state_deps,
    }
}

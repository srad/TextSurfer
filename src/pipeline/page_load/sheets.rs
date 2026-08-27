use crate::css::{CssRule, MediaQueryList, MediaRule, StateDeps, StyleSheet};

use super::super::render::{RenderedPage, render_document_with_images};
use super::{PageLoad, RootSource};

impl PageLoad {
    pub(super) fn render_page(&mut self) -> RenderedPage {
        let sheets = self.ordered_sheets();
        self.state_deps = sheets.iter().fold(StateDeps::default(), |deps, sheet| {
            deps.union(sheet.state_deps)
        });
        let images = self.image_resources();
        render_document_with_images(
            self.document.clone(),
            &sheets,
            self.media,
            self.palette,
            self.parse_errors,
            &images,
        )
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

use std::time::Duration;

use crate::css::cascade::media_query_list_matches;

use super::super::render::RenderedPage;
use super::{FetchState, PageLoad, RootSource};

impl PageLoad {
    pub fn render_if_ready(&mut self, now: Duration) -> Option<RenderedPage> {
        let settled = self.applicable_graph_settled();
        if !self.first_painted {
            if !settled && now < self.deadline && !self.external_disabled {
                return None;
            }
            self.first_painted = true;
            self.final_painted = settled;
            self.dirty = false;
            return Some(self.render_page());
        }
        if !self.final_painted && settled && self.dirty {
            self.final_painted = true;
            self.dirty = false;
            return Some(self.render_page());
        }
        None
    }

    pub fn force_render(&mut self) -> RenderedPage {
        self.first_painted = true;
        self.final_painted = self.applicable_graph_settled();
        self.dirty = false;
        self.render_page()
    }

    pub(super) fn applicable_graph_settled(&self) -> bool {
        if self.external_disabled {
            return true;
        }
        self.roots.iter().all(|root| match root {
            RootSource::Inline {
                queries, imports, ..
            } => {
                !media_query_list_matches(queries, self.media)
                    || imports
                        .iter()
                        .all(|occurrence| self.occurrence_settled(*occurrence, true))
            }
            RootSource::External(occurrence) => self.occurrence_settled(*occurrence, true),
        })
    }

    fn occurrence_settled(&self, occurrence_id: usize, inherited_match: bool) -> bool {
        let occurrence = &self.occurrences[occurrence_id];
        if occurrence.failed
            || !inherited_match
            || !media_query_list_matches(&occurrence.queries, self.media)
        {
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
}

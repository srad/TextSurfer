mod areas;
mod placement;
mod shorthand;
mod tracks;

use super::super::{BasicCascade, Cascade};
use crate::core::dom::{Document, ElementNs, NodeId};
use crate::core::style::{
    ComputedStyle, GridAreasData, GridIdent, GridLength, GridStyle, GridTemplateData, StyleStore,
    StyleTree, TrackBreadthMax, TrackBreadthMin, TrackSize,
};
use crate::css::{CssParser, CssparserParser, MediaContext};

/// A cascaded element plus the tree its grid handles resolve through.
pub(super) struct Grid {
    tree: StyleTree,
    node: NodeId,
}

pub(super) fn grid(declarations: &str) -> Grid {
    let mut document = Document::new();
    let node = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse(&format!("div {{ {declarations} }}"));
    Grid {
        tree: BasicCascade.apply(&[sheet], &document, MediaContext::screen()),
        node,
    }
}

impl Grid {
    pub(super) fn style(&self) -> GridStyle {
        self.tree.get(self.node).grid
    }

    pub(super) fn columns(&self) -> Option<&GridTemplateData> {
        self.tree.grid().template(self.style().template_columns?)
    }

    pub(super) fn rows(&self) -> Option<&GridTemplateData> {
        self.tree.grid().template(self.style().template_rows?)
    }

    pub(super) fn areas(&self) -> Option<&GridAreasData> {
        self.tree.grid().areas(self.style().template_areas?)
    }

    pub(super) fn auto_columns(&self) -> Option<&[TrackSize]> {
        self.tree.grid().tracks(self.style().auto_columns?)
    }

    pub(super) fn auto_rows(&self) -> Option<&[TrackSize]> {
        self.tree.grid().tracks(self.style().auto_rows?)
    }

    pub(super) fn ident(&self, handle: GridIdent) -> &str {
        self.tree.grid().ident(handle).expect("interned ident")
    }

    /// Line-name sets as plain strings, so a test can state the expected shape inline.
    pub(super) fn names(&self, sets: &[Vec<GridIdent>]) -> Vec<Vec<&str>> {
        sets.iter()
            .map(|set| set.iter().map(|name| self.ident(*name)).collect())
            .collect()
    }
}

pub(super) fn cells(value: usize) -> TrackSize {
    TrackSize::breadth(
        TrackBreadthMin::Length(GridLength::Cells(value)),
        TrackBreadthMax::Length(GridLength::Cells(value)),
    )
}

pub(super) fn fr(value: f32) -> TrackSize {
    TrackSize::breadth(
        TrackBreadthMin::Auto,
        TrackBreadthMax::Fr(crate::core::style::CssNumber::new(value).unwrap()),
    )
}

pub(super) fn min_content() -> TrackSize {
    TrackSize::breadth(TrackBreadthMin::MinContent, TrackBreadthMax::MinContent)
}

#[test]
fn invalid_declarations_roll_back_grid_and_math_storage() {
    let mut style = ComputedStyle::default();
    let mut store = StyleStore::default();
    for index in 0..128 {
        assert!(super::super::grid::apply_grid_declaration(
            &mut style,
            "grid-template-columns",
            &format!("[leak-{index}] calc(1ch + 1%) trailing"),
            MediaContext::screen(),
            &mut store,
        ));
    }
    assert_eq!(store.grid.stored_nodes(), 0);
    assert_eq!(store.calculations.stored_nodes(), 0);
    assert!(style.grid.template_columns.is_none());
}

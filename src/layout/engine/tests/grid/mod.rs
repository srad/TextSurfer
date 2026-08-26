mod areas;
mod content;
mod placement;
mod tracks;

use crate::core::dom::{Attr, Document, ElementNs, NodeId};
use crate::core::geom::Size;
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};
use crate::layout::{BoxTree, LayoutBox, LayoutEngine, TaffyLayoutEngine};

pub(super) struct GridFixture {
    pub(super) tree: BoxTree,
    pub(super) items: Vec<NodeId>,
}

/// A `display: grid` container over one `div` per entry in `items`, each carrying its own
/// declarations and its index as text so it occupies a cell.
pub(super) fn grid_fixture(container: &str, items: &[&str], viewport: Size) -> GridFixture {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let mut nodes = Vec::new();
    for (index, _) in items.iter().enumerate() {
        let child = document.insert_element(
            Some(root),
            "div",
            ElementNs::Html,
            vec![Attr::plain("class", &format!("i{index}"))],
        );
        document.insert_text(Some(child), &index.to_string());
        nodes.push(child);
    }
    let rules: String = items
        .iter()
        .enumerate()
        .map(|(index, declarations)| format!(".i{index} {{ {declarations} }}"))
        .collect();
    let sheet = CssparserParser.parse(&format!(
        "main {{ display: grid; margin: 0; {container} }}
         main > div {{ margin: 0; padding: 0 }}
         {rules}"
    ));
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    GridFixture {
        tree: TaffyLayoutEngine.layout(&document, &styles, viewport),
        items: nodes,
    }
}

impl GridFixture {
    /// The item's border box as `(col, row, width, height)`.
    pub(super) fn rect(&self, index: usize) -> (usize, usize, usize, usize) {
        let rect = box_for(&self.tree, self.items[index]).border_rect;
        (rect.col, rect.row, rect.width, rect.height)
    }
}

pub(super) fn box_for(tree: &BoxTree, node: NodeId) -> &LayoutBox {
    tree.boxes.iter().find(|box_| box_.node == node).unwrap()
}

pub(super) fn viewport(cols: u16) -> Size {
    Size { cols, rows: 24 }
}

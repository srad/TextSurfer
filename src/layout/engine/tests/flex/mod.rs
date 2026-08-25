mod axes;
mod content;
mod sizing;

use crate::core::dom::{Document, ElementNs, NodeId};
use crate::core::geom::Size;
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};
use crate::layout::{BoxTree, LayoutBox, LayoutEngine, TaffyLayoutEngine};

pub(super) struct FlexFixture {
    pub(super) tree: BoxTree,
    pub(super) container: NodeId,
    pub(super) items: Vec<NodeId>,
}

pub(super) fn flex_fixture(
    container: &str,
    item: &str,
    item_count: usize,
    viewport: Size,
) -> FlexFixture {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let mut items = Vec::new();
    for index in 0..item_count {
        let child = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
        document.insert_text(Some(child), &index.to_string());
        items.push(child);
    }
    let sheet = CssparserParser.parse(&format!(
        "main {{ display: flex; margin: 0; {container} }} main > div {{ margin: 0; padding: 0; {item} }}"
    ));
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    FlexFixture {
        tree: TaffyLayoutEngine.layout(&document, &styles, viewport),
        container: root,
        items,
    }
}

pub(super) fn box_for(tree: &BoxTree, node: NodeId) -> &LayoutBox {
    tree.boxes.iter().find(|box_| box_.node == node).unwrap()
}

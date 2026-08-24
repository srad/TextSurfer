use std::collections::HashMap;

use unicode_width::UnicodeWidthStr;

use crate::core::dom::{AttrNs, Document, ElementNs, Node, NodeId};
use crate::core::style::StyleTree;

use super::{BoxTree, LayoutRect, LinkBox};

pub(super) fn assign_link_rects(document: &Document, tree: &mut BoxTree) {
    if tree.links.is_empty() {
        return;
    }
    let owners: HashMap<NodeId, usize> = tree
        .links
        .iter()
        .enumerate()
        .map(|(index, link)| (link.node, index))
        .collect();
    let mut resolved: HashMap<NodeId, Option<usize>> = HashMap::new();
    let mut rects: Vec<Vec<LayoutRect>> = vec![Vec::new(); tree.links.len()];
    for fragment in &tree.fragments {
        let Some(index) = *resolved
            .entry(fragment.node)
            .or_insert_with(|| nearest_link(document, fragment.node, &owners))
        else {
            continue;
        };
        let rect = LayoutRect {
            col: fragment.col,
            row: fragment.row,
            width: UnicodeWidthStr::width(fragment.text.as_str()),
            height: 1,
        };
        if rect.width == 0 {
            continue;
        }
        match rects[index].last_mut() {
            Some(last) if last.row == rect.row && last.col + last.width == rect.col => {
                last.width += rect.width;
            }
            _ => rects[index].push(rect),
        }
    }
    for (index, link) in tree.links.iter_mut().enumerate() {
        link.rects = std::mem::take(&mut rects[index]);
    }
}

fn nearest_link(
    document: &Document,
    node: NodeId,
    owners: &HashMap<NodeId, usize>,
) -> Option<usize> {
    let mut current = Some(node);
    while let Some(id) = current {
        if let Some(index) = owners.get(&id) {
            return Some(*index);
        }
        current = document.parent(id);
    }
    None
}

pub(super) fn collect_links(
    document: &Document,
    styles: &StyleTree,
    id: NodeId,
    links: &mut Vec<LinkBox>,
) {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        if styles.get(id).display.is_none() {
            continue;
        }
        match document.node(id) {
            Some(Node::Element { name, ns, attrs }) => {
                if *ns == ElementNs::Html
                    && name == "a"
                    && let Some(href) = attrs
                        .iter()
                        .find(|attr| attr.ns == AttrNs::None && attr.name == "href")
                {
                    links.push(LinkBox {
                        node: id,
                        href: href.value.clone(),
                        rects: Vec::new(),
                    });
                }
                let children = document.children(id);
                stack.extend(children.into_iter().rev());
            }
            Some(Node::DocumentFragment) => {
                let children = document.children(id);
                stack.extend(children.into_iter().rev());
            }
            _ => {}
        }
    }
}

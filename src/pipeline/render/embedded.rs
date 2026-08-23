use crate::core::dom::{Document, Node};
use crate::css::{CssParser, CssparserParser, StyleSheet};

pub fn embedded_style_sheets(document: &Document) -> Vec<StyleSheet> {
    let parser = CssparserParser;
    let mut sheets = Vec::new();
    let mut stack: Vec<_> = document.roots().iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        if let Some(Node::Element { name, ns, .. }) = document.node(id)
            && *ns == crate::core::dom::ElementNs::Html
            && name == "style"
        {
            let source = document
                .children(id)
                .iter()
                .filter_map(|child| match document.node(*child) {
                    Some(Node::Text { data }) => Some(data.as_str()),
                    _ => None,
                })
                .collect::<String>();
            sheets.push(parser.parse(&source));
        }
        let children = document.children(id);
        stack.extend(children.into_iter().rev());
    }
    sheets
}

use cssparser::{Parser, ParserInput, Token};

use crate::core::dom::Document;
use crate::core::dom::{AttrNs, ElementNs, Node, NodeId};
use crate::core::style::{ComputedStyle, Display, EdgeSizes, StyleTree, WhiteSpace};
use crate::css::parser::parse_declarations;
use crate::css::selectors::matching_specificity;
use crate::css::{Declaration, StyleSheet};

pub trait Cascade: Send + Sync {
    fn apply(&self, sheets: &[StyleSheet], document: &Document) -> StyleTree;
}

#[derive(Default)]
pub struct BasicCascade;

impl Cascade for BasicCascade {
    fn apply(&self, sheets: &[StyleSheet], document: &Document) -> StyleTree {
        let mut tree = StyleTree::default();
        for id in elements_in_document_order(document) {
            let mut style = ua_style(document, id);
            let mut declarations = Vec::new();
            let mut order = 0usize;
            for sheet in sheets {
                for rule in &sheet.rules {
                    if let Some(specificity) = matching_specificity(&rule.selectors, document, id) {
                        for declaration in &rule.declarations {
                            declarations.push((
                                declaration.important,
                                specificity,
                                order,
                                declaration.clone(),
                            ));
                            order += 1;
                        }
                    }
                }
            }
            if let Some(inline) = inline_style(document, id) {
                for declaration in parse_declarations(inline) {
                    declarations.push((declaration.important, u32::MAX, order, declaration));
                    order += 1;
                }
            }
            declarations.sort_by_key(|(important, specificity, order, _)| {
                (*important, *specificity, *order)
            });
            for (_, _, _, declaration) in declarations {
                apply_declaration(&mut style, &declaration);
            }
            tree.insert(id, style);
        }
        tree
    }
}

fn elements_in_document_order(document: &Document) -> Vec<NodeId> {
    let mut elements = Vec::new();
    let mut stack: Vec<NodeId> = document.roots().iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        if matches!(document.node(id), Some(Node::Element { .. })) {
            elements.push(id);
        }
        let children = document.children(id);
        stack.extend(children.into_iter().rev());
    }
    elements
}

fn ua_style(document: &Document, id: NodeId) -> ComputedStyle {
    let Some(Node::Element { name, ns, .. }) = document.node(id) else {
        return ComputedStyle::default();
    };
    if *ns != ElementNs::Html {
        return ComputedStyle::default();
    }
    let display = if matches!(
        name.as_str(),
        "head" | "base" | "link" | "meta" | "title" | "style" | "script" | "template"
    ) {
        Display::None
    } else if matches!(
        name.as_str(),
        "html"
            | "body"
            | "main"
            | "article"
            | "section"
            | "nav"
            | "aside"
            | "header"
            | "footer"
            | "address"
            | "div"
            | "p"
            | "pre"
            | "blockquote"
            | "ul"
            | "ol"
            | "li"
            | "dl"
            | "dt"
            | "dd"
            | "figure"
            | "figcaption"
            | "form"
            | "fieldset"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
    ) {
        Display::Block
    } else {
        Display::Inline
    };
    let mut style = ComputedStyle {
        display,
        ..Default::default()
    };
    if name == "pre" {
        style.white_space = WhiteSpace::Pre;
    }
    if matches!(
        name.as_str(),
        "p" | "pre" | "blockquote" | "ul" | "ol" | "table"
    ) {
        style.margin.bottom = 1;
    }
    if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
        style.margin.top = 1;
        style.margin.bottom = 1;
    }
    if name == "blockquote" {
        style.margin.left = 2;
    }
    style
}

fn inline_style(document: &Document, id: NodeId) -> Option<&str> {
    let Some(Node::Element { attrs, .. }) = document.node(id) else {
        return None;
    };
    attrs
        .iter()
        .find(|attr| attr.ns == AttrNs::None && attr.name == "style")
        .map(|attr| attr.value.as_str())
}

fn apply_declaration(style: &mut ComputedStyle, declaration: &Declaration) {
    match declaration.name.as_str() {
        "display" => {
            if let Some(display) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "none" => Some(Display::None),
                    "block" | "flow-root" | "list-item" | "table" | "flex" | "grid"
                    | "inline-block" | "inline-flex" | "inline-grid" => Some(Display::Block),
                    "inline" | "initial" => Some(Display::Inline),
                    _ => None,
                })
            {
                style.display = display;
            }
        }
        "white-space" => {
            if let Some(white_space) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "pre" | "pre-wrap" | "break-spaces" => Some(WhiteSpace::Pre),
                    "normal" | "nowrap" | "pre-line" | "initial" => Some(WhiteSpace::Normal),
                    _ => None,
                })
            {
                style.white_space = white_space;
            }
        }
        "margin" => assign_edges(&mut style.margin, &parse_lengths(&declaration.value)),
        "padding" => assign_edges(&mut style.padding, &parse_lengths(&declaration.value)),
        "margin-top" => assign_one(&mut style.margin.top, &declaration.value),
        "margin-right" => assign_one(&mut style.margin.right, &declaration.value),
        "margin-bottom" => assign_one(&mut style.margin.bottom, &declaration.value),
        "margin-left" => assign_one(&mut style.margin.left, &declaration.value),
        "padding-top" => assign_one(&mut style.padding.top, &declaration.value),
        "padding-right" => assign_one(&mut style.padding.right, &declaration.value),
        "padding-bottom" => assign_one(&mut style.padding.bottom, &declaration.value),
        "padding-left" => assign_one(&mut style.padding.left, &declaration.value),
        "border" => {
            if let Some(border) = parse_border(&declaration.value) {
                style.border = border;
            }
        }
        "border-style" => {
            if let Some(border) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "none" | "hidden" | "initial" => Some(false),
                    "solid" | "dotted" | "dashed" | "double" | "groove" | "ridge" | "inset"
                    | "outset" => Some(true),
                    _ => None,
                })
            {
                style.border = border;
            }
        }
        _ => {}
    }
}

fn parse_ident(source: &str) -> Option<String> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parser.expect_ident_cloned().ok()?;
    parser.expect_exhausted().ok()?;
    Some(value.to_ascii_lowercase())
}

fn parse_border(source: &str) -> Option<bool> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut visible_style = false;
    let mut zero_width = false;
    while let Ok(token) = parser.next() {
        match token {
            Token::Ident(value) if value.eq_ignore_ascii_case("none") => return Some(false),
            Token::Ident(value) if value.eq_ignore_ascii_case("hidden") => return Some(false),
            Token::Ident(value) if value.eq_ignore_ascii_case("initial") => return Some(false),
            Token::Ident(value)
                if [
                    "solid", "dotted", "dashed", "double", "groove", "ridge", "inset", "outset",
                ]
                .iter()
                .any(|style| value.eq_ignore_ascii_case(style)) =>
            {
                visible_style = true;
            }
            Token::Number { value, .. } | Token::Dimension { value, .. } if *value == 0.0 => {
                zero_width = true;
            }
            _ => {}
        }
    }
    if zero_width {
        Some(false)
    } else if visible_style {
        Some(true)
    } else {
        None
    }
}

fn assign_one(target: &mut usize, value: &str) {
    if let Some(value) = parse_lengths(value).first() {
        *target = *value;
    }
}

fn assign_edges(edges: &mut EdgeSizes, values: &[usize]) {
    match values {
        [all] => {
            *edges = EdgeSizes {
                top: *all,
                right: *all,
                bottom: *all,
                left: *all,
            }
        }
        [vertical, horizontal] => {
            *edges = EdgeSizes {
                top: *vertical,
                right: *horizontal,
                bottom: *vertical,
                left: *horizontal,
            };
        }
        [top, horizontal, bottom] => {
            *edges = EdgeSizes {
                top: *top,
                right: *horizontal,
                bottom: *bottom,
                left: *horizontal,
            };
        }
        [top, right, bottom, left, ..] => {
            *edges = EdgeSizes {
                top: *top,
                right: *right,
                bottom: *bottom,
                left: *left,
            };
        }
        [] => {}
    }
}

fn parse_lengths(source: &str) -> Vec<usize> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while let Ok(token) = parser.next() {
        match token {
            Token::Dimension { value, .. } | Token::Number { value, .. } if *value >= 0.0 => {
                values.push(value.round() as usize);
            }
            _ => {}
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dom::{Attr, ElementNs};
    use crate::css::{CssParser, CssparserParser};

    #[test]
    fn ua_and_author_rules_form_a_computed_style_tree() {
        let mut document = Document::new();
        let main = document.insert_element(None, "main", ElementNs::Html, vec![]);
        let p = document.insert_element(
            Some(main),
            "p",
            ElementNs::Html,
            vec![Attr::plain("class", "note")],
        );
        let span = document.insert_element(
            Some(p),
            "span",
            ElementNs::Html,
            vec![Attr::plain("style", "display: inline")],
        );
        let sheet = CssparserParser.parse("main > p.note { display: none; margin: 2px 3px }");
        let styles = BasicCascade.apply(&[sheet], &document);
        assert_eq!(styles.get(main).display, Display::Block);
        assert_eq!(styles.get(p).display, Display::None);
        assert_eq!(styles.get(p).margin.left, 3);
        assert_eq!(styles.get(span).display, Display::Inline);
    }

    #[test]
    fn important_author_rules_override_normal_inline_declarations() {
        let mut document = Document::new();
        let p = document.insert_element(
            None,
            "p",
            ElementNs::Html,
            vec![Attr::plain("style", "display: block")],
        );
        let sheet = CssparserParser.parse("p { display: none !important }");
        let styles = BasicCascade.apply(&[sheet], &document);
        assert_eq!(styles.get(p).display, Display::None);
    }

    #[test]
    fn comments_are_tokenized_and_invalid_values_do_not_override_ua_defaults() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let pre = document.insert_element(Some(p), "pre", ElementNs::Html, vec![]);
        let sheet = CssparserParser
            .parse("p { display: block /**/; border: none /**/ } pre { white-space: invalid }");
        let styles = BasicCascade.apply(&[sheet], &document);
        assert_eq!(styles.get(p).display, Display::Block);
        assert!(!styles.get(p).border);
        assert_eq!(styles.get(pre).white_space, WhiteSpace::Pre);
    }
}

use taffy::prelude::{
    AvailableSpace, Dimension, Display as TaffyDisplay, LengthPercentageAuto, Rect as TaffyRect,
    Size as TaffySize, Style as TaffyStyle, TaffyTree,
};
use textwrap::Options;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::core::dom::{AttrNs, Document, ElementNs, Node, NodeId};
use crate::core::geom::Size;
use crate::core::style::{ComputedStyle, Display, StyleTree, WhiteSpace};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoxTree {
    pub width: usize,
    pub height: usize,
    pub boxes: Vec<LayoutBox>,
    pub lines: Vec<LayoutLine>,
    pub links: Vec<LinkBox>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutRect {
    pub col: usize,
    pub row: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutBox {
    pub node: NodeId,
    pub rect: LayoutRect,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutLine {
    pub node: NodeId,
    pub col: usize,
    pub row: usize,
    pub text: String,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkBox {
    pub node: NodeId,
    pub href: String,
}

pub trait LayoutEngine: Send + Sync {
    fn layout(&self, document: &Document, styles: &StyleTree, viewport: Size) -> BoxTree;
}

#[derive(Default)]
pub struct TaffyLayoutEngine;

impl LayoutEngine for TaffyLayoutEngine {
    fn layout(&self, document: &Document, styles: &StyleTree, viewport: Size) -> BoxTree {
        let width = usize::from(viewport.cols).max(1);
        let mut drafts = Vec::new();
        let mut links = Vec::new();
        for root in document.roots() {
            collect_links(document, styles, *root, &mut links);
            collect_blocks(document, styles, *root, &mut drafts, 0);
        }
        let mut taffy: TaffyTree<usize> = TaffyTree::new();
        let mut nodes = Vec::with_capacity(drafts.len());
        for (index, draft) in drafts.iter_mut().enumerate() {
            let chrome = usize::from(draft.style.border) * 2
                + draft.style.padding.left
                + draft.style.padding.right;
            let content_width = width
                .saturating_sub(draft.style.margin.left + draft.style.margin.right + chrome)
                .max(1);
            draft.lines = wrap_text(&draft.text, content_width, draft.style.white_space);
            let height = draft.lines.len()
                + draft.style.padding.top
                + draft.style.padding.bottom
                + usize::from(draft.style.border) * 2;
            let style = TaffyStyle {
                display: TaffyDisplay::Block,
                size: TaffySize {
                    width: Dimension::percent(1.0),
                    height: Dimension::length(height as f32),
                },
                margin: TaffyRect {
                    left: LengthPercentageAuto::length(draft.style.margin.left as f32),
                    right: LengthPercentageAuto::length(draft.style.margin.right as f32),
                    top: LengthPercentageAuto::length(draft.style.margin.top as f32),
                    bottom: LengthPercentageAuto::length(draft.style.margin.bottom as f32),
                },
                ..Default::default()
            };
            nodes.push(
                taffy
                    .new_leaf_with_context(style, index)
                    .expect("taffy leaf"),
            );
        }
        let root = taffy
            .new_with_children(
                TaffyStyle {
                    display: TaffyDisplay::Block,
                    size: TaffySize {
                        width: Dimension::length(width as f32),
                        height: Dimension::auto(),
                    },
                    ..Default::default()
                },
                &nodes,
            )
            .expect("taffy root");
        taffy
            .compute_layout(
                root,
                TaffySize {
                    width: AvailableSpace::Definite(width as f32),
                    height: AvailableSpace::MaxContent,
                },
            )
            .expect("taffy block layout");

        let mut tree = BoxTree {
            width,
            height: taffy.layout(root).expect("root layout").size.height.ceil() as usize,
            links,
            ..Default::default()
        };
        for (draft, node) in drafts.iter().zip(nodes) {
            let layout = taffy.layout(node).expect("leaf layout");
            let rect = LayoutRect {
                col: layout.location.x.max(0.0).round() as usize,
                row: layout.location.y.max(0.0).round() as usize,
                width: layout.size.width.max(0.0).round() as usize,
                height: layout.size.height.max(0.0).round() as usize,
            };
            tree.boxes.push(LayoutBox {
                node: draft.node,
                rect,
                depth: draft.depth,
            });
            append_box_lines(&mut tree.lines, draft, rect);
        }
        tree
    }
}

struct BlockDraft {
    node: NodeId,
    text: String,
    lines: Vec<String>,
    style: ComputedStyle,
    depth: usize,
}

fn collect_blocks(
    document: &Document,
    styles: &StyleTree,
    id: NodeId,
    drafts: &mut Vec<BlockDraft>,
    depth: usize,
) {
    let mut stack = vec![(id, depth)];
    while let Some((id, depth)) = stack.pop() {
        match document.node(id) {
            Some(Node::Element { name, ns, .. }) => {
                let style = styles.get(id);
                if style.display == Display::None {
                    continue;
                }
                if style.display == Display::Block {
                    let mut text = inline_text(document, styles, id);
                    if *ns == ElementNs::Html {
                        if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                            let level = name[1..].parse::<usize>().unwrap_or(1);
                            text = format!("{} {text}", "#".repeat(level));
                        } else if name == "li" {
                            text = format!("• {text}");
                        } else if name == "hr" {
                            text = "─".to_string();
                        }
                    }
                    if !text.trim().is_empty() || style.border {
                        drafts.push(BlockDraft {
                            node: id,
                            text,
                            lines: Vec::new(),
                            style,
                            depth,
                        });
                    }
                }
                let children = document.children(id);
                stack.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
            }
            Some(Node::DocumentFragment) => {
                let children = document.children(id);
                stack.extend(children.into_iter().rev().map(|child| (child, depth)));
            }
            _ => {}
        }
    }
}

fn collect_links(document: &Document, styles: &StyleTree, id: NodeId, links: &mut Vec<LinkBox>) {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        if styles.get(id).display == Display::None {
            continue;
        }
        if let Some(Node::Element { name, ns, attrs }) = document.node(id) {
            if *ns == ElementNs::Html
                && name == "a"
                && let Some(href) = attrs
                    .iter()
                    .find(|attr| attr.ns == AttrNs::None && attr.name == "href")
            {
                links.push(LinkBox {
                    node: id,
                    href: href.value.clone(),
                });
            }
            let children = document.children(id);
            stack.extend(children.into_iter().rev());
        }
    }
}

fn inline_text(document: &Document, styles: &StyleTree, id: NodeId) -> String {
    let mut text = String::new();
    let mut stack: Vec<_> = document.children(id).into_iter().rev().collect();
    while let Some(child) = stack.pop() {
        match document.node(child) {
            Some(Node::Text { data }) => text.push_str(data),
            Some(Node::Element { .. }) if styles.get(child).display == Display::None => {}
            Some(Node::Element { .. }) if styles.get(child).display == Display::Block => {}
            Some(Node::Element { name, .. }) => {
                if name == "br" {
                    text.push('\n');
                } else {
                    let children = document.children(child);
                    stack.extend(children.into_iter().rev());
                }
            }
            _ => {}
        }
    }
    text
}

fn wrap_text(text: &str, width: usize, white_space: WhiteSpace) -> Vec<String> {
    if white_space == WhiteSpace::Pre {
        return text.split('\n').map(str::to_string).collect();
    }
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return Vec::new();
    }
    textwrap::wrap(&collapsed, Options::new(width).break_words(true))
        .into_iter()
        .map(|line| line.into_owned())
        .collect()
}

fn append_box_lines(lines: &mut Vec<LayoutLine>, draft: &BlockDraft, rect: LayoutRect) {
    let border = usize::from(draft.style.border);
    if draft.style.border && rect.width >= 2 {
        lines.push(LayoutLine {
            node: draft.node,
            col: rect.col,
            row: rect.row,
            text: format!("┌{}┐", "─".repeat(rect.width.saturating_sub(2))),
            depth: draft.depth,
        });
    }
    let content_col = rect.col + border + draft.style.padding.left;
    let content_row = rect.row + border + draft.style.padding.top;
    for (offset, text) in draft.lines.iter().enumerate() {
        let text = if draft.style.border && rect.width >= 2 {
            let inner = rect.width.saturating_sub(2);
            let text = fit_cells(text, inner);
            let padding = inner.saturating_sub(UnicodeWidthStr::width(text.as_str()));
            format!("│{text}{}│", " ".repeat(padding))
        } else {
            text.clone()
        };
        lines.push(LayoutLine {
            node: draft.node,
            col: if draft.style.border {
                rect.col
            } else {
                content_col
            },
            row: content_row + offset,
            text,
            depth: draft.depth,
        });
    }
    if draft.style.border && rect.width >= 2 {
        lines.push(LayoutLine {
            node: draft.node,
            col: rect.col,
            row: rect.row + rect.height.saturating_sub(1),
            text: format!("└{}┘", "─".repeat(rect.width.saturating_sub(2))),
            depth: draft.depth,
        });
    }
}

fn fit_cells(text: &str, width: usize) -> String {
    let mut fitted = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let cells = UnicodeWidthStr::width(grapheme);
        if used + cells > width {
            break;
        }
        fitted.push_str(grapheme);
        used += cells;
    }
    fitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dom::ElementNs;
    use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};
    use crate::paint::{BasicPainter, Painter};
    use proptest::prelude::*;

    fn paragraph(text: &str) -> (Document, StyleTree) {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        document.insert_text(Some(p), text);
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        (document, styles)
    }

    proptest! {
        #[test]
        fn wider_viewports_never_increase_single_block_height(
            text in "[a-z ]{0,200}",
            narrow in 1u16..40,
            extra in 0u16..40,
        ) {
            let (document, styles) = paragraph(&text);
            let narrow_tree = TaffyLayoutEngine.layout(
                &document,
                &styles,
                Size { cols: narrow, rows: 5 },
            );
            let wide_tree = TaffyLayoutEngine.layout(
                &document,
                &styles,
                Size { cols: narrow + extra, rows: 5 },
            );
            prop_assert!(wide_tree.height <= narrow_tree.height);
        }

        #[test]
        fn painted_rows_never_exceed_the_viewport(
            chars in prop::collection::vec(any::<char>(), 0..100),
            width in 1u16..80,
        ) {
            let text: String = chars.into_iter().collect();
            let (document, styles) = paragraph(&text);
            let tree = TaffyLayoutEngine.layout(
                &document,
                &styles,
                Size { cols: width, rows: 5 },
            );
            let display = BasicPainter.paint(&tree);
            for line in display.lines {
                prop_assert!(UnicodeWidthStr::width(line.as_str()) <= usize::from(width));
            }
        }
    }

    #[test]
    fn block_text_wraps_to_the_terminal_width_and_hidden_nodes_disappear() {
        let mut document = Document::new();
        let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
        let head = document.insert_element(Some(html), "head", ElementNs::Html, vec![]);
        document.insert_text(Some(head), "hidden");
        let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
        let p = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
        document.insert_text(Some(p), "one two three four");
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 9, rows: 5 });
        let text: Vec<&str> = tree.lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(text, vec!["one two", "three", "four"]);
        assert!(!text.iter().any(|line| line.contains("hidden")));
    }

    #[test]
    fn long_words_break_to_terminal_width_even_when_author_css_requests_normal_wrapping() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        document.insert_text(Some(p), "abcdefghijk");
        let sheet = CssparserParser.parse("p { overflow-wrap: normal }");
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 4, rows: 5 });
        assert_eq!(
            tree.lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            vec!["abcd", "efgh", "ijk"]
        );
    }

    #[test]
    fn preformatted_text_preserves_spaces_and_line_breaks() {
        let mut document = Document::new();
        let pre = document.insert_element(None, "pre", ElementNs::Html, vec![]);
        document.insert_text(Some(pre), "  a\n b");
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 4, rows: 5 });
        assert_eq!(
            tree.lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            vec!["  a", " b"]
        );
    }

    #[test]
    fn links_are_discovered_inside_inline_content() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let link = document.insert_element(
            Some(p),
            "a",
            ElementNs::Html,
            vec![crate::core::dom::Attr::plain("href", "/next")],
        );
        document.insert_text(Some(link), "next");
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
        assert_eq!(
            tree.links,
            vec![LinkBox {
                node: link,
                href: "/next".to_string()
            }]
        );
    }

    #[test]
    fn deeply_nested_inline_content_is_walked_iteratively() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let mut parent = p;
        for _ in 0..4096 {
            parent = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
        }
        document.insert_text(Some(parent), "deep");
        let mut styles = StyleTree::default();
        styles.insert(
            p,
            ComputedStyle {
                display: Display::Block,
                ..Default::default()
            },
        );
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
        assert_eq!(tree.lines[0].text, "deep");
    }

    #[test]
    fn bordered_wide_text_keeps_the_right_border_in_its_cell() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        document.insert_text(Some(p), "界x");
        let mut styles = StyleTree::default();
        styles.insert(
            p,
            ComputedStyle {
                display: Display::Block,
                border: true,
                ..Default::default()
            },
        );
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 6, rows: 5 });
        assert_eq!(tree.lines[1].text, "│界x │");
        assert_eq!(UnicodeWidthStr::width(tree.lines[1].text.as_str()), 6);
    }
}

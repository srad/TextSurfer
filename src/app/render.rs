use mediatype::{MediaType, names};

use crate::core::dom::{Document, Node, SharedDocument};
use crate::core::geom::Size;
use crate::core::style::{Palette, StyleTree};
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext, StyleSheet};
use crate::html::{Html5everParser, HtmlParser};
use crate::layout::{LayoutEngine, TaffyLayoutEngine};
use crate::paint::{BasicPainter, DisplayList, Painter};

pub enum ResponseKind {
    Html,
    PlainText,
    Unsupported(String),
}

pub struct RenderedPage {
    pub document: SharedDocument,
    pub styles: StyleTree,
    pub painted: DisplayList,
    pub parse_errors: usize,
    pub css_warnings: usize,
}

pub fn response_kind(content_type: Option<&str>) -> ResponseKind {
    let Some(content_type) = content_type else {
        return ResponseKind::Html;
    };
    let Ok(media_type) = MediaType::parse(content_type) else {
        return ResponseKind::Html;
    };
    if media_type.ty == names::TEXT && media_type.subty == names::HTML
        || media_type.ty == names::APPLICATION
            && media_type.subty == names::XHTML
            && media_type.suffix == Some(names::XML)
    {
        ResponseKind::Html
    } else if media_type.ty == names::TEXT && media_type.subty == names::PLAIN {
        ResponseKind::PlainText
    } else {
        ResponseKind::Unsupported(media_type.essence().to_string())
    }
}

pub fn render_html(source: &str, width: usize, palette: Palette, scripting: bool) -> RenderedPage {
    let outcome = Html5everParser::new(scripting).parse_document(source);
    let sheets = embedded_style_sheets(&outcome.document.borrow());
    let css_warnings = sheets
        .iter()
        .map(|sheet| sheet.diagnostics.total())
        .sum::<usize>();
    let styles = BasicCascade.apply(
        &sheets,
        &outcome.document.borrow(),
        MediaContext::screen().with_palette(palette),
    );
    let painted = paint_document(&outcome.document.borrow(), &styles, width, palette);
    RenderedPage {
        document: outcome.document,
        styles,
        painted,
        parse_errors: outcome.parse_errors,
        css_warnings,
    }
}

pub fn paint_document(
    document: &Document,
    styles: &StyleTree,
    width: usize,
    palette: Palette,
) -> DisplayList {
    let boxes = TaffyLayoutEngine.layout(
        document,
        styles,
        Size {
            cols: width.min(usize::from(u16::MAX)) as u16,
            rows: u16::MAX,
        },
    );
    BasicPainter.paint(&boxes, palette)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendering_html_reports_parse_and_style_diagnostics_with_the_painted_page() {
        let page = render_html(
            "<style>p { display: block; color: bogus }</style><p>hello</p>",
            20,
            Palette::default(),
            false,
        );
        assert!(
            page.painted
                .text_lines()
                .iter()
                .any(|line| line.contains("hello"))
        );
        assert_eq!(page.css_warnings, 0);
        assert!(
            page.styles.get(page.document.borrow().roots()[0]).display
                != crate::core::style::Display::None
        );
    }

    #[test]
    fn media_types_route_to_the_matching_renderer() {
        assert!(matches!(response_kind(None), ResponseKind::Html));
        assert!(matches!(
            response_kind(Some("text/html; charset=utf-8")),
            ResponseKind::Html
        ));
        assert!(matches!(
            response_kind(Some("text/plain")),
            ResponseKind::PlainText
        ));
        assert!(matches!(
            response_kind(Some("image/png")),
            ResponseKind::Unsupported(kind) if kind == "image/png"
        ));
    }
}

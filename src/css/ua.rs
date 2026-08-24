//! The user-agent stylesheet, hardcoded as Rust rather than CSS text (locked in ROADMAP.md). A
//! policy table keyed on element name, not cascade logic.

use crate::core::dom::{AttrNs, Document, ElementNs, Node, NodeId, attr_value};
use crate::core::style::{
    BorderSpacing, ComputedStyle, CssMargin, Display, FontSize, ListStyleType, Palette, Rgba,
    TextAlign, VerticalAlign, WhiteSpace,
};

use super::values::parse_list_style_type;

pub(super) fn ua_style(
    document: &Document,
    id: NodeId,
    parent_style: Option<ComputedStyle>,
    palette: Palette,
) -> ComputedStyle {
    let inherited = parent_style.unwrap_or_default();
    let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
        return ComputedStyle {
            color: inherited.color,
            bold: inherited.bold,
            underline: inherited.underline,
            strike: inherited.strike,
            list_style_type: inherited.list_style_type,
            list_style_position: inherited.list_style_position,
            border_collapse: inherited.border_collapse,
            border_spacing: inherited.border_spacing,
            caption_side: inherited.caption_side,
            text_align: inherited.text_align,
            legacy_align: inherited.legacy_align,
            font_size: inherited.font_size,
            ..Default::default()
        };
    };
    if *ns != ElementNs::Html {
        return ComputedStyle {
            color: inherited.color,
            bold: inherited.bold,
            underline: inherited.underline,
            strike: inherited.strike,
            list_style_type: inherited.list_style_type,
            list_style_position: inherited.list_style_position,
            border_collapse: inherited.border_collapse,
            border_spacing: inherited.border_spacing,
            caption_side: inherited.caption_side,
            font_size: inherited.font_size,
            ..Default::default()
        };
    }
    let display = if matches!(
        name.as_str(),
        "head" | "base" | "link" | "meta" | "title" | "style" | "script" | "template"
    ) {
        Display::NONE
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
            | "center"
            | "p"
            | "pre"
            | "blockquote"
            | "ul"
            | "ol"
            | "dl"
            | "dt"
            | "dd"
            | "figure"
            | "figcaption"
            | "form"
            | "fieldset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
    ) {
        Display::BLOCK
    } else if name == "li" {
        Display::LIST_ITEM
    } else {
        match name.as_str() {
            "table" => Display::TABLE,
            "thead" => Display::TABLE_HEADER_GROUP,
            "tbody" => Display::TABLE_ROW_GROUP,
            "tfoot" => Display::TABLE_FOOTER_GROUP,
            "tr" => Display::TABLE_ROW,
            "td" | "th" => Display::TABLE_CELL,
            "col" => Display::TABLE_COLUMN,
            "colgroup" => Display::TABLE_COLUMN_GROUP,
            "caption" => Display::TABLE_CAPTION,
            _ => Display::INLINE,
        }
    };
    let mut style = ComputedStyle {
        display,
        white_space: inherited.white_space,
        color: inherited.color,
        bold: inherited.bold,
        underline: inherited.underline,
        strike: inherited.strike,
        list_style_type: inherited.list_style_type,
        list_style_position: inherited.list_style_position,
        border_collapse: inherited.border_collapse,
        border_spacing: inherited.border_spacing,
        caption_side: inherited.caption_side,
        text_align: inherited.text_align,
        legacy_align: inherited.legacy_align,
        font_size: inherited.font_size,
        ..Default::default()
    };
    if name == "pre" {
        style.white_space = WhiteSpace::Pre;
    }
    if matches!(name.as_str(), "ul" | "menu") {
        style.list_style_type = nested_bullet_type(document, id);
    }
    if name == "ol" {
        style.list_style_type = ListStyleType::Decimal;
    }
    if matches!(name.as_str(), "ol" | "ul" | "menu" | "li")
        && let Some(value) = attr_value(attrs, "type")
        && let Some(list_type) = parse_html_list_type(value)
    {
        style.list_style_type = list_type;
    }
    if name == "table" {
        style.border_spacing = BorderSpacing::new(1, 0);
    }
    if name == "caption" {
        style.text_align = TextAlign::Center;
    }
    if matches!(name.as_str(), "thead" | "tbody" | "tfoot")
        || name == "tr" && document.parent(id).is_some_and(|parent| {
            matches!(document.node(parent), Some(Node::Element { name, ns, .. }) if *ns == ElementNs::Html && name == "table")
        })
    {
        style.vertical_align = VerticalAlign::Middle;
    }
    if matches!(name.as_str(), "tr" | "td" | "th") {
        style.vertical_align = inherited.vertical_align;
    }
    if name == "th" && inherited.text_align == TextAlign::Start {
        style.text_align = TextAlign::Center;
    }
    if name == "hr" {
        style.margin.left = CssMargin::Auto;
        style.margin.right = CssMargin::Auto;
    }
    if name == "a"
        && attrs
            .iter()
            .any(|attr| attr.ns == AttrNs::None && attr.name == "href")
    {
        style.color = Some(Rgba::opaque(palette.link));
        style.underline = true;
    }
    if matches!(name.as_str(), "b" | "strong" | "th") {
        style.bold = true;
    }
    if matches!(name.as_str(), "u" | "ins") {
        style.underline = true;
    }
    if matches!(name.as_str(), "s" | "del" | "strike") {
        style.strike = true;
    }
    if matches!(
        name.as_str(),
        "p" | "pre" | "blockquote" | "ul" | "ol" | "table"
    ) {
        style.margin.bottom = CssMargin::Cells(1);
    }
    if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
        style.margin.top = CssMargin::Cells(1);
        style.margin.bottom = CssMargin::Cells(1);
        style.bold = true;
        let ratio = match name.as_str() {
            "h1" => 2.0,
            "h2" => 1.5,
            "h3" => 1.17,
            "h4" => 1.0,
            "h5" => 0.83,
            _ => 0.67,
        };
        style.font_size =
            FontSize::from_px(inherited.font_size.px() * ratio).unwrap_or(FontSize::INITIAL);
    }
    if name == "blockquote" {
        style.margin.left = CssMargin::Cells(2);
    }
    style
}

/// Real UA sheets step the bullet through disc, circle and square as unordered lists nest.
fn nested_bullet_type(document: &Document, id: NodeId) -> ListStyleType {
    let mut lists = 0usize;
    let mut current = document.parent(id);
    while let Some(node) = current {
        if let Some(Node::Element { name, ns, .. }) = document.node(node)
            && *ns == ElementNs::Html
            && matches!(name.as_str(), "ul" | "menu")
        {
            lists += 1;
        }
        current = document.parent(node);
    }
    match lists % 3 {
        0 => ListStyleType::Disc,
        1 => ListStyleType::Circle,
        _ => ListStyleType::Square,
    }
}

fn parse_html_list_type(value: &str) -> Option<ListStyleType> {
    match value.trim() {
        "1" => Some(ListStyleType::Decimal),
        "a" => Some(ListStyleType::LowerAlpha),
        "A" => Some(ListStyleType::UpperAlpha),
        "i" => Some(ListStyleType::LowerRoman),
        "I" => Some(ListStyleType::UpperRoman),
        other => parse_list_style_type(other),
    }
}

pub(super) fn inline_style(document: &Document, id: NodeId) -> Option<&str> {
    let Some(Node::Element { attrs, .. }) = document.node(id) else {
        return None;
    };
    attrs
        .iter()
        .find(|attr| attr.ns == AttrNs::None && attr.name == "style")
        .map(|attr| attr.value.as_str())
}

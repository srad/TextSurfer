use crate::core::dom::{Document, ElementNs, Node, NodeId, attr_value};
use crate::core::style::{LegacyAlign, Rgb};

use super::legacy::{dimension, legacy_color, non_negative_integer};
use super::{HintDeclaration, PresentationalHints};

fn parse_list_style_type(value: &str) -> Option<crate::core::style::ListStyleType> {
    use crate::core::style::ListStyleType;
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(ListStyleType::None),
        "disc" => Some(ListStyleType::Disc),
        "circle" => Some(ListStyleType::Circle),
        "square" => Some(ListStyleType::Square),
        "decimal" => Some(ListStyleType::Decimal),
        "decimal-leading-zero" => Some(ListStyleType::DecimalLeadingZero),
        "lower-alpha" | "lower-latin" => Some(ListStyleType::LowerAlpha),
        "upper-alpha" | "upper-latin" => Some(ListStyleType::UpperAlpha),
        "lower-roman" => Some(ListStyleType::LowerRoman),
        "upper-roman" => Some(ListStyleType::UpperRoman),
        _ => None,
    }
}

pub(in crate::css) fn synthesized_hints(document: &Document, id: NodeId) -> PresentationalHints {
    let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
        return PresentationalHints {
            declarations: Vec::new(),
            legacy_align: None,
        };
    };
    if *ns != ElementNs::Html {
        return PresentationalHints {
            declarations: Vec::new(),
            legacy_align: None,
        };
    }
    let mut hints = Vec::new();
    let mut legacy_align = None;
    if name == "center" {
        push(&mut hints, "text-align", "center");
        legacy_align = Some(LegacyAlign::Center);
    }
    if let Some(align) = attr_value(attrs, "align") {
        map_align(name, align, &mut hints, &mut legacy_align);
    }
    map_float_spacing(name, attrs, &mut hints);
    if name == "br"
        && let Some(value) = attr_value(attrs, "clear")
    {
        match value.trim().to_ascii_lowercase().as_str() {
            "left" => push(&mut hints, "clear", "left"),
            "right" => push(&mut hints, "clear", "right"),
            "all" | "both" => push(&mut hints, "clear", "both"),
            _ => {}
        }
    }
    if let Some(value) = attr_value(attrs, "bgcolor")
        && matches!(
            name.as_str(),
            "body" | "table" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th"
        )
        && let Some(color) = legacy_color(value)
    {
        push_color(&mut hints, "background-color", color);
    }
    if name == "font"
        && let Some(value) = attr_value(attrs, "color")
        && let Some(color) = legacy_color(value)
    {
        push_color(&mut hints, "color", color);
    }
    if let Some(value) = attr_value(attrs, "width") {
        let ignore_zero = matches!(name.as_str(), "table" | "td" | "th");
        if matches!(name.as_str(), "table" | "col" | "td" | "th" | "hr")
            && let Some(value) = dimension(value, ignore_zero)
        {
            push(&mut hints, "width", &value);
        }
    }
    if matches!(
        name.as_str(),
        "thead" | "tbody" | "tfoot" | "tr" | "td" | "th"
    ) && let Some(value) = attr_value(attrs, "valign")
        && matches_ci(value, &["top", "middle", "bottom", "baseline"])
    {
        push(&mut hints, "vertical-align", &value.to_ascii_lowercase());
    }
    if name == "table" {
        map_table(attrs, &mut hints);
    }
    if matches!(name.as_str(), "td" | "th")
        && let Some(table) = corresponding_table(document, id)
        && let Some(Node::Element {
            attrs: table_attrs, ..
        }) = document.node(table)
    {
        if let Some(value) = attr_value(table_attrs, "cellpadding").and_then(non_negative_integer) {
            push(&mut hints, "padding", &format!("{value}px"));
        }
        map_cell_table_borders(table_attrs, &mut hints);
    }
    map_rule_tracks(document, id, name, &mut hints);
    if matches!(name.as_str(), "ol" | "ul" | "menu" | "li")
        && let Some(value) = attr_value(attrs, "type")
        && let Some(list_type) = parse_html_list_type(value)
    {
        push(&mut hints, "list-style-type", list_type);
    }
    PresentationalHints {
        declarations: hints,
        legacy_align,
    }
}

fn map_align(
    name: &str,
    value: &str,
    hints: &mut Vec<HintDeclaration>,
    legacy_align: &mut Option<LegacyAlign>,
) {
    let value = value.trim().to_ascii_lowercase();
    match name {
        "div" => match value.as_str() {
            "middle" => {
                push(hints, "text-align", "center");
                *legacy_align = Some(LegacyAlign::Center);
            }
            "left" | "right" | "center" | "justify" => {
                push(hints, "text-align", &value);
                *legacy_align = Some(match value.as_str() {
                    "right" => LegacyAlign::Right,
                    "center" => LegacyAlign::Center,
                    _ => LegacyAlign::Left,
                });
            }
            _ => {}
        },
        "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            if matches_ci(&value, &["left", "right", "center", "justify"]) {
                push(hints, "text-align", &value);
            }
        }
        "thead" | "tbody" | "tfoot" | "tr" | "td" | "th" => match value.as_str() {
            "middle" | "absmiddle" => push(hints, "text-align", "center"),
            "left" | "right" | "center" | "justify" => push(hints, "text-align", &value),
            _ => {}
        },
        "table" => match value.as_str() {
            "left" | "right" => push(hints, "float", &value),
            "center" => {
                push(hints, "margin-left", "auto");
                push(hints, "margin-right", "auto");
            }
            _ => {}
        },
        "embed" | "iframe" | "img" | "object" if matches!(value.as_str(), "left" | "right") => {
            push(hints, "float", &value);
        }
        "input" if matches!(value.as_str(), "left" | "right") => {
            push(hints, "float", &value);
        }
        "caption" if value == "bottom" => push(hints, "caption-side", "bottom"),
        "hr" => match value.as_str() {
            "left" => {
                push(hints, "margin-left", "0");
                push(hints, "margin-right", "auto");
            }
            "right" => {
                push(hints, "margin-left", "auto");
                push(hints, "margin-right", "0");
            }
            "center" => {
                push(hints, "margin-left", "auto");
                push(hints, "margin-right", "auto");
            }
            _ => {}
        },
        _ => {}
    }
}

fn map_float_spacing(
    name: &str,
    attrs: &[crate::core::dom::Attr],
    hints: &mut Vec<HintDeclaration>,
) {
    if !matches!(name, "embed" | "img" | "object" | "input") {
        return;
    }
    if name == "input"
        && !attr_value(attrs, "type").is_some_and(|value| value.eq_ignore_ascii_case("image"))
    {
        return;
    }
    if let Some(value) = attr_value(attrs, "hspace").and_then(non_negative_integer) {
        push(hints, "margin-left", &format!("{value}px"));
        push(hints, "margin-right", &format!("{value}px"));
    }
    if let Some(value) = attr_value(attrs, "vspace").and_then(non_negative_integer) {
        push(hints, "margin-top", &format!("{value}px"));
        push(hints, "margin-bottom", &format!("{value}px"));
    }
}

fn map_table(attrs: &[crate::core::dom::Attr], hints: &mut Vec<HintDeclaration>) {
    if let Some(value) = attr_value(attrs, "cellspacing").and_then(non_negative_integer) {
        push(hints, "border-spacing", &format!("{value}px"));
    }
    let rules = attr_value(attrs, "rules").map(str::trim);
    if rules.is_some_and(|value| matches_ci(value, &["none", "groups", "rows", "cols", "all"])) {
        push(hints, "border-style", "hidden");
        push(hints, "border-collapse", "collapse");
        push(hints, "border-color", "black");
    }
    if let Some(raw) = attr_value(attrs, "border") {
        let parsed = non_negative_integer(raw);
        let width = parsed.unwrap_or(1);
        push(hints, "border-width", &format!("{width}px"));
        if parsed != Some(0) {
            push(hints, "border-style", "outset");
        }
    }
    if let Some(frame) = attr_value(attrs, "frame").map(|value| value.trim().to_ascii_lowercase()) {
        let style = match frame.as_str() {
            "void" => Some("hidden"),
            "above" => Some("outset hidden hidden hidden"),
            "below" => Some("hidden hidden outset hidden"),
            "hsides" => Some("outset hidden"),
            "lhs" => Some("hidden hidden hidden outset"),
            "rhs" => Some("hidden outset hidden hidden"),
            "vsides" => Some("hidden outset"),
            "box" | "border" => Some("outset"),
            _ => None,
        };
        if let Some(style) = style {
            push(hints, "border-style", style);
            push(hints, "border-color", "black");
        }
    }
}

fn map_cell_table_borders(attrs: &[crate::core::dom::Attr], hints: &mut Vec<HintDeclaration>) {
    if let Some(raw) = attr_value(attrs, "border")
        && non_negative_integer(raw) != Some(0)
    {
        push(hints, "border-width", "1px");
        push(hints, "border-style", "inset");
    }
    match attr_value(attrs, "rules").map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if matches!(value.as_str(), "none" | "groups" | "rows") => {
            push(hints, "border-width", "1px");
            push(hints, "border-style", "none");
            push(hints, "border-color", "black");
        }
        Some(value) if value == "cols" => {
            push(hints, "border-width", "1px");
            push(hints, "border-style", "none solid");
            push(hints, "border-color", "black");
        }
        Some(value) if value == "all" => {
            push(hints, "border-width", "1px");
            push(hints, "border-style", "solid");
            push(hints, "border-color", "black");
        }
        _ => {}
    }
}

fn map_rule_tracks(document: &Document, id: NodeId, name: &str, hints: &mut Vec<HintDeclaration>) {
    let Some(table) = direct_table(document, id, name) else {
        return;
    };
    let Some(Node::Element { attrs, .. }) = document.node(table) else {
        return;
    };
    match attr_value(attrs, "rules").map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "groups" && name == "colgroup" => {
            push(hints, "border-left-width", "1px");
            push(hints, "border-right-width", "1px");
            push(hints, "border-left-style", "solid");
            push(hints, "border-right-style", "solid");
        }
        Some(value) if value == "groups" && matches!(name, "thead" | "tbody" | "tfoot") => {
            push(hints, "border-top-width", "1px");
            push(hints, "border-bottom-width", "1px");
            push(hints, "border-top-style", "solid");
            push(hints, "border-bottom-style", "solid");
        }
        Some(value) if value == "rows" && name == "tr" => {
            push(hints, "border-top-width", "1px");
            push(hints, "border-bottom-width", "1px");
            push(hints, "border-top-style", "solid");
            push(hints, "border-bottom-style", "solid");
        }
        _ => {}
    }
}

fn corresponding_table(document: &Document, id: NodeId) -> Option<NodeId> {
    let mut current = document.parent(id);
    while let Some(node) = current {
        if element_name(document, node) == Some("table") {
            return Some(node);
        }
        current = document.parent(node);
    }
    None
}

fn direct_table(document: &Document, id: NodeId, name: &str) -> Option<NodeId> {
    let parent = document.parent(id)?;
    if element_name(document, parent) == Some("table") {
        return Some(parent);
    }
    if name == "tr"
        && matches!(
            element_name(document, parent),
            Some("thead" | "tbody" | "tfoot")
        )
    {
        let table = document.parent(parent)?;
        return (element_name(document, table) == Some("table")).then_some(table);
    }
    None
}

fn element_name(document: &Document, id: NodeId) -> Option<&str> {
    match document.node(id) {
        Some(Node::Element { name, ns, .. }) if *ns == ElementNs::Html => Some(name),
        _ => None,
    }
}

fn push(hints: &mut Vec<HintDeclaration>, name: &str, value: &str) {
    hints.push(HintDeclaration {
        name: name.to_string(),
        value: value.to_string(),
    });
}

fn push_color(hints: &mut Vec<HintDeclaration>, name: &str, color: Rgb) {
    push(
        hints,
        name,
        &format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b),
    );
}

fn parse_html_list_type(value: &str) -> Option<&'static str> {
    match value.trim() {
        "1" => Some("decimal"),
        "a" => Some("lower-alpha"),
        "A" => Some("upper-alpha"),
        "i" => Some("lower-roman"),
        "I" => Some("upper-roman"),
        other => match parse_list_style_type(other)? {
            crate::core::style::ListStyleType::None => Some("none"),
            crate::core::style::ListStyleType::Disc => Some("disc"),
            crate::core::style::ListStyleType::Circle => Some("circle"),
            crate::core::style::ListStyleType::Square => Some("square"),
            crate::core::style::ListStyleType::Decimal => Some("decimal"),
            crate::core::style::ListStyleType::DecimalLeadingZero => Some("decimal-leading-zero"),
            crate::core::style::ListStyleType::LowerAlpha => Some("lower-alpha"),
            crate::core::style::ListStyleType::UpperAlpha => Some("upper-alpha"),
            crate::core::style::ListStyleType::LowerRoman => Some("lower-roman"),
            crate::core::style::ListStyleType::UpperRoman => Some("upper-roman"),
        },
    }
}

fn matches_ci(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

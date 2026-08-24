use crate::core::dom::{Document, ElementNs, Node, NodeId, attr_value};
use crate::core::style::Rgb;
use crate::css::Declaration;

use super::legacy::{dimension, legacy_color, non_negative_integer};

pub(in crate::css) fn presentational_hints(document: &Document, id: NodeId) -> Vec<Declaration> {
    let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
        return Vec::new();
    };
    if *ns != ElementNs::Html {
        return Vec::new();
    }
    let mut hints = Vec::new();
    if name == "center" {
        push(&mut hints, "text-align", "center");
        push(&mut hints, "-textsurfer-legacy-align", "center");
    }
    if let Some(align) = attr_value(attrs, "align") {
        map_align(name, align, &mut hints);
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
    hints
}

fn map_align(name: &str, value: &str, hints: &mut Vec<Declaration>) {
    let value = value.trim().to_ascii_lowercase();
    match name {
        "div" => match value.as_str() {
            "middle" => {
                push(hints, "text-align", "center");
                push(hints, "-textsurfer-legacy-align", "center");
            }
            "left" | "right" | "center" | "justify" => {
                push(hints, "text-align", &value);
                let legacy = if value == "justify" {
                    "left"
                } else {
                    value.as_str()
                };
                push(hints, "-textsurfer-legacy-align", legacy);
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
        "table" if value == "center" => {
            push(hints, "margin-left", "auto");
            push(hints, "margin-right", "auto");
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

fn map_table(attrs: &[crate::core::dom::Attr], hints: &mut Vec<Declaration>) {
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

fn map_cell_table_borders(attrs: &[crate::core::dom::Attr], hints: &mut Vec<Declaration>) {
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

fn map_rule_tracks(document: &Document, id: NodeId, name: &str, hints: &mut Vec<Declaration>) {
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

fn push(hints: &mut Vec<Declaration>, name: &str, value: &str) {
    hints.push(Declaration {
        name: name.to_string(),
        value: value.to_string(),
        important: false,
    });
}

fn push_color(hints: &mut Vec<Declaration>, name: &str, color: Rgb) {
    push(
        hints,
        name,
        &format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b),
    );
}

fn matches_ci(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

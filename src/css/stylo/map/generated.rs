use servo_arc::Arc as ServoArc;
use style::counter_style::CounterStyle;
use style::properties::ComputedValues;
use style::selector_parser::PseudoElement as StyloPseudo;
use style::values::computed::{Content, ContentItem};
use style_traits::ToCss;
use unicode_width::UnicodeWidthStr;

use crate::core::dom::{Document, Node, NodeId, attr_value};
use crate::core::style::{
    Clear, ComputedStyle, LegacyAlign, ListStylePosition, ListStyleType, Marker, PseudoBox,
    PseudoElement, StyleTree,
};
use crate::css::cascade::counters::{
    CounterOps, CounterScopes, LIST_ITEM_COUNTER, merge_counter_ops, ua_counter_ops,
};

use super::super::dom::StyleDom;
use super::super::engine::StyloEngine;
use super::{ElementPolicy, Mapper, text};

struct PendingMarker {
    node: NodeId,
    parent: Option<NodeId>,
    text: String,
    position: ListStylePosition,
    style: ComputedStyle,
}

enum GeneratedContent {
    Normal,
    None,
    Text(String),
}

pub(super) fn populate(
    dom: &StyleDom<'_>,
    engine: &StyloEngine,
    document: &Document,
    mapper: &mut Mapper,
    tree: &mut StyleTree,
) {
    let mut counters = CounterScopes::default();
    let mut markers = Vec::new();
    let mut hidden_depth = None;
    let mut quote_depth = 0usize;
    for (id, depth) in elements(document) {
        counters.enter(depth);
        if hidden_depth.is_some_and(|hidden| depth <= hidden) {
            hidden_depth = None;
        }
        let Some(element) = dom.element(id) else {
            continue;
        };
        let Some(primary) = element.primary_style() else {
            continue;
        };
        let style = tree.get(id);
        if style.display.is_none() && hidden_depth.is_none() {
            hidden_depth = Some(depth);
        }
        if hidden_depth.is_some() {
            continue;
        }
        counters.run(depth, &counter_ops(document, id, style, &primary));
        for (stylo, which) in [
            (StyloPseudo::Before, PseudoElement::Before),
            (StyloPseudo::After, PseudoElement::After),
        ] {
            let Some(values) = element.pseudo_style(stylo) else {
                continue;
            };
            if let Some(box_) = pseudo_box(
                document,
                id,
                &values,
                mapper,
                style,
                PseudoContent {
                    counters: &counters,
                    fallback: None,
                    quote_depth: &mut quote_depth,
                },
            ) {
                tree.insert_pseudo(id, which, box_);
            }
        }
        if style.display.is_list_item() {
            let fallback = default_marker(style.list_style_type, &counters);
            let values = engine.marker_style(element);
            let marker = values
                .as_ref()
                .and_then(|values| {
                    pseudo_box(
                        document,
                        id,
                        values,
                        mapper,
                        style,
                        PseudoContent {
                            counters: &counters,
                            fallback: fallback.clone(),
                            quote_depth: &mut quote_depth,
                        },
                    )
                })
                .or_else(|| fallback.map(|text| PseudoBox { text, style }));
            if let Some(marker) = marker {
                markers.push(PendingMarker {
                    node: id,
                    parent: document.parent(id),
                    text: marker.text,
                    position: style.list_style_position,
                    style: marker.style,
                });
            }
        }
    }
    let mut fields = std::collections::HashMap::new();
    for marker in &markers {
        if marker.position == ListStylePosition::Outside {
            let width = UnicodeWidthStr::width(marker.text.as_str());
            let field = fields.entry(marker.parent).or_insert(0usize);
            *field = (*field).max(width);
        }
    }
    for marker in markers {
        tree.insert_marker(
            marker.node,
            Marker {
                reserve: if marker.position == ListStylePosition::Outside {
                    fields.get(&marker.parent).copied().unwrap_or_default()
                } else {
                    0
                },
                text: marker.text,
                position: marker.position,
                style: marker.style,
            },
        );
    }
}

pub(super) fn requires_full_mapping(
    dom: &StyleDom<'_>,
    previous: &StyleTree,
    touched: &[NodeId],
) -> bool {
    touched.iter().any(|id| {
        previous.marker(*id).is_some()
            || previous.pseudo(*id, PseudoElement::Before).is_some()
            || previous.pseudo(*id, PseudoElement::After).is_some()
            || dom.element(*id).is_some_and(|element| {
                element
                    .primary_style()
                    .is_some_and(|values| super::display::display(&values).is_list_item())
                    || element.pseudo_style(StyloPseudo::Before).is_some()
                    || element.pseudo_style(StyloPseudo::After).is_some()
            })
    })
}

struct PseudoContent<'a> {
    counters: &'a CounterScopes,
    fallback: Option<String>,
    quote_depth: &'a mut usize,
}

fn pseudo_box(
    document: &Document,
    id: NodeId,
    values: &ServoArc<ComputedValues>,
    mapper: &mut Mapper,
    origin: ComputedStyle,
    content_input: PseudoContent<'_>,
) -> Option<PseudoBox> {
    let content = content_text(
        &values.clone_content(),
        document,
        id,
        content_input.counters,
        content_input.quote_depth,
    );
    let text = match content {
        GeneratedContent::Normal => content_input.fallback?,
        GeneratedContent::None => return None,
        GeneratedContent::Text(text) => text,
    };
    let mut decorations = text::decorations(values);
    if origin.underline {
        decorations |= style::values::computed::text::TextDecorationLine::UNDERLINE;
    }
    if origin.strike {
        decorations |= style::values::computed::text::TextDecorationLine::LINE_THROUGH;
    }
    let style = mapper.style(
        values,
        ElementPolicy {
            decorations,
            control: None,
            legacy_align: LegacyAlign::None,
        },
    );
    if text.is_empty()
        && !style.float.is_floating()
        && style.clear == Clear::None
        && style.display.is_inline_flow()
    {
        return None;
    }
    Some(PseudoBox { text, style })
}

fn content_text(
    content: &Content,
    document: &Document,
    id: NodeId,
    counters: &CounterScopes,
    quote_depth: &mut usize,
) -> GeneratedContent {
    match content {
        Content::None => GeneratedContent::None,
        Content::Normal => GeneratedContent::Normal,
        Content::Items(items) => {
            let mut output = String::new();
            for item in &items.items[..items.alt_start] {
                match item {
                    ContentItem::String(value) => output.push_str(value),
                    ContentItem::Counter(name, style) => output
                        .push_str(&counter_style(style).render(counters.value(name.0.as_ref()))),
                    ContentItem::Counters(name, separator, style) => {
                        let rendered = counters
                            .values(name.0.as_ref())
                            .into_iter()
                            .map(|value| counter_style(style).render(value))
                            .collect::<Vec<_>>();
                        output.push_str(&rendered.join(separator));
                    }
                    ContentItem::OpenQuote => {
                        output.push(if *quote_depth == 0 { '“' } else { '‘' });
                        *quote_depth = quote_depth.saturating_add(1);
                    }
                    ContentItem::CloseQuote => {
                        *quote_depth = quote_depth.saturating_sub(1);
                        output.push(if *quote_depth == 0 { '”' } else { '’' });
                    }
                    ContentItem::NoOpenQuote => *quote_depth = quote_depth.saturating_add(1),
                    ContentItem::NoCloseQuote => *quote_depth = quote_depth.saturating_sub(1),
                    ContentItem::Attr(attr) => {
                        if let Some(Node::Element { attrs, .. }) = document.node(id) {
                            output.push_str(
                                attr_value(attrs, attr.attribute.as_ref())
                                    .unwrap_or(attr.fallback.as_ref()),
                            );
                        }
                    }
                    ContentItem::Image(_) => {}
                }
            }
            GeneratedContent::Text(output)
        }
    }
}

fn counter_ops(
    document: &Document,
    id: NodeId,
    style: ComputedStyle,
    values: &ComputedValues,
) -> CounterOps {
    let implicit = ua_counter_ops(document, id, style);
    let reset = values
        .clone_counter_reset()
        .iter()
        .map(|pair| (pair.name.0.as_ref().to_string(), i64::from(pair.value)))
        .collect::<Vec<_>>();
    let increment = values
        .clone_counter_increment()
        .iter()
        .map(|pair| (pair.name.0.as_ref().to_string(), i64::from(pair.value)))
        .collect::<Vec<_>>();
    CounterOps {
        reset: merge_counter_ops(Some(&reset), implicit.reset),
        increment: merge_counter_ops(Some(&increment), implicit.increment),
        set: implicit.set,
    }
}

fn default_marker(style: ListStyleType, counters: &CounterScopes) -> Option<String> {
    if style == ListStyleType::None {
        return None;
    }
    let rendered = style.render(counters.value(LIST_ITEM_COUNTER));
    Some(if style.is_numeric() {
        format!("{rendered}. ")
    } else {
        format!("{rendered} ")
    })
}

fn counter_style(style: &CounterStyle) -> ListStyleType {
    text::list_style_type_name(&style.to_css_string()).unwrap_or(ListStyleType::Decimal)
}

fn elements(document: &Document) -> Vec<(NodeId, usize)> {
    let mut output = Vec::new();
    let mut stack = document
        .roots()
        .iter()
        .rev()
        .map(|id| (*id, 0usize))
        .collect::<Vec<_>>();
    while let Some((id, depth)) = stack.pop() {
        if matches!(document.node(id), Some(Node::Element { .. })) {
            output.push((id, depth));
        }
        stack.extend(
            document
                .children(id)
                .into_iter()
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }
    output
}

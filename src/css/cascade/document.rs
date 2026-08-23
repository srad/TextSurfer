use std::collections::HashMap;

use unicode_width::UnicodeWidthStr;

use crate::core::dom::{AttrNs, Document, ElementNs, Node, NodeId};
use crate::core::style::{
    ComputedStyle, CssWidth, Display, ListStylePosition, Marker, PseudoBox, PseudoElement,
    StyleTree,
};
use crate::css::StyleSheet;
use crate::css::parser::{StyleRule, parse_declarations};
use crate::css::selectors::{BucketKey, MatchTarget, bucket_keys, matching_specificity};
use crate::css::ua::{inline_style, ua_style};

use super::content::{ContentSpec, default_marker_text, parse_content, resolve_content};
use super::counters::{AuthoredCounterOps, CounterScopes};
use super::declaration::apply_declaration;
use super::media::{MediaContext, active_style_rules};

pub(super) fn cascade_document(
    sheets: &[StyleSheet],
    document: &Document,
    media: MediaContext,
    bucketed: bool,
) -> StyleTree {
    let mut tree = StyleTree::default();
    let rules = active_style_rules(sheets, media);
    let index = bucketed.then(|| RuleIndex::new(&rules, document));
    let mut counters = CounterScopes::default();
    let mut markers: Vec<PendingMarker> = Vec::new();
    let mut hidden_depth: Option<usize> = None;
    for (id, depth) in elements_in_document_order(document) {
        counters.enter(depth);
        if hidden_depth.is_some_and(|hidden| depth <= hidden) {
            hidden_depth = None;
        }
        let parent_style = document.parent(id).map(|parent| tree.get(parent));
        let mut style = ua_style(document, id, parent_style, media.palette);
        let mut declarations = Vec::new();
        let mut order = 0usize;
        let candidates: Vec<usize> = index.as_ref().map_or_else(
            || (0..rules.len()).collect(),
            |index| index.candidates(document, id),
        );
        for rule_index in candidates.iter().copied() {
            let rule = rules[rule_index];
            if let Some(specificity) = matching_specificity(
                &rule.selectors,
                document,
                id,
                media.state,
                MatchTarget::Element,
            ) {
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
        if let Some(inline) = inline_style(document, id) {
            for declaration in parse_declarations(inline) {
                declarations.push((declaration.important, u32::MAX, order, declaration));
                order += 1;
            }
        }
        declarations
            .sort_by_key(|(important, specificity, order, _)| (*important, *specificity, *order));
        let mut authored_counters = AuthoredCounterOps::default();
        for (_, _, _, declaration) in declarations {
            apply_declaration(&mut style, parent_style, &declaration);
            authored_counters.apply(&declaration);
        }
        if matches!(
            style.display,
            Display::None
                | Display::Inline
                | Display::TableHeaderGroup
                | Display::TableRowGroup
                | Display::TableFooterGroup
                | Display::TableRow
        ) {
            style.width = CssWidth::Auto;
        }
        tree.insert(id, style);
        if style.display == Display::None && hidden_depth.is_none() {
            hidden_depth = Some(depth);
        }
        if hidden_depth.is_some() {
            continue;
        }
        counters.run(depth, &authored_counters.resolve(document, id, style));
        for which in [PseudoElement::Before, PseudoElement::After] {
            if let Some(pseudo) = cascade_pseudo(
                &rules,
                &candidates,
                document,
                id,
                media,
                which,
                style,
                &counters,
                None,
            ) {
                tree.insert_pseudo(id, which, pseudo);
            }
        }
        if style.display == Display::ListItem {
            let fallback = default_marker_text(style.list_style_type, &counters);
            if let Some(marker) = cascade_pseudo(
                &rules,
                &candidates,
                document,
                id,
                media,
                PseudoElement::Marker,
                style,
                &counters,
                fallback,
            ) {
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
    let mut fields: HashMap<Option<NodeId>, usize> = HashMap::new();
    for marker in &markers {
        if marker.position == ListStylePosition::Outside {
            let width = UnicodeWidthStr::width(marker.text.as_str());
            let field = fields.entry(marker.parent).or_default();
            *field = (*field).max(width);
        }
    }
    for marker in markers {
        let reserve = if marker.position == ListStylePosition::Outside {
            fields.get(&marker.parent).copied().unwrap_or_default()
        } else {
            0
        };
        tree.insert_marker(
            marker.node,
            Marker {
                text: marker.text,
                reserve,
                position: marker.position,
                style: marker.style,
            },
        );
    }
    tree
}

struct PendingMarker {
    node: NodeId,
    parent: Option<NodeId>,
    text: String,
    position: ListStylePosition,
    style: ComputedStyle,
}

/// Cascades one pseudo-element of `id`. `fallback` is the UA-supplied content used when no author
/// rule declares `content` — markers have one, `::before`/`::after` do not.
#[allow(clippy::too_many_arguments)]
fn cascade_pseudo(
    rules: &[&StyleRule],
    candidates: &[usize],
    document: &Document,
    id: NodeId,
    media: MediaContext,
    which: PseudoElement,
    origin: ComputedStyle,
    counters: &CounterScopes,
    fallback: Option<String>,
) -> Option<PseudoBox> {
    let mut declarations = Vec::new();
    let mut order = 0usize;
    for rule_index in candidates.iter().copied() {
        let rule = rules[rule_index];
        if let Some(specificity) = matching_specificity(
            &rule.selectors,
            document,
            id,
            media.state,
            MatchTarget::Pseudo(which),
        ) {
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
    if declarations.is_empty() && fallback.is_none() {
        return None;
    }
    declarations
        .sort_by_key(|(important, specificity, order, _)| (*important, *specificity, *order));
    let inherited = ComputedStyle {
        display: Display::Inline,
        white_space: origin.white_space,
        color: origin.color,
        background: origin.background,
        bold: origin.bold,
        underline: origin.underline,
        strike: origin.strike,
        reverse: origin.reverse,
        list_style_type: origin.list_style_type,
        list_style_position: origin.list_style_position,
        border_collapse: origin.border_collapse,
        border_spacing: origin.border_spacing,
        caption_side: origin.caption_side,
        ..Default::default()
    };
    let mut style = inherited;
    let mut content = None;
    for (_, _, _, declaration) in declarations {
        apply_declaration(&mut style, Some(inherited), &declaration);
        if declaration.name == "content"
            && let Some(spec) = parse_content(&declaration.value)
        {
            content = Some(spec);
        }
    }
    style.display = Display::Inline;
    let text = match content {
        Some(ContentSpec::None) => return None,
        Some(ContentSpec::Pieces(pieces)) => resolve_content(&pieces, document, id, counters),
        None => fallback?,
    };
    if text.is_empty() {
        return None;
    }
    Some(PseudoBox { text, style })
}

#[derive(Default)]
struct RuleIndex {
    ids: HashMap<String, Vec<usize>>,
    classes: HashMap<String, Vec<usize>>,
    local_names: HashMap<String, Vec<usize>>,
    universal: Vec<usize>,
    quirks: bool,
}

impl RuleIndex {
    fn new(rules: &[&StyleRule], document: &Document) -> Self {
        let quirks = document.quirks_mode() == crate::core::dom::DomQuirksMode::Quirks;
        let mut index = Self {
            quirks,
            ..Self::default()
        };
        for (rule_index, rule) in rules.iter().enumerate() {
            for key in bucket_keys(&rule.selectors, quirks) {
                let bucket = match key {
                    BucketKey::Id(value) => index.ids.entry(value).or_default(),
                    BucketKey::Class(value) => index.classes.entry(value).or_default(),
                    BucketKey::LocalName(value) => index.local_names.entry(value).or_default(),
                    BucketKey::Universal => &mut index.universal,
                };
                bucket.push(rule_index);
            }
        }
        index
    }

    fn candidates(&self, document: &Document, id: NodeId) -> Vec<usize> {
        let mut candidates = self.universal.clone();
        let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
            return candidates;
        };
        let local_name = if *ns == ElementNs::Html {
            name.to_ascii_lowercase()
        } else {
            name.clone()
        };
        if let Some(rules) = self.local_names.get(&local_name) {
            candidates.extend(rules);
        }
        for attr in attrs {
            if attr.ns != AttrNs::None {
                continue;
            }
            if attr.name == "id" {
                let id = self.normalize(&attr.value);
                if let Some(rules) = self.ids.get(&id) {
                    candidates.extend(rules);
                }
            } else if attr.name == "class" {
                for class in attr.value.split_ascii_whitespace() {
                    let class = self.normalize(class);
                    if let Some(rules) = self.classes.get(&class) {
                        candidates.extend(rules);
                    }
                }
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        candidates
    }

    fn normalize(&self, value: &str) -> String {
        if self.quirks {
            value.to_ascii_lowercase()
        } else {
            value.to_string()
        }
    }
}

pub(super) fn elements_in_document_order(document: &Document) -> Vec<(NodeId, usize)> {
    let mut elements = Vec::new();
    let mut stack: Vec<(NodeId, usize)> = document
        .roots()
        .iter()
        .rev()
        .map(|root| (*root, 0usize))
        .collect();
    while let Some((id, depth)) = stack.pop() {
        if matches!(document.node(id), Some(Node::Element { .. })) {
            elements.push((id, depth));
        }
        let children = document.children(id);
        stack.extend(
            children
                .into_iter()
                .rev()
                .map(|child| (child, depth.saturating_add(1))),
        );
    }
    elements
}

use std::collections::HashMap;
use std::rc::Rc;

use unicode_width::UnicodeWidthStr;

use crate::core::dom::{AttrNs, Document, ElementNs, Node, NodeId};
use crate::core::style::{
    ComputedStyle, CssMaxSize, CssSize, Display, DisplayInside, ListStylePosition, Marker,
    PseudoBox, PseudoElement, StyleStore, StyleTree,
};
use crate::css::StyleSheet;
use crate::css::parser::{StyleRule, parse_declarations};
use crate::css::presentational::presentational_hints;
use crate::css::selectors::{BucketKey, MatchTarget, bucket_keys, matching_specificity};
use crate::css::ua::{UaContext, inline_style, ua_style};
use crate::css::variables::{Environment, contains_var, derive_environment, is_custom_name};

use super::content::{ContentSpec, default_marker_text, parse_content, resolve_content};
use super::counters::{AuthoredCounterOps, CounterScopes, parse_counter_values};
use super::declaration::{apply_declaration, declaration_value_is_valid};
use super::media::{MediaContext, active_style_rules};
use super::typography::{apply_font_size, font_size_value_is_valid};

pub(super) fn cascade_document(
    sheets: &[StyleSheet],
    document: &Document,
    media: MediaContext,
    bucketed: bool,
) -> StyleTree {
    let mut tree = StyleTree::default();
    let mut store = StyleStore::default();
    let rules = active_style_rules(sheets, media);
    let index = bucketed.then(|| RuleIndex::new(&rules, document));
    let mut counters = CounterScopes::default();
    let mut markers: Vec<PendingMarker> = Vec::new();
    let mut hidden_depth: Option<usize> = None;
    let mut root_font_size = media.root_font_size;
    let root_environment = Environment::root();
    let mut environments: HashMap<NodeId, Rc<Environment>> = HashMap::new();
    for (id, depth) in elements_in_document_order(document) {
        counters.enter(depth);
        if hidden_depth.is_some_and(|hidden| depth <= hidden) {
            hidden_depth = None;
        }
        let parent_style = document.parent(id).map(|parent| tree.get(parent));
        let mut style = ua_style(
            document,
            id,
            parent_style,
            UaContext {
                palette: media.palette,
                state: media.state,
            },
        );
        let ua_baseline = style;
        let mut declarations = Vec::new();
        let mut order = 0usize;
        for declaration in presentational_hints(document, id) {
            declarations.push((false, 0, order, declaration));
            order += 1;
        }
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
        let parent_environment = document
            .parent(id)
            .and_then(|parent| environments.get(&parent))
            .cloned()
            .unwrap_or_else(|| root_environment.clone());
        let custom_winners = declarations
            .iter()
            .filter(|(_, _, _, declaration)| is_custom_name(&declaration.name))
            .map(|(_, _, _, declaration)| (declaration.name.clone(), declaration.value.clone()))
            .collect();
        let environment = derive_environment(parent_environment, custom_winners);
        environments.insert(id, environment.clone());
        let declarations = resolve_declarations(
            declarations,
            &environment,
            parent_style,
            ua_baseline,
            media.with_font_sizes(
                parent_style.map_or(root_font_size, |parent| parent.font_size),
                root_font_size,
            ),
        );
        for (_, _, _, declaration) in &declarations {
            apply_font_size(
                &mut style,
                parent_style,
                ua_baseline,
                declaration,
                media.with_font_sizes(
                    parent_style.map_or(root_font_size, |parent| parent.font_size),
                    root_font_size,
                ),
            );
        }
        if parent_style.is_none() {
            root_font_size = style.font_size;
        }
        style.text_presentation = style.font_size.presentation(media.text_rendering);
        let element_media = media.with_font_sizes(style.font_size, root_font_size);
        let mut authored_counters = AuthoredCounterOps::default();
        for (_, _, _, declaration) in declarations {
            apply_declaration(
                &mut style,
                parent_style,
                ua_baseline,
                &declaration,
                element_media,
                &mut store,
            );
            authored_counters.apply(&declaration);
        }
        style.overflow = style.overflow.computed();
        compute_box_values(document, &tree, id, &mut style);
        if style.display.is_inline_flow() && !is_replaced_element(document, id)
            || matches!(
                style.display,
                Display::TABLE_HEADER_GROUP
                    | Display::TABLE_ROW_GROUP
                    | Display::TABLE_FOOTER_GROUP
                    | Display::TABLE_ROW
            )
        {
            style.width = CssSize::Auto;
            style.height = CssSize::Auto;
            style.min_width = CssSize::Auto;
            style.min_height = CssSize::Auto;
            style.max_width = CssMaxSize::None;
            style.max_height = CssMaxSize::None;
        }
        tree.insert(id, style);
        if style.display.is_none() && hidden_depth.is_none() {
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
                element_media,
                which,
                style,
                &counters,
                None,
                pseudo_is_layout_item(document, &tree, id, style),
                environment.clone(),
                &mut store,
            ) {
                tree.insert_pseudo(id, which, pseudo);
            }
        }
        if style.display.is_list_item() {
            let fallback = default_marker_text(style.list_style_type, &counters);
            if let Some(marker) = cascade_pseudo(
                &rules,
                &candidates,
                document,
                id,
                element_media,
                PseudoElement::Marker,
                style,
                &counters,
                fallback,
                false,
                environment.clone(),
                &mut store,
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
    tree.set_store(store);
    tree
}

fn is_replaced_element(document: &Document, id: NodeId) -> bool {
    matches!(
        document.node(id),
        Some(crate::core::dom::Node::Element { name, ns, .. })
            if *ns == crate::core::dom::ElementNs::Html
                && matches!(name.as_str(), "img" | "input" | "button" | "select" | "textarea")
    )
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
    flex_item: bool,
    origin_environment: Rc<Environment>,
    store: &mut StyleStore,
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
        display: Display::INLINE,
        white_space: origin.white_space,
        cursor: origin.cursor,
        visibility: origin.visibility,
        color: origin.color,
        background: (!origin.display.is_contents())
            .then_some(origin.background)
            .flatten(),
        bold: origin.bold,
        underline: origin.underline,
        strike: origin.strike,
        reverse: origin.reverse,
        font_size: origin.font_size,
        text_presentation: origin.text_presentation,
        list_style_type: origin.list_style_type,
        list_style_position: origin.list_style_position,
        border_collapse: origin.border_collapse,
        border_spacing: origin.border_spacing,
        caption_side: origin.caption_side,
        text_align: origin.text_align,
        legacy_align: origin.legacy_align,
        ..Default::default()
    };
    let ua_baseline = inherited;
    let custom_winners = declarations
        .iter()
        .filter(|(_, _, _, declaration)| is_custom_name(&declaration.name))
        .map(|(_, _, _, declaration)| (declaration.name.clone(), declaration.value.clone()))
        .collect();
    let environment = derive_environment(origin_environment, custom_winners);
    let declarations = resolve_declarations(
        declarations,
        &environment,
        Some(inherited),
        ua_baseline,
        media,
    );
    let mut style = inherited;
    let mut content = None;
    for (_, _, _, declaration) in &declarations {
        apply_font_size(&mut style, Some(inherited), ua_baseline, declaration, media);
    }
    style.text_presentation = style.font_size.presentation(media.text_rendering);
    let pseudo_media = media.with_font_sizes(style.font_size, media.root_font_size);
    for (_, _, _, declaration) in declarations {
        apply_declaration(
            &mut style,
            Some(inherited),
            ua_baseline,
            &declaration,
            pseudo_media,
            store,
        );
        if declaration.name == "content"
            && let Some(spec) = parse_content(&declaration.value)
        {
            content = Some(spec);
        }
    }
    style.overflow = style.overflow.computed();
    if style.position.is_absolute() || flex_item {
        style.float = crate::core::style::CssFloat::None;
        style.display = style.display.blockify();
    } else if style.float.is_floating() {
        style.display = style.display.blockify();
    }
    let text = match content {
        Some(ContentSpec::None) => return None,
        Some(ContentSpec::Pieces(pieces)) => resolve_content(&pieces, document, id, counters),
        Some(ContentSpec::Normal) | None => fallback?,
    };
    if text.is_empty()
        && !style.float.is_floating()
        && matches!(style.clear, crate::core::style::Clear::None)
        && style.display.is_inline_flow()
    {
        return None;
    }
    Some(PseudoBox { text, style })
}

fn resolve_declarations(
    declarations: Vec<(bool, u32, usize, crate::css::Declaration)>,
    environment: &Environment,
    parent_style: Option<ComputedStyle>,
    ua_style: ComputedStyle,
    media: MediaContext,
) -> Vec<(bool, u32, usize, crate::css::Declaration)> {
    declarations
        .into_iter()
        .filter_map(|(important, specificity, order, mut declaration)| {
            if is_custom_name(&declaration.name) {
                return None;
            }
            if contains_var(&declaration.value) {
                declaration.value = match environment.substitute(&declaration.value) {
                    Ok(value)
                        if computed_value_is_valid(
                            &declaration.name,
                            &value,
                            parent_style,
                            ua_style,
                            media,
                        ) =>
                    {
                        value
                    }
                    Ok(_) | Err(()) => "unset".to_string(),
                };
            }
            Some((important, specificity, order, declaration))
        })
        .collect()
}

fn computed_value_is_valid(
    property: &str,
    value: &str,
    parent_style: Option<ComputedStyle>,
    ua_style: ComputedStyle,
    media: MediaContext,
) -> bool {
    match property {
        "font-size" => font_size_value_is_valid(value, media),
        "content" => parse_content(value).is_some(),
        "counter-reset" | "counter-set" => parse_counter_values(value, 0).is_some(),
        "counter-increment" => parse_counter_values(value, 1).is_some(),
        _ => declaration_value_is_valid(
            parent_style,
            ua_style,
            &crate::css::Declaration {
                name: property.to_string(),
                value: value.to_string(),
                important: false,
            },
            media,
        ),
    }
}

fn compute_box_values(
    document: &Document,
    tree: &StyleTree,
    id: NodeId,
    style: &mut ComputedStyle,
) {
    style.display = if style.display.is_contents()
        && matches!(
            document.node(id),
            Some(Node::Element {
                ns: ElementNs::Html,
                name,
                ..
            }) if matches!(
                name.as_str(),
                "br"
                    | "wbr"
                    | "meter"
                    | "progress"
                    | "canvas"
                    | "embed"
                    | "object"
                    | "audio"
                    | "iframe"
                    | "img"
                    | "video"
                    | "frame"
                    | "frameset"
                    | "input"
                    | "textarea"
                    | "select"
            )
        ) {
        Display::NONE
    } else {
        style.display
    };
    if style.position.is_absolute() {
        style.float = crate::core::style::CssFloat::None;
        style.display = style.display.blockify();
    } else if document.parent(id).is_none() {
        style.float = crate::core::style::CssFloat::None;
        style.display = if style.display.is_contents() {
            Display::BLOCK
        } else {
            style.display.blockify()
        };
    } else if nearest_box_parent_lays_out_items(document, tree, id) {
        style.float = crate::core::style::CssFloat::None;
        style.display = style.display.blockify();
    } else if style.float.is_floating() {
        style.display = style.display.blockify();
    }
}

/// Flex and grid items are both blockified, so both formatting contexts answer this the same way.
fn pseudo_is_layout_item(
    document: &Document,
    tree: &StyleTree,
    id: NodeId,
    style: ComputedStyle,
) -> bool {
    is_layout_container(style)
        || (style.display.is_contents() && nearest_box_parent_lays_out_items(document, tree, id))
}

fn is_layout_container(style: ComputedStyle) -> bool {
    matches!(
        style.display.inside(),
        Some(DisplayInside::Flex | DisplayInside::Grid)
    )
}

fn nearest_box_parent_lays_out_items(document: &Document, tree: &StyleTree, id: NodeId) -> bool {
    let mut parent = document.parent(id);
    while let Some(node) = parent {
        if !matches!(document.node(node), Some(Node::Element { .. })) {
            parent = document.parent(node);
            continue;
        }
        let style = tree.get(node);
        if style.display.is_contents() {
            parent = document.parent(node);
            continue;
        }
        return is_layout_container(style);
    }
    false
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

use std::collections::HashMap;

use cssparser::color::clamp_unit_f32;
use cssparser::{Parser, ParserInput, Token};
use cssparser_color::{Color as CssColor, hsl_to_rgb, hwb_to_rgb};

use crate::core::dom::Document;
use crate::core::dom::{AttrNs, ElementNs, Node, NodeId};
use crate::core::geom::Size;
use crate::core::style::{
    BorderCollapse, BorderColor, BorderEdges, BorderLineStyle, BorderSide, BorderSpacing,
    BoxSizing, CaptionSide, ComputedStyle, CssPercentage, CssWidth, Display, EdgeSizes,
    ListStylePosition, ListStyleType, Marker, Palette, PseudoBox, PseudoElement, Rgb, StyleTree,
    TableLayoutMode, WhiteSpace,
};
use crate::css::parser::{
    ColorScheme, CssRule, MediaAxis, MediaComparison, MediaFeature, MediaQuery, MediaQueryList,
    ScriptingValue, StyleRule, parse_declarations,
};
use crate::css::selectors::{
    BucketKey, DynamicState, MatchTarget, bucket_keys, matching_specificity,
};
use crate::css::{Declaration, StyleSheet};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MediaTarget {
    Screen,
    Print,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaContext {
    target: MediaTarget,
    pub palette: Palette,
    pub state: DynamicState,
    pub scripting: bool,
    pub color_scheme: ColorScheme,
    pub viewport: Size,
}

impl MediaContext {
    pub const fn screen() -> Self {
        Self {
            target: MediaTarget::Screen,
            palette: Palette::DEFAULT,
            state: DynamicState::INERT,
            scripting: false,
            color_scheme: ColorScheme::Dark,
            viewport: Size { cols: 80, rows: 24 },
        }
    }

    pub const fn print() -> Self {
        Self {
            target: MediaTarget::Print,
            palette: Palette::DEFAULT,
            state: DynamicState::INERT,
            scripting: false,
            color_scheme: ColorScheme::Dark,
            viewport: Size { cols: 80, rows: 24 },
        }
    }

    pub fn with_palette(self, palette: Palette) -> Self {
        Self { palette, ..self }
    }

    pub fn with_state(self, state: DynamicState) -> Self {
        Self { state, ..self }
    }

    pub fn with_scripting(self, scripting: bool) -> Self {
        Self { scripting, ..self }
    }

    pub fn with_color_scheme(self, color_scheme: ColorScheme) -> Self {
        Self {
            color_scheme,
            ..self
        }
    }

    pub fn with_viewport(self, viewport: Size) -> Self {
        Self { viewport, ..self }
    }
}

pub trait Cascade: Send + Sync {
    fn apply(&self, sheets: &[StyleSheet], document: &Document, media: MediaContext) -> StyleTree;
}

#[derive(Default)]
pub struct BasicCascade;

impl Cascade for BasicCascade {
    fn apply(&self, sheets: &[StyleSheet], document: &Document, media: MediaContext) -> StyleTree {
        cascade_document(sheets, document, media, true)
    }
}

fn cascade_document(
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

/// The UA marker carries its own trailing separator, so an author `::marker` that supplies its
/// own spacing is not charged for a second gutter.
fn default_marker_text(list_style_type: ListStyleType, counters: &CounterScopes) -> Option<String> {
    if list_style_type == ListStyleType::None {
        return None;
    }
    let rendered = list_style_type.render(counters.value(LIST_ITEM_COUNTER));
    Some(if list_style_type.is_numeric() {
        format!("{rendered}. ")
    } else {
        format!("{rendered} ")
    })
}

#[cfg(test)]
fn apply_naive(sheets: &[StyleSheet], document: &Document, media: MediaContext) -> StyleTree {
    cascade_document(sheets, document, media, false)
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

fn active_style_rules(sheets: &[StyleSheet], media: MediaContext) -> Vec<&StyleRule> {
    let mut stack = Vec::new();
    for sheet in sheets.iter().rev() {
        stack.extend(sheet.rules.iter().rev());
    }
    let mut active = Vec::new();
    while let Some(rule) = stack.pop() {
        match rule {
            CssRule::Style(rule) => active.push(rule),
            CssRule::Media(rule) if media_query_list_matches(&rule.queries, media) => {
                stack.extend(rule.rules.iter().rev());
            }
            CssRule::Media(_) => {}
            CssRule::Import(_) => {}
        }
    }
    active
}

pub fn media_query_list_matches(queries: &MediaQueryList, media: MediaContext) -> bool {
    match queries {
        MediaQueryList::Always => true,
        MediaQueryList::Any(queries) => queries.iter().any(|query| match query {
            MediaQuery::Type {
                negated,
                media_type,
            } => {
                let matched = match media_type.as_str() {
                    "all" => true,
                    "screen" => media.target == MediaTarget::Screen,
                    "print" => media.target == MediaTarget::Print,
                    _ => false,
                };
                matched != *negated
            }
            MediaQuery::Condition {
                negated,
                media_type,
                features,
            } => {
                let type_matches = media_type
                    .as_deref()
                    .is_none_or(|media_type| media_type_matches(media_type, media));
                let matched = type_matches
                    && features
                        .iter()
                        .all(|feature| media_feature_matches(*feature, media));
                matched != *negated
            }
            MediaQuery::Never => false,
        }),
    }
}

fn media_type_matches(media_type: &str, media: MediaContext) -> bool {
    match media_type {
        "all" => true,
        "screen" => media.target == MediaTarget::Screen,
        "print" => media.target == MediaTarget::Print,
        _ => false,
    }
}

fn media_feature_matches(feature: MediaFeature, media: MediaContext) -> bool {
    match feature {
        MediaFeature::Scripting(None) => media.scripting,
        MediaFeature::Scripting(Some(ScriptingValue::None)) => !media.scripting,
        MediaFeature::Scripting(Some(ScriptingValue::InitialOnly)) => false,
        MediaFeature::Scripting(Some(ScriptingValue::Enabled)) => media.scripting,
        MediaFeature::PrefersColorScheme(None) => true,
        MediaFeature::PrefersColorScheme(Some(scheme)) => media.color_scheme == scheme,
        MediaFeature::Dimension {
            axis,
            comparison,
            value,
        } => {
            let actual = match axis {
                MediaAxis::Width => media.viewport.cols,
                MediaAxis::Height => media.viewport.rows,
            };
            match value {
                None => actual != 0,
                Some(expected) => match comparison {
                    MediaComparison::Equal => actual == expected,
                    MediaComparison::Minimum => actual >= expected,
                    MediaComparison::Maximum => actual <= expected,
                },
            }
        }
    }
}

fn elements_in_document_order(document: &Document) -> Vec<(NodeId, usize)> {
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

pub const LIST_ITEM_COUNTER: &str = "list-item";

/// One counter instance. A counter created at `depth` stays in scope for that element, its
/// descendants and its following siblings, which is exactly "entries deeper than the element
/// being visited are out of scope".
struct CounterEntry {
    depth: usize,
    name: String,
    value: i64,
}

#[derive(Default)]
struct CounterScopes {
    entries: Vec<CounterEntry>,
}

impl CounterScopes {
    fn enter(&mut self, depth: usize) {
        self.entries.retain(|entry| entry.depth <= depth);
    }

    fn run(&mut self, depth: usize, ops: &CounterOps) {
        for (name, value) in &ops.reset {
            self.reset(depth, name, *value);
        }
        for (name, value) in &ops.increment {
            self.adjust(name, *value);
        }
        for (name, value) in &ops.set {
            self.assign(name, *value);
        }
    }

    fn reset(&mut self, depth: usize, name: &str, value: i64) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.depth == depth && entry.name == name)
        {
            entry.value = value;
            return;
        }
        self.entries.push(CounterEntry {
            depth,
            name: name.to_string(),
            value,
        });
    }

    fn adjust(&mut self, name: &str, by: i64) {
        match self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.name == name)
        {
            Some(entry) => entry.value = entry.value.saturating_add(by),
            None => self.entries.push(CounterEntry {
                depth: 0,
                name: name.to_string(),
                value: by,
            }),
        }
    }

    fn assign(&mut self, name: &str, value: i64) {
        match self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.name == name)
        {
            Some(entry) => entry.value = value,
            None => self.entries.push(CounterEntry {
                depth: 0,
                name: name.to_string(),
                value,
            }),
        }
    }

    fn value(&self, name: &str) -> i64 {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.name == name)
            .map_or(0, |entry| entry.value)
    }

    fn values(&self, name: &str) -> Vec<i64> {
        self.entries
            .iter()
            .filter(|entry| entry.name == name)
            .map(|entry| entry.value)
            .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CounterOps {
    reset: Vec<(String, i64)>,
    increment: Vec<(String, i64)>,
    set: Vec<(String, i64)>,
}

/// The counter properties an author declared on one element. `None` means "not declared", so the
/// UA-derived list for that property survives; a declared list replaces it outright.
#[derive(Default)]
struct AuthoredCounterOps {
    reset: Option<Vec<(String, i64)>>,
    increment: Option<Vec<(String, i64)>>,
    set: Option<Vec<(String, i64)>>,
}

impl AuthoredCounterOps {
    fn apply(&mut self, declaration: &Declaration) {
        let slot = match declaration.name.as_str() {
            "counter-reset" => &mut self.reset,
            "counter-increment" => &mut self.increment,
            "counter-set" => &mut self.set,
            _ => return,
        };
        let default = if declaration.name == "counter-increment" {
            1
        } else {
            0
        };
        if let Some(values) = parse_counter_values(&declaration.value, default) {
            *slot = Some(values);
        }
    }

    fn resolve(&self, document: &Document, id: NodeId, style: ComputedStyle) -> CounterOps {
        let ua = ua_counter_ops(document, id, style);
        CounterOps {
            reset: merge_counter_ops(self.reset.as_deref(), ua.reset),
            increment: merge_counter_ops(self.increment.as_deref(), ua.increment),
            set: merge_counter_ops(self.set.as_deref(), ua.set),
        }
    }
}

/// The `list-item` operations implied by `display: list-item` and by `ol`/`ul` are *implicit*: an
/// author who increments some counter of their own does not thereby stop a list from numbering.
/// Only naming the same counter overrides the implicit operation.
fn merge_counter_ops(
    authored: Option<&[(String, i64)]>,
    implicit: Vec<(String, i64)>,
) -> Vec<(String, i64)> {
    let Some(authored) = authored else {
        return implicit;
    };
    let mut merged: Vec<(String, i64)> = implicit
        .into_iter()
        .filter(|(name, _)| !authored.iter().any(|(other, _)| other == name))
        .collect();
    merged.extend(authored.iter().cloned());
    merged
}

/// `ol`/`ul` open a `list-item` scope, list items step it, and the HTML ordinal attributes
/// (`start`, `reversed`, `value`) are expressed as counter operations at UA origin.
fn ua_counter_ops(document: &Document, id: NodeId, style: ComputedStyle) -> CounterOps {
    let mut ops = CounterOps::default();
    let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
        return ops;
    };
    if *ns == ElementNs::Html && matches!(name.as_str(), "ol" | "ul" | "menu") {
        let reversed = name == "ol" && has_attr(attrs, "reversed");
        let start = attr_number(attrs, "start");
        let first = if reversed {
            start.unwrap_or_else(|| list_item_count(document, id))
        } else {
            start.unwrap_or(1)
        };
        let step: i64 = if reversed { -1 } else { 1 };
        ops.reset
            .push((LIST_ITEM_COUNTER.to_string(), first.saturating_sub(step)));
    }
    if style.display == Display::ListItem {
        let reversed = document
            .parent(id)
            .and_then(|parent| document.node(parent))
            .is_some_and(|node| match node {
                Node::Element { name, ns, attrs } => {
                    *ns == ElementNs::Html && name == "ol" && has_attr(attrs, "reversed")
                }
                _ => false,
            });
        ops.increment
            .push((LIST_ITEM_COUNTER.to_string(), if reversed { -1 } else { 1 }));
        if *ns == ElementNs::Html
            && name == "li"
            && let Some(value) = attr_number(attrs, "value")
        {
            ops.set.push((LIST_ITEM_COUNTER.to_string(), value));
        }
    }
    ops
}

fn list_item_count(document: &Document, list: NodeId) -> i64 {
    document
        .children(list)
        .into_iter()
        .filter(|child| match document.node(*child) {
            Some(Node::Element { name, ns, .. }) => *ns == ElementNs::Html && name == "li",
            _ => false,
        })
        .count() as i64
}

fn has_attr(attrs: &[crate::core::dom::Attr], name: &str) -> bool {
    attrs
        .iter()
        .any(|attr| attr.ns == AttrNs::None && attr.name.eq_ignore_ascii_case(name))
}

fn attr_value<'a>(attrs: &'a [crate::core::dom::Attr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|attr| attr.ns == AttrNs::None && attr.name.eq_ignore_ascii_case(name))
        .map(|attr| attr.value.as_str())
}

fn attr_number(attrs: &[crate::core::dom::Attr], name: &str) -> Option<i64> {
    attr_value(attrs, name)?.trim().parse::<i64>().ok()
}

fn parse_counter_values(source: &str, default: i64) -> Option<Vec<(String, i64)>> {
    if parse_ident(source).as_deref() == Some("none") {
        return Some(Vec::new());
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values: Vec<(String, i64)> = Vec::new();
    while !parser.is_exhausted() {
        let name = parser.expect_ident_cloned().ok()?.to_string();
        if is_css_wide_keyword(&name) {
            return None;
        }
        let value = parser
            .try_parse(|input| input.expect_integer())
            .map_or(default, i64::from);
        values.push((name, value));
    }
    (!values.is_empty()).then_some(values)
}

fn is_css_wide_keyword(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "initial" | "inherit" | "unset" | "revert" | "none"
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ContentSpec {
    None,
    Pieces(Vec<ContentPiece>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ContentPiece {
    Text(String),
    Counter {
        name: String,
        style: ListStyleType,
    },
    Counters {
        name: String,
        separator: String,
        style: ListStyleType,
    },
    Attr(String),
}

/// Parses a `content` value. Anything the terminal cannot render (`url()`, quotes, images) makes
/// the whole declaration invalid, which is what the spec asks for and keeps half-rendered
/// generated content off the screen.
fn parse_content(source: &str) -> Option<ContentSpec> {
    if let Some(keyword) = parse_ident(source) {
        return match keyword.as_str() {
            "none" | "normal" => Some(ContentSpec::None),
            _ => None,
        };
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut pieces = Vec::new();
    while !parser.is_exhausted() {
        let token = parser.next().ok()?.clone();
        match token {
            Token::QuotedString(value) => pieces.push(ContentPiece::Text(value.to_string())),
            Token::Function(name) => {
                let name = name.to_ascii_lowercase();
                let piece = parser
                    .parse_nested_block(|input| parse_content_function(&name, input))
                    .ok()?;
                pieces.push(piece);
            }
            _ => return None,
        }
    }
    (!pieces.is_empty()).then_some(ContentSpec::Pieces(pieces))
}

fn parse_content_function<'i>(
    name: &str,
    input: &mut Parser<'i, '_>,
) -> Result<ContentPiece, cssparser::ParseError<'i, ()>> {
    let invalid = |input: &Parser<'i, '_>| input.new_custom_error(());
    match name {
        "attr" => {
            let attribute = input.expect_ident_cloned()?.to_string();
            input.expect_exhausted()?;
            Ok(ContentPiece::Attr(attribute))
        }
        "counter" => {
            let counter = input.expect_ident_cloned()?.to_string();
            let style = parse_counter_style_argument(input)?;
            input.expect_exhausted()?;
            Ok(ContentPiece::Counter {
                name: counter,
                style,
            })
        }
        "counters" => {
            let counter = input.expect_ident_cloned()?.to_string();
            input.expect_comma()?;
            let separator = input.expect_string_cloned()?.to_string();
            let style = parse_counter_style_argument(input)?;
            input.expect_exhausted()?;
            Ok(ContentPiece::Counters {
                name: counter,
                separator,
                style,
            })
        }
        _ => Err(invalid(input)),
    }
}

fn parse_counter_style_argument<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<ListStyleType, cssparser::ParseError<'i, ()>> {
    if input.try_parse(|input| input.expect_comma()).is_err() {
        return Ok(ListStyleType::Decimal);
    }
    let style = input.expect_ident_cloned()?;
    Ok(parse_list_style_type(&style).unwrap_or(ListStyleType::Decimal))
}

fn resolve_content(
    pieces: &[ContentPiece],
    document: &Document,
    id: NodeId,
    counters: &CounterScopes,
) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            ContentPiece::Text(text) => out.push_str(text),
            ContentPiece::Counter { name, style } => {
                out.push_str(&style.render(counters.value(name)));
            }
            ContentPiece::Counters {
                name,
                separator,
                style,
            } => {
                let rendered: Vec<_> = counters
                    .values(name)
                    .into_iter()
                    .map(|value| style.render(value))
                    .collect();
                out.push_str(&rendered.join(separator));
            }
            ContentPiece::Attr(name) => {
                if let Some(Node::Element { attrs, .. }) = document.node(id)
                    && let Some(value) = attr_value(attrs, name)
                {
                    out.push_str(value);
                }
            }
        }
    }
    out
}

fn parse_list_style_position(value: &str) -> Option<ListStylePosition> {
    match value.to_ascii_lowercase().as_str() {
        "outside" => Some(ListStylePosition::Outside),
        "inside" => Some(ListStylePosition::Inside),
        _ => None,
    }
}

/// `list-style` sets type, position and image; `none` may stand for either type or image, and an
/// image we cannot render leaves the type alone.
fn apply_list_style_shorthand(style: &mut ComputedStyle, source: &str) {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut list_type = None;
    let mut position = None;
    let mut saw_none = false;
    while !parser.is_exhausted() {
        let Ok(token) = parser.next().cloned() else {
            return;
        };
        match token {
            Token::Ident(value) => {
                if value.eq_ignore_ascii_case("none") {
                    saw_none = true;
                } else if let Some(value) = parse_list_style_position(&value) {
                    position = Some(value);
                } else if let Some(value) = parse_list_style_type(&value) {
                    list_type = Some(value);
                } else {
                    return;
                }
            }
            Token::UnquotedUrl(_) => {}
            Token::Function(name) if name.eq_ignore_ascii_case("url") => {
                if parser.parse_nested_block(consume_block).is_err() {
                    return;
                }
            }
            _ => return,
        }
    }
    if let Some(value) = list_type {
        style.list_style_type = value;
    } else if saw_none {
        style.list_style_type = ListStyleType::None;
    }
    if let Some(value) = position {
        style.list_style_position = value;
    }
}

fn consume_block<'i>(input: &mut Parser<'i, '_>) -> Result<(), cssparser::ParseError<'i, ()>> {
    while input.next().is_ok() {}
    Ok(())
}

fn parse_list_style_type(value: &str) -> Option<ListStyleType> {
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

fn ua_style(
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
            ..Default::default()
        };
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
        Display::Block
    } else if name == "li" {
        Display::ListItem
    } else {
        match name.as_str() {
            "table" => Display::Table,
            "thead" => Display::TableHeaderGroup,
            "tbody" => Display::TableRowGroup,
            "tfoot" => Display::TableFooterGroup,
            "tr" => Display::TableRow,
            "td" | "th" => Display::TableCell,
            "col" => Display::TableColumn,
            "colgroup" => Display::TableColumnGroup,
            "caption" => Display::TableCaption,
            _ => Display::Inline,
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
    if name == "a"
        && attrs
            .iter()
            .any(|attr| attr.ns == AttrNs::None && attr.name == "href")
    {
        style.color = Some(palette.link);
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
        style.margin.bottom = 1;
    }
    if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
        style.margin.top = 1;
        style.margin.bottom = 1;
        style.bold = true;
    }
    if name == "blockquote" {
        style.margin.left = 2;
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

fn inline_style(document: &Document, id: NodeId) -> Option<&str> {
    let Some(Node::Element { attrs, .. }) = document.node(id) else {
        return None;
    };
    attrs
        .iter()
        .find(|attr| attr.ns == AttrNs::None && attr.name == "style")
        .map(|attr| attr.value.as_str())
}

fn apply_declaration(
    style: &mut ComputedStyle,
    parent_style: Option<ComputedStyle>,
    declaration: &Declaration,
) {
    if let Some(keyword) = parse_ident(&declaration.value)
        && matches!(keyword.as_str(), "initial" | "inherit" | "unset")
    {
        apply_css_wide(style, parent_style, &declaration.name, &keyword);
        return;
    }
    match declaration.name.as_str() {
        "display" => {
            if let Some(display) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "none" => Some(Display::None),
                    "block" | "flow-root" | "list-item" | "flex" | "grid" | "inline-block"
                    | "inline-flex" | "inline-grid" => Some(Display::Block),
                    "inline" => Some(Display::Inline),
                    "table" => Some(Display::Table),
                    "inline-table" => Some(Display::InlineTable),
                    "table-header-group" => Some(Display::TableHeaderGroup),
                    "table-row-group" => Some(Display::TableRowGroup),
                    "table-footer-group" => Some(Display::TableFooterGroup),
                    "table-row" => Some(Display::TableRow),
                    "table-cell" => Some(Display::TableCell),
                    "table-column" => Some(Display::TableColumn),
                    "table-column-group" => Some(Display::TableColumnGroup),
                    "table-caption" => Some(Display::TableCaption),
                    _ => None,
                })
            {
                style.display = display;
            }
        }
        "white-space" => {
            if let Some(white_space) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "normal" => Some(WhiteSpace::Normal),
                    "nowrap" => Some(WhiteSpace::NoWrap),
                    "pre" => Some(WhiteSpace::Pre),
                    "pre-wrap" => Some(WhiteSpace::PreWrap),
                    "pre-line" => Some(WhiteSpace::PreLine),
                    "break-spaces" => Some(WhiteSpace::BreakSpaces),
                    _ => None,
                })
            {
                style.white_space = white_space;
            }
        }
        "width" => {
            if let Some(width) = parse_width(&declaration.value) {
                style.width = width;
            }
        }
        "box-sizing" => {
            if let Some(box_sizing) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "content-box" => Some(BoxSizing::ContentBox),
                    "border-box" => Some(BoxSizing::BorderBox),
                    _ => None,
                })
            {
                style.box_sizing = box_sizing;
            }
        }
        "margin" => {
            if let Some(values) = parse_lengths(&declaration.value) {
                assign_edges(&mut style.margin, &values);
            }
        }
        "padding" => {
            if let Some(values) = parse_lengths(&declaration.value) {
                assign_edges(&mut style.padding, &values);
            }
        }
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
        "border-top" => assign_border_side(&mut style.border.top, &declaration.value),
        "border-right" => assign_border_side(&mut style.border.right, &declaration.value),
        "border-bottom" => assign_border_side(&mut style.border.bottom, &declaration.value),
        "border-left" => assign_border_side(&mut style.border.left, &declaration.value),
        "color" => {
            if let Some(color) = parse_color(&declaration.value) {
                style.color = color;
            }
        }
        "background-color" | "background" => {
            if let Some(color) = parse_color(&declaration.value) {
                style.background = color;
            }
        }
        "font-weight" => {
            if let Some(bold) = parse_font_weight(&declaration.value) {
                style.bold = bold;
            }
        }
        "text-decoration" | "text-decoration-line" => {
            if let Some((underline, strike)) = parse_text_decoration(&declaration.value) {
                style.underline = underline;
                style.strike = strike;
            }
        }
        "border-style" => assign_border_styles(&mut style.border, &declaration.value),
        "border-top-style" => assign_border_style(&mut style.border.top, &declaration.value),
        "border-right-style" => assign_border_style(&mut style.border.right, &declaration.value),
        "border-bottom-style" => assign_border_style(&mut style.border.bottom, &declaration.value),
        "border-left-style" => assign_border_style(&mut style.border.left, &declaration.value),
        "border-width" => assign_border_widths(&mut style.border, &declaration.value),
        "border-top-width" => assign_border_width(&mut style.border.top, &declaration.value),
        "border-right-width" => assign_border_width(&mut style.border.right, &declaration.value),
        "border-bottom-width" => assign_border_width(&mut style.border.bottom, &declaration.value),
        "border-left-width" => assign_border_width(&mut style.border.left, &declaration.value),
        "border-color" => assign_border_colors(&mut style.border, &declaration.value),
        "border-top-color" => assign_border_color(&mut style.border.top, &declaration.value),
        "border-right-color" => assign_border_color(&mut style.border.right, &declaration.value),
        "border-bottom-color" => assign_border_color(&mut style.border.bottom, &declaration.value),
        "border-left-color" => assign_border_color(&mut style.border.left, &declaration.value),
        "table-layout" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "auto" => Some(TableLayoutMode::Auto),
                    "fixed" => Some(TableLayoutMode::Fixed),
                    _ => None,
                })
            {
                style.table_layout = value;
            }
        }
        "border-collapse" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "separate" => Some(BorderCollapse::Separate),
                    "collapse" => Some(BorderCollapse::Collapse),
                    _ => None,
                })
            {
                style.border_collapse = value;
            }
        }
        "border-spacing" => {
            if let Some(values) = parse_lengths(&declaration.value)
                && let [horizontal] | [horizontal, _] = values.as_slice()
            {
                style.border_spacing =
                    BorderSpacing::new(*horizontal, values.get(1).copied().unwrap_or(*horizontal));
            }
        }
        "caption-side" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "top" => Some(CaptionSide::Top),
                    "bottom" => Some(CaptionSide::Bottom),
                    _ => None,
                })
            {
                style.caption_side = value;
            }
        }
        "list-style-type" => {
            if let Some(value) = parse_ident(&declaration.value)
                .as_deref()
                .and_then(parse_list_style_type)
            {
                style.list_style_type = value;
            }
        }
        "list-style-position" => {
            if let Some(value) =
                parse_ident(&declaration.value).and_then(|value| parse_list_style_position(&value))
            {
                style.list_style_position = value;
            }
        }
        "list-style" => apply_list_style_shorthand(style, &declaration.value),
        _ => {}
    }
}

fn apply_css_wide(
    style: &mut ComputedStyle,
    parent_style: Option<ComputedStyle>,
    property: &str,
    keyword: &str,
) {
    let initial = ComputedStyle::default();
    let inherited = parent_style.unwrap_or_default();
    let source = if keyword == "inherit" || (keyword == "unset" && is_inherited(property)) {
        inherited
    } else {
        initial
    };
    match property {
        "display" => style.display = source.display,
        "white-space" => style.white_space = source.white_space,
        "color" => style.color = source.color,
        "background-color" | "background" => style.background = source.background,
        "font-weight" => style.bold = source.bold,
        "text-decoration" | "text-decoration-line" => {
            style.underline = source.underline;
            style.strike = source.strike;
        }
        "width" => style.width = source.width,
        "box-sizing" => style.box_sizing = source.box_sizing,
        "margin" => style.margin = source.margin,
        "padding" => style.padding = source.padding,
        "margin-top" => style.margin.top = source.margin.top,
        "margin-right" => style.margin.right = source.margin.right,
        "margin-bottom" => style.margin.bottom = source.margin.bottom,
        "margin-left" => style.margin.left = source.margin.left,
        "padding-top" => style.padding.top = source.padding.top,
        "padding-right" => style.padding.right = source.padding.right,
        "padding-bottom" => style.padding.bottom = source.padding.bottom,
        "padding-left" => style.padding.left = source.padding.left,
        "border" | "border-style" | "border-width" | "border-color" => style.border = source.border,
        "border-top" | "border-top-style" | "border-top-width" | "border-top-color" => {
            style.border.top = source.border.top
        }
        "border-right" | "border-right-style" | "border-right-width" | "border-right-color" => {
            style.border.right = source.border.right
        }
        "border-bottom" | "border-bottom-style" | "border-bottom-width" | "border-bottom-color" => {
            style.border.bottom = source.border.bottom
        }
        "border-left" | "border-left-style" | "border-left-width" | "border-left-color" => {
            style.border.left = source.border.left
        }
        "table-layout" => style.table_layout = source.table_layout,
        "border-collapse" => style.border_collapse = source.border_collapse,
        "border-spacing" => style.border_spacing = source.border_spacing,
        "caption-side" => style.caption_side = source.caption_side,
        "list-style-type" => style.list_style_type = source.list_style_type,
        "list-style-position" => style.list_style_position = source.list_style_position,
        "list-style" => {
            style.list_style_type = source.list_style_type;
            style.list_style_position = source.list_style_position;
        }
        _ => {}
    }
}

fn is_inherited(property: &str) -> bool {
    matches!(
        property,
        "white-space"
            | "color"
            | "font-weight"
            | "border-collapse"
            | "border-spacing"
            | "caption-side"
            | "list-style"
            | "list-style-type"
            | "list-style-position"
    )
}

fn parse_color(source: &str) -> Option<Option<Rgb>> {
    if parse_ident(source).as_deref() == Some("transparent") {
        return Some(None);
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let color = CssColor::parse(&mut parser).ok()?;
    parser.expect_exhausted().ok()?;
    css_color_to_rgb(color).map(Some)
}

fn css_color_to_rgb(color: CssColor) -> Option<Rgb> {
    let (r, g, b) = match color {
        CssColor::Rgba(rgba) => (rgba.red, rgba.green, rgba.blue),
        CssColor::Hsl(hsl) => float_rgb(hsl_to_rgb(
            hsl.hue.unwrap_or_default() / 360.0,
            hsl.saturation.unwrap_or_default(),
            hsl.lightness.unwrap_or_default(),
        )),
        CssColor::Hwb(hwb) => float_rgb(hwb_to_rgb(
            hwb.hue.unwrap_or_default() / 360.0,
            hwb.whiteness.unwrap_or_default(),
            hwb.blackness.unwrap_or_default(),
        )),
        _ => return None,
    };
    Some(Rgb::new(r, g, b))
}

fn float_rgb(components: (f32, f32, f32)) -> (u8, u8, u8) {
    let (red, green, blue) = components;
    (
        clamp_unit_f32(red),
        clamp_unit_f32(green),
        clamp_unit_f32(blue),
    )
}

fn parse_font_weight(source: &str) -> Option<bool> {
    if let Some(keyword) = parse_ident(source) {
        return match keyword.as_str() {
            "bold" | "bolder" => Some(true),
            "normal" | "lighter" => Some(false),
            _ => None,
        };
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let weight = parser.expect_number().ok()?;
    parser.expect_exhausted().ok()?;
    Some(weight > 500.0)
}

fn parse_text_decoration(source: &str) -> Option<(bool, bool)> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut underline = false;
    let mut strike = false;
    let mut seen = false;
    while let Ok(token) = parser.next() {
        let Token::Ident(value) = token else {
            continue;
        };
        seen = true;
        match value.to_ascii_lowercase().as_str() {
            "none" | "initial" => return Some((false, false)),
            "underline" | "overline" => underline = true,
            "line-through" => strike = true,
            _ => {}
        }
    }
    seen.then_some((underline, strike))
}

fn parse_ident(source: &str) -> Option<String> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parser.expect_ident_cloned().ok()?;
    parser.expect_exhausted().ok()?;
    Some(value.to_ascii_lowercase())
}

fn parse_border(source: &str) -> Option<BorderEdges> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut side = BorderSide::default();
    let mut saw_width = false;
    let mut saw_style = false;
    let mut saw_color = false;
    while !parser.is_exhausted() {
        if !saw_color && let Ok(color) = parser.try_parse(CssColor::parse) {
            side.color = border_color(color)?;
            saw_color = true;
            continue;
        }
        let token = parser.next().ok()?;
        match token {
            Token::Ident(value) => {
                if !saw_style && let Some(border_style) = parse_border_style_ident(value) {
                    side.style = border_style;
                    saw_style = true;
                } else if !saw_width && let Some(width) = parse_border_width_ident(value) {
                    side.width = width;
                    saw_width = true;
                } else {
                    return None;
                }
            }
            Token::Number { value, .. } | Token::Dimension { value, .. }
                if !saw_width && value.is_finite() && *value >= 0.0 =>
            {
                side.width = usize::from(*value > 0.0);
                saw_width = true;
            }
            _ => return None,
        }
    }
    (saw_width || saw_style || saw_color).then_some(BorderEdges::uniform(side))
}

fn border_color(color: CssColor) -> Option<BorderColor> {
    match color {
        CssColor::CurrentColor => Some(BorderColor::CurrentColor),
        CssColor::Rgba(rgba) if rgba.alpha <= 0.0 => Some(BorderColor::Transparent),
        color => css_color_to_rgb(color).map(BorderColor::Rgb),
    }
}

fn parse_border_style_ident(value: &str) -> Option<BorderLineStyle> {
    match value.to_ascii_lowercase().as_str() {
        "none" => Some(BorderLineStyle::None),
        "hidden" => Some(BorderLineStyle::Hidden),
        "inset" => Some(BorderLineStyle::Inset),
        "groove" => Some(BorderLineStyle::Groove),
        "outset" => Some(BorderLineStyle::Outset),
        "ridge" => Some(BorderLineStyle::Ridge),
        "dotted" => Some(BorderLineStyle::Dotted),
        "dashed" => Some(BorderLineStyle::Dashed),
        "solid" => Some(BorderLineStyle::Solid),
        "double" => Some(BorderLineStyle::Double),
        _ => None,
    }
}

fn parse_border_width_ident(value: &str) -> Option<usize> {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "thin" | "medium" | "thick"
    )
    .then_some(1)
}

fn parse_border_styles(source: &str) -> Option<Vec<BorderLineStyle>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() && values.len() < 4 {
        values.push(parse_border_style_ident(parser.expect_ident().ok()?)?);
    }
    (!values.is_empty() && parser.is_exhausted()).then_some(values)
}

fn parse_border_widths(source: &str) -> Option<Vec<usize>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() && values.len() < 4 {
        let value = match parser.next().ok()? {
            Token::Ident(value) => parse_border_width_ident(value)?,
            Token::Number { value, .. } | Token::Dimension { value, .. }
                if value.is_finite() && *value >= 0.0 =>
            {
                usize::from(*value > 0.0)
            }
            _ => return None,
        };
        values.push(value);
    }
    (!values.is_empty() && parser.is_exhausted()).then_some(values)
}

fn parse_border_colors(source: &str) -> Option<Vec<BorderColor>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() && values.len() < 4 {
        values.push(border_color(CssColor::parse(&mut parser).ok()?)?);
    }
    (!values.is_empty() && parser.is_exhausted()).then_some(values)
}

fn expanded_edges<T: Copy>(values: &[T]) -> Option<[T; 4]> {
    match values {
        [all] => Some([*all; 4]),
        [vertical, horizontal] => Some([*vertical, *horizontal, *vertical, *horizontal]),
        [top, horizontal, bottom] => Some([*top, *horizontal, *bottom, *horizontal]),
        [top, right, bottom, left] => Some([*top, *right, *bottom, *left]),
        _ => None,
    }
}

fn assign_border_side(target: &mut BorderSide, value: &str) {
    if let Some(border) = parse_border(value) {
        *target = border.top;
    }
}

fn assign_border_styles(target: &mut BorderEdges, value: &str) {
    if let Some(values) = parse_border_styles(value).and_then(|values| expanded_edges(&values)) {
        target.top.style = values[0];
        target.right.style = values[1];
        target.bottom.style = values[2];
        target.left.style = values[3];
    }
}

fn assign_border_style(target: &mut BorderSide, value: &str) {
    if let Some(value) = parse_ident(value).and_then(|value| parse_border_style_ident(&value)) {
        target.style = value;
    }
}

fn assign_border_widths(target: &mut BorderEdges, value: &str) {
    if let Some(values) = parse_border_widths(value).and_then(|values| expanded_edges(&values)) {
        target.top.width = values[0];
        target.right.width = values[1];
        target.bottom.width = values[2];
        target.left.width = values[3];
    }
}

fn assign_border_width(target: &mut BorderSide, value: &str) {
    if let Some(values) = parse_border_widths(value)
        && let [width] = values.as_slice()
    {
        target.width = *width;
    }
}

fn assign_border_colors(target: &mut BorderEdges, value: &str) {
    if let Some(values) = parse_border_colors(value).and_then(|values| expanded_edges(&values)) {
        target.top.color = values[0];
        target.right.color = values[1];
        target.bottom.color = values[2];
        target.left.color = values[3];
    }
}

fn assign_border_color(target: &mut BorderSide, value: &str) {
    if let Some(values) = parse_border_colors(value)
        && let [color] = values.as_slice()
    {
        target.color = *color;
    }
}

fn assign_one(target: &mut usize, value: &str) {
    if let Some(value) = parse_length(value) {
        *target = value;
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
        [top, right, bottom, left] => {
            *edges = EdgeSizes {
                top: *top,
                right: *right,
                bottom: *bottom,
                left: *left,
            };
        }
        [] | [_, _, _, _, _, ..] => {}
    }
}

fn parse_width(source: &str) -> Option<CssWidth> {
    if parse_ident(source).as_deref() == Some("auto") {
        return Some(CssWidth::Auto);
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let percentage = match parser.next() {
        Ok(Token::Percentage { unit_value, .. }) => Some(*unit_value),
        _ => None,
    };
    if let Some(unit_value) = percentage
        && unit_value.is_finite()
        && unit_value >= 0.0
        && parser.is_exhausted()
    {
        return Some(CssWidth::Percent(CssPercentage::new(
            (unit_value * 10_000.0).round().min(u32::MAX as f32) as u32,
        )));
    }
    parse_length(source).map(CssWidth::Cells)
}

fn parse_length(source: &str) -> Option<usize> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_length_token(&mut parser)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

fn parse_lengths(source: &str) -> Option<Vec<usize>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values = Vec::new();
    while !parser.is_exhausted() {
        if values.len() == 4 {
            return None;
        }
        values.push(parse_length_token(&mut parser)?);
    }
    (!values.is_empty()).then_some(values)
}

fn parse_length_token(parser: &mut Parser<'_, '_>) -> Option<usize> {
    let value = match parser.next().ok()? {
        Token::Number { value, .. } if *value == 0.0 => *value,
        Token::Dimension { value, .. } if value.is_finite() && *value >= 0.0 => *value,
        _ => return None,
    };
    Some(value.round().min(65_535.0) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render::embedded_style_sheets;
    use crate::core::dom::{Attr, ElementNs, SharedDocument};
    use crate::css::{CssParser, CssparserParser};
    use crate::html::{Html5everParser, HtmlParser};

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
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
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
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        assert_eq!(styles.get(p).display, Display::None);
    }

    #[test]
    fn table_roles_and_properties_reach_the_computed_style() {
        let mut document = Document::new();
        let table = document.insert_element(None, "table", ElementNs::Html, vec![]);
        let body = document.insert_element(Some(table), "tbody", ElementNs::Html, vec![]);
        let row = document.insert_element(Some(body), "tr", ElementNs::Html, vec![]);
        let cell = document.insert_element(Some(row), "td", ElementNs::Html, vec![]);
        let caption = document.insert_element(Some(table), "caption", ElementNs::Html, vec![]);
        let sheet = CssparserParser.parse(
            "table { table-layout: fixed; border-collapse: collapse; border-spacing: 3px 2px;
                      width: 50%; border: 2px dashed red; caption-side: bottom }
             td { border-left: 0 hidden blue; border-top-color: currentcolor }",
        );
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());

        let table_style = styles.get(table);
        assert_eq!(table_style.display, Display::Table);
        assert_eq!(table_style.table_layout, TableLayoutMode::Fixed);
        assert_eq!(table_style.border_collapse, BorderCollapse::Collapse);
        assert_eq!(table_style.border_spacing, BorderSpacing::new(3, 2));
        assert_eq!(
            table_style.width,
            CssWidth::Percent(CssPercentage::new(5_000))
        );
        assert_eq!(table_style.caption_side, CaptionSide::Bottom);
        assert_eq!(table_style.border.top.style, BorderLineStyle::Dashed);
        assert_eq!(
            table_style.border.top.color,
            BorderColor::Rgb(Rgb::new(255, 0, 0))
        );
        assert_eq!(styles.get(body).display, Display::TableRowGroup);
        assert_eq!(styles.get(row).display, Display::TableRow);
        assert_eq!(styles.get(cell).display, Display::TableCell);
        assert_eq!(styles.get(cell).border.left.width, 0);
        assert_eq!(styles.get(cell).border.left.style, BorderLineStyle::Hidden);
        assert_eq!(styles.get(cell).border.top.color, BorderColor::CurrentColor);
        assert_eq!(styles.get(caption).display, Display::TableCaption);
    }

    #[test]
    fn transparent_and_hidden_borders_keep_their_geometry_without_painting() {
        let mut document = Document::new();
        let transparent = document.insert_element(
            None,
            "div",
            ElementNs::Html,
            vec![Attr::plain("style", "border: thin solid transparent")],
        );
        let hidden = document.insert_element(
            None,
            "div",
            ElementNs::Html,
            vec![Attr::plain("style", "border: thick hidden currentcolor")],
        );
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        assert_eq!(styles.get(transparent).border.left.layout_width(), 1);
        assert!(!styles.get(transparent).border.left.is_visible());
        assert_eq!(styles.get(hidden).border.left.layout_width(), 0);
        assert!(!styles.get(hidden).border.left.is_visible());
    }

    #[test]
    fn comments_are_tokenized_and_invalid_values_do_not_override_ua_defaults() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let pre = document.insert_element(Some(p), "pre", ElementNs::Html, vec![]);
        let sheet = CssparserParser
            .parse("p { display: block /**/; border: none /**/ } pre { white-space: invalid }");
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        assert_eq!(styles.get(p).display, Display::Block);
        assert!(!styles.get(p).border.is_visible());
        assert_eq!(styles.get(pre).white_space, WhiteSpace::Pre);
    }

    #[test]
    fn author_colors_weight_and_decoration_reach_the_computed_style() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let em = document.insert_element(Some(p), "em", ElementNs::Html, vec![]);
        let sheet = CssparserParser.parse(
            "p { color: #ff0000; background-color: rgb(0, 128, 0); font-weight: 700;
                 text-decoration: underline line-through }
             em { color: hsl(240, 100%, 50%) }",
        );
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        let paragraph = styles.get(p);
        assert_eq!(paragraph.color, Some(Rgb::new(255, 0, 0)));
        assert_eq!(paragraph.background, Some(Rgb::new(0, 128, 0)));
        assert!(paragraph.bold && paragraph.underline && paragraph.strike);
        assert_eq!(styles.get(em).color, Some(Rgb::new(0, 0, 255)));
    }

    #[test]
    fn font_weight_is_bold_above_five_hundred_only() {
        assert_eq!(parse_font_weight("500"), Some(false));
        assert_eq!(parse_font_weight("501"), Some(true));
        assert_eq!(parse_font_weight("bold"), Some(true));
        assert_eq!(parse_font_weight("normal"), Some(false));
        assert_eq!(parse_font_weight("chunky"), None);
    }

    #[test]
    fn unsupported_color_spaces_and_junk_never_override_the_inherited_value() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let span = document.insert_element(Some(p), "span", ElementNs::Html, vec![]);
        let sheet = CssparserParser.parse(
            "p { color: #00ff00 } span { color: oklch(0.5 0.1 200); background: not-a-color }",
        );
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        assert_eq!(styles.get(span).color, Some(Rgb::new(0, 255, 0)));
        assert_eq!(styles.get(span).background, None);
    }

    #[test]
    fn color_inherits_while_background_does_not() {
        let mut document = Document::new();
        let div = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let p = document.insert_element(Some(div), "p", ElementNs::Html, vec![]);
        let sheet = CssparserParser.parse("div { color: #123456; background-color: #654321 }");
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        assert_eq!(styles.get(p).color, Some(Rgb::new(0x12, 0x34, 0x56)));
        assert_eq!(styles.get(p).background, None);
    }

    #[test]
    fn the_ua_sheet_styles_links_bold_and_struck_text_from_the_palette() {
        let mut document = Document::new();
        let link = document.insert_element(
            None,
            "a",
            ElementNs::Html,
            vec![Attr::plain("href", "https://example.com/")],
        );
        let anchor = document.insert_element(None, "a", ElementNs::Html, vec![]);
        let strong = document.insert_element(None, "strong", ElementNs::Html, vec![]);
        let struck = document.insert_element(None, "del", ElementNs::Html, vec![]);
        let palette = Palette {
            text: Rgb::WHITE,
            background: Rgb::new(0, 0, 128),
            link: Rgb::new(255, 255, 0),
        };
        let styles =
            BasicCascade.apply(&[], &document, MediaContext::screen().with_palette(palette));
        assert_eq!(styles.get(link).color, Some(Rgb::new(255, 255, 0)));
        assert!(styles.get(link).underline);
        assert_eq!(
            styles.get(anchor).color,
            None,
            "an anchor without href is not a link"
        );
        assert!(styles.get(strong).bold);
        assert!(styles.get(struck).strike);
    }

    #[test]
    fn dynamic_pseudo_class_rules_apply_instead_of_being_discarded() {
        let mut document = Document::new();
        let link = document.insert_element(
            None,
            "a",
            ElementNs::Html,
            vec![Attr::plain("href", "https://example.com/")],
        );
        let sheet = CssparserParser.parse(
            "a:link { color: #00ff00 } a:visited { color: #ff0000 } p, a:hover { border: solid }",
        );
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        assert_eq!(
            styles.get(link).color,
            Some(Rgb::new(0, 255, 0)),
            "a:link must beat the UA link colour by source order"
        );
    }

    fn cascade_contract(cascade: &dyn Cascade) {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let sheet = CssparserParser.parse(
            "p { display: inline }
             @media screen { p { display: block } }
             @media print { p { display: none } }
             @media not unknown { p { border: solid } }",
        );
        let screen = cascade.apply(
            std::slice::from_ref(&sheet),
            &document,
            MediaContext::screen(),
        );
        assert_eq!(screen.get(p).display, Display::Block);
        assert!(screen.get(p).border.is_visible());
        let print = cascade.apply(&[sheet], &document, MediaContext::print());
        assert_eq!(print.get(p).display, Display::None);
        assert!(print.get(p).border.is_visible());
    }

    #[test]
    fn basic_cascade_passes_the_cascade_contract() {
        cascade_contract(&BasicCascade);
    }

    #[test]
    fn nested_media_preserves_lexical_source_order() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let sheet = CssparserParser.parse(
            "p { display: inline }
             @media screen {
                 p { display: none }
                 @media not print { p { display: block } }
             }
             p { border: solid }",
        );
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        assert_eq!(styles.get(p).display, Display::Block);
        assert!(styles.get(p).border.is_visible());
    }

    #[test]
    fn unsupported_layout_modes_follow_the_degradation_contract() {
        for value in [
            "inline-block",
            "flex",
            "grid",
            "flow-root",
            "list-item",
            "inline-flex",
            "inline-grid",
        ] {
            let mut document = Document::new();
            let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
            let sheet = CssparserParser.parse(&format!("p {{ display: {value} }}"));
            let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
            assert_eq!(styles.get(p).display, Display::Block, "{value}");
        }

        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let baseline = BasicCascade.apply(&[], &document, MediaContext::screen());
        let sheet =
            CssparserParser.parse("p { position: absolute; inset: 4px; top: 1px; height: 50% }");
        let degraded = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        assert_eq!(degraded.get(p), baseline.get(p));
    }

    #[test]
    fn white_space_modes_inherit_and_css_wide_keywords_resolve() {
        let mut document = Document::new();
        let parent = document.insert_element(
            None,
            "div",
            ElementNs::Html,
            vec![Attr::plain("style", "white-space: pre-wrap")],
        );
        let inherited = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
        let initial = document.insert_element(
            Some(parent),
            "span",
            ElementNs::Html,
            vec![Attr::plain("style", "white-space: initial")],
        );
        let unset = document.insert_element(
            Some(parent),
            "span",
            ElementNs::Html,
            vec![Attr::plain("style", "white-space: unset")],
        );
        let sheet = CssparserParser.parse("div { width: 20px } span { box-sizing: border-box }");
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        assert_eq!(styles.get(parent).white_space, WhiteSpace::PreWrap);
        assert_eq!(styles.get(inherited).white_space, WhiteSpace::PreWrap);
        assert_eq!(styles.get(initial).white_space, WhiteSpace::Normal);
        assert_eq!(styles.get(unset).white_space, WhiteSpace::PreWrap);
        assert_eq!(styles.get(parent).width, CssWidth::Cells(20));
        assert_eq!(styles.get(inherited).width, CssWidth::Auto);
        assert_eq!(styles.get(inherited).box_sizing, BoxSizing::BorderBox);
    }

    #[test]
    fn lengths_are_strict_and_bounded_without_partial_overrides() {
        let mut document = Document::new();
        let p = document.insert_element(
            None,
            "p",
            ElementNs::Html,
            vec![Attr::plain(
                "style",
                "margin: 2px; margin: 3px bogus; padding: 1px 2px 3px 4px 5px; \
                 padding-left: 999999px; width: 12px; width: -1px",
            )],
        );
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        assert_eq!(
            styles.get(p).margin,
            EdgeSizes {
                top: 2,
                right: 2,
                bottom: 2,
                left: 2
            }
        );
        assert_eq!(styles.get(p).padding.top, 0);
        assert_eq!(styles.get(p).padding.left, 65_535);
        assert_eq!(styles.get(p).width, CssWidth::Cells(12));
    }

    #[test]
    fn bucketed_and_naive_cascades_agree_for_complex_selector_lists() {
        let mut document = Document::new();
        let article = document.insert_element(
            None,
            "article",
            ElementNs::Html,
            vec![Attr::plain("id", "hero")],
        );
        document.insert_element(
            Some(article),
            "p",
            ElementNs::Html,
            vec![Attr::plain("class", "note a"), Attr::plain("data-x", "yes")],
        );
        document.insert_element(
            Some(article),
            "span",
            ElementNs::Html,
            vec![Attr::plain("class", "b")],
        );
        let sheet = CssparserParser.parse(
            "#hero, .missing { color: red }
             .note { font-weight: 700 }
             article > p { display: block }
             [data-x] { text-decoration: underline }
             :is(.a, .b) { background-color: blue }
             p:not(.skip) { width: 12px }",
        );
        let media = MediaContext::screen();
        assert_eq!(
            BasicCascade.apply(std::slice::from_ref(&sheet), &document, media),
            apply_naive(std::slice::from_ref(&sheet), &document, media)
        );
    }

    #[test]
    fn bucket_normalization_matches_naive_cascade_in_quirks_mode() {
        let mut document = Document::new();
        document.set_quirks_mode(crate::core::dom::DomQuirksMode::Quirks);
        document.insert_element(
            None,
            "p",
            ElementNs::Html,
            vec![Attr::plain("class", "loud")],
        );
        let sheet = CssparserParser.parse(".LOUD { font-weight: bold }");
        let media = MediaContext::screen();
        assert_eq!(
            BasicCascade.apply(std::slice::from_ref(&sheet), &document, media),
            apply_naive(std::slice::from_ref(&sheet), &document, media)
        );
    }

    #[test]
    fn bucketed_and_naive_cascades_agree_on_the_render_fixture_corpus() {
        for source in [
            include_str!("../../tests/fixtures/borders.html"),
            include_str!("../../tests/fixtures/headings.html"),
            include_str!("../../tests/fixtures/links.html"),
            include_str!("../../tests/fixtures/margins.html"),
            include_str!("../../tests/fixtures/pre.html"),
            include_str!("../../tests/fixtures/wide.html"),
            include_str!("../../tests/fixtures/lists.html"),
            include_str!("../../tests/fixtures/generated.html"),
        ] {
            let outcome = Html5everParser::new(false).parse_document(source);
            let document = outcome.document.borrow();
            let sheets = embedded_style_sheets(&document);
            let media = MediaContext::screen();
            assert_eq!(
                BasicCascade.apply(&sheets, &document, media),
                apply_naive(&sheets, &document, media)
            );
        }
    }

    #[test]
    fn media_features_use_the_injected_terminal_context() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let sheet = CssparserParser.parse(
            "@media screen and (scripting: none) and (prefers-color-scheme: dark)
                    and (min-width: 80px) and (max-height: 24px) {
                p { display: none }
             }",
        );
        let matching = MediaContext::screen()
            .with_viewport(Size { cols: 80, rows: 24 })
            .with_color_scheme(ColorScheme::Dark)
            .with_scripting(false);
        assert_eq!(
            BasicCascade
                .apply(std::slice::from_ref(&sheet), &document, matching)
                .get(p)
                .display,
            Display::None
        );
        let narrow = matching.with_viewport(Size { cols: 79, rows: 24 });
        assert_ne!(
            BasicCascade
                .apply(&[sheet], &document, narrow)
                .get(p)
                .display,
            Display::None
        );
    }

    /// Cascades `source` and returns the style tree plus every element in document order, so a
    /// test can name nodes by their position without rebuilding the DOM by hand.
    fn cascade_source(source: &str) -> (SharedDocument, StyleTree, Vec<NodeId>) {
        let outcome = Html5everParser::new(false).parse_document(source);
        let (styles, order) = {
            let document = outcome.document.borrow();
            let sheets = embedded_style_sheets(&document);
            let styles = BasicCascade.apply(&sheets, &document, MediaContext::screen());
            let order = elements_in_document_order(&document)
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            (styles, order)
        };
        (outcome.document, styles, order)
    }

    fn markers_of(source: &str) -> Vec<String> {
        let (_, styles, order) = cascade_source(source);
        order
            .iter()
            .filter_map(|id| styles.marker(*id))
            .map(|marker| marker.text.trim_end().to_string())
            .collect()
    }

    fn pseudo_texts(source: &str, which: PseudoElement) -> Vec<String> {
        let (_, styles, order) = cascade_source(source);
        order
            .iter()
            .filter_map(|id| styles.pseudo(*id, which))
            .map(|pseudo| pseudo.text.clone())
            .collect()
    }

    #[test]
    fn ordered_lists_number_and_unordered_lists_bullet() {
        assert_eq!(
            markers_of("<ol><li>a</li><li>b</li><li>c</li></ol>"),
            vec!["1.", "2.", "3."]
        );
        assert_eq!(markers_of("<ul><li>a</li><li>b</li></ul>"), vec!["•", "•"]);
    }

    #[test]
    fn nested_lists_number_independently_of_their_parent_list() {
        assert_eq!(
            markers_of(
                "<ol><li>one<ol><li>inner one</li><li>inner two</li></ol></li>\
                 <li>two</li></ol>"
            ),
            vec!["1.", "1.", "2.", "2."]
        );
    }

    #[test]
    fn a_sibling_counter_reset_does_not_leak_into_an_earlier_siblings_subtree() {
        // `second` resets the counter at its own depth. `first`'s child was visited earlier and
        // must still see the outer value, while `second`'s child sees the reset one.
        let source = "<style>
                #second { counter-reset: n 10 }
                span { counter-increment: n }
                span::before { content: counter(n) }
             </style>
             <div id='first'><span>a</span></div>
             <div id='second'><span>b</span></div>
             <div id='third'><span>c</span></div>";
        assert_eq!(
            pseudo_texts(source, PseudoElement::Before),
            ["1", "11", "12"]
        );
    }

    #[test]
    fn counters_joins_every_nesting_level_outer_to_inner() {
        let source = "<style>
                ol { counter-reset: step }
                li { counter-increment: step }
                li::before { content: counters(step, '.') }
             </style>
             <ol><li>a<ol><li>b</li><li>c</li></ol></li><li>d</li></ol>";
        assert_eq!(
            pseudo_texts(source, PseudoElement::Before),
            ["1", "1.1", "1.2", "2"]
        );
    }

    #[test]
    fn display_none_subtrees_neither_increment_counters_nor_generate_content() {
        let source = "<style>
                span { counter-increment: n }
                span::before { content: counter(n) }
                .hidden { display: none }
             </style>
             <span>a</span>
             <div class='hidden'><span>skipped</span></div>
             <span>b</span>";
        assert_eq!(pseudo_texts(source, PseudoElement::Before), ["1", "2"]);
    }

    #[test]
    fn ordered_list_attributes_drive_the_list_item_counter() {
        assert_eq!(
            markers_of("<ol start='3'><li>a</li><li value='7'>b</li><li>c</li></ol>"),
            vec!["3.", "7.", "8."]
        );
        assert_eq!(
            markers_of("<ol reversed><li>a</li><li>b</li><li>c</li></ol>"),
            vec!["3.", "2.", "1."]
        );
        assert_eq!(
            markers_of("<ol type='i'><li>a</li><li>b</li><li>c</li><li>d</li></ol>"),
            vec!["i.", "ii.", "iii.", "iv."]
        );
    }

    #[test]
    fn unsupported_content_components_invalidate_only_their_own_declaration() {
        let source = "<style>
                p::before { content: 'kept ' }
                p::after { content: url(nope.png) }
             </style>
             <p>body</p>";
        assert_eq!(pseudo_texts(source, PseudoElement::Before), ["kept "]);
        assert!(pseudo_texts(source, PseudoElement::After).is_empty());
    }

    #[test]
    fn a_pseudo_element_in_a_selector_list_no_longer_discards_its_siblings() {
        let (_, styles, order) = cascade_source(
            "<style>h1, .note::before { color: #ff0000 }</style>\
             <h1>heading</h1><p class='note'>note</p>",
        );
        let heading = order
            .iter()
            .copied()
            .find(|id| styles.get(*id).color == Some(Rgb::new(255, 0, 0)));
        assert!(heading.is_some(), "the h1 half of the list still matches");
    }

    #[test]
    fn generated_content_inherits_from_its_originating_element() {
        let (_, styles, order) = cascade_source(
            "<style>p { color: #00ff00 } p::before { content: 'x' }</style><p>body</p>",
        );
        let pseudo = order
            .iter()
            .find_map(|id| styles.pseudo(*id, PseudoElement::Before))
            .expect("generated content");
        assert_eq!(pseudo.style.color, Some(Rgb::new(0, 255, 0)));
    }

    #[test]
    fn an_author_counter_does_not_stop_a_list_from_numbering() {
        // Real sites (Wikipedia's reference lists among them) increment a counter of their own on
        // list items. The implicit `list-item` step that `display: list-item` implies must survive.
        assert_eq!(
            markers_of(
                "<style>li { counter-increment: mine }</style>\
                 <ol><li>a</li><li>b</li><li>c</li></ol>"
            ),
            vec!["1.", "2.", "3."]
        );
        assert_eq!(
            markers_of(
                "<style>ol { counter-reset: mine }</style>\
                 <ol><li>a</li><li>b</li></ol>"
            ),
            vec!["1.", "2."]
        );
    }

    #[test]
    fn naming_the_list_item_counter_explicitly_does_override_the_implicit_step() {
        assert_eq!(
            markers_of(
                "<style>li { counter-increment: list-item 2 }</style>\
                 <ol><li>a</li><li>b</li></ol>"
            ),
            vec!["2.", "4."]
        );
    }

    #[test]
    fn a_marker_rule_overrides_the_ua_marker_text() {
        assert_eq!(
            markers_of(
                "<style>li::marker { content: '-> ' }</style>\
                 <ul><li>a</li><li>b</li></ul>"
            ),
            vec!["->", "->"]
        );
    }

    #[test]
    fn list_style_type_none_suppresses_the_marker_entirely() {
        assert!(
            markers_of("<style>li { list-style-type: none }</style><ul><li>a</li></ul>").is_empty()
        );
    }
}

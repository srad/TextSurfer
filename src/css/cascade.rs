use std::collections::HashMap;

use cssparser::color::clamp_unit_f32;
use cssparser::{Parser, ParserInput, Token};
use cssparser_color::{Color as CssColor, hsl_to_rgb, hwb_to_rgb};

use crate::core::dom::Document;
use crate::core::dom::{AttrNs, ElementNs, Node, NodeId};
use crate::core::geom::Size;
use crate::core::style::{
    BorderCollapse, BorderColor, BorderEdges, BorderLineStyle, BorderSide, BorderSpacing,
    BoxSizing, CaptionSide, ComputedStyle, CssPercentage, CssWidth, Display, EdgeSizes, Palette,
    Rgb, StyleTree, TableLayoutMode, WhiteSpace,
};
use crate::css::parser::{
    ColorScheme, CssRule, MediaAxis, MediaComparison, MediaFeature, MediaQuery, MediaQueryList,
    ScriptingValue, StyleRule, parse_declarations,
};
use crate::css::selectors::{BucketKey, DynamicState, bucket_keys, matching_specificity};
use crate::css::{Declaration, StyleSheet};

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
    for id in elements_in_document_order(document) {
        let parent_style = document.parent(id).map(|parent| tree.get(parent));
        let mut style = ua_style(document, id, parent_style, media.palette);
        let mut declarations = Vec::new();
        let mut order = 0usize;
        let candidates = index.as_ref().map_or_else(
            || (0..rules.len()).collect(),
            |index| index.candidates(document, id),
        );
        for rule_index in candidates {
            let rule = rules[rule_index];
            if let Some(specificity) =
                matching_specificity(&rule.selectors, document, id, media.state)
            {
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
        for (_, _, _, declaration) in declarations {
            apply_declaration(&mut style, parent_style, &declaration);
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
    }
    tree
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
            ..Default::default()
        };
    };
    if *ns != ElementNs::Html {
        return ComputedStyle {
            color: inherited.color,
            bold: inherited.bold,
            underline: inherited.underline,
            strike: inherited.strike,
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
            | "li"
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
        ..Default::default()
    };
    if name == "pre" {
        style.white_space = WhiteSpace::Pre;
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
    use crate::core::dom::{Attr, ElementNs};
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
}

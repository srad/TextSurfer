use cssparser::color::clamp_unit_f32;
use cssparser::{Parser, ParserInput, Token};
use cssparser_color::{Color as CssColor, hsl_to_rgb, hwb_to_rgb};

use crate::core::dom::Document;
use crate::core::dom::{AttrNs, ElementNs, Node, NodeId};
use crate::core::style::{
    BoxSizing, ComputedStyle, CssWidth, Display, EdgeSizes, Palette, Rgb, StyleTree, WhiteSpace,
};
use crate::css::parser::{CssRule, MediaQuery, MediaQueryList, StyleRule, parse_declarations};
use crate::css::selectors::{DynamicState, matching_specificity};
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
}

impl MediaContext {
    pub const fn screen() -> Self {
        Self {
            target: MediaTarget::Screen,
            palette: Palette::DEFAULT,
            state: DynamicState::INERT,
        }
    }

    pub const fn print() -> Self {
        Self {
            target: MediaTarget::Print,
            palette: Palette::DEFAULT,
            state: DynamicState::INERT,
        }
    }

    pub fn with_palette(self, palette: Palette) -> Self {
        Self { palette, ..self }
    }

    pub fn with_state(self, state: DynamicState) -> Self {
        Self { state, ..self }
    }
}

pub trait Cascade: Send + Sync {
    fn apply(&self, sheets: &[StyleSheet], document: &Document, media: MediaContext) -> StyleTree;
}

#[derive(Default)]
pub struct BasicCascade;

impl Cascade for BasicCascade {
    fn apply(&self, sheets: &[StyleSheet], document: &Document, media: MediaContext) -> StyleTree {
        let mut tree = StyleTree::default();
        let rules = active_style_rules(sheets, media);
        for id in elements_in_document_order(document) {
            let parent_style = document.parent(id).map(|parent| tree.get(parent));
            let mut style = ua_style(document, id, parent_style, media.palette);
            let mut declarations = Vec::new();
            let mut order = 0usize;
            for rule in &rules {
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
            declarations.sort_by_key(|(important, specificity, order, _)| {
                (*important, *specificity, *order)
            });
            for (_, _, _, declaration) in declarations {
                apply_declaration(&mut style, parent_style, &declaration);
            }
            if style.display != Display::Block {
                style.width = CssWidth::Auto;
            }
            tree.insert(id, style);
        }
        tree
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
        }
    }
    active
}

fn media_query_list_matches(queries: &MediaQueryList, media: MediaContext) -> bool {
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
            MediaQuery::Never => false,
        }),
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
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
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
        Display::Inline
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
                    "block" | "flow-root" | "list-item" | "table" | "flex" | "grid"
                    | "inline-block" | "inline-flex" | "inline-grid" => Some(Display::Block),
                    "inline" => Some(Display::Inline),
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
        "border-style" => {
            if let Some(border) =
                parse_ident(&declaration.value).and_then(|value| match value.as_str() {
                    "none" | "hidden" | "initial" => Some(false),
                    "solid" | "dotted" | "dashed" | "double" | "groove" | "ridge" | "inset"
                    | "outset" => Some(true),
                    _ => None,
                })
            {
                style.border = border;
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
        "border" | "border-style" => style.border = source.border,
        _ => {}
    }
}

fn is_inherited(property: &str) -> bool {
    matches!(property, "white-space" | "color" | "font-weight")
}

fn parse_color(source: &str) -> Option<Option<Rgb>> {
    if parse_ident(source).as_deref() == Some("transparent") {
        return Some(None);
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let color = CssColor::parse(&mut parser).ok()?;
    parser.expect_exhausted().ok()?;
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
    Some(Some(Rgb::new(r, g, b)))
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

fn parse_border(source: &str) -> Option<bool> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut visible_style = false;
    let mut zero_width = false;
    while let Ok(token) = parser.next() {
        match token {
            Token::Ident(value) if value.eq_ignore_ascii_case("none") => return Some(false),
            Token::Ident(value) if value.eq_ignore_ascii_case("hidden") => return Some(false),
            Token::Ident(value) if value.eq_ignore_ascii_case("initial") => return Some(false),
            Token::Ident(value)
                if [
                    "solid", "dotted", "dashed", "double", "groove", "ridge", "inset", "outset",
                ]
                .iter()
                .any(|style| value.eq_ignore_ascii_case(style)) =>
            {
                visible_style = true;
            }
            Token::Number { value, .. } | Token::Dimension { value, .. } if *value == 0.0 => {
                zero_width = true;
            }
            _ => {}
        }
    }
    if zero_width {
        Some(false)
    } else if visible_style {
        Some(true)
    } else {
        None
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
    use crate::core::dom::{Attr, ElementNs};
    use crate::css::{CssParser, CssparserParser};

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
    fn comments_are_tokenized_and_invalid_values_do_not_override_ua_defaults() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let pre = document.insert_element(Some(p), "pre", ElementNs::Html, vec![]);
        let sheet = CssparserParser
            .parse("p { display: block /**/; border: none /**/ } pre { white-space: invalid }");
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        assert_eq!(styles.get(p).display, Display::Block);
        assert!(!styles.get(p).border);
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
        assert!(screen.get(p).border);
        let print = cascade.apply(&[sheet], &document, MediaContext::print());
        assert_eq!(print.get(p).display, Display::None);
        assert!(print.get(p).border);
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
        assert!(styles.get(p).border);
    }

    #[test]
    fn unsupported_layout_modes_follow_the_degradation_contract() {
        for value in [
            "table",
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
}

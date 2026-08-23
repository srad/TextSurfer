use super::document::elements_in_document_order;
use super::*;
use crate::core::dom::{Attr, Document, ElementNs, NodeId, SharedDocument};
use crate::core::geom::Size;
use crate::core::style::{
    BorderCollapse, BorderColor, BorderLineStyle, BorderSpacing, BoxSizing, CaptionSide,
    CssPercentage, CssWidth, Display, EdgeSizes, Palette, PseudoElement, Rgb, Rgba, StyleTree,
    TableLayoutMode, WhiteSpace,
};
use crate::css::values::parse_font_weight;
use crate::css::{ColorScheme, CssParser, CssparserParser};
use crate::html::{Html5everParser, HtmlParser};
use crate::pipeline::render::embedded_style_sheets;

fn apply_naive(sheets: &[StyleSheet], document: &Document, media: MediaContext) -> StyleTree {
    super::document::cascade_document(sheets, document, media, false)
}

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
fn table_properties_inherit_without_css_wide_keywords() {
    let mut document = Document::new();
    let parent = document.insert_element(
        None,
        "div",
        ElementNs::Html,
        vec![Attr::plain("id", "parent")],
    );
    let table = document.insert_element(
        Some(parent),
        "div",
        ElementNs::Html,
        vec![Attr::plain("id", "table")],
    );
    let caption = document.insert_element(
        Some(table),
        "span",
        ElementNs::Html,
        vec![Attr::plain("id", "caption")],
    );
    let sheet = CssparserParser.parse(
        "#parent { border-collapse: collapse; border-spacing: 3px 2px; caption-side: bottom }
         #table { display: table } #caption { display: table-caption }",
    );
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(table).border_collapse, BorderCollapse::Collapse);
    assert_eq!(styles.get(table).border_spacing, BorderSpacing::new(3, 2));
    assert_eq!(styles.get(caption).caption_side, CaptionSide::Bottom);
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
    assert_eq!(paragraph.color, Some(Rgba::new(255, 0, 0, 255)));
    assert_eq!(paragraph.background, Some(Rgb::new(0, 128, 0)));
    assert!(paragraph.bold && paragraph.underline && paragraph.strike);
    assert_eq!(styles.get(em).color, Some(Rgba::new(0, 0, 255, 255)));
}

#[test]
fn foreground_alpha_survives_supported_color_forms_and_inheritance() {
    let mut document = Document::new();
    let parent = document.insert_element(
        None,
        "div",
        ElementNs::Html,
        vec![Attr::plain("style", "color: rgba(255, 255, 255, 0.5)")],
    );
    let inherited = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
    let hsl = document.insert_element(
        Some(parent),
        "span",
        ElementNs::Html,
        vec![Attr::plain("style", "color: hsla(0, 100%, 50%, 0.25)")],
    );
    let hwb = document.insert_element(
        Some(parent),
        "span",
        ElementNs::Html,
        vec![Attr::plain("style", "color: hwb(120 0% 0% / 75%)")],
    );
    let transparent = document.insert_element(
        Some(parent),
        "span",
        ElementNs::Html,
        vec![Attr::plain("style", "color: transparent")],
    );
    let initial = document.insert_element(
        Some(parent),
        "span",
        ElementNs::Html,
        vec![Attr::plain("style", "color: initial")],
    );
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    assert_eq!(
        styles.get(parent).color,
        Some(Rgba::new(255, 255, 255, 128))
    );
    assert_eq!(styles.get(inherited).color, styles.get(parent).color);
    assert_eq!(styles.get(hsl).color, Some(Rgba::new(255, 0, 0, 64)));
    assert_eq!(styles.get(hwb).color, Some(Rgba::new(0, 255, 0, 191)));
    assert_eq!(styles.get(transparent).color, Some(Rgba::new(0, 0, 0, 0)));
    assert_eq!(styles.get(initial).color, None);
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
    let sheet = CssparserParser
        .parse("p { color: #00ff00 } span { color: oklch(0.5 0.1 200); background: not-a-color }");
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(span).color, Some(Rgba::new(0, 255, 0, 255)));
    assert_eq!(styles.get(span).background, None);
}

#[test]
fn color_inherits_while_background_does_not() {
    let mut document = Document::new();
    let div = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let p = document.insert_element(Some(div), "p", ElementNs::Html, vec![]);
    let sheet = CssparserParser.parse("div { color: #123456; background-color: #654321 }");
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    assert_eq!(styles.get(p).color, Some(Rgba::new(0x12, 0x34, 0x56, 255)));
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
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen().with_palette(palette));
    assert_eq!(styles.get(link).color, Some(Rgba::new(255, 255, 0, 255)));
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
        Some(Rgba::new(0, 255, 0, 255)),
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
        include_str!("../../../tests/fixtures/borders.html"),
        include_str!("../../../tests/fixtures/headings.html"),
        include_str!("../../../tests/fixtures/links.html"),
        include_str!("../../../tests/fixtures/margins.html"),
        include_str!("../../../tests/fixtures/pre.html"),
        include_str!("../../../tests/fixtures/wide.html"),
        include_str!("../../../tests/fixtures/lists.html"),
        include_str!("../../../tests/fixtures/generated.html"),
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
        .find(|id| styles.get(*id).color == Some(Rgba::new(255, 0, 0, 255)));
    assert!(heading.is_some(), "the h1 half of the list still matches");
}

#[test]
fn generated_content_inherits_from_its_originating_element() {
    let (_, styles, order) =
        cascade_source("<style>p { color: #00ff00 } p::before { content: 'x' }</style><p>body</p>");
    let pseudo = order
        .iter()
        .find_map(|id| styles.pseudo(*id, PseudoElement::Before))
        .expect("generated content");
    assert_eq!(pseudo.style.color, Some(Rgba::new(0, 255, 0, 255)));
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

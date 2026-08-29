use crate::core::dom::{Attr, Document, ElementNs};
use crate::core::style::{
    Cursor, Display, LegacyAlign, ListStyleType, Palette, Rgb, Rgba, TextAlign, VerticalAlign,
    WhiteSpace,
};
use stylo_dom::ElementState;

use super::{Both, both, both_with_palette, custom_cascade, stylo_cascade};

fn control(tag: &str, attrs: Vec<Attr>) -> super::Both {
    both("", &[(tag, attrs)])
}

#[test]
fn structural_user_agent_styles_match_as_complete_styles() {
    let styles = both(
        "",
        &[
            ("p", vec![]),
            ("pre", vec![]),
            ("strong", vec![]),
            ("table", vec![]),
            ("caption", vec![]),
            ("hr", vec![]),
        ],
    );
    for index in 0..6 {
        styles.agree_all(index);
    }
}

#[test]
fn form_control_defaults_match_for_text_non_text_and_hidden_states() {
    for kind in [
        "", "unknown", "text", "password", "checkbox", "submit", "file", "hidden",
    ] {
        let attrs = if !kind.is_empty() {
            vec![Attr::plain("type", kind)]
        } else {
            Vec::new()
        };
        let both = control("input", attrs);
        both.agree_all(0);
    }
    for (tag, display, cursor, align) in [
        ("textarea", Display::BLOCK, Cursor::Text, TextAlign::Start),
        (
            "select",
            Display::INLINE,
            Cursor::Pointer,
            TextAlign::Center,
        ),
        (
            "button",
            Display::INLINE,
            Cursor::Pointer,
            TextAlign::Center,
        ),
    ] {
        let both = control(tag, vec![]);
        both.agree_all(0);
        let style = both.stylo(0);
        assert_eq!(style.display, display);
        assert_eq!(style.cursor, cursor);
        assert_eq!(style.text_align, align);
        assert_eq!(style.white_space, WhiteSpace::Pre);
    }
}

#[test]
fn supported_presentational_hints_enter_the_same_cascade_position() {
    let styles = both(
        "",
        &[
            ("div", vec![Attr::plain("align", "center")]),
            ("td", vec![Attr::plain("valign", "top")]),
            ("font", vec![Attr::plain("color", "#123456")]),
            ("ol", vec![Attr::plain("type", "A")]),
        ],
    );
    for index in 0..4 {
        styles.agree_all(index);
    }
    assert_eq!(styles.stylo(0).legacy_align, LegacyAlign::Center);
    assert_eq!(styles.stylo(1).vertical_align, VerticalAlign::Top);

    let overridden = both(
        "div { text-align: center }",
        &[("div", vec![Attr::plain("align", "center")])],
    );
    overridden.agree_all(0);
    assert_eq!(overridden.stylo(0).legacy_align, LegacyAlign::None);
}

#[test]
fn author_rules_override_user_agent_and_zero_specificity_hints() {
    let ua = both("p { display: inline }", &[("p", vec![])]);
    ua.agree_all(0);
    assert_eq!(ua.stylo(0).display, Display::INLINE);

    let hint = both(
        ":where(div) { text-align: right }",
        &[("div", vec![Attr::plain("align", "center")])],
    );
    hint.agree_all(0);
    assert_eq!(hint.stylo(0).text_align, TextAlign::Right);
    assert_eq!(hint.stylo(0).legacy_align, LegacyAlign::None);
}

#[test]
fn palette_links_are_user_origin_and_rebuilt_per_engine() {
    let palette = Palette {
        link: Rgb::new(10, 20, 30),
        link_hover: Rgb::new(40, 50, 60),
        ..Palette::default()
    };
    let link = both_with_palette("", &[("a", vec![Attr::plain("href", "/")])], palette);
    link.agree_all(0);
    assert_eq!(link.stylo(0).color, Some(Rgba::new(10, 20, 30, 255)));

    let authored = both_with_palette(
        "a { color: #090807 }",
        &[("a", vec![Attr::plain("href", "/")])],
        palette,
    );
    authored.agree_all(0);
    assert_eq!(authored.stylo(0).color, Some(Rgba::new(9, 8, 7, 255)));

    let changed = both_with_palette(
        "",
        &[("a", vec![Attr::plain("href", "/")])],
        Palette {
            link: Rgb::new(7, 8, 9),
            ..palette
        },
    );
    assert_eq!(changed.stylo(0).color, Some(Rgba::new(7, 8, 9, 255)));
}

#[test]
fn palette_hover_colour_matches_direct_element_state() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let link = document.insert_element(
        Some(body),
        "a",
        ElementNs::Html,
        vec![Attr::plain("href", "/")],
    );
    let palette = Palette {
        link: Rgb::new(10, 20, 30),
        link_hover: Rgb::new(40, 50, 60),
        ..Palette::default()
    };
    let styles = super::stylo_cascade_with_palette_and_state(
        "",
        &document,
        palette,
        link,
        ElementState::HOVER,
    );
    assert_eq!(styles.get(link).color, Some(Rgba::new(40, 50, 60, 255)));
}

#[test]
fn html_user_agent_selectors_do_not_style_foreign_namespaces() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let html_p = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
    let svg_p = document.insert_element(Some(body), "p", ElementNs::Svg, vec![]);
    let styles = Both {
        custom: custom_cascade("", &document),
        stylo: stylo_cascade("", &document),
        ids: vec![html_p, svg_p],
    };
    styles.agree_all(0);
    styles.agree_all(1);
    assert_eq!(styles.stylo(0).display, Display::BLOCK);
    assert_eq!(styles.stylo(1).display, Display::INLINE);
}

#[test]
fn table_projection_and_heading_alignment_match() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let table = document.insert_element(
        Some(body),
        "table",
        ElementNs::Html,
        vec![Attr::plain("border", "1"), Attr::plain("cellpadding", "8")],
    );
    let row = document.insert_element(Some(table), "tr", ElementNs::Html, vec![]);
    let cell = document.insert_element(Some(row), "td", ElementNs::Html, vec![]);
    let heading = document.insert_element(Some(row), "th", ElementNs::Html, vec![]);
    let styles = Both {
        custom: custom_cascade("table { text-align: right }", &document),
        stylo: stylo_cascade("table { text-align: right }", &document),
        ids: vec![cell, heading],
    };
    styles.agree_all(0);
    styles.agree_all(1);
    assert_eq!(styles.stylo(1).text_align, TextAlign::Right);
}

#[test]
fn nested_list_standardization_is_an_explicit_bridge_divergence() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let first = document.insert_element(Some(body), "ul", ElementNs::Html, vec![]);
    let second = document.insert_element(Some(first), "ul", ElementNs::Html, vec![]);
    let third = document.insert_element(Some(second), "ul", ElementNs::Html, vec![]);
    let fourth = document.insert_element(Some(third), "ul", ElementNs::Html, vec![]);
    let custom = custom_cascade("", &document);
    let stylo = stylo_cascade("", &document);
    assert_eq!(custom.get(fourth).list_style_type, ListStyleType::Disc);
    assert_eq!(stylo.get(fourth).list_style_type, ListStyleType::Square);
}

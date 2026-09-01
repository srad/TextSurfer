use super::contrast::MIN_CONTRAST;
use super::*;
use crate::core::dom::{Document, ElementNs};
use crate::core::style::{BorderColor, BorderEdges, BorderLineStyle, BorderSide, Rgb, Rgba};
use crate::layout::{BackgroundFill, BorderStroke, LayoutBox, TextFragment};

fn painted(tree: &BoxTree) -> DisplayList {
    BasicPainter.paint(tree, Palette::default())
}

fn plain_box(node: NodeId, rect: LayoutRect, depth: usize) -> LayoutBox {
    LayoutBox {
        node,
        paint_source: crate::layout::engine::PaintStyleSource::Element(node),
        background_handled: false,
        border_rect: rect,
        content_rect: rect,
        depth,
        style: CellStyle::default(),
    }
}

#[test]
fn image_and_scaled_text_overlays_follow_document_paint_order() {
    let mut document = Document::new();
    let image_node = document.insert_element(None, "img", ElementNs::Html, vec![]);
    let text_node = document.insert_element(None, "span", ElementNs::Html, vec![]);
    let rect = LayoutRect {
        col: 0,
        row: 0,
        width: 2,
        height: 2,
    };
    let tree = BoxTree {
        width: 2,
        height: 2,
        fragments: vec![TextFragment {
            node: text_node,
            col: 0,
            row: 0,
            text: "X".to_string(),
            depth: 1,
            style: CellStyle {
                scale: 2,
                ..Default::default()
            },
        }],
        images: vec![crate::layout::ImagePlacement {
            node: image_node,
            asset_id: crate::core::image::ImageAssetId(1),
            revision: 1,
            rect,
            clip: rect,
            depth: 1,
        }],
        paint_order: HashMap::from([(image_node, 0), (text_node, 1)]),
        ..Default::default()
    };
    let painted = painted(&tree);
    assert_eq!(
        painted.overlays,
        vec![PaintOverlay::Image(0), PaintOverlay::ScaledText(0)]
    );
    assert_eq!(painted.hit_test(0, 0), Some(text_node));
}

#[test]
fn paints_wide_graphemes_without_exceeding_the_cell_width() {
    let mut document = Document::new();
    let node = document.insert_element(None, "p", ElementNs::Html, vec![]);
    let tree = BoxTree {
        width: 4,
        height: 1,
        fragments: vec![TextFragment {
            node,
            col: 1,
            row: 0,
            text: "界x".to_string(),
            depth: 0,
            style: CellStyle::default(),
        }],
        ..Default::default()
    };
    assert_eq!(painted(&tree).text_lines(), vec![" 界x"]);
}

#[test]
fn scaled_text_is_reserved_and_emitted_as_an_explicit_run() {
    let mut document = Document::new();
    let node = document.insert_element(None, "a", ElementNs::Html, vec![]);
    let style = CellStyle {
        fg: Some(Rgba::opaque(Rgb::WHITE)),
        bg: Some(Rgb::new(0, 0, 128)),
        scale: 2,
        underline: true,
        strike: true,
        ..Default::default()
    };
    let rect = LayoutRect {
        col: 1,
        row: 1,
        width: 2,
        height: 2,
    };
    let tree = BoxTree {
        width: 5,
        height: 4,
        fragments: vec![TextFragment {
            node,
            col: rect.col,
            row: rect.row,
            text: "A".to_string(),
            depth: 3,
            style,
        }],
        links: vec![crate::layout::LinkBox {
            node,
            href: "https://example.com".to_string(),
            rects: vec![rect],
            hit_nodes: vec![node],
        }],
        ..Default::default()
    };
    let display = painted(&tree);
    assert_eq!(display.scaled_text.len(), 1);
    assert_eq!(display.scaled_text[0].rect, rect);
    assert!(
        display.rows[1]
            .spans
            .iter()
            .any(|span| span.style.bg == style.bg)
    );
    assert!(
        display.rows[2]
            .spans
            .iter()
            .any(|span| span.style.bg == style.bg)
    );
    assert!(display.link_at(2, 2).is_some());
    assert!(
        display.scaled_text[0]
            .style
            .fg
            .is_some_and(|foreground| foreground.alpha == 255)
    );
    assert!(display.scaled_text[0].style.underline);
    assert!(display.scaled_text[0].style.strike);
    assert!(
        display.rows[1]
            .spans
            .iter()
            .all(|span| !span.style.underline && !span.style.strike)
    );
}

#[test]
fn hit_testing_returns_the_deepest_box() {
    let mut document = Document::new();
    let parent = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let child = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
    let parent_rect = LayoutRect {
        col: 0,
        row: 0,
        width: 10,
        height: 3,
    };
    let child_rect = LayoutRect {
        col: 1,
        row: 1,
        width: 4,
        height: 1,
    };
    let tree = BoxTree {
        width: 10,
        height: 3,
        boxes: vec![
            plain_box(parent, parent_rect, 0),
            plain_box(child, child_rect, 1),
        ],
        ..Default::default()
    };
    let display = painted(&tree);
    assert_eq!(display.hit_test(2, 1), Some(child));
    assert_eq!(display.hit_test(8, 1), Some(parent));
}

#[test]
fn text_fragments_win_over_a_containing_box_at_equal_depth() {
    let mut document = Document::new();
    let paragraph = document.insert_element(None, "p", ElementNs::Html, vec![]);
    let text = document.insert_text(Some(paragraph), "inline");
    let rect = LayoutRect {
        col: 1,
        row: 0,
        width: 6,
        height: 1,
    };
    let tree = BoxTree {
        width: 8,
        height: 1,
        boxes: vec![plain_box(paragraph, rect, 0)],
        fragments: vec![TextFragment {
            node: text,
            col: rect.col,
            row: rect.row,
            text: "inline".to_string(),
            depth: 0,
            style: CellStyle::default(),
        }],
        ..Default::default()
    };
    assert_eq!(painted(&tree).hit_test(2, 0), Some(text));
}

#[test]
fn overwriting_a_wide_grapheme_never_leaves_an_overwide_row() {
    let mut document = Document::new();
    let back = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let front = document.insert_element(None, "span", ElementNs::Html, vec![]);
    let tree = BoxTree {
        width: 2,
        height: 1,
        fragments: vec![
            TextFragment {
                node: back,
                col: 0,
                row: 0,
                text: "界".to_string(),
                depth: 0,
                style: CellStyle::default(),
            },
            TextFragment {
                node: front,
                col: 1,
                row: 0,
                text: "x".to_string(),
                depth: 1,
                style: CellStyle::default(),
            },
        ],
        ..Default::default()
    };
    let lines = painted(&tree).text_lines();
    assert_eq!(lines, vec![" x"]);
    assert_eq!(UnicodeWidthStr::width(lines[0].as_str()), 2);
}

#[test]
fn borders_are_drawn_from_box_geometry() {
    let mut document = Document::new();
    let node = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let rect = LayoutRect {
        col: 1,
        row: 0,
        width: 4,
        height: 3,
    };
    let tree = BoxTree {
        width: 6,
        height: 3,
        boxes: vec![LayoutBox {
            node,
            paint_source: crate::layout::engine::PaintStyleSource::Element(node),
            background_handled: false,
            border_rect: rect,
            content_rect: LayoutRect {
                col: 2,
                row: 1,
                width: 2,
                height: 1,
            },
            depth: 0,
            style: CellStyle::default(),
        }],
        strokes: vec![BorderStroke {
            rect,
            edges: BorderEdges::uniform(BorderSide {
                style: BorderLineStyle::Solid,
                ..Default::default()
            }),
            current_color: None,
            source_node: None,
            source_edge: None,
            depth: 0,
            merge_group: 1,
        }],
        ..Default::default()
    };
    assert_eq!(painted(&tree).text_lines(), vec![" ┌──┐", " │  │", " └──┘"]);
}

#[test]
fn border_ink_preserves_the_painted_background_and_drops_text_modifiers() {
    let rect = LayoutRect {
        col: 0,
        row: 0,
        width: 4,
        height: 3,
    };
    let background = Rgb::new(0, 64, 0);
    let tree = BoxTree {
        width: 4,
        height: 3,
        fills: vec![BackgroundFill {
            rect,
            color: Some(background),
            depth: 0,
            paint_source: crate::layout::engine::PaintStyleSource::Missing,
        }],
        strokes: vec![BorderStroke {
            rect,
            edges: BorderEdges::uniform(BorderSide {
                color: BorderColor::CurrentColor,
                style: BorderLineStyle::Solid,
                ..Default::default()
            }),
            current_color: Some(Rgba::opaque(Rgb::BLACK)),
            source_node: None,
            source_edge: None,
            depth: 0,
            merge_group: 1,
        }],
        ..Default::default()
    };
    let display = painted(&tree);
    for span in display.rows.iter().flat_map(|row| &row.spans) {
        assert_eq!(span.style.bg, Some(background));
        assert!(!span.style.bold);
        assert!(!span.style.underline);
        assert!(!span.style.strike);
        assert!(!span.style.reverse);
        assert!(!span.style.dim);
        assert_eq!(span.style.scale, 1);
    }
}

#[test]
fn untouched_document_rows_do_not_require_dense_cell_buffers() {
    let tree = BoxTree {
        width: 200,
        height: 20_000,
        ..Default::default()
    };
    let display = painted(&tree);
    assert_eq!(display.rows.len(), 20_000);
    assert!(display.rows.iter().all(|row| row.spans.is_empty()));
}

#[test]
fn backgrounds_paint_under_text_in_depth_order() {
    let mut document = Document::new();
    let outer = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let inner = document.insert_element(Some(outer), "span", ElementNs::Html, vec![]);
    let outer_rect = LayoutRect {
        col: 0,
        row: 0,
        width: 4,
        height: 1,
    };
    let inner_rect = LayoutRect {
        col: 2,
        row: 0,
        width: 2,
        height: 1,
    };
    let tree = BoxTree {
        width: 4,
        height: 1,
        boxes: vec![
            LayoutBox {
                style: CellStyle {
                    bg: Some(Rgb::new(10, 10, 10)),
                    ..Default::default()
                },
                ..plain_box(outer, outer_rect, 0)
            },
            LayoutBox {
                style: CellStyle {
                    bg: Some(Rgb::new(20, 20, 20)),
                    ..Default::default()
                },
                ..plain_box(inner, inner_rect, 1)
            },
        ],
        fills: vec![
            BackgroundFill {
                rect: outer_rect,
                color: Some(Rgb::new(10, 10, 10)),
                depth: 0,
                paint_source: crate::layout::engine::PaintStyleSource::Missing,
            },
            BackgroundFill {
                rect: inner_rect,
                color: Some(Rgb::new(20, 20, 20)),
                depth: 1,
                paint_source: crate::layout::engine::PaintStyleSource::Missing,
            },
        ],
        fragments: vec![TextFragment {
            node: inner,
            col: 2,
            row: 0,
            text: "hi".to_string(),
            depth: 1,
            style: CellStyle {
                fg: Some(Rgba::opaque(Rgb::WHITE)),
                ..Default::default()
            },
        }],
        ..Default::default()
    };
    let display = painted(&tree);
    let spans = &display.rows[0].spans;
    assert_eq!(spans[0].style.bg, Some(Rgb::new(10, 10, 10)));
    assert_eq!(spans[1].text, "hi");
    assert_eq!(spans[1].style.bg, Some(Rgb::new(20, 20, 20)));
    assert_eq!(spans[1].style.fg, Some(Rgba::opaque(Rgb::WHITE)));
}

#[test]
fn partial_foreground_alpha_resolves_against_the_deepest_background() {
    let mut document = Document::new();
    let outer = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let inner = document.insert_element(Some(outer), "span", ElementNs::Html, vec![]);
    let outer_rect = LayoutRect {
        col: 0,
        row: 0,
        width: 8,
        height: 1,
    };
    let inner_rect = LayoutRect {
        col: 1,
        row: 0,
        width: 6,
        height: 1,
    };
    let tree = BoxTree {
        width: 8,
        height: 1,
        fills: vec![
            BackgroundFill {
                rect: outer_rect,
                color: Some(Rgb::new(0, 0, 128)),
                depth: 0,
                paint_source: crate::layout::engine::PaintStyleSource::Missing,
            },
            BackgroundFill {
                rect: inner_rect,
                color: Some(Rgb::BLACK),
                depth: 1,
                paint_source: crate::layout::engine::PaintStyleSource::Missing,
            },
        ],
        fragments: vec![TextFragment {
            node: inner,
            col: 2,
            row: 0,
            text: "half".to_string(),
            depth: 1,
            style: CellStyle {
                fg: Some(Rgba::new(255, 255, 255, 128)),
                ..Default::default()
            },
        }],
        ..Default::default()
    };
    let display = painted(&tree);
    let span = display.rows[0]
        .spans
        .iter()
        .find(|span| span.text == "half")
        .unwrap();
    assert_eq!(span.style.bg, Some(Rgb::BLACK));
    assert_eq!(span.style.fg, Some(Rgba::new(128, 128, 128, 255)));
    assert!(display.rows.iter().flat_map(|row| &row.spans).all(|span| {
        span.style
            .fg
            .is_none_or(|foreground| foreground.alpha == 255)
    }));
}

#[test]
fn transparent_text_loses_ink_but_keeps_layout_and_interaction_geometry() {
    let mut document = Document::new();
    let hidden = document.insert_element(None, "a", ElementNs::Html, vec![]);
    let visible = document.insert_element(None, "span", ElementNs::Html, vec![]);
    let hidden_rect = LayoutRect {
        col: 0,
        row: 0,
        width: 6,
        height: 1,
    };
    let visible_rect = LayoutRect {
        col: 6,
        row: 0,
        width: 5,
        height: 1,
    };
    let tree = BoxTree {
        width: 11,
        height: 1,
        boxes: vec![
            plain_box(hidden, hidden_rect, 0),
            plain_box(visible, visible_rect, 0),
        ],
        fragments: vec![
            TextFragment {
                node: hidden,
                col: 0,
                row: 0,
                text: "secret".to_string(),
                depth: 0,
                style: CellStyle {
                    fg: Some(Rgba::new(255, 255, 255, 0)),
                    bold: true,
                    underline: true,
                    strike: true,
                    reverse: true,
                    ..Default::default()
                },
            },
            TextFragment {
                node: visible,
                col: 6,
                row: 0,
                text: "after".to_string(),
                depth: 0,
                style: CellStyle {
                    fg: Some(Rgba::opaque(Rgb::WHITE)),
                    ..Default::default()
                },
            },
        ],
        links: vec![crate::layout::LinkBox {
            node: hidden,
            href: "https://example.com/".to_string(),
            rects: vec![hidden_rect],
            hit_nodes: vec![hidden],
        }],
        ..Default::default()
    };
    let display = painted(&tree);
    assert_eq!(display.text_lines()[0], "      after");
    assert_eq!(display.hit_test(2, 0), Some(hidden));
    assert_eq!(
        display.link_at(2, 0).map(|link| link.href.as_str()),
        Some("https://example.com/")
    );
    assert!(display.rows[0].spans.iter().all(|span| {
        span.text != "secret"
            && (!span.text.chars().all(char::is_whitespace)
                || (!span.style.bold
                    && !span.style.underline
                    && !span.style.strike
                    && !span.style.reverse))
    }));
}

#[test]
fn unreadable_author_colours_are_corrected_towards_the_theme_text() {
    let palette = Palette {
        text: Rgb::WHITE,
        background: Rgb::new(0, 0, 128),
        link: Rgb::new(255, 255, 0),
        link_hover: Rgb::new(255, 255, 0),
    };
    let corrected = legible_foreground(Rgb::BLACK, palette.background, palette);
    assert!(corrected.contrast_ratio(palette.background) >= MIN_CONTRAST);
    let legible = Rgb::new(255, 255, 0);
    assert_eq!(
        legible_foreground(legible, palette.background, palette),
        legible
    );
}

#[test]
fn links_carry_their_geometry_into_the_display_list() {
    let mut document = Document::new();
    let node = document.insert_element(None, "a", ElementNs::Html, vec![]);
    let rect = LayoutRect {
        col: 0,
        row: 0,
        width: 4,
        height: 1,
    };
    let tree = BoxTree {
        width: 8,
        height: 1,
        links: vec![crate::layout::LinkBox {
            node,
            href: "https://example.com/".to_string(),
            rects: vec![rect],
            hit_nodes: Vec::new(),
        }],
        ..Default::default()
    };
    let display = painted(&tree);
    assert_eq!(display.links.len(), 1);
    assert_eq!(
        display.link_at(2, 0).map(|link| link.href.as_str()),
        Some("https://example.com/")
    );
    assert!(display.link_at(6, 0).is_none());
}

#[test]
fn topmost_paint_order_is_shared_by_hit_testing_and_link_activation() {
    let mut document = Document::new();
    let link = document.insert_element(None, "a", ElementNs::Html, vec![]);
    let overlay = document.insert_element(None, "span", ElementNs::Html, vec![]);
    let rect = LayoutRect {
        col: 0,
        row: 0,
        width: 4,
        height: 1,
    };
    let tree = BoxTree {
        width: 4,
        height: 1,
        fragments: vec![
            TextFragment {
                node: link,
                col: 0,
                row: 0,
                text: "link".to_string(),
                depth: 0,
                style: CellStyle::default(),
            },
            TextFragment {
                node: overlay,
                col: 0,
                row: 0,
                text: "top!".to_string(),
                depth: 1,
                style: CellStyle::default(),
            },
        ],
        links: vec![crate::layout::LinkBox {
            node: link,
            href: "https://example.com/".to_string(),
            rects: vec![rect],
            hit_nodes: vec![link],
        }],
        ..Default::default()
    };
    let display = painted(&tree);
    assert_eq!(display.hit_test(2, 0), Some(overlay));
    assert!(display.link_at(2, 0).is_none());
    assert_eq!(display.hit_rows.first().map(Vec::len), Some(2));
}

#[test]
fn plain_text_becomes_an_unstyled_display_list() {
    let lines = vec!["first".to_string(), String::new(), "third".to_string()];
    let display = DisplayList::from_lines(&lines);
    assert_eq!(display.text_lines(), lines);
    assert!(display.hits.is_empty() && display.links.is_empty());
}

#[test]
fn an_invalid_display_patch_is_atomic() {
    let mut display = DisplayList::from_lines(&["before".to_string()]);
    let baseline = display.clone();
    let patch = DisplayPatch {
        rows: vec![
            (
                0,
                DisplayList::from_lines(&["after".to_string()])
                    .rows
                    .remove(0),
            ),
            (1, PaintedRow::default()),
        ],
        scaled_text: Vec::new(),
    };
    assert!(!patch.fits(&display));
    assert!(!patch.apply(&mut display));
    assert_eq!(display, baseline);
}

#[test]
fn display_patch_damage_is_sorted_and_disjoint() {
    let patch = DisplayPatch {
        rows: vec![
            (4, PaintedRow::default()),
            (1, PaintedRow::default()),
            (2, PaintedRow::default()),
        ],
        scaled_text: Vec::new(),
    };
    assert_eq!(patch.changed_rows(), vec![1..3, 4..5]);
}

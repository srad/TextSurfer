use super::*;
use crate::core::dom::ElementNs;
use crate::core::style::{
    BorderEdges, BorderLineStyle, BorderSide, ComputedStyle, Display, Palette,
};
use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};
use crate::paint::{BasicPainter, Painter};
use proptest::prelude::*;

fn paragraph(text: &str) -> (Document, StyleTree) {
    let mut document = Document::new();
    let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
    document.insert_text(Some(p), text);
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    (document, styles)
}

fn fragment_text(tree: &BoxTree) -> Vec<&str> {
    tree.fragments
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect()
}

fn nested_document(text: &str, depth: usize) -> (Document, StyleTree) {
    let mut document = Document::new();
    let body = document.insert_element(None, "body", ElementNs::Html, vec![]);
    let mut parent = body;
    for _ in 0..depth {
        parent = document.insert_element(
            Some(parent),
            "div",
            ElementNs::Html,
            vec![crate::core::dom::Attr::plain("style", "padding-left: 1ch")],
        );
    }
    let p = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
    document.insert_text(Some(p), text);
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    (document, styles)
}

fn covers(outer: LayoutRect, inner: LayoutRect) -> bool {
    inner.col >= outer.col
        && inner.col + inner.width <= outer.col + outer.width
        && inner.row >= outer.row
        && inner.row + inner.height <= outer.row + outer.height
}

fn disjoint(a: LayoutRect, b: LayoutRect) -> bool {
    a.col + a.width <= b.col
        || b.col + b.width <= a.col
        || a.row + a.height <= b.row
        || b.row + b.height <= a.row
}

proptest! {
    #[test]
    fn wider_viewports_never_increase_single_block_height(
        text in "[a-z ]{0,200}",
        narrow in 1u16..40,
        extra in 0u16..40,
    ) {
        let (document, styles) = paragraph(&text);
        let narrow_tree = TaffyLayoutEngine.layout(
            &document,
            &styles,
            Size { cols: narrow, rows: 5 },
        );
        let wide_tree = TaffyLayoutEngine.layout(
            &document,
            &styles,
            Size { cols: narrow + extra, rows: 5 },
        );
        prop_assert!(wide_tree.height <= narrow_tree.height);
    }

    #[test]
    fn painted_rows_never_exceed_the_viewport(
        chars in prop::collection::vec(any::<char>(), 0..100),
        width in 1u16..80,
    ) {
        let text: String = chars.into_iter().collect();
        let (document, styles) = paragraph(&text);
        let tree = TaffyLayoutEngine.layout(
            &document,
            &styles,
            Size { cols: width, rows: 5 },
        );
        let display = BasicPainter.paint(&tree, Palette::default());
        for line in display.text_lines() {
            prop_assert!(UnicodeWidthStr::width(line.as_str()) <= usize::from(width));
        }
    }

    #[test]
    fn leaf_text_cells_never_overlap_within_a_row(
        text in "[a-z ]{0,120}",
        width in 4u16..40,
    ) {
        let (document, styles) = paragraph(&text);
        let tree = TaffyLayoutEngine.layout(
            &document,
            &styles,
            Size { cols: width, rows: 5 },
        );
        let mut occupied: Vec<(usize, usize, usize)> = Vec::new();
        for fragment in &tree.fragments {
            let span = UnicodeWidthStr::width(fragment.text.as_str());
            for (row, from, to) in &occupied {
                if *row == fragment.row {
                    prop_assert!(fragment.col >= *to || fragment.col + span <= *from);
                }
            }
            occupied.push((fragment.row, fragment.col, fragment.col + span));
        }
    }

    #[test]
    fn scaled_text_rectangles_never_overlap(
        text in "[a-z ]{0,80}",
        width in 8u16..60,
    ) {
        let mut document = Document::new();
        let heading = document.insert_element(None, "h1", ElementNs::Html, vec![]);
        document.insert_text(Some(heading), &text);
        let styles = BasicCascade.apply(
            &[],
            &document,
            MediaContext::screen().with_text_rendering(crate::core::style::TextRendering::ScaledBitmap),
        );
        let tree = TaffyLayoutEngine.layout(
            &document,
            &styles,
            Size { cols: width, rows: 20 },
        );
        for (index, left) in tree.fragments.iter().enumerate() {
            for right in tree.fragments.iter().skip(index + 1) {
                prop_assert!(disjoint(left.rect(), right.rect()));
            }
        }
    }

    #[test]
    fn boxes_form_a_laminar_family(
        text in "[a-z ]{0,80}",
        depth in 0usize..6,
        width in 6u16..40,
    ) {
        let (document, styles) = nested_document(&text, depth);
        let tree = TaffyLayoutEngine.layout(
            &document,
            &styles,
            Size { cols: width, rows: 5 },
        );
        for (index, outer) in tree.boxes.iter().enumerate() {
            for inner in &tree.boxes[index + 1..] {
                let a = outer.border_rect;
                let b = inner.border_rect;
                if a.width == 0 || a.height == 0 || b.width == 0 || b.height == 0 {
                    continue;
                }
                prop_assert!(covers(a, b) || covers(b, a) || disjoint(a, b));
            }
        }
    }

    #[test]
    fn hit_testing_round_trips_to_the_deepest_engine_box(
        text in "[a-z ]{1,80}",
        depth in 0usize..5,
        width in 8u16..40,
    ) {
        let (document, styles) = nested_document(&text, depth);
        let tree = TaffyLayoutEngine.layout(
            &document,
            &styles,
            Size { cols: width, rows: 5 },
        );
        let display = BasicPainter.paint(&tree, Palette::default());
        for layout_box in &tree.boxes {
            let rect = layout_box.border_rect;
            if rect.width == 0 || rect.height == 0 {
                continue;
            }
            let deepest = tree
                .boxes
                .iter()
                .filter(|candidate| {
                    let other = candidate.border_rect;
                    rect.col >= other.col
                        && rect.col < other.col + other.width
                        && rect.row >= other.row
                        && rect.row < other.row + other.height
                })
                .max_by_key(|candidate| candidate.depth)
                .unwrap();
            prop_assert_eq!(display.hit_test(rect.col, rect.row), Some(deepest.node));
        }
    }
}

#[test]
fn block_text_wraps_to_the_terminal_width_and_hidden_nodes_disappear() {
    let mut document = Document::new();
    let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
    let head = document.insert_element(Some(html), "head", ElementNs::Html, vec![]);
    document.insert_text(Some(head), "hidden");
    let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
    let p = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
    document.insert_text(Some(p), "one two three four");
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 9, rows: 5 });
    assert_eq!(fragment_text(&tree), vec!["one two", "three", "four"]);
    assert!(
        !fragment_text(&tree)
            .iter()
            .any(|line| line.contains("hidden"))
    );
}

#[test]
fn long_words_break_to_terminal_width_even_when_author_css_requests_normal_wrapping() {
    let mut document = Document::new();
    let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
    document.insert_text(Some(p), "abcdefghijk");
    let sheet = CssparserParser.parse("p { overflow-wrap: normal }");
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 4, rows: 5 });
    assert_eq!(fragment_text(&tree), vec!["abcd", "efgh", "ijk"]);
}

#[test]
fn block_min_content_width_is_the_widest_unbreakable_word() {
    let (document, styles) = paragraph("small elephant ox");
    let flow = build_flow_tree(&document, &styles, 40);
    let inline = flow.iter().find(|box_| !box_.inline.is_empty()).unwrap();
    assert_eq!(min_content_width(&inline.inline), "elephant".len());
}

#[test]
fn mixed_preformatted_whitespace_stays_in_its_unbreakable_segment() {
    let mut document = Document::new();
    let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
    document.insert_text(Some(p), "aa");
    let pre = document.insert_element(
        Some(p),
        "span",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain("style", "white-space: pre")],
    );
    document.insert_text(Some(pre), " \t ");
    document.insert_text(Some(p), "bb");
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let flow = build_flow_tree(&document, &styles, 40);
    let inline = flow.iter().find(|box_| !box_.inline.is_empty()).unwrap();
    assert_eq!(min_content_width(&inline.inline), 11);
}

#[test]
fn a_wrapping_tab_breaks_after_its_first_expanded_space() {
    let mut document = Document::new();
    let p = document.insert_element(
        None,
        "p",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain(
            "style",
            "white-space: pre-wrap",
        )],
    );
    document.insert_text(Some(p), "aa\tbb");
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let flow = build_flow_tree(&document, &styles, 40);
    let inline = flow.iter().find(|box_| !box_.inline.is_empty()).unwrap();
    assert_eq!(min_content_width(&inline.inline), 3);
}

#[test]
fn preformatted_text_preserves_spaces_and_line_breaks() {
    let mut document = Document::new();
    let pre = document.insert_element(None, "pre", ElementNs::Html, vec![]);
    document.insert_text(Some(pre), "  a\n b");
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 4, rows: 5 });
    assert_eq!(fragment_text(&tree), vec!["  a", " b"]);
}

#[test]
fn images_render_their_alt_text_and_rules_span_the_content_width() {
    let mut document = Document::new();
    let body = document.insert_element(None, "body", ElementNs::Html, vec![]);
    let described = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
    document.insert_element(
        Some(described),
        "img",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain("alt", "a cat")],
    );
    let empty = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
    document.insert_text(Some(empty), "before");
    document.insert_element(
        Some(empty),
        "img",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain("alt", "")],
    );
    document.insert_text(Some(empty), "after");
    let whitespace = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
    document.insert_text(Some(whitespace), "left");
    document.insert_element(
        Some(whitespace),
        "img",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain("alt", "   ")],
    );
    document.insert_text(Some(whitespace), "right");
    document.insert_element(Some(body), "hr", ElementNs::Html, vec![]);
    let bare = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
    document.insert_element(Some(bare), "img", ElementNs::Html, vec![]);
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 12, rows: 5 });
    let lines = BasicPainter.paint(&tree, Palette::default()).text_lines();
    assert!(lines.iter().any(|line| line == "[a cat]"));
    assert!(lines.iter().any(|line| line == "beforeafter"));
    assert!(lines.iter().any(|line| line == "leftright"));
    assert_eq!(lines.iter().filter(|line| *line == "[img]").count(), 1);
    assert!(lines.iter().any(|line| line == "────────────"));
}

#[test]
fn accented_words_never_break_mid_word_while_wide_scripts_still_wrap() {
    let (document, styles) = paragraph("Grüße Grüße");
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 6, rows: 5 });
    let lines = BasicPainter.paint(&tree, Palette::default()).text_lines();
    assert_eq!(&lines[..2], ["Grüße", "Grüße"]);
    let (document, styles) = paragraph("日本語のテキスト");
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 6, rows: 5 });
    let lines = BasicPainter.paint(&tree, Palette::default()).text_lines();
    assert!(lines.len() > 1);
}

#[test]
fn inline_styles_travel_with_the_text_fragments() {
    let mut document = Document::new();
    let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
    document.insert_text(Some(p), "plain ");
    let strong = document.insert_element(Some(p), "strong", ElementNs::Html, vec![]);
    document.insert_text(Some(strong), "loud");
    let sheet = CssparserParser.parse("p { color: #112233 }");
    let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
    let plain = tree
        .fragments
        .iter()
        .find(|fragment| fragment.text.starts_with("plain"))
        .unwrap();
    let loud = tree
        .fragments
        .iter()
        .find(|fragment| fragment.text == "loud")
        .unwrap();
    assert_eq!(
        plain.style.fg,
        Some(crate::core::style::Rgba::new(17, 34, 51, 255))
    );
    assert!(!plain.style.bold);
    assert!(loud.style.bold);
    assert_eq!(loud.style.fg, plain.style.fg);
}

#[test]
fn links_are_discovered_inside_inline_content() {
    let mut document = Document::new();
    let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
    let link = document.insert_element(
        Some(p),
        "a",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain("href", "/next")],
    );
    document.insert_text(Some(link), "next");
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
    assert_eq!(tree.links.len(), 1);
    assert_eq!(tree.links[0].node, link);
    assert_eq!(tree.links[0].href, "/next");
    assert_eq!(
        tree.links[0].rects,
        vec![LayoutRect {
            col: 0,
            row: 0,
            width: 4,
            height: 1
        }]
    );
}

#[test]
fn deeply_nested_inline_content_is_walked_iteratively() {
    let mut document = Document::new();
    let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
    let mut parent = p;
    for _ in 0..4096 {
        parent = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
    }
    document.insert_text(Some(parent), "deep");
    let mut styles = StyleTree::default();
    styles.insert(
        p,
        ComputedStyle {
            display: Display::BLOCK,
            ..Default::default()
        },
    );
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
    assert_eq!(fragment_text(&tree), vec!["deep"]);
}

#[test]
fn bordered_wide_text_keeps_the_right_border_in_its_cell() {
    let mut document = Document::new();
    let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
    document.insert_text(Some(p), "界x");
    let mut styles = StyleTree::default();
    styles.insert(
        p,
        ComputedStyle {
            display: Display::BLOCK,
            border: BorderEdges::uniform(BorderSide {
                style: BorderLineStyle::Solid,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 6, rows: 5 });
    let lines = BasicPainter.paint(&tree, Palette::default()).text_lines();
    assert_eq!(lines[1], "│界x │");
    assert_eq!(UnicodeWidthStr::width(lines[1].as_str()), 6);
}

#[test]
fn mixed_inline_and_block_children_create_ordered_anonymous_runs() {
    let mut document = Document::new();
    let outer = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let before = document.insert_text(Some(outer), "before");
    let inner = document.insert_element(Some(outer), "p", ElementNs::Html, vec![]);
    let inside = document.insert_text(Some(inner), "inside");
    let after = document.insert_text(Some(outer), "after");
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 1 });
    assert_eq!(
        tree.fragments
            .iter()
            .map(|fragment| fragment.node)
            .collect::<Vec<_>>(),
        vec![before, inside, after]
    );
    assert_eq!(fragment_text(&tree), vec!["before", "inside", "after"]);
    let outer_box = tree
        .boxes
        .iter()
        .find(|layout_box| layout_box.node == outer)
        .unwrap();
    let inner_box = tree
        .boxes
        .iter()
        .find(|layout_box| layout_box.node == inner)
        .unwrap();
    assert!(inner_box.border_rect.row >= outer_box.content_rect.row);
    assert!(
        inner_box.border_rect.row + inner_box.border_rect.height
            <= outer_box.content_rect.row + outer_box.content_rect.height
    );
}

#[test]
fn display_contents_keeps_order_and_inheritance_without_a_principal_box() {
    let mut document = Document::new();
    let outer = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let before = document.insert_text(Some(outer), "before ");
    let contents = document.insert_element(
        Some(outer),
        "a",
        ElementNs::Html,
        vec![
            crate::core::dom::Attr::plain(
                "style",
                "display: contents; color: red; background: blue; font-weight: bold; padding: 2ch",
            ),
            crate::core::dom::Attr::plain("href", "/kept"),
        ],
    );
    let inside = document.insert_text(Some(contents), "inside");
    let after = document.insert_text(Some(outer), " after");
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 40, rows: 1 });
    assert_eq!(
        tree.fragments
            .iter()
            .map(|fragment| fragment.node)
            .collect::<Vec<_>>(),
        vec![before, inside, after]
    );
    assert!(
        !tree
            .boxes
            .iter()
            .any(|layout_box| layout_box.node == contents)
    );
    let inside_fragment = tree
        .fragments
        .iter()
        .find(|fragment| fragment.node == inside)
        .unwrap();
    assert_eq!(
        inside_fragment.style.fg,
        Some(crate::core::style::Rgba::new(255, 0, 0, 255))
    );
    assert!(inside_fragment.style.bold);
    assert_eq!(inside_fragment.style.bg, None);
    assert_eq!(tree.links.len(), 1);
    assert_eq!(tree.links[0].node, contents);
    assert_eq!(tree.links[0].href, "/kept");
}

#[test]
fn inline_flow_root_is_an_atomic_inline_box_without_forced_breaks() {
    let mut document = Document::new();
    let outer = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let before = document.insert_text(Some(outer), "before ");
    let atom = document.insert_element(
        Some(outer),
        "span",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain(
            "style",
            "display: inline-block; width: 6ch; margin: 0 1ch; border: solid",
        )],
    );
    let first_block = document.insert_element(
        Some(atom),
        "div",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain("style", "display: flow-root")],
    );
    let first_inside = document.insert_text(Some(first_block), "one");
    let last_block = document.insert_element(
        Some(atom),
        "div",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain("style", "display: grid")],
    );
    let last_inside = document.insert_text(Some(last_block), "two");
    let after = document.insert_text(Some(outer), " after");
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 40, rows: 1 });
    let atom_box = tree
        .boxes
        .iter()
        .find(|layout_box| layout_box.node == atom)
        .unwrap();
    assert_eq!(atom_box.border_rect.width, 6);
    let before_fragment = tree
        .fragments
        .iter()
        .find(|fragment| fragment.node == before)
        .unwrap();
    let first_fragment = tree
        .fragments
        .iter()
        .find(|fragment| fragment.node == first_inside)
        .unwrap();
    let last_fragment = tree
        .fragments
        .iter()
        .find(|fragment| fragment.node == last_inside)
        .unwrap();
    let after_fragment = tree
        .fragments
        .iter()
        .find(|fragment| fragment.node == after)
        .unwrap();
    assert_eq!(before_fragment.row, after_fragment.row);
    assert!(first_fragment.row < last_fragment.row);
    assert_eq!(before_fragment.row, last_fragment.row);
    assert_eq!(atom_box.border_rect.col, 8);
    assert!(first_fragment.col >= atom_box.content_rect.col);
    assert!(last_fragment.col >= atom_box.content_rect.col);
    assert_eq!(
        after_fragment.col,
        atom_box.border_rect.col + atom_box.border_rect.width + 1
    );
}

#[test]
fn improper_table_roles_form_block_and_inline_anonymous_tables() {
    let mut document = Document::new();
    let outer = document.insert_element(None, "div", ElementNs::Html, vec![]);
    let block_before = document.insert_text(Some(outer), "before");
    let first_cell = document.insert_element(
        Some(outer),
        "span",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain(
            "style",
            "display: table-cell; padding: 0",
        )],
    );
    let first_text = document.insert_text(Some(first_cell), "A");
    let contents = document.insert_element(
        Some(outer),
        "span",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain("style", "display: contents")],
    );
    let second_cell = document.insert_element(
        Some(contents),
        "span",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain(
            "style",
            "display: table-cell; padding: 0",
        )],
    );
    let second_text = document.insert_text(Some(second_cell), "B");
    let block_after = document.insert_text(Some(outer), "after");

    let line = document.insert_element(Some(outer), "p", ElementNs::Html, vec![]);
    let inline_before = document.insert_text(Some(line), "left ");
    let host = document.insert_element(Some(line), "span", ElementNs::Html, vec![]);
    let inline_cell = document.insert_element(
        Some(host),
        "span",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain(
            "style",
            "display: table-cell; padding: 0",
        )],
    );
    let inline_text = document.insert_text(Some(inline_cell), "cell");
    let inline_after = document.insert_text(Some(line), " right");

    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 40, rows: 8 });
    let fragment = |node| {
        tree.fragments
            .iter()
            .find(|item| item.node == node)
            .unwrap()
    };
    assert!(fragment(block_before).row < fragment(first_text).row);
    assert_eq!(fragment(first_text).row, fragment(second_text).row);
    assert!(fragment(second_text).row < fragment(block_after).row);
    assert_eq!(fragment(inline_before).row, fragment(inline_text).row);
    assert_eq!(fragment(inline_text).row, fragment(inline_after).row);
}

#[test]
fn box_sizing_changes_fixed_width_border_geometry() {
    let mut document = Document::new();
    let content = document.insert_element(
        None,
        "div",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain(
            "style",
            "width: 6ch; padding: 1rem 1ch; border: solid; box-sizing: content-box",
        )],
    );
    document.insert_text(Some(content), "x");
    let border = document.insert_element(
        None,
        "div",
        ElementNs::Html,
        vec![crate::core::dom::Attr::plain(
            "style",
            "width: 6ch; padding: 1rem 1ch; border: solid; box-sizing: border-box",
        )],
    );
    document.insert_text(Some(border), "x");
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 1 });
    let content_box = tree
        .boxes
        .iter()
        .find(|layout_box| layout_box.node == content)
        .unwrap();
    let border_box = tree
        .boxes
        .iter()
        .find(|layout_box| layout_box.node == border)
        .unwrap();
    assert_eq!(content_box.border_rect.width, 10);
    assert_eq!(content_box.content_rect.width, 6);
    assert_eq!(border_box.border_rect.width, 6);
    assert_eq!(border_box.content_rect.width, 2);
}

#[test]
fn intrinsic_height_is_not_limited_by_viewport_rows() {
    let mut document = Document::new();
    let pre = document.insert_element(None, "pre", ElementNs::Html, vec![]);
    let text = (0..300).map(|_| "x").collect::<Vec<_>>().join("\n");
    document.insert_text(Some(pre), &text);
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 4, rows: 1 });
    assert_eq!(tree.fragments.len(), 300);
    assert!(tree.height >= 300);
}

#[test]
fn all_white_space_modes_apply_their_collapse_break_and_wrap_rules() {
    let cases = [
        ("normal", " a  b c ", 4, vec!["a b", "c"]),
        ("nowrap", " a  b c ", 4, vec!["a b c"]),
        ("pre", " a\nb ", 2, vec![" a", "b "]),
        ("pre-wrap", "ab cd", 3, vec!["ab ", "cd"]),
        ("pre-line", " a  b\n c ", 4, vec!["a b", "c"]),
        ("break-spaces", "a  b", 2, vec!["a ", " b"]),
    ];
    for (mode, text, width, expected) in cases {
        let mut document = Document::new();
        let declaration = format!("white-space: {mode}");
        let p = document.insert_element(
            None,
            "p",
            ElementNs::Html,
            vec![crate::core::dom::Attr::plain("style", &declaration)],
        );
        document.insert_text(Some(p), text);
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(
            &document,
            &styles,
            Size {
                cols: width,
                rows: 1,
            },
        );
        assert_eq!(fragment_text(&tree), expected, "{mode}");
    }
}

#[test]
fn segment_breaks_tabs_forced_breaks_and_cross_node_graphemes_stay_owned() {
    let mut document = Document::new();
    let pre = document.insert_element(None, "pre", ElementNs::Html, vec![]);
    let first = document.insert_text(Some(pre), "a\tX\r\nb\u{000c}c\n");
    let br = document.insert_element(Some(pre), "br", ElementNs::Html, vec![]);
    let base = document.insert_text(Some(pre), "e");
    document.insert_text(Some(pre), "\u{301}");
    let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 1 });
    assert_eq!(
        tree.fragments
            .iter()
            .map(|fragment| (fragment.node, fragment.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (first, "a       X"),
            (first, "b"),
            (first, "c"),
            (base, "e\u{301}")
        ]
    );
    assert_eq!(tree.fragments.last().unwrap().row, 4);
    assert!(tree.fragments.iter().all(|fragment| fragment.node != br));
}

#[test]
fn zero_width_keeps_zero_geometry_but_text_layout_makes_progress() {
    let (document, styles) = paragraph("ab");
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 0, rows: 1 });
    assert_eq!(tree.width, 0);
    assert_eq!(tree.boxes[0].border_rect.width, 0);
    assert!(tree.height >= 2);
    assert!(
        BasicPainter
            .paint(&tree, Palette::default())
            .text_lines()
            .iter()
            .all(String::is_empty)
    );
}

fn rendered_rows(source: &str, cols: u16) -> Vec<String> {
    crate::pipeline::render::render_html(source, Size { cols, rows: 24 }, Palette::DEFAULT, false)
        .painted
        .text_lines()
}

#[test]
fn the_marker_field_is_shared_by_every_sibling_so_numbers_share_one_text_column() {
    let rows = rendered_rows(
        "<ol start='9'><li>nine</li><li>ten</li><li>eleven</li></ol>",
        30,
    );
    assert_eq!(rows, [" 9. nine", "10. ten", "11. eleven", ""]);
}

#[test]
fn outside_markers_hang_so_wrapped_lines_align_under_the_item_text() {
    let rows = rendered_rows("<ul><li>alpha beta gamma delta</li></ul>", 12);
    assert_eq!(rows, ["• alpha beta", "  gamma", "  delta", ""]);
}

#[test]
fn inside_markers_keep_the_marker_inline_with_no_hanging_indent() {
    let rows = rendered_rows(
        "<ul style='list-style-position: inside'><li>alpha beta gamma</li></ul>",
        12,
    );
    assert_eq!(rows, ["• alpha beta", "gamma", ""]);
}

#[test]
fn a_suppressed_marker_reserves_no_field_at_all() {
    let rows = rendered_rows("<ul style='list-style-type: none'><li>alpha</li></ul>", 12);
    assert_eq!(rows, ["alpha", ""]);
}

#[test]
fn generated_content_and_markers_render_inside_table_cells() {
    let rows = rendered_rows(
        "<style>td::before { content: '> ' }</style>\
         <table><tr><td><ul><li>cell item</li></ul></td></tr></table>",
        24,
    );
    let text = rows.join("\n");
    assert!(text.contains('>'));
    assert!(text.contains('\u{2022}'));
}

#[test]
fn generated_content_is_tagged_with_its_originating_element_for_hit_testing() {
    use crate::html::HtmlParser;
    let outcome = crate::html::Html5everParser::new(false)
        .parse_document("<style>a::after { content: ' (link)' }</style><a href='/x'>go</a>");
    let document = outcome.document.borrow();
    let sheets = crate::pipeline::render::embedded_style_sheets(&document);
    let styles = BasicCascade.apply(&sheets, &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 40, rows: 24 });
    let link = tree.links.first().unwrap();
    let painted: usize = link.rects.iter().map(|rect| rect.width).sum();
    assert_eq!(painted, "go (link)".len());
}

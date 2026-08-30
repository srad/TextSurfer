use super::{box_for, grid_fixture, viewport};
use crate::core::dom::{Attr, Document, ElementNs};
use crate::core::geom::Size;
use crate::css::{Cascade, CssParser, CssparserParser, MediaContext, StyloCascade};
use crate::layout::{LayoutEngine, TaffyLayoutEngine};

#[test]
fn a_named_area_places_an_item_across_the_cells_it_covers() {
    let fixture = grid_fixture(
        "grid-template-columns: 4ch 6ch;
         grid-template-areas: \"head head\" \"nav main\";
         width: 10ch",
        &["grid-area: head", "grid-area: nav", "grid-area: main"],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 10, 1));
    assert_eq!(fixture.rect(1), (0, 1, 4, 1));
    assert_eq!(fixture.rect(2), (4, 1, 6, 1));
}

#[test]
fn the_implicit_area_lines_are_addressable_by_name() {
    let fixture = grid_fixture(
        "grid-template-columns: repeat(3, 4ch);
         grid-template-areas: \"a a b\";
         width: 12ch",
        &[
            "grid-column: a-start / a-end",
            "grid-column: b-start / b-end",
        ],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 8, 1));
    assert_eq!(fixture.rect(1), (8, 0, 4, 1));
}

#[test]
fn a_null_cell_leaves_its_track_free_for_auto_placement() {
    let fixture = grid_fixture(
        "grid-template-columns: repeat(2, 4ch);
         grid-template-areas: \". a\";
         width: 8ch",
        &["grid-area: a", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (4, 0, 4, 1));
    assert_eq!(fixture.rect(1), (0, 0, 4, 1));
}

/// The regression this milestone item exists to fix: Wikipedia's Vector 2022 skin lays its whole
/// page skeleton out with `grid-template-areas` and `grid-area`. Before grid was enabled every one
/// of these fell back to normal flow and the skeleton stacked vertically.
#[test]
fn the_vector_2022_page_skeleton_is_placed_rather_than_stacked() {
    let mut document = Document::new();
    let root = document.insert_element(
        None,
        "div",
        ElementNs::Html,
        vec![Attr::plain("class", "grid")],
    );
    let mut parts = Vec::new();
    for name in ["header", "titlebar", "column", "content", "footer"] {
        let node = document.insert_element(
            Some(root),
            "div",
            ElementNs::Html,
            vec![Attr::plain("class", name)],
        );
        document.insert_text(Some(node), name);
        parts.push((name, node));
    }
    let sheet = CssparserParser.parse(
        "div.grid {
             display: grid;
             margin: 0;
             width: 40ch;
             grid-template-columns: 10ch 1fr;
             grid-template-rows: repeat(4, 16px);
             grid-template-areas:
                 'header   header'
                 'titlebar titlebar'
                 'column   content'
                 'footer   footer';
         }
         div.grid > div { margin: 0; padding: 0 }
         .header { grid-area: header }
         .titlebar { grid-area: titlebar }
         .column { grid-area: column }
         .content { grid-area: content }
         .footer { grid-area: footer }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 40, rows: 24 });

    let rect = |name: &str| {
        let node = parts.iter().find(|(part, _)| *part == name).unwrap().1;
        let rect = box_for(&tree, node).border_rect;
        (rect.col, rect.row, rect.width, rect.height)
    };
    assert_eq!(rect("header"), (0, 0, 40, 1));
    assert_eq!(rect("titlebar"), (0, 1, 40, 1));
    // The sidebar and the article share a row instead of stacking, which is the whole point.
    assert_eq!(rect("column"), (0, 2, 10, 1));
    assert_eq!(rect("content"), (10, 2, 30, 1));
    assert_eq!(rect("footer"), (0, 3, 40, 1));
}

#[test]
fn generated_content_becomes_a_grid_item_rather_than_inline_text() {
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    let child = document.insert_element(Some(root), "div", ElementNs::Html, vec![]);
    document.insert_text(Some(child), "B");
    let sheet = CssparserParser.parse(
        "main { display: grid; margin: 0; grid-template-columns: 2ch 2ch; width: 4ch }
         main > div { margin: 0; padding: 0 }
         main::before { content: 'A' }",
    );
    let styles = StyloCascade.apply(&[sheet], &document, MediaContext::screen());
    let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 24 });
    // `::before` takes the first track, pushing the real child into the second.
    let rect = box_for(&tree, child).border_rect;
    assert_eq!((rect.col, rect.row), (2, 0));
}

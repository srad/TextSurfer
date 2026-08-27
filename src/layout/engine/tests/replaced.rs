//! Replaced elements — images and form controls — render a text stand-in instead of their absent
//! or already-folded children, in normal flow and inside table cells alike.

use crate::core::form::FormState;
use crate::core::geom::Size;
use crate::css::{BasicCascade, Cascade, MediaContext};
use crate::html::{Html5everParser, HtmlParser};
use crate::layout::engine::{BoxTree, LayoutEngine, TaffyLayoutEngine};

fn laid_out(source: &str, cols: u16) -> BoxTree {
    let outcome = Html5everParser::new(false).parse_document(source);
    let document = outcome.document.borrow();
    let viewport = Size { cols, rows: 24 };
    let styles = BasicCascade.apply(
        &[],
        &document,
        MediaContext::screen().with_viewport(viewport),
    );
    TaffyLayoutEngine.layout_with_form_state(&document, &styles, viewport, FormState::empty())
}

/// Every painted line of `source`, in row order.
fn lines(source: &str, cols: u16) -> Vec<String> {
    let tree = laid_out(source, cols);
    let mut rows: Vec<(usize, usize, String)> = tree
        .fragments
        .iter()
        .map(|fragment| (fragment.row, fragment.col, fragment.text.clone()))
        .collect();
    rows.sort_by_key(|(row, col, _)| (*row, *col));
    let mut out: Vec<String> = Vec::new();
    let mut current = None;
    for (row, _, text) in rows {
        if current != Some(row) {
            out.push(String::new());
            current = Some(row);
        }
        out.last_mut().expect("a row was started").push_str(&text);
    }
    out
}

fn painted(source: &str, cols: u16) -> String {
    lines(source, cols).join("\n")
}

/// The rendering laid out over a cell grid, so a glyph landing on a border is visible as such.
/// `lines` concatenates fragments in column order and would hide the overlap that motivated this.
fn grid(source: &str, cols: u16) -> Vec<String> {
    let tree = laid_out(source, cols);
    let mut rows: Vec<Vec<char>> = Vec::new();
    let mut put = |col: usize, row: usize, text: &str| {
        while rows.len() <= row {
            rows.push(vec![' '; usize::from(cols)]);
        }
        for (offset, ch) in text.chars().enumerate() {
            if let Some(cell) = rows[row].get_mut(col + offset) {
                *cell = ch;
            }
        }
    };
    for stroke in &tree.strokes {
        let rect = stroke.rect;
        for row in rect.row..rect.row.saturating_add(rect.height) {
            for col in rect.col..rect.col.saturating_add(rect.width) {
                let edge = row == rect.row
                    || row + 1 == rect.row + rect.height
                    || col == rect.col
                    || col + 1 == rect.col + rect.width;
                if edge {
                    put(col, row, "#");
                }
            }
        }
    }
    for fragment in &tree.fragments {
        put(fragment.col, fragment.row, &fragment.text);
    }
    rows.into_iter()
        .map(|row| row.into_iter().collect())
        .collect()
}

#[test]
fn a_block_level_image_still_renders_its_alt_text() {
    // The display branches used to be tested before the `img` arm, so a block-level replaced
    // element became a block box that recursed into children it does not have and painted
    // nothing at all. `display: block` on an `<img>` or an `<input>` is ordinary author CSS.
    let output = painted(
        "<p>before</p><img alt=BLOCK style='display:block'><p>after</p>",
        40,
    );
    assert!(
        output.contains("[BLOCK]"),
        "a block-level image must still paint its alt text, got {output:?}"
    );
}

#[test]
fn every_supported_control_paints_a_box_with_its_initial_value() {
    let output = painted(
        "<input type=text value=hi size=4>\
         <input type=checkbox checked><input type=checkbox>\
         <input type=radio checked><input type=radio>\
         <input type=submit value=Go><input type=reset>\
         <select><option>one<option selected>two</select>\
         <input type=color>",
        80,
    );
    for expected in [
        "[hi  ]",
        "[X]",
        "[ ]",
        "(*)",
        "( )",
        "[Go]",
        "[Reset]",
        "[two \u{25be}]",
        "[color?]",
    ] {
        assert!(
            output.contains(expected),
            "expected {expected:?} in {output:?}"
        );
    }
}

#[test]
fn a_selected_option_inside_an_optgroup_is_the_one_shown() {
    // The select's selection resolves through the optgroup to the select, not to the option's
    // immediate parent — resolving against the parent would never find the select's options.
    let output = painted(
        "<select><option>one<optgroup label=g><option selected>two</optgroup></select>",
        40,
    );
    assert!(output.contains("two"), "got {output:?}");
    assert!(
        !output.contains("one"),
        "only the selected option is shown, got {output:?}"
    );
}

#[test]
fn a_hidden_input_and_a_stray_option_paint_nothing() {
    let output = painted(
        "<p>a<input type=hidden name=t value=secret>b</p><option>orphan</option>",
        40,
    );
    assert!(
        !output.contains("secret"),
        "a hidden input's value must never reach the screen, got {output:?}"
    );
    assert!(
        !output.contains("orphan"),
        "an option outside a select paints nothing, got {output:?}"
    );
    assert!(
        output.contains("ab"),
        "the text around it still runs together, got {output:?}"
    );
}

#[test]
fn a_password_value_is_never_painted_in_clear_text() {
    let output = painted("<input type=password value=hunter2 size=10>", 40);
    assert!(
        !output.contains("hunter2"),
        "a password value must not be painted, got {output:?}"
    );
    assert!(
        output.contains("[*******   ]"),
        "it is masked at its own width, got {output:?}"
    );
}

#[test]
fn a_fields_padding_is_blank_and_survives_the_surrounding_white_space() {
    // The stand-in's blanks are the field's own cells, so they must not collapse the way the
    // parent's `white-space: normal` collapses ordinary text. This is what makes it safe to pad
    // with spaces instead of a filler glyph.
    let output = painted("<p><input value=hi size=6></p>", 40);
    assert!(
        output.contains("[hi    ]"),
        "the field keeps its declared width, got {output:?}"
    );
}

#[test]
fn a_control_is_painted_as_a_filled_field_and_body_text_is_not() {
    // The field's extent is carried by reverse video, not by a filler glyph, so losing the style
    // bit would leave a text input indistinguishable from its surroundings.
    let tree = laid_out("<p>text <input value=v size=3></p>", 40);
    let field = tree
        .fragments
        .iter()
        .find(|fragment| fragment.text.contains('['))
        .expect("the field was painted");
    assert!(field.style.reverse, "the control is a filled field");
    assert!(
        tree.fragments
            .iter()
            .filter(|fragment| fragment.text.contains("text"))
            .all(|fragment| !fragment.style.reverse),
        "and the text beside it is not"
    );
}

#[test]
fn a_long_value_keeps_the_end_the_caret_would_sit_at() {
    let output = painted("<input value=abcdefghij size=4>", 40);
    assert!(
        output.contains("[ghij]"),
        "the window trims from the left, got {output:?}"
    );
}

#[test]
fn a_textarea_paints_its_rows_as_a_block() {
    let rows = lines("<textarea cols=6 rows=3>ab</textarea>", 40);
    assert_eq!(
        rows,
        vec![
            "ab    ".to_string(),
            "      ".to_string(),
            "      ".to_string()
        ],
        "a textarea occupies exactly its declared cols by rows"
    );
}

#[test]
fn a_button_label_is_painted_once_not_twice_inside_a_table_cell() {
    // The table cell walker's `img` arm fell through and pushed the element's children, which is
    // harmless for a void `<img>` and would have painted a button's label a second time.
    let output = painted(
        "<table><tr><td><button>Press</button></td></tr></table>",
        40,
    );
    assert_eq!(
        output.matches("Press").count(),
        1,
        "the label belongs to the stand-in only, got {output:?}"
    );
}

#[test]
fn controls_render_identically_in_normal_flow_and_in_a_table_cell() {
    let markup = "<input type=text value=v size=3><input type=checkbox checked>\
                  <button>B</button><select><option>o</option></select>";
    let flow = painted(markup, 60);
    let cell = painted(&format!("<table><tr><td>{markup}</td></tr></table>"), 60);
    for expected in ["[v  ]", "[X]", "[B]", "[o \u{25be}]"] {
        assert!(flow.contains(expected), "normal flow: {flow:?}");
        assert!(cell.contains(expected), "table cell: {cell:?}");
    }
}

#[test]
fn a_bordered_control_paints_inside_its_border_not_on_it() {
    // The stand-in used to ride the box's own `inline` list, which is emitted at the *border-box*
    // origin — correct only for anonymous boxes, which have no border. On Wikipedia the search
    // field painted straight over its own `┌───`.
    let rows = grid(
        "<input value=hi size=4 style='display:block;border:1px solid;width:8ch'>",
        20,
    );
    // `width: 8ch` is a *content* width under the default `content-box`, so the border box is ten.
    assert_eq!(
        rows[0].trim_end(),
        "##########",
        "the top border is a border and nothing else"
    );
    assert!(
        rows[1].starts_with("#hi"),
        "and the value sits on the row below it, got {:?}",
        rows[1]
    );
}

#[test]
fn a_control_with_overflow_hidden_still_shows_its_label() {
    // Same bug, other face: a label emitted at the border row falls outside the box's own padding
    // box, so `overflow: hidden` clipped it away and Wikipedia's Search button rendered empty.
    let output = painted(
        "<button style='display:block;border:1px solid;overflow:hidden'>Search</button>",
        20,
    );
    assert!(
        output.contains("Search"),
        "the label must survive its own clip, got {output:?}"
    );
}

#[test]
fn a_border_box_control_keeps_intrinsic_room_for_padding_and_border() {
    let output = painted(
        "<button style='display:block;box-sizing:border-box;border:1px solid;padding:0 2ch;overflow:hidden;height:48px'>Search</button>",
        20,
    );
    assert!(
        output.contains("Search"),
        "the label must keep its intrinsic content width, got {output:?}"
    );
}

#[test]
fn a_bordered_control_keeps_a_content_row_when_max_height_would_crush_it() {
    // A 1px border costs a whole cell here, so Wikipedia's `max-height: 2rem` leaves zero content
    // rows and the field renders blank. The quantisation is ours, not the author's, so a replaced
    // box keeps room for its own rows — CSS resolves min over max, which is what makes that work.
    let output = painted(
        "<input value=hi size=4 style='display:block;border:1px solid;max-height:2rem'>",
        20,
    );
    assert!(
        output.contains("hi"),
        "the value must survive a max-height the border already spent, got {output:?}"
    );
}

#[test]
fn wikipedia_flex_search_keeps_the_field_prompt_and_button_label() {
    let source = "<form style='display:flex;border:1px solid;width:70ch'>
        <div style='flex-grow:1;margin:-1px'>
        <input type=search placeholder='Search Wikipedia'
        style='display:block;box-sizing:border-box;min-height:32px;max-height:2rem;
        width:100%;margin:0;border:1px solid;padding:4px 8px;overflow:hidden'></div>
        <button style='min-height:32px;margin:-1px;border:1px solid;padding:4px 12px;
        overflow:hidden'>Search</button></form>";
    let tree = laid_out(source, 160);
    let prompt = tree
        .fragments
        .iter()
        .find(|fragment| fragment.text.contains("Search Wikipedia"))
        .expect("the search field keeps a content row for its prompt");
    let button = tree
        .fragments
        .iter()
        .find(|fragment| fragment.text.trim() == "Search")
        .unwrap_or_else(|| {
            panic!(
                "the search button keeps a content row for its label: {:?}",
                tree.fragments
                    .iter()
                    .map(|fragment| (&fragment.text, fragment.col, fragment.row))
                    .collect::<Vec<_>>()
            )
        });
    assert!(prompt.style.dim);
    assert!(!button.style.dim);
    assert_eq!(prompt.row, button.row);
    assert!(prompt.col < button.col);
}

#[test]
fn a_css_widened_field_fills_its_content_box() {
    let rows = grid(
        "<input value=hi size=2 style='display:block;border:1px solid;width:12ch'>",
        20,
    );
    assert_eq!(
        rows[1].trim_end(),
        "#hi          #",
        "the field follows its used width (12 content cells), not its size=2 attribute"
    );
}

#[test]
fn a_block_control_keeps_its_size_width_instead_of_filling_its_container() {
    // `input { display: block }` does not stretch in a real browser: a replaced element has an
    // intrinsic size, and `width: auto` resolves to it.
    let output = painted("<input value=hi size=4 style='display:block'>", 40);
    assert_eq!(
        output.trim_end(),
        "[hi  ]",
        "a block control is its own width — size=4 plus its brackets — not the container's 40"
    );
}

#[test]
fn an_unstyled_inline_control_keeps_its_bracketed_intrinsic_width() {
    // The inline path is what DDG Lite renders through, and it must not change.
    let output = painted("<input value=hi size=6>", 40);
    assert_eq!(output.trim_end(), "[hi    ]");
}

#[test]
fn a_bordered_control_drops_its_brackets() {
    // Brackets say "this is a control" where nothing else does. A border already says it, and
    // doubling the two is what made the first attempt look cluttered.
    let bordered = painted(
        "<input value=hi size=4 style='display:block;border:1px solid'>",
        20,
    );
    assert!(!bordered.contains('['), "got {bordered:?}");
    let bare = painted("<input value=hi size=4 style='display:block'>", 20);
    assert!(bare.contains('['), "got {bare:?}");
}

#[test]
fn a_control_inside_a_flex_container_is_laid_out_as_a_flex_item() {
    // A flex item is blockified, so the button takes the box path. Its label used to vanish here
    // because it rode `inline` on a box that had a border and padding of its own.
    let output = painted(
        "<div style='display:flex'><span>x</span>\
         <button style='border:1px solid'>Go</button></div>",
        30,
    );
    assert!(output.contains("Go"), "got {output:?}");
    assert!(
        output.contains('x'),
        "the sibling item is still there, got {output:?}"
    );
}

#[test]
fn an_opacity_zero_control_paints_nothing_but_keeps_its_geometry() {
    // `opacity: 0` over a styled box is how the web builds a custom control; Wikipedia's header
    // has three, and they used to paint as stray checkboxes over the article chrome.
    let tree = laid_out("<input type=checkbox style='opacity:0'><p>after</p>", 20);
    assert!(
        tree.fragments
            .iter()
            .all(|fragment| !fragment.text.contains('[')),
        "nothing of the control is painted, got {:?}",
        tree.fragments.iter().map(|f| &f.text).collect::<Vec<_>>()
    );
    assert!(
        tree.boxes.len() > 1,
        "but it still occupies its box, so the page does not reflow around it"
    );
    let opaque = painted("<input type=checkbox style='opacity:0.5'>", 20);
    assert!(
        opaque.contains("[ ]"),
        "only fully transparent is honoured; partial alpha stays opaque, got {opaque:?}"
    );
}

#[test]
fn an_empty_field_shows_its_placeholder_dimmed() {
    let tree = laid_out(
        "<input placeholder='Search Wikipedia' size=20 style='display:block;border:1px solid'>",
        40,
    );
    let field = tree
        .fragments
        .iter()
        .find(|fragment| fragment.text.contains("Search"))
        .expect("the placeholder was painted");
    assert!(field.style.dim, "a placeholder is a prompt, not a value");
}

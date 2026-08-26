use super::{grid_fixture, viewport};

#[test]
fn explicit_line_numbers_place_an_item_regardless_of_source_order() {
    let fixture = grid_fixture(
        "grid-template-columns: repeat(3, 4ch); width: 12ch",
        &["grid-row: 1; grid-column: 3", "grid-row: 1; grid-column: 1"],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (8, 0, 4, 1));
    assert_eq!(fixture.rect(1), (0, 0, 4, 1));
}

#[test]
fn a_definite_column_with_an_auto_row_never_moves_the_cursor_backwards() {
    // Sparse packing: the second item cannot reuse the first row's spare column to its left, so it
    // starts a new row. `grid-auto-flow: dense` is what relaxes this.
    let sparse = grid_fixture(
        "grid-template-columns: repeat(3, 4ch); width: 12ch",
        &["grid-column: 3", "grid-column: 1"],
        viewport(20),
    );
    assert_eq!(sparse.rect(0), (8, 0, 4, 1));
    assert_eq!(sparse.rect(1), (0, 1, 4, 1));

    let dense = grid_fixture(
        "grid-template-columns: repeat(3, 4ch); grid-auto-flow: row dense; width: 12ch",
        &["grid-column: 3", "grid-column: 1"],
        viewport(20),
    );
    assert_eq!(dense.rect(1), (0, 0, 4, 1));
}

#[test]
fn a_span_covers_several_tracks_and_their_gaps() {
    let fixture = grid_fixture(
        "grid-template-columns: repeat(3, 4ch); column-gap: 1ch; width: 14ch",
        &["grid-column: 1 / span 2", ""],
        viewport(20),
    );
    // Two 4ch tracks plus the 1ch gutter between them.
    assert_eq!(fixture.rect(0), (0, 0, 9, 1));
    assert_eq!(fixture.rect(1), (10, 0, 4, 1));
}

#[test]
fn negative_lines_count_back_from_the_end_of_the_explicit_grid() {
    let fixture = grid_fixture(
        "grid-template-columns: repeat(3, 4ch); width: 12ch",
        &["grid-column: -2 / -1"],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (8, 0, 4, 1));
}

#[test]
fn named_lines_place_items_by_name_and_by_repeated_name_index() {
    let fixture = grid_fixture(
        "grid-template-columns: [start] 4ch [mid] 4ch [mid] 4ch [end]; width: 12ch",
        &["grid-column: start / mid", "grid-column: mid 2 / end"],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 4, 1));
    assert_eq!(fixture.rect(1), (8, 0, 4, 1));
}

#[test]
fn a_named_span_reaches_the_next_line_of_that_name() {
    let fixture = grid_fixture(
        "grid-template-columns: [a] 4ch [a] 4ch [a] 4ch [a]; width: 12ch",
        &["grid-column: 1 / span a 2"],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 8, 1));
}

#[test]
fn a_repeated_track_list_carries_its_line_names_into_every_repetition() {
    // Taffy asserts unless each repetition holds exactly `tracks + 1` name sets, so this exercises
    // the construction invariant end to end.
    let fixture = grid_fixture(
        "grid-template-columns: repeat(3, [col] 4ch); width: 12ch",
        &["grid-column: col 3"],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (8, 0, 4, 1));
}

#[test]
fn an_item_placed_outside_the_explicit_grid_creates_implicit_tracks() {
    let fixture = grid_fixture(
        "grid-template-columns: 4ch; grid-auto-columns: 6ch; width: 10ch",
        &["grid-column: 2"],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (4, 0, 6, 1));
}

#[test]
fn grid_area_places_an_item_across_both_axes() {
    let fixture = grid_fixture(
        "grid-template-columns: repeat(2, 4ch); grid-template-rows: repeat(2, 16px); width: 8ch",
        &["grid-area: 2 / 1 / 3 / 3"],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 1, 8, 1));
}

#[test]
fn order_reorders_auto_placement_without_moving_the_dom() {
    let fixture = grid_fixture(
        "grid-template-columns: repeat(2, 4ch); width: 8ch",
        &["order: 1", ""],
        viewport(20),
    );
    // The second item is placed first, so it takes the first column.
    assert_eq!(fixture.rect(0), (4, 0, 4, 1));
    assert_eq!(fixture.rect(1), (0, 0, 4, 1));
}

#[test]
fn overlapping_items_keep_a_single_source_order_for_paint_and_hit_testing() {
    let fixture = grid_fixture(
        "grid-template-columns: 6ch; width: 6ch",
        &["grid-area: 1 / 1", "grid-area: 1 / 1"],
        viewport(20),
    );
    // Both occupy the same cell.
    assert_eq!(fixture.rect(0), fixture.rect(1));
    // The later item is painted after the earlier one, so it wins a hit at that cell.
    let first = fixture
        .tree
        .boxes
        .iter()
        .position(|box_| box_.node == fixture.items[0]);
    let second = fixture
        .tree
        .boxes
        .iter()
        .position(|box_| box_.node == fixture.items[1]);
    assert!(first < second);
}

#[test]
fn justify_and_align_self_position_an_item_inside_its_area() {
    let fixture = grid_fixture(
        "grid-template-columns: 8ch; grid-template-rows: 48px; width: 8ch",
        &["justify-self: end; align-self: end; width: 2ch; height: 16px"],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (6, 2, 2, 1));
}

#[test]
fn justify_items_applies_to_every_item_that_does_not_override_it() {
    let fixture = grid_fixture(
        "grid-template-columns: 8ch; justify-items: center; width: 8ch",
        &["width: 2ch", "width: 2ch; justify-self: start"],
        viewport(20),
    );
    assert_eq!(fixture.rect(0).0, 3);
    assert_eq!(fixture.rect(1).0, 0);
}

#[test]
fn justify_content_distributes_whole_tracks_within_a_wider_container() {
    let fixture = grid_fixture(
        "grid-template-columns: 2ch 2ch; justify-content: space-between; width: 10ch",
        &["", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 2, 1));
    assert_eq!(fixture.rect(1), (8, 0, 2, 1));
}

#[test]
fn a_grid_container_stretches_its_tracks_when_content_alignment_is_normal() {
    // `normal` behaves as `stretch` for a grid container, unlike a flex container's main axis.
    let fixture = grid_fixture(
        "grid-template-columns: auto; width: 10ch",
        &[""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 10, 1));
}

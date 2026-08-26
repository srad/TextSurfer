use super::{grid_fixture, viewport};
use proptest::prelude::*;

#[test]
fn fixed_columns_place_items_side_by_side_in_source_order() {
    let fixture = grid_fixture(
        "grid-template-columns: 4ch 6ch; width: 10ch",
        &["", "", "", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 4, 1));
    assert_eq!(fixture.rect(1), (4, 0, 6, 1));
    // The third item wraps into an implicit second row.
    assert_eq!(fixture.rect(2), (0, 1, 4, 1));
    assert_eq!(fixture.rect(3), (4, 1, 6, 1));
}

#[test]
fn fractional_columns_divide_the_container_width() {
    let fixture = grid_fixture(
        "grid-template-columns: 1fr 3fr; width: 16ch",
        &["", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 4, 1));
    assert_eq!(fixture.rect(1), (4, 0, 12, 1));
}

#[test]
fn percentage_and_minmax_columns_resolve_against_the_container() {
    let fixture = grid_fixture(
        "grid-template-columns: 25% minmax(2ch, 1fr); width: 16ch",
        &["", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 4, 1));
    assert_eq!(fixture.rect(1), (4, 0, 12, 1));
}

#[test]
fn a_fixed_repeat_expands_into_that_many_tracks() {
    let fixture = grid_fixture(
        "grid-template-columns: repeat(3, 5ch); width: 15ch",
        &["", "", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 5, 1));
    assert_eq!(fixture.rect(1), (5, 0, 5, 1));
    assert_eq!(fixture.rect(2), (10, 0, 5, 1));
}

#[test]
fn auto_fill_generates_as_many_tracks_as_the_container_holds() {
    let fixture = grid_fixture(
        "grid-template-columns: repeat(auto-fill, 5ch); width: 16ch",
        &["", "", "", ""],
        viewport(20),
    );
    // Three 5ch tracks fit in 16ch; the fourth item starts a second row.
    assert_eq!(fixture.rect(0), (0, 0, 5, 1));
    assert_eq!(fixture.rect(2), (10, 0, 5, 1));
    assert_eq!(fixture.rect(3), (0, 1, 5, 1));
}

#[test]
fn an_auto_repeat_beside_an_indefinite_track_is_dropped_without_aborting_layout() {
    // Taffy's own `.unwrap()` on this shape is guarded, but the declaration is invalid CSS, so the
    // earlier template has to be what actually lays out.
    let fixture = grid_fixture(
        "grid-template-columns: 4ch 6ch;
         grid-template-columns: auto repeat(auto-fill, 5ch);
         width: 10ch",
        &["", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 4, 1));
    assert_eq!(fixture.rect(1), (4, 0, 6, 1));
}

#[test]
fn gaps_separate_tracks_without_being_part_of_them() {
    let fixture = grid_fixture(
        "grid-template-columns: 4ch 4ch; gap: 16px 2ch; width: 10ch",
        &["", "", "", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 4, 1));
    assert_eq!(fixture.rect(1), (6, 0, 4, 1));
    // One 16px row gap is one cell, so the second row starts two rows down.
    assert_eq!(fixture.rect(2), (0, 2, 4, 1));
    assert_eq!(fixture.rect(3), (6, 2, 4, 1));
}

#[test]
fn explicit_row_tracks_size_the_rows_they_name() {
    let fixture = grid_fixture(
        "grid-template-columns: 4ch; grid-template-rows: 32px 48px; width: 4ch",
        &["", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 4, 2));
    assert_eq!(fixture.rect(1), (0, 2, 4, 3));
}

#[test]
fn implicit_tracks_take_their_size_from_the_auto_track_properties() {
    let fixture = grid_fixture(
        "grid-template-columns: 4ch; grid-template-rows: 16px; grid-auto-rows: 48px; width: 4ch",
        &["", "", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 4, 1));
    assert_eq!(fixture.rect(1), (0, 1, 4, 3));
    assert_eq!(fixture.rect(2), (0, 4, 4, 3));
}

#[test]
fn column_flow_fills_down_each_column_before_moving_across() {
    let fixture = grid_fixture(
        "grid-template-rows: 16px 16px; grid-auto-flow: column;
         grid-auto-columns: 4ch; width: 8ch",
        &["", "", ""],
        viewport(20),
    );
    assert_eq!(fixture.rect(0), (0, 0, 4, 1));
    assert_eq!(fixture.rect(1), (0, 1, 4, 1));
    assert_eq!(fixture.rect(2), (4, 0, 4, 1));
}

#[test]
fn a_dense_flow_backfills_a_hole_an_earlier_span_left() {
    let sparse = grid_fixture(
        "grid-template-columns: repeat(3, 4ch); width: 12ch",
        &["grid-column: span 2", "grid-column: span 2", ""],
        viewport(20),
    );
    // Sparse placement never goes back: the third item follows the second.
    assert_eq!(sparse.rect(1), (0, 1, 8, 1));
    assert_eq!(sparse.rect(2), (8, 1, 4, 1));

    let dense = grid_fixture(
        "grid-template-columns: repeat(3, 4ch); grid-auto-flow: row dense; width: 12ch",
        &["grid-column: span 2", "grid-column: span 2", ""],
        viewport(20),
    );
    // Dense placement puts the single-track item back into the first row's spare column.
    assert_eq!(dense.rect(2), (8, 0, 4, 1));
}

#[test]
fn the_grid_shorthand_lays_out_the_same_grid_as_its_longhands() {
    let shorthand = grid_fixture(
        "grid: 16px 32px / 4ch 6ch; width: 10ch",
        &["", ""],
        viewport(20),
    );
    let longhand = grid_fixture(
        "grid-template-rows: 16px 32px; grid-template-columns: 4ch 6ch; width: 10ch",
        &["", ""],
        viewport(20),
    );
    assert_eq!(shorthand.rect(0), longhand.rect(0));
    assert_eq!(shorthand.rect(1), longhand.rect(1));
}

proptest! {
    #[test]
    fn wider_auto_fill_grids_do_not_increase_height(
        narrow in 4u16..24,
        extra in 0u16..24,
        item_count in 1usize..32,
    ) {
        let items = vec!["white-space:nowrap"; item_count];
        let container = "grid-template-columns:repeat(auto-fill,4ch);width:100%";
        let narrow_grid = grid_fixture(container, &items, viewport(narrow));
        let wide_grid = grid_fixture(container, &items, viewport(narrow + extra));
        prop_assert!(wide_grid.tree.height <= narrow_grid.tree.height);
    }

    #[test]
    fn bounded_generated_grid_declarations_never_abort_layout(
        columns in 1u16..16,
        track_width in 1u16..8,
        gap in 0u16..4,
        item_count in 0usize..32,
    ) {
        let container = format!(
            "grid-template-columns:repeat({columns},{track_width}ch);gap:{gap}ch;width:100%"
        );
        let items = vec![""; item_count];
        let fixture = grid_fixture(&container, &items, viewport(40));
        prop_assert_eq!(fixture.items.len(), item_count);
    }
}

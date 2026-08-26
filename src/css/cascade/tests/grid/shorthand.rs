use super::{cells, fr, grid};
use crate::core::style::{GridAutoFlow, GridTemplateComponent, TrackSize};

fn singles(components: &[GridTemplateComponent]) -> Vec<TrackSize> {
    components
        .iter()
        .map(|component| match component {
            GridTemplateComponent::Single(track) => *track,
            GridTemplateComponent::Repeat(_) => panic!("expected single tracks"),
        })
        .collect()
}

#[test]
fn grid_template_takes_the_rows_slash_columns_form() {
    let style = grid("grid-template:10ch 1fr / 20ch auto");
    assert_eq!(
        singles(&style.rows().expect("rows").components),
        vec![cells(5), fr(1.0)]
    );
    assert_eq!(
        singles(&style.columns().expect("columns").components),
        vec![cells(20), TrackSize::AUTO]
    );
    assert!(style.style().template_areas.is_none());
}

#[test]
fn grid_template_accepts_none_on_either_axis() {
    let columns = grid("grid-template:none / 1fr");
    assert!(columns.style().template_rows.is_none());
    assert_eq!(
        singles(&columns.columns().expect("columns").components),
        vec![fr(1.0)]
    );

    let rows = grid("grid-template:1fr / none");
    assert_eq!(
        singles(&rows.rows().expect("rows").components),
        vec![fr(1.0)]
    );
    assert!(rows.style().template_columns.is_none());
}

#[test]
fn grid_template_takes_the_area_string_form_with_row_sizes_and_line_names() {
    let style = grid(
        "grid-template:
            [top] \"head head\" 32px
                  \"nav  main\" 1fr
            [bot] / 8ch 1fr",
    );
    let rows = style.rows().expect("rows");
    assert_eq!(singles(&rows.components), vec![cells(2), fr(1.0)]);
    assert_eq!(
        style.names(&rows.line_names),
        vec![vec!["top"], Vec::<&str>::new(), vec!["bot"]]
    );
    assert_eq!(
        singles(&style.columns().expect("columns").components),
        vec![cells(8), fr(1.0)]
    );
    let areas = style.areas().expect("areas");
    assert_eq!(areas.row_count, 2);
    assert_eq!(areas.column_count, 2);
    assert_eq!(areas.areas.len(), 3);
}

#[test]
fn an_area_row_without_a_track_size_defaults_to_auto() {
    let style = grid("grid-template:\"a\" \"b\" 32px");
    assert_eq!(
        singles(&style.rows().expect("rows").components),
        vec![TrackSize::AUTO, cells(2)]
    );
}

#[test]
fn grid_template_is_a_resetting_shorthand() {
    // The areas set by the first declaration must not survive the second. `5ch` is 40px, which is
    // two and a half of the 16px rows, so a row track rounds it to three cells.
    let style = grid("grid-template:\"a\" / 10ch;grid-template:5ch / 6ch");
    assert!(style.style().template_areas.is_none());
    assert_eq!(
        singles(&style.rows().expect("rows").components),
        vec![cells(3)]
    );
    let none = grid("grid-template:\"a\" / 10ch;grid-template:none");
    assert!(none.style().template_areas.is_none());
    assert!(none.style().template_rows.is_none());
    assert!(none.style().template_columns.is_none());
}

#[test]
fn the_grid_shorthand_accepts_every_grid_template_value() {
    let style = grid("grid:10ch 1fr / 20ch");
    assert_eq!(
        singles(&style.rows().expect("rows").components),
        vec![cells(5), fr(1.0)]
    );
    assert_eq!(
        singles(&style.columns().expect("columns").components),
        vec![cells(20)]
    );
    assert_eq!(style.style().auto_flow, GridAutoFlow::Row);
}

#[test]
fn the_grid_shorthand_auto_flow_forms_set_the_implicit_axis() {
    let columns = grid("grid:10ch 1fr / auto-flow dense 4ch");
    assert_eq!(columns.style().auto_flow, GridAutoFlow::ColumnDense);
    assert_eq!(columns.auto_columns().expect("auto columns"), [cells(4)]);
    assert_eq!(
        singles(&columns.rows().expect("rows").components),
        vec![cells(5), fr(1.0)]
    );

    let rows = grid("grid:auto-flow 32px / repeat(3, 1fr)");
    assert_eq!(rows.style().auto_flow, GridAutoFlow::Row);
    assert_eq!(rows.auto_rows().expect("auto rows"), [cells(2)]);
    assert_eq!(rows.columns().expect("columns").components.len(), 1);

    // `dense` may lead.
    let leading = grid("grid:dense auto-flow / 1fr");
    assert_eq!(leading.style().auto_flow, GridAutoFlow::RowDense);
}

#[test]
fn grid_auto_flow_accepts_none_for_the_explicit_axis() {
    let style = grid("grid:none / auto-flow 1fr");
    assert!(style.style().template_rows.is_none());
    assert_eq!(style.style().auto_flow, GridAutoFlow::Column);
    assert_eq!(style.auto_columns().expect("auto columns"), [fr(1.0)]);
}

#[test]
fn area_form_columns_reject_repeat_syntax() {
    let style = grid("grid-template:4ch / 5ch;grid-template:\"a\" / repeat(2, 1fr)");
    assert_eq!(
        singles(&style.rows().expect("earlier rows survive").components),
        vec![cells(2)]
    );
    assert_eq!(
        singles(&style.columns().expect("earlier columns survive").components),
        vec![cells(5)]
    );
}

#[test]
fn the_grid_shorthand_resets_the_implicit_track_longhands_it_does_not_set() {
    let style = grid("grid-auto-rows:9ch;grid-auto-flow:column;grid:10ch / 20ch");
    assert!(style.style().auto_rows.is_none());
    assert!(style.style().auto_columns.is_none());
    assert_eq!(style.style().auto_flow, GridAutoFlow::Row);
}

#[test]
fn an_invalid_shorthand_leaves_every_longhand_it_would_have_set_intact() {
    for source in [
        "grid:10ch /",
        "grid:auto-flow / auto-flow",
        "grid-template:\"a\" \"b b\"",
        "grid-template:10ch",
    ] {
        let style = grid(&format!("grid-template:4ch / 5ch;{source}"));
        assert_eq!(
            singles(&style.rows().expect("earlier value survives").components),
            vec![cells(2)],
            "{source}"
        );
        assert_eq!(
            singles(&style.columns().expect("earlier value survives").components),
            vec![cells(5)],
            "{source}"
        );
    }
}

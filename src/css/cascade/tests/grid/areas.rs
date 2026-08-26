use super::grid;

type AreaBounds = (String, u16, u16, u16, u16);

/// Areas as `(name, row_start, row_end, column_start, column_end)` in 1-based line coordinates.
fn areas(source: &str) -> Option<(Vec<AreaBounds>, u16, u16)> {
    let style = grid(&format!("grid-template-areas:{source}"));
    let data = style.areas()?;
    Some((
        data.areas
            .iter()
            .map(|area| {
                (
                    style.ident(area.name).to_owned(),
                    area.row_start,
                    area.row_end,
                    area.column_start,
                    area.column_end,
                )
            })
            .collect(),
        data.row_count,
        data.column_count,
    ))
}

#[test]
fn a_single_cell_area_spans_one_track_in_each_axis() {
    let (areas, rows, columns) = areas("\"a\"").expect("areas");
    assert_eq!(areas, vec![("a".to_owned(), 1, 2, 1, 2)]);
    assert_eq!((rows, columns), (1, 1));
}

#[test]
fn the_vector_2022_header_template_places_two_named_columns_in_one_row() {
    let (areas, rows, columns) = areas("\"headerStart headerEnd\"").expect("areas");
    assert_eq!(
        areas,
        vec![
            ("headerStart".to_owned(), 1, 2, 1, 2),
            ("headerEnd".to_owned(), 1, 2, 2, 3),
        ]
    );
    assert_eq!((rows, columns), (1, 2));
}

#[test]
fn areas_spanning_rows_and_columns_take_their_bounding_rectangle() {
    let (areas, rows, columns) = areas(
        "\"head head\"
         \"nav  main\"
         \"foot foot\"",
    )
    .expect("areas");
    assert_eq!(
        areas,
        vec![
            ("head".to_owned(), 1, 2, 1, 3),
            ("nav".to_owned(), 2, 3, 1, 2),
            ("main".to_owned(), 2, 3, 2, 3),
            ("foot".to_owned(), 3, 4, 1, 3),
        ]
    );
    assert_eq!((rows, columns), (3, 2));
}

#[test]
fn null_cells_take_a_column_without_naming_an_area() {
    // A null cell token is a run of one or more dots, so `..` is still a single cell.
    let (areas, rows, columns) = areas("\"a . b\" \"a .. b\"").expect("areas");
    assert_eq!(
        areas,
        vec![("a".to_owned(), 1, 3, 1, 2), ("b".to_owned(), 1, 3, 3, 4),]
    );
    assert_eq!((rows, columns), (2, 3));
}

#[test]
fn a_template_of_only_null_cells_still_sizes_the_grid() {
    let (areas, rows, columns) = areas("\". .\"").expect("areas");
    assert!(areas.is_empty());
    assert_eq!((rows, columns), (1, 2));
}

#[test]
fn non_rectangular_ragged_and_empty_templates_invalidate_the_declaration() {
    for source in [
        // L-shaped: `a` does not fill its bounding box.
        "\"a a\" \"a b\" \"c a\"",
        // Two disjoint runs of the same name.
        "\"a b a\"",
        // Ragged rows.
        "\"a b\" \"a\"",
        // No rows, and a row with no columns.
        "\"\"",
        "none-such",
    ] {
        let style = grid(&format!(
            "grid-template-areas:\"keep\";grid-template-areas:{source}"
        ));
        let data = style.areas().expect("earlier value survives");
        assert_eq!(data.areas.len(), 1, "{source}");
        assert_eq!(style.ident(data.areas[0].name), "keep", "{source}");
    }
}

#[test]
fn area_names_are_case_sensitive_and_are_not_restricted_like_line_names() {
    let (mixed, ..) = areas("\"aB Ab\"").expect("areas");
    assert_eq!(mixed[0].0, "aB");
    assert_eq!(mixed[1].0, "Ab");
    // A named cell token is any name, unlike the `<custom-ident>` of a line name: `span` here is
    // useless (no placement can name it) but it must not invalidate the whole template.
    for name in ["span", "auto", "inherit"] {
        let (named, ..) = areas(&format!("\"{name}\"")).expect("areas");
        assert_eq!(named[0].0, name, "{name}");
    }
}

#[test]
fn area_names_may_start_with_digits() {
    let (named, rows, columns) = areas("\"2xl 2xl\"").expect("areas");
    assert_eq!(named, vec![("2xl".to_owned(), 1, 2, 1, 3)]);
    assert_eq!((rows, columns), (1, 2));
}

#[test]
fn none_clears_the_area_template() {
    let style = grid("grid-template-areas:\"a\";grid-template-areas:none");
    assert!(style.style().template_areas.is_none());
}

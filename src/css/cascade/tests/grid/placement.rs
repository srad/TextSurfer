use super::grid;
use crate::core::style::{GridAutoFlow, GridLines, GridPlacement};

impl super::Grid {
    fn placement(&self, value: GridPlacement) -> String {
        match value {
            GridPlacement::Auto => "auto".to_owned(),
            GridPlacement::Line(line) => line.to_string(),
            GridPlacement::NamedLine(name, index) => format!("{} {index}", self.ident(name)),
            GridPlacement::Span(count) => format!("span {count}"),
            GridPlacement::NamedSpan(name, count) => {
                format!("span {} {count}", self.ident(name))
            }
        }
    }

    fn lines(&self, value: GridLines) -> (String, String) {
        (self.placement(value.start), self.placement(value.end))
    }
}

#[test]
fn every_grid_line_form_computes_to_its_placement() {
    for (source, expected) in [
        ("auto", "auto"),
        ("3", "3"),
        ("-1", "-1"),
        ("span 2", "span 2"),
        ("header", "header 1"),
        ("header 2", "header 2"),
        ("2 header", "header 2"),
        ("span header", "span header 1"),
        ("span header 3", "span header 3"),
        ("span 3 header", "span header 3"),
        ("header span", "span header 1"),
        ("3 span", "span 3"),
        ("header 3 span", "span header 3"),
        ("3 header span", "span header 3"),
    ] {
        let style = grid(&format!("grid-row-start:{source}"));
        assert_eq!(
            style.placement(style.style().row.start),
            expected,
            "{source}"
        );
    }
}

#[test]
fn a_zero_line_and_a_zero_span_are_rejected_before_they_can_reach_layout() {
    // Taffy panics on a grid line of zero, so this must never leave the cascade.
    for source in ["0", "span 0", "-0", "header 0", "span header 0", "span"] {
        let style = grid(&format!("grid-row-start:4;grid-row-start:{source}"));
        assert_eq!(
            style.placement(style.style().row.start),
            "4",
            "{source} must leave the earlier value intact"
        );
    }
}

#[test]
fn the_two_value_shorthands_apply_the_slash_separated_lines() {
    let style = grid("grid-row:2 / 4;grid-column:1 / span 3");
    assert_eq!(style.lines(style.style().row), ("2".into(), "4".into()));
    assert_eq!(
        style.lines(style.style().column),
        ("1".into(), "span 3".into())
    );
}

#[test]
fn an_omitted_end_is_auto_except_after_a_bare_name_which_repeats() {
    let numeric = grid("grid-row:2");
    assert_eq!(
        numeric.lines(numeric.style().row),
        ("2".into(), "auto".into())
    );
    let named = grid("grid-row:header");
    assert_eq!(
        named.lines(named.style().row),
        ("header 1".into(), "header 1".into())
    );
    let span = grid("grid-column:span 2");
    assert_eq!(
        span.lines(span.style().column),
        ("span 2".into(), "auto".into())
    );
    for source in ["header 2", "2 header"] {
        let indexed = grid(&format!("grid-row:{source}"));
        assert_eq!(
            indexed.lines(indexed.style().row),
            ("header 2".into(), "auto".into()),
            "{source}"
        );
    }
}

#[test]
fn none_is_a_valid_named_grid_line() {
    let style = grid("grid-row:none");
    assert_eq!(
        style.lines(style.style().row),
        ("none 1".into(), "none 1".into())
    );
}

#[test]
fn grid_area_fills_all_four_lines_from_one_to_four_values() {
    // One bare name sets all four, which is how `grid-area: header` resolves through the
    // `header-start` / `header-end` lines the area template implies.
    let one = grid("grid-area:header");
    assert_eq!(
        one.lines(one.style().row),
        ("header 1".into(), "header 1".into())
    );
    assert_eq!(
        one.lines(one.style().column),
        ("header 1".into(), "header 1".into())
    );

    let two = grid("grid-area:2 / 3");
    assert_eq!(two.lines(two.style().row), ("2".into(), "auto".into()));
    assert_eq!(two.lines(two.style().column), ("3".into(), "auto".into()));

    let four = grid("grid-area:1 / 2 / 3 / 4");
    assert_eq!(four.lines(four.style().row), ("1".into(), "3".into()));
    assert_eq!(four.lines(four.style().column), ("2".into(), "4".into()));
}

#[test]
fn auto_flow_accepts_both_keyword_orders_and_requires_at_least_one() {
    for (source, expected) in [
        ("row", GridAutoFlow::Row),
        ("column", GridAutoFlow::Column),
        ("dense", GridAutoFlow::RowDense),
        ("row dense", GridAutoFlow::RowDense),
        ("dense row", GridAutoFlow::RowDense),
        ("column dense", GridAutoFlow::ColumnDense),
        ("dense column", GridAutoFlow::ColumnDense),
    ] {
        assert_eq!(
            grid(&format!("grid-auto-flow:{source}")).style().auto_flow,
            expected,
            "{source}"
        );
    }
    for source in ["row column", "dense dense", "sideways"] {
        assert_eq!(
            grid(&format!("grid-auto-flow:column;grid-auto-flow:{source}"))
                .style()
                .auto_flow,
            GridAutoFlow::Column,
            "{source}"
        );
    }
}

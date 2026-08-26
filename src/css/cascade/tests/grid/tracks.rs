use super::{cells, fr, grid, min_content};
use crate::core::style::{
    CssNumber, CssPercentage, GridLength, GridTemplateComponent, RepeatCount, TrackBreadthMax,
    TrackBreadthMin, TrackSize,
};

fn single(source: &str) -> Vec<TrackSize> {
    grid(&format!("grid-template-columns:{source}"))
        .columns()
        .expect("template")
        .components
        .iter()
        .map(|component| match component {
            GridTemplateComponent::Single(track) => *track,
            GridTemplateComponent::Repeat(_) => panic!("expected single tracks"),
        })
        .collect()
}

#[test]
fn every_track_breadth_form_computes_to_its_min_and_max_pair() {
    assert_eq!(single("10ch"), vec![cells(10)]);
    assert_eq!(single("auto"), vec![TrackSize::AUTO]);
    assert_eq!(single("min-content"), vec![min_content()]);
    assert_eq!(
        single("max-content"),
        vec![TrackSize::breadth(
            TrackBreadthMin::MaxContent,
            TrackBreadthMax::MaxContent
        )]
    );
    assert_eq!(single("1fr"), vec![fr(1.0)]);
    assert_eq!(
        single("25%"),
        vec![TrackSize::breadth(
            TrackBreadthMin::Length(GridLength::Percent(CssPercentage::new(2_500))),
            TrackBreadthMax::Length(GridLength::Percent(CssPercentage::new(2_500))),
        )]
    );
    assert_eq!(
        single("10ch auto 2fr"),
        vec![cells(10), TrackSize::AUTO, fr(2.0)]
    );
}

#[test]
fn math_functions_are_preserved_for_layout_time_resolution() {
    for source in [
        "calc(10ch + 25%)",
        "min(10ch, 25%)",
        "max(10ch, 25%)",
        "clamp(2ch, 25%, 20ch)",
    ] {
        let tracks = single(source);
        assert!(
            matches!(
                tracks.as_slice(),
                [TrackSize {
                    min: TrackBreadthMin::Length(GridLength::Calc(_)),
                    max: TrackBreadthMax::Length(GridLength::Calc(_)),
                }]
            ),
            "{source}"
        );
    }
}

#[test]
fn minmax_and_fit_content_map_onto_the_pair_the_spec_defines() {
    assert_eq!(
        single("minmax(10ch, 1fr)"),
        vec![TrackSize::breadth(
            TrackBreadthMin::Length(GridLength::Cells(10)),
            TrackBreadthMax::Fr(CssNumber::new(1.0).unwrap()),
        )]
    );
    assert_eq!(
        single("minmax(auto, 20ch)"),
        vec![TrackSize::breadth(
            TrackBreadthMin::Auto,
            TrackBreadthMax::Length(GridLength::Cells(20)),
        )]
    );
    // `fit-content(x)` is `minmax(auto, fit-content(x))`.
    assert_eq!(
        single("fit-content(8ch)"),
        vec![TrackSize::breadth(
            TrackBreadthMin::Auto,
            TrackBreadthMax::FitContent(GridLength::Cells(8)),
        )]
    );
}

#[test]
fn a_flexible_minimum_invalidates_the_whole_declaration() {
    let style = grid("grid-template-columns:10ch;grid-template-columns:minmax(1fr, 2fr)");
    assert_eq!(
        style
            .columns()
            .expect("earlier value survives")
            .components
            .len(),
        1
    );
    // `fit-content()` is a track size, never a minmax argument.
    let nested =
        grid("grid-template-columns:10ch;grid-template-columns:minmax(fit-content(2ch), 1fr)");
    assert_eq!(
        nested.columns().expect("earlier value survives").components,
        vec![GridTemplateComponent::Single(cells(10))]
    );
}

#[test]
fn repeat_counts_and_auto_repetitions_parse_with_their_track_lists() {
    let style = grid("grid-template-columns:repeat(3, 10ch 1fr)");
    let template = style.columns().expect("template");
    let GridTemplateComponent::Repeat(repeat) = &template.components[0] else {
        panic!("expected a repetition");
    };
    assert_eq!(repeat.count, RepeatCount::Count(3));
    assert_eq!(repeat.tracks, vec![cells(10), fr(1.0)]);

    for (source, expected) in [
        ("repeat(auto-fill, 10ch)", RepeatCount::AutoFill),
        ("repeat(auto-fit, 10ch)", RepeatCount::AutoFit),
    ] {
        let style = grid(&format!("grid-template-columns:{source}"));
        let template = style.columns().expect("template");
        let GridTemplateComponent::Repeat(repeat) = &template.components[0] else {
            panic!("expected a repetition");
        };
        assert_eq!(repeat.count, expected, "{source}");
    }
}

#[test]
fn an_auto_repeat_requires_one_repetition_and_fixed_tracks_throughout() {
    // Taffy discards such a list at layout time; CSS requires the declaration itself to be
    // invalid, so the earlier value has to survive.
    for source in [
        "repeat(auto-fill, 10ch) repeat(auto-fit, 5ch)",
        "auto repeat(auto-fill, 10ch)",
        "1fr repeat(auto-fill, 10ch)",
        "repeat(auto-fill, auto)",
        "min-content repeat(auto-fit, 10ch)",
    ] {
        let style = grid(&format!(
            "grid-template-columns:10ch;grid-template-columns:{source}"
        ));
        assert_eq!(
            style.columns().expect("earlier value survives").components,
            vec![GridTemplateComponent::Single(cells(10))],
            "{source}"
        );
    }
    // A percentage is a fixed component, so this one is valid.
    let valid = grid("grid-template-columns:25% repeat(auto-fill, 10ch)");
    assert_eq!(valid.columns().expect("template").components.len(), 2);
}

#[test]
fn repeat_rejects_zero_and_non_positive_counts_and_empty_track_lists() {
    for source in ["repeat(0, 10ch)", "repeat(-2, 10ch)", "repeat(2)"] {
        let style = grid(&format!(
            "grid-template-columns:10ch;grid-template-columns:{source}"
        ));
        assert_eq!(
            style.columns().expect("earlier value survives").components,
            vec![GridTemplateComponent::Single(cells(10))],
            "{source}"
        );
    }
}

#[test]
fn line_names_are_stored_one_set_per_line_including_both_edges() {
    let style = grid("grid-template-columns:[full-start] 10ch [mid] 1fr [full-end]");
    let template = style.columns().expect("template");
    assert_eq!(template.components.len(), 2);
    assert_eq!(
        style.names(&template.line_names),
        vec![vec!["full-start"], vec!["mid"], vec!["full-end"]]
    );

    // Unnamed lines still get a set, so the vector always has `components.len() + 1` entries.
    let sparse = grid("grid-template-columns:[start] 10ch 1fr");
    let template = sparse.columns().expect("template");
    assert_eq!(
        sparse.names(&template.line_names),
        vec![vec!["start"], Vec::<&str>::new(), Vec::<&str>::new()]
    );
}

#[test]
fn a_repetition_stores_one_line_name_set_per_repeated_line() {
    // Taffy asserts on any non-zero length other than `tracks.len() + 1`.
    let style = grid("grid-template-columns:repeat(2, [a] 10ch 1fr)");
    let template = style.columns().expect("template");
    let GridTemplateComponent::Repeat(repeat) = &template.components[0] else {
        panic!("expected a repetition");
    };
    assert_eq!(repeat.line_names.len(), repeat.tracks.len() + 1);
    assert_eq!(
        style.names(&repeat.line_names),
        vec![vec!["a"], Vec::<&str>::new(), Vec::<&str>::new()]
    );
}

#[test]
fn multiple_names_on_one_line_share_a_set_and_are_case_sensitive() {
    let style = grid("grid-template-columns:[a B] 10ch [b]");
    let template = style.columns().expect("template");
    assert_eq!(
        style.names(&template.line_names),
        vec![vec!["a", "B"], vec!["b"]]
    );
}

#[test]
fn reserved_words_are_not_valid_line_names() {
    for name in [
        "span",
        "auto",
        "initial",
        "inherit",
        "unset",
        "revert",
        "revert-layer",
        "default",
    ] {
        let style = grid(&format!(
            "grid-template-columns:10ch;grid-template-columns:[{name}] 20ch"
        ));
        assert_eq!(
            style.columns().expect("earlier value survives").components,
            vec![GridTemplateComponent::Single(cells(10))],
            "{name}"
        );
    }

    let none = grid("grid-template-columns:[none] 10ch");
    assert_eq!(
        none.names(&none.columns().expect("template").line_names),
        vec![vec!["none"], Vec::<&str>::new()]
    );
}

#[test]
fn repeat_accepts_large_bounded_counts_but_rejects_excessive_counts() {
    let valid = grid("grid-template-columns:repeat(10000, 1ch)");
    assert_eq!(valid.columns().expect("template").components.len(), 1);
    let invalid = grid("grid-template-columns:10ch;grid-template-columns:repeat(10001, 1ch)");
    assert_eq!(
        invalid
            .columns()
            .expect("earlier value survives")
            .components,
        vec![GridTemplateComponent::Single(cells(10))]
    );
}

#[test]
fn auto_repeat_rejects_math_tracks_that_taffy_cannot_classify_as_fixed() {
    let style =
        grid("grid-template-columns:10ch;grid-template-columns:repeat(auto-fill, calc(10ch + 1%))");
    assert_eq!(
        style.columns().expect("earlier value survives").components,
        vec![GridTemplateComponent::Single(cells(10))]
    );
}

#[test]
fn rows_resolve_lengths_against_the_vertical_axis() {
    // 16px is one row but two columns' worth of an 8x16 cell.
    let style = grid("grid-template-rows:16px;grid-template-columns:16px");
    assert_eq!(
        style.rows().expect("rows").components,
        vec![GridTemplateComponent::Single(cells(1))]
    );
    assert_eq!(
        style.columns().expect("columns").components,
        vec![GridTemplateComponent::Single(cells(2))]
    );
}

#[test]
fn none_clears_a_template_and_auto_clears_the_implicit_tracks() {
    let style = grid("grid-template-columns:10ch;grid-template-columns:none");
    assert!(style.style().template_columns.is_none());
    let auto = grid("grid-auto-columns:10ch;grid-auto-columns:auto");
    assert!(auto.style().auto_columns.is_none());
}

#[test]
fn implicit_track_lists_take_bare_track_sizes_only() {
    let style = grid("grid-auto-rows:10ch minmax(1ch, 1fr);grid-auto-columns:1fr");
    assert_eq!(
        style.auto_rows().expect("auto rows"),
        [
            cells(5),
            TrackSize::breadth(
                TrackBreadthMin::Length(GridLength::Cells(1)),
                TrackBreadthMax::Fr(CssNumber::new(1.0).unwrap()),
            )
        ]
    );
    assert_eq!(style.auto_columns().expect("auto columns"), [fr(1.0)]);
    // Repeats and line names are not part of the implicit-track grammar.
    let invalid = grid("grid-auto-rows:10ch;grid-auto-rows:repeat(2, 1fr)");
    assert_eq!(
        invalid.auto_rows().expect("earlier value survives"),
        [cells(5)]
    );
}

#[test]
fn subgrid_and_masonry_are_rejected_so_the_grid_stays_a_plain_one() {
    for source in ["subgrid", "masonry", "subgrid [a]"] {
        let style = grid(&format!(
            "grid-template-columns:10ch;grid-template-columns:{source}"
        ));
        assert_eq!(
            style.columns().expect("earlier value survives").components,
            vec![GridTemplateComponent::Single(cells(10))],
            "{source}"
        );
    }
}

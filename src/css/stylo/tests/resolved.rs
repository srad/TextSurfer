//! Store-free descriptions of the values that `ComputedStyle` holds by handle.
//!
//! `CssCalc`, `GridTemplate`, `GridTracks`, `GridAreas` and `GridIdent` are indices into the
//! `StyleStore` of the tree that produced them. Two cascades intern in whatever order they happen
//! to visit declarations, so comparing the handles directly compares nothing — a single-declaration
//! test passes because both sides allocated index 0, and the same test fails the moment a second
//! value is interned. That is a property of the test, not of the mapper, so the oracle resolves
//! every handle through its own tree before comparing.
//!
//! Descriptions are strings because the shapes are recursive and a textual diff is what a failing
//! assertion needs to be readable.

use std::fmt::Write;

use crate::core::style::{
    CssCalc, CssGap, CssInset, CssMargin, CssMaxSize, CssPadding, CssSize, FlexBasis, GridIdent,
    GridLength, GridPlacement, GridStyle, GridTemplateComponent, GridTracks, StyleTree, TrackSize,
};

/// Bases a `calc()` is probed at. Zero separates the length part from the percentage part, and the
/// rest catch a wrong percentage scale or a mis-ordered `min`/`max`/`clamp`.
const PROBES: [f32; 5] = [0.0, 1.0, 10.0, 40.0, 100.0];

pub(super) fn calc(tree: &StyleTree, value: CssCalc) -> String {
    let mut out = String::from("calc(");
    for (index, basis) in PROBES.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        match tree.resolve_calc(value, *basis) {
            Some(resolved) => {
                let _ = write!(out, "{resolved:.4}");
            }
            None => out.push('!'),
        }
    }
    out.push(')');
    out
}

pub(super) fn size(tree: &StyleTree, value: CssSize) -> String {
    match value {
        CssSize::Auto => "auto".into(),
        CssSize::Cells(cells) => format!("{cells}c"),
        CssSize::Percent(percent) => format!("{}bp", percent.basis_points()),
        CssSize::Calc(handle) => calc(tree, handle),
    }
}

pub(super) fn max_size(tree: &StyleTree, value: CssMaxSize) -> String {
    match value {
        CssMaxSize::None => "none".into(),
        CssMaxSize::Cells(cells) => format!("{cells}c"),
        CssMaxSize::Percent(percent) => format!("{}bp", percent.basis_points()),
        CssMaxSize::Calc(handle) => calc(tree, handle),
    }
}

pub(super) fn margin(tree: &StyleTree, value: CssMargin) -> String {
    match value {
        CssMargin::Auto => "auto".into(),
        CssMargin::Cells(cells) => format!("{cells}c"),
        CssMargin::Percent(percent) => format!("{}bp", percent.basis_points()),
        CssMargin::Calc(handle) => calc(tree, handle),
    }
}

/// `CssPadding` spells zero twice — `Zero` is the enum default and `Cells(0)` is what resolving
/// `padding: 0` gives — and `CssPadding::cells()` returns `Some(0)` for both. The description
/// collapses them, so the oracle compares the value rather than which spelling each engine reached
/// for.
pub(super) fn padding(tree: &StyleTree, value: CssPadding) -> String {
    match value {
        CssPadding::Zero | CssPadding::Cells(0) => "0c".into(),
        CssPadding::Cells(cells) => format!("{cells}c"),
        CssPadding::Percent(percent) => format!("{}bp", percent.basis_points()),
        CssPadding::Calc(handle) => calc(tree, handle),
    }
}

pub(super) fn inset(tree: &StyleTree, value: CssInset) -> String {
    match value {
        CssInset::Auto => "auto".into(),
        CssInset::Cells(cells) => format!("{cells}c"),
        CssInset::Percent(percent) => format!("{}bp", percent.basis_points()),
        CssInset::Calc(handle) => calc(tree, handle),
    }
}

pub(super) fn gap(tree: &StyleTree, value: CssGap) -> String {
    match value {
        CssGap::Normal => "normal".into(),
        CssGap::Cells(cells) => format!("{cells}c"),
        CssGap::Percent(percent) => format!("{}bp", percent.basis_points()),
        CssGap::Calc(handle) => calc(tree, handle),
    }
}

pub(super) fn flex_basis(tree: &StyleTree, value: FlexBasis) -> String {
    match value {
        FlexBasis::Auto => "auto".into(),
        FlexBasis::Content => "content".into(),
        FlexBasis::Cells(axes) => format!("{}c/{}c", axes.horizontal, axes.vertical),
        FlexBasis::Percent(percent) => format!("{}bp", percent.basis_points()),
        FlexBasis::Calc(axes) => {
            format!(
                "{}/{}",
                calc(tree, axes.horizontal),
                calc(tree, axes.vertical)
            )
        }
    }
}

pub(super) fn grid(tree: &StyleTree, value: GridStyle) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "cols={} rows={} areas={} auto-cols={} auto-rows={} flow={:?} row={} col={}",
        template(tree, value.template_columns),
        template(tree, value.template_rows),
        areas(tree, value.template_areas),
        tracks(tree, value.auto_columns),
        tracks(tree, value.auto_rows),
        value.auto_flow,
        lines(tree, value.row),
        lines(tree, value.column),
    );
    out
}

fn template(tree: &StyleTree, handle: Option<crate::core::style::GridTemplate>) -> String {
    let Some(handle) = handle else {
        return "none".into();
    };
    let Some(data) = tree.grid().template(handle) else {
        return "<missing>".into();
    };
    let components = data
        .components
        .iter()
        .map(|component| match component {
            GridTemplateComponent::Single(size) => track(tree, *size),
            GridTemplateComponent::Repeat(repeat) => format!(
                "repeat({:?},[{}],{})",
                repeat.count,
                repeat
                    .tracks
                    .iter()
                    .map(|size| track(tree, *size))
                    .collect::<Vec<_>>()
                    .join(" "),
                names(tree, &repeat.line_names),
            ),
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("[{components}|{}]", names(tree, &data.line_names))
}

fn tracks(tree: &StyleTree, handle: Option<GridTracks>) -> String {
    let Some(handle) = handle else {
        return "none".into();
    };
    let Some(sizes) = tree.grid().tracks(handle) else {
        return "<missing>".into();
    };
    format!(
        "[{}]",
        sizes
            .iter()
            .map(|size| track(tree, *size))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn track(tree: &StyleTree, size: TrackSize) -> String {
    format!(
        "minmax({},{})",
        match size.min {
            crate::core::style::TrackBreadthMin::Auto => "auto".to_owned(),
            crate::core::style::TrackBreadthMin::MinContent => "min-content".to_owned(),
            crate::core::style::TrackBreadthMin::MaxContent => "max-content".to_owned(),
            crate::core::style::TrackBreadthMin::Length(value) => grid_length(tree, value),
        },
        match size.max {
            crate::core::style::TrackBreadthMax::Auto => "auto".to_owned(),
            crate::core::style::TrackBreadthMax::MinContent => "min-content".to_owned(),
            crate::core::style::TrackBreadthMax::MaxContent => "max-content".to_owned(),
            crate::core::style::TrackBreadthMax::Length(value) => grid_length(tree, value),
            crate::core::style::TrackBreadthMax::Fr(value) => format!("{}fr", value.get()),
            crate::core::style::TrackBreadthMax::FitContent(value) =>
                format!("fit-content({})", grid_length(tree, value)),
        }
    )
}

fn grid_length(tree: &StyleTree, value: GridLength) -> String {
    match value {
        GridLength::Cells(cells) => format!("{cells}c"),
        GridLength::Percent(percent) => format!("{}bp", percent.basis_points()),
        GridLength::Calc(handle) => calc(tree, handle),
    }
}

fn areas(tree: &StyleTree, handle: Option<crate::core::style::GridAreas>) -> String {
    let Some(handle) = handle else {
        return "none".into();
    };
    let Some(data) = tree.grid().areas(handle) else {
        return "<missing>".into();
    };
    format!(
        "{}x{}[{}]",
        data.row_count,
        data.column_count,
        data.areas
            .iter()
            .map(|area| format!(
                "{}:{}/{}/{}/{}",
                ident(tree, area.name),
                area.row_start,
                area.column_start,
                area.row_end,
                area.column_end
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn lines(tree: &StyleTree, value: crate::core::style::GridLines) -> String {
    format!(
        "{}/{}",
        placement(tree, value.start),
        placement(tree, value.end)
    )
}

fn placement(tree: &StyleTree, value: GridPlacement) -> String {
    match value {
        GridPlacement::Auto => "auto".into(),
        GridPlacement::Line(line) => format!("{line}"),
        GridPlacement::NamedLine(name, line) => format!("{}({line})", ident(tree, name)),
        GridPlacement::Span(count) => format!("span {count}"),
        GridPlacement::NamedSpan(name, count) => format!("span {}({count})", ident(tree, name)),
    }
}

fn names(tree: &StyleTree, sets: &[Vec<GridIdent>]) -> String {
    sets.iter()
        .map(|set| {
            set.iter()
                .map(|name| ident(tree, *name))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn ident(tree: &StyleTree, name: GridIdent) -> String {
    tree.grid().ident(name).unwrap_or("<missing>").to_owned()
}

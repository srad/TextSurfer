use style::properties::ComputedValues;
use style::values::computed::{
    GridLine, GridTemplateComponent, ImplicitGridTracks, LengthPercentage,
};
use style::values::generics::grid::{
    GenericTrackBreadth, GenericTrackListValue, GenericTrackSize, RepeatCount as StyloRepeatCount,
    TrackRepeat,
};
use style::values::specified::position::GridTemplateAreas;

use crate::core::style::{
    CalcRange, CssNumber, GridArea, GridAreasData, GridAutoFlow, GridIdent, GridLength, GridLines,
    GridPlacement, GridRepeat, GridStyle, GridTemplateComponent as Component, GridTemplateData,
    LengthAxis, RepeatCount, StyleStore, TrackBreadthMax, TrackBreadthMin, TrackSize,
};

use super::length::{Axes, Lengths, Value};

pub(super) fn grid(
    values: &ComputedValues,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> GridStyle {
    GridStyle {
        template_columns: template(
            &values.clone_grid_template_columns(),
            LengthAxis::Horizontal,
            lengths,
            store,
        ),
        template_rows: template(
            &values.clone_grid_template_rows(),
            LengthAxis::Vertical,
            lengths,
            store,
        ),
        template_areas: areas(&values.clone_grid_template_areas(), store),
        auto_columns: implicit(
            &values.clone_grid_auto_columns(),
            LengthAxis::Horizontal,
            lengths,
            store,
        ),
        auto_rows: implicit(
            &values.clone_grid_auto_rows(),
            LengthAxis::Vertical,
            lengths,
            store,
        ),
        auto_flow: auto_flow(values),
        row: GridLines {
            start: placement(&values.clone_grid_row_start(), store),
            end: placement(&values.clone_grid_row_end(), store),
        },
        column: GridLines {
            start: placement(&values.clone_grid_column_start(), store),
            end: placement(&values.clone_grid_column_end(), store),
        },
    }
}

/// `subgrid` and `masonry` are declared out of scope for M6's Grid and map to `none`.
///
/// A template that fails [`GridTemplateData::is_valid`] is refused too: CSS's `<fixed-breadth>`
/// includes `calc()`, so Stylo accepts `repeat(auto-fill, calc(10px + 5%))` while Taffy cannot
/// represent it. Both refusals fall back to the initial value rather than invalidating a
/// declaration, because by mapping time there is no declaration left to invalidate.
fn template(
    value: &GridTemplateComponent,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> Option<crate::core::style::GridTemplate> {
    let GridTemplateComponent::TrackList(list) = value else {
        return None;
    };
    let mut components = Vec::with_capacity(list.values.len());
    for entry in list.values.iter() {
        components.push(match entry {
            GenericTrackListValue::TrackSize(size) => {
                Component::Single(track_size(size, axis, lengths, store)?)
            }
            GenericTrackListValue::TrackRepeat(repeat) => {
                Component::Repeat(track_repeat(repeat, axis, lengths, store)?)
            }
        });
    }
    let data = GridTemplateData {
        components,
        line_names: line_names(&list.line_names, store)?,
    };
    if !data.is_valid() {
        return None;
    }
    store.grid.insert_template(data)
}

fn track_repeat(
    repeat: &TrackRepeat<LengthPercentage, i32>,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> Option<GridRepeat> {
    let count = match repeat.count {
        StyloRepeatCount::Number(count) => RepeatCount::Count(u16::try_from(count).ok()?),
        StyloRepeatCount::AutoFill => RepeatCount::AutoFill,
        StyloRepeatCount::AutoFit => RepeatCount::AutoFit,
    };
    let mut tracks = Vec::with_capacity(repeat.track_sizes.len());
    for size in repeat.track_sizes.iter() {
        tracks.push(track_size(size, axis, lengths, store)?);
    }
    Some(GridRepeat {
        count,
        tracks,
        line_names: line_names(&repeat.line_names, store)?,
    })
}

/// A bare `<track-breadth>` is **not** `minmax(t, t)`.
///
/// `css/cascade/grid/tracks.rs::into_track_size` sets the min side to `Auto` for `auto` *and* for
/// `<flex>`, and to the breadth otherwise; the max side is always the breadth. `fit-content(x)` is
/// `minmax(auto, fit-content(x))` and is never a `minmax()` argument.
fn track_size(
    size: &GenericTrackSize<LengthPercentage>,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> Option<TrackSize> {
    Some(match size {
        GenericTrackSize::Breadth(breadth) => TrackSize::breadth(
            match breadth {
                GenericTrackBreadth::Auto | GenericTrackBreadth::Flex(_) => TrackBreadthMin::Auto,
                GenericTrackBreadth::MinContent => TrackBreadthMin::MinContent,
                GenericTrackBreadth::MaxContent => TrackBreadthMin::MaxContent,
                GenericTrackBreadth::Breadth(length) => {
                    TrackBreadthMin::Length(grid_length(length, axis, lengths, store)?)
                }
            },
            breadth_max(breadth, axis, lengths, store)?,
        ),
        GenericTrackSize::Minmax(min, max) => TrackSize::breadth(
            match min {
                GenericTrackBreadth::Auto => TrackBreadthMin::Auto,
                GenericTrackBreadth::MinContent => TrackBreadthMin::MinContent,
                GenericTrackBreadth::MaxContent => TrackBreadthMin::MaxContent,
                GenericTrackBreadth::Breadth(length) => {
                    TrackBreadthMin::Length(grid_length(length, axis, lengths, store)?)
                }
                // `minmax(1fr, …)` is invalid; Stylo rejects it at parse time, so this is
                // unreachable rather than a policy decision.
                GenericTrackBreadth::Flex(_) => return None,
            },
            breadth_max(max, axis, lengths, store)?,
        ),
        GenericTrackSize::FitContent(breadth) => {
            let GenericTrackBreadth::Breadth(length) = breadth else {
                return None;
            };
            TrackSize::breadth(
                TrackBreadthMin::Auto,
                TrackBreadthMax::FitContent(grid_length(length, axis, lengths, store)?),
            )
        }
    })
}

fn breadth_max(
    breadth: &GenericTrackBreadth<LengthPercentage>,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> Option<TrackBreadthMax> {
    Some(match breadth {
        GenericTrackBreadth::Auto => TrackBreadthMax::Auto,
        GenericTrackBreadth::MinContent => TrackBreadthMax::MinContent,
        GenericTrackBreadth::MaxContent => TrackBreadthMax::MaxContent,
        GenericTrackBreadth::Flex(flex) => {
            TrackBreadthMax::Fr(CssNumber::new(flex.0).unwrap_or(CssNumber::ZERO))
        }
        GenericTrackBreadth::Breadth(length) => {
            TrackBreadthMax::Length(grid_length(length, axis, lengths, store)?)
        }
    })
}

fn grid_length(
    length: &LengthPercentage,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> Option<GridLength> {
    Some(
        match lengths.resolve(length, Axes::same(axis), CalcRange::NonNegative, store)? {
            Value::Cells(cells) => GridLength::Cells(cells.max(0) as usize),
            Value::Percent(fraction) => {
                GridLength::Percent(super::sizing::percentage(fraction.max(0.0)))
            }
            Value::Calc(handle) => GridLength::Calc(handle),
        },
    )
}

fn line_names(
    names: &[style::OwnedSlice<style::values::CustomIdent>],
    store: &mut StyleStore,
) -> Option<Vec<Vec<GridIdent>>> {
    names
        .iter()
        .map(|set| {
            set.iter()
                .map(|name| store.grid.insert_ident(&name.0))
                .collect::<Option<Vec<_>>>()
        })
        .collect()
}

fn implicit(
    tracks: &ImplicitGridTracks,
    axis: LengthAxis,
    lengths: &mut Lengths,
    store: &mut StyleStore,
) -> Option<crate::core::style::GridTracks> {
    if tracks.0.is_empty() {
        return None;
    }
    let mut sizes = Vec::with_capacity(tracks.0.len());
    for size in tracks.0.iter() {
        sizes.push(track_size(size, axis, lengths, store)?);
    }
    store.grid.insert_tracks(sizes)
}

/// Stylo's `NamedArea` already uses 1-based, end-exclusive grid lines — `TemplateAreasParser`
/// starts `rows.start` at 1 and sets `rows.end = row + 1` — which is exactly the numbering
/// `css/cascade/grid/areas.rs` produces. No translation.
fn areas(
    value: &GridTemplateAreas,
    store: &mut StyleStore,
) -> Option<crate::core::style::GridAreas> {
    let GridTemplateAreas::Areas(template) = value else {
        return None;
    };
    let mut mapped = Vec::with_capacity(template.0.areas.len());
    for area in template.0.areas.iter() {
        mapped.push(GridArea {
            name: store.grid.insert_ident(&area.name)?,
            row_start: u16::try_from(area.rows.start).ok()?,
            row_end: u16::try_from(area.rows.end).ok()?,
            column_start: u16::try_from(area.columns.start).ok()?,
            column_end: u16::try_from(area.columns.end).ok()?,
        });
    }
    store.grid.insert_areas(GridAreasData {
        areas: mapped,
        row_count: u16::try_from(template.0.strings.len()).ok()?,
        column_count: u16::try_from(template.0.width).ok()?,
    })
}

/// `grid-auto-flow` is bitflags in Stylo, not an enum: `COLUMN` picks the axis and `DENSE` the
/// packing.
fn auto_flow(values: &ComputedValues) -> GridAutoFlow {
    let flow = values.clone_grid_auto_flow();
    let dense = flow.contains(style::values::specified::position::GridAutoFlow::DENSE);
    if flow.contains(style::values::specified::position::GridAutoFlow::COLUMN) {
        if dense {
            GridAutoFlow::ColumnDense
        } else {
            GridAutoFlow::Column
        }
    } else if dense {
        GridAutoFlow::RowDense
    } else {
        GridAutoFlow::Row
    }
}

/// A named line with `line_num == 0` is the *first* line of that name, which our model stores as
/// index 1 — matching `css/cascade/grid/placement.rs`'s `NamedLine(name, 1)`.
///
/// The `grid-row`/`grid-column`/`grid-area` shorthands never reach here: Stylo expands them into
/// these four longhands, applying the "a bare name implies the same name on the end line" rule
/// itself.
fn placement(line: &GridLine, store: &mut StyleStore) -> GridPlacement {
    if line.is_auto() {
        return GridPlacement::Auto;
    }
    let named = (!line.ident.0.is_empty())
        .then(|| store.grid.insert_ident(&line.ident.0))
        .flatten();
    if line.is_span {
        let count = u16::try_from(line.line_num.max(1)).unwrap_or(1);
        return match named {
            Some(name) => GridPlacement::NamedSpan(name, count),
            None => GridPlacement::Span(count),
        };
    }
    match named {
        Some(name) => {
            let index = i16::try_from(line.line_num).unwrap_or(1);
            GridPlacement::NamedLine(name, if index == 0 { 1 } else { index })
        }
        None => match i16::try_from(line.line_num) {
            Ok(0) | Err(_) => GridPlacement::Auto,
            Ok(index) => GridPlacement::Line(index),
        },
    }
}

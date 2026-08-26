use taffy::geometry::{Line as TaffyLine, Point as TaffyPoint};
use taffy::prelude::{
    BoxSizing as TaffyBoxSizing, Dimension, Display as TaffyDisplay, LengthPercentage,
    LengthPercentageAuto, Rect as TaffyRect, Size as TaffySize, Style as TaffyStyle,
    TaffyFitContent as _,
};
use taffy::style::{
    AlignContent as TaffyAlignContent, AlignContentKeyword as TaffyContentKeyword,
    AlignItems as TaffyAlignItems, AlignItemsKeyword as TaffyItemKeyword,
    AlignmentSafety as TaffySafety, FlexDirection as TaffyFlexDirection, FlexWrap as TaffyFlexWrap,
    GridAutoFlow as TaffyGridAutoFlow, GridPlacement as TaffyGridPlacement,
    GridTemplateArea as TaffyGridTemplateArea, GridTemplateAreas as TaffyGridTemplateAreas,
    GridTemplateComponent as TaffyGridTemplateComponent,
    GridTemplateRepetition as TaffyGridTemplateRepetition, MaxTrackSizingFunction as TaffyMaxTrack,
    MinTrackSizingFunction as TaffyMinTrack, Overflow as TaffyOverflow, Position as TaffyPosition,
    RepetitionCount as TaffyRepetitionCount, TextAlign as TaffyTextAlign,
    TrackSizingFunction as TaffyTrackSizingFunction,
};

use crate::core::style::{
    Alignment, AlignmentSafety, BoxSizing, ContentAlignment, CssGap, CssInset, CssMargin,
    CssMaxSize, CssSize, DisplayInside, FlexBasis, FlexDirection, FlexWrap, GridAreas,
    GridAutoFlow, GridLength, GridLines, GridPlacement, GridTemplate, GridTemplateComponent,
    GridTracks, ItemAlignment, LegacyAlign, Overflow, Position, RepeatCount, StyleTree,
    TrackBreadthMax, TrackBreadthMin, TrackSize,
};

use super::LayoutRect;
use super::flow::FlowBox;

/// Everything one box needs to become a Taffy style. A struct rather than six positional
/// arguments: `root` and `viewport_width` sit next to each other and would otherwise be a `bool`
/// and a `usize` that nothing but their order distinguishes.
pub(super) struct TaffyStyleInput<'a> {
    pub(super) flow: &'a FlowBox,
    pub(super) root: bool,
    pub(super) viewport_width: usize,
    pub(super) parent_direction: Option<FlexDirection>,
    pub(super) calc_values: &'a std::cell::RefCell<Vec<crate::core::style::CssCalc>>,
    pub(super) styles: &'a StyleTree,
}

pub(super) fn taffy_style(input: TaffyStyleInput<'_>) -> TaffyStyle {
    let TaffyStyleInput {
        flow,
        root,
        viewport_width,
        parent_direction,
        calc_values,
        styles,
    } = input;
    if root {
        return TaffyStyle {
            display: TaffyDisplay::Block,
            box_sizing: TaffyBoxSizing::BorderBox,
            size: TaffySize {
                width: Dimension::length(viewport_width as f32),
                height: Dimension::auto(),
            },
            ..Default::default()
        };
    }
    if flow.owner.is_none() {
        return TaffyStyle {
            display: TaffyDisplay::Block,
            size: TaffySize {
                width: Dimension::auto(),
                height: Dimension::auto(),
            },
            ..Default::default()
        };
    }
    // A replaced box has no children to size it, so its own intrinsic size is what Taffy gets when
    // the author supplies nothing. Expressed in the box's own `box-sizing` space, since that is
    // what `size` and `min_size` are measured in.
    let intrinsic = flow.replaced.as_ref().map(|replaced| {
        let (cols, rows) = (replaced.intrinsic_cols, replaced.intrinsic_rows);
        if matches!(flow.style.box_sizing, BoxSizing::BorderBox) {
            let horizontal = flow.style.border.left.layout_width()
                + flow.style.border.right.layout_width()
                + flow.style.padding.left
                + flow.style.padding.right;
            let vertical = flow.style.border.top.layout_width()
                + flow.style.border.bottom.layout_width()
                + flow.style.padding.top
                + flow.style.padding.bottom;
            (cols + horizontal, rows + vertical)
        } else {
            (cols, rows)
        }
    });
    let inside = flow.style.display.inside();
    let is_grid = matches!(inside, Some(DisplayInside::Grid));
    TaffyStyle {
        display: match inside {
            Some(DisplayInside::Flex) => TaffyDisplay::Flex,
            Some(DisplayInside::Grid) => TaffyDisplay::Grid,
            _ => TaffyDisplay::Block,
        },
        text_align: match flow.style.legacy_align {
            LegacyAlign::None => TaffyTextAlign::Auto,
            LegacyAlign::Left => TaffyTextAlign::LegacyLeft,
            LegacyAlign::Right => TaffyTextAlign::LegacyRight,
            LegacyAlign::Center => TaffyTextAlign::LegacyCenter,
        },
        item_is_table: flow.table.is_some(),
        box_sizing: match flow.style.box_sizing {
            BoxSizing::ContentBox => TaffyBoxSizing::ContentBox,
            BoxSizing::BorderBox => TaffyBoxSizing::BorderBox,
        },
        overflow: TaffyPoint {
            x: overflow(flow.style.overflow.x),
            y: overflow(flow.style.overflow.y),
        },
        scrollbar_width: 0.0,
        position: match flow.style.position {
            Position::Absolute | Position::Fixed => TaffyPosition::Absolute,
            Position::Static | Position::Relative | Position::Sticky => TaffyPosition::Relative,
        },
        inset: TaffyRect {
            left: inset(flow.style.inset.left),
            right: inset(flow.style.inset.right),
            top: inset(flow.style.inset.top),
            bottom: inset(flow.style.inset.bottom),
        },
        size: TaffySize {
            // A replaced element has an intrinsic size and does not fill its container when the
            // author says nothing: `input { display: block }` keeps its `size=` width in a real
            // browser, and a `<textarea cols=6>` stays six cells wide.
            width: match (intrinsic.map(|size| size.0), flow.style.width) {
                (Some(width), CssSize::Auto) => Dimension::length(width as f32),
                _ => dimension(flow.style.width, calc_values),
            },
            height: match (intrinsic.map(|size| size.1), flow.style.height) {
                (Some(height), CssSize::Auto) => Dimension::length(height as f32),
                _ if flow.rule && flow.style.height == CssSize::Auto => Dimension::length(1.0),
                _ => dimension(flow.style.height, calc_values),
            },
        },
        min_size: TaffySize {
            width: minimum(flow.style.min_width, calc_values),
            // A replaced element keeps room for its own rows even against a smaller `max-height`.
            // Without this Wikipedia's search field renders blank: it sets `max-height: 2rem` — two
            // cells — and a 1px border costs a whole cell per edge here, leaving zero content rows.
            // Our metric quantises the border, not CSS; honouring the number would be faithful to
            // it and not to the intent. Taffy resolves min over max (`maybe_clamp` is
            // `base.min(max).max(min)`), which is what CSS requires, so a min is all it takes.
            height: match intrinsic.map(|size| size.1) {
                Some(rows) => {
                    // A percentage or `calc()` author minimum cannot be compared here, so the
                    // intrinsic wins outright in that case; only a definite one competes.
                    let authored = match flow.style.min_height {
                        CssSize::Cells(value) => value,
                        _ => 0,
                    };
                    LengthPercentageAuto::length(rows.max(authored) as f32)
                }
                None => minimum(flow.style.min_height, calc_values),
            },
        },
        max_size: TaffySize {
            width: maximum(flow.style.max_width, calc_values),
            height: maximum(flow.style.max_height, calc_values),
        },
        margin: TaffyRect {
            left: margin(flow.style.margin.left),
            right: margin(flow.style.margin.right),
            top: margin(flow.style.margin.top),
            bottom: margin(flow.style.margin.bottom),
        },
        padding: TaffyRect {
            left: LengthPercentage::length(
                flow.style
                    .padding
                    .left
                    .saturating_add(flow.marker.as_ref().map_or(0, |marker| marker.reserve))
                    as f32,
            ),
            right: LengthPercentage::length(flow.style.padding.right as f32),
            top: LengthPercentage::length(flow.style.padding.top as f32),
            bottom: LengthPercentage::length(flow.style.padding.bottom as f32),
        },
        border: TaffyRect {
            left: LengthPercentage::length(flow.style.border.left.layout_width() as f32),
            right: LengthPercentage::length(flow.style.border.right.layout_width() as f32),
            top: LengthPercentage::length(flow.style.border.top.layout_width() as f32),
            bottom: LengthPercentage::length(flow.style.border.bottom.layout_width() as f32),
        },
        flex_direction: match flow.style.flex.direction {
            FlexDirection::Row => TaffyFlexDirection::Row,
            FlexDirection::RowReverse => TaffyFlexDirection::RowReverse,
            FlexDirection::Column => TaffyFlexDirection::Column,
            FlexDirection::ColumnReverse => TaffyFlexDirection::ColumnReverse,
        },
        flex_wrap: match flow.style.flex.wrap {
            FlexWrap::NoWrap => TaffyFlexWrap::NoWrap,
            FlexWrap::Wrap => TaffyFlexWrap::Wrap,
            FlexWrap::WrapReverse => TaffyFlexWrap::WrapReverse,
        },
        flex_grow: flow.style.flex.grow.get(),
        flex_shrink: flow.style.flex.shrink.get(),
        flex_basis: flex_basis(flow.style.flex.basis, parent_direction),
        align_items: Some(item_alignment(flow.style.alignment.align_items)),
        align_self: flow.style.alignment.align_self.map(item_alignment),
        justify_items: Some(item_alignment(flow.style.alignment.justify_items)),
        justify_self: flow.style.alignment.justify_self.map(item_alignment),
        align_content: Some(content_alignment(
            flow.style.alignment.align_content,
            false,
            flow.style.flex.direction,
            is_grid,
        )),
        justify_content: Some(content_alignment(
            flow.style.alignment.justify_content,
            true,
            flow.style.flex.direction,
            is_grid,
        )),
        gap: TaffySize {
            width: gap(flow.style.alignment.column_gap),
            height: gap(flow.style.alignment.row_gap),
        },
        grid_template_columns: template(flow.style.grid.template_columns, styles, calc_values),
        grid_template_rows: template(flow.style.grid.template_rows, styles, calc_values),
        grid_template_column_names: line_names(flow.style.grid.template_columns, styles),
        grid_template_row_names: line_names(flow.style.grid.template_rows, styles),
        grid_auto_columns: auto_tracks(flow.style.grid.auto_columns, styles, calc_values),
        grid_auto_rows: auto_tracks(flow.style.grid.auto_rows, styles, calc_values),
        grid_auto_flow: match flow.style.grid.auto_flow {
            GridAutoFlow::Row => TaffyGridAutoFlow::Row,
            GridAutoFlow::Column => TaffyGridAutoFlow::Column,
            GridAutoFlow::RowDense => TaffyGridAutoFlow::RowDense,
            GridAutoFlow::ColumnDense => TaffyGridAutoFlow::ColumnDense,
        },
        grid_template_areas: template_areas(flow.style.grid.template_areas, styles),
        grid_row: placement_line(flow.style.grid.row, styles),
        grid_column: placement_line(flow.style.grid.column, styles),
        ..Default::default()
    }
}

/// Taffy's grid engine reads the whole track list, its line names and its areas out of the style
/// it is handed, so the interned payloads are expanded here — the one place that knows both our
/// style types and Taffy's.
fn template(
    handle: Option<GridTemplate>,
    styles: &StyleTree,
    values: &std::cell::RefCell<Vec<crate::core::style::CssCalc>>,
) -> Vec<TaffyGridTemplateComponent<String>> {
    let Some(data) = handle.and_then(|handle| styles.grid().template(handle)) else {
        return Vec::new();
    };
    data.components
        .iter()
        .map(|component| match component {
            GridTemplateComponent::Single(track) => {
                TaffyGridTemplateComponent::Single(track_size(*track, values))
            }
            GridTemplateComponent::Repeat(repeat) => {
                TaffyGridTemplateComponent::Repeat(TaffyGridTemplateRepetition {
                    count: match repeat.count {
                        RepeatCount::Count(count) => TaffyRepetitionCount::Count(count),
                        RepeatCount::AutoFill => TaffyRepetitionCount::AutoFill,
                        RepeatCount::AutoFit => TaffyRepetitionCount::AutoFit,
                    },
                    tracks: repeat
                        .tracks
                        .iter()
                        .map(|track| track_size(*track, values))
                        .collect(),
                    line_names: names(&repeat.line_names, styles),
                })
            }
        })
        .collect()
}

fn line_names(handle: Option<GridTemplate>, styles: &StyleTree) -> Vec<Vec<String>> {
    handle
        .and_then(|handle| styles.grid().template(handle))
        .map(|data| names(&data.line_names, styles))
        .unwrap_or_default()
}

fn names(sets: &[Vec<crate::core::style::GridIdent>], styles: &StyleTree) -> Vec<Vec<String>> {
    sets.iter()
        .map(|set| {
            set.iter()
                .filter_map(|name| styles.grid().ident(*name).map(str::to_owned))
                .collect()
        })
        .collect()
}

fn auto_tracks(
    handle: Option<GridTracks>,
    styles: &StyleTree,
    values: &std::cell::RefCell<Vec<crate::core::style::CssCalc>>,
) -> Vec<TaffyTrackSizingFunction> {
    handle
        .and_then(|handle| styles.grid().tracks(handle))
        .map(|tracks| {
            tracks
                .iter()
                .map(|track| track_size(*track, values))
                .collect()
        })
        .unwrap_or_default()
}

fn track_size(
    value: TrackSize,
    values: &std::cell::RefCell<Vec<crate::core::style::CssCalc>>,
) -> TaffyTrackSizingFunction {
    TaffyTrackSizingFunction {
        min: match value.min {
            TrackBreadthMin::Auto => TaffyMinTrack::auto(),
            TrackBreadthMin::MinContent => TaffyMinTrack::min_content(),
            TrackBreadthMin::MaxContent => TaffyMinTrack::max_content(),
            TrackBreadthMin::Length(GridLength::Cells(cells)) => {
                TaffyMinTrack::length(cells as f32)
            }
            TrackBreadthMin::Length(GridLength::Percent(percent)) => {
                TaffyMinTrack::percent(percent.basis_points() as f32 / 10_000.0)
            }
            TrackBreadthMin::Length(GridLength::Calc(value)) => {
                TaffyMinTrack::calc(calc_handle(values, value))
            }
        },
        max: match value.max {
            TrackBreadthMax::Auto => TaffyMaxTrack::auto(),
            TrackBreadthMax::MinContent => TaffyMaxTrack::min_content(),
            TrackBreadthMax::MaxContent => TaffyMaxTrack::max_content(),
            TrackBreadthMax::Length(GridLength::Cells(cells)) => {
                TaffyMaxTrack::length(cells as f32)
            }
            TrackBreadthMax::Length(GridLength::Percent(percent)) => {
                TaffyMaxTrack::percent(percent.basis_points() as f32 / 10_000.0)
            }
            TrackBreadthMax::Length(GridLength::Calc(value)) => {
                TaffyMaxTrack::calc(calc_handle(values, value))
            }
            TrackBreadthMax::Fr(value) => TaffyMaxTrack::fr(value.get()),
            TrackBreadthMax::FitContent(GridLength::Cells(cells)) => {
                TaffyMaxTrack::fit_content(LengthPercentage::length(cells as f32))
            }
            TrackBreadthMax::FitContent(GridLength::Percent(percent)) => {
                TaffyMaxTrack::fit_content(LengthPercentage::percent(
                    percent.basis_points() as f32 / 10_000.0,
                ))
            }
            TrackBreadthMax::FitContent(GridLength::Calc(value)) => {
                TaffyMaxTrack::fit_content(LengthPercentage::calc(calc_handle(values, value)))
            }
        },
    }
}

fn template_areas(
    handle: Option<GridAreas>,
    styles: &StyleTree,
) -> Option<TaffyGridTemplateAreas<String>> {
    let data = handle.and_then(|handle| styles.grid().areas(handle))?;
    Some(TaffyGridTemplateAreas {
        areas: data
            .areas
            .iter()
            .filter_map(|area| {
                Some(TaffyGridTemplateArea {
                    name: styles.grid().ident(area.name)?.to_owned(),
                    row_start: area.row_start,
                    row_end: area.row_end,
                    column_start: area.column_start,
                    column_end: area.column_end,
                })
            })
            .collect(),
        row_count: data.row_count,
        column_count: data.column_count,
    })
}

fn placement_line(value: GridLines, styles: &StyleTree) -> TaffyLine<TaffyGridPlacement<String>> {
    TaffyLine {
        start: placement(value.start, styles),
        end: placement(value.end, styles),
    }
}

fn placement(value: GridPlacement, styles: &StyleTree) -> TaffyGridPlacement<String> {
    let named = |name: crate::core::style::GridIdent| styles.grid().ident(name).map(str::to_owned);
    match value {
        GridPlacement::Auto => TaffyGridPlacement::Auto,
        GridPlacement::Line(line) => TaffyGridPlacement::Line(line.into()),
        GridPlacement::Span(count) => TaffyGridPlacement::Span(count),
        GridPlacement::NamedLine(name, index) => named(name)
            .map(|name| TaffyGridPlacement::NamedLine(name, index))
            .unwrap_or(TaffyGridPlacement::Auto),
        GridPlacement::NamedSpan(name, count) => named(name)
            .map(|name| TaffyGridPlacement::NamedSpan(name, count))
            .unwrap_or(TaffyGridPlacement::Auto),
    }
}

fn overflow(value: Overflow) -> TaffyOverflow {
    match value {
        Overflow::Visible => TaffyOverflow::Visible,
        Overflow::Hidden | Overflow::Auto => TaffyOverflow::Hidden,
        Overflow::Clip => TaffyOverflow::Clip,
        Overflow::Scroll => TaffyOverflow::Scroll,
    }
}

fn inset(value: CssInset) -> LengthPercentageAuto {
    match value {
        CssInset::Auto => LengthPercentageAuto::auto(),
        CssInset::Cells(value) => LengthPercentageAuto::length(value as f32),
        CssInset::Percent(value) => {
            LengthPercentageAuto::percent(value.basis_points() as f32 / 10_000.0)
        }
    }
}

fn dimension(
    value: CssSize,
    values: &std::cell::RefCell<Vec<crate::core::style::CssCalc>>,
) -> Dimension {
    match value {
        CssSize::Auto => Dimension::auto(),
        CssSize::Cells(value) => Dimension::length(value as f32),
        CssSize::Percent(value) => Dimension::percent(value.basis_points() as f32 / 10_000.0),
        CssSize::Calc(value) => Dimension::calc(calc_handle(values, value)),
    }
}

fn minimum(
    value: CssSize,
    values: &std::cell::RefCell<Vec<crate::core::style::CssCalc>>,
) -> LengthPercentageAuto {
    match value {
        CssSize::Auto => LengthPercentageAuto::auto(),
        CssSize::Cells(value) => LengthPercentageAuto::length(value as f32),
        CssSize::Percent(value) => {
            LengthPercentageAuto::percent(value.basis_points() as f32 / 10_000.0)
        }
        CssSize::Calc(value) => LengthPercentageAuto::calc(calc_handle(values, value)),
    }
}

fn maximum(
    value: CssMaxSize,
    values: &std::cell::RefCell<Vec<crate::core::style::CssCalc>>,
) -> LengthPercentageAuto {
    match value {
        CssMaxSize::None => LengthPercentageAuto::auto(),
        CssMaxSize::Cells(value) => LengthPercentageAuto::length(value as f32),
        CssMaxSize::Percent(value) => {
            LengthPercentageAuto::percent(value.basis_points() as f32 / 10_000.0)
        }
        CssMaxSize::Calc(value) => LengthPercentageAuto::calc(calc_handle(values, value)),
    }
}

fn calc_handle(
    values: &std::cell::RefCell<Vec<crate::core::style::CssCalc>>,
    value: crate::core::style::CssCalc,
) -> *const () {
    let mut values = values.borrow_mut();
    values.push(value);
    std::ptr::without_provenance(values.len() << 3)
}

fn flex_basis(value: FlexBasis, parent_direction: Option<FlexDirection>) -> Dimension {
    match value {
        FlexBasis::Auto => Dimension::auto(),
        FlexBasis::Content => Dimension::content(),
        FlexBasis::Cells(value) => {
            if parent_direction.is_some_and(FlexDirection::is_column) {
                Dimension::length(value.vertical as f32)
            } else {
                Dimension::length(value.horizontal as f32)
            }
        }
        FlexBasis::Percent(value) => Dimension::percent(value.basis_points() as f32 / 10_000.0),
    }
}

fn gap(value: CssGap) -> LengthPercentage {
    match value {
        CssGap::Normal => LengthPercentage::length(0.0),
        CssGap::Cells(value) => LengthPercentage::length(value as f32),
        CssGap::Percent(value) => LengthPercentage::percent(value.basis_points() as f32 / 10_000.0),
    }
}

fn safety(value: AlignmentSafety) -> TaffySafety {
    match value {
        AlignmentSafety::Unsafe => TaffySafety::Unsafe,
        AlignmentSafety::Safe => TaffySafety::Safe,
    }
}

fn item_alignment(value: Alignment<ItemAlignment>) -> TaffyAlignItems {
    TaffyAlignItems {
        keyword: match value.keyword {
            ItemAlignment::Normal | ItemAlignment::Stretch => TaffyItemKeyword::Stretch,
            ItemAlignment::Start => TaffyItemKeyword::Start,
            ItemAlignment::End => TaffyItemKeyword::End,
            ItemAlignment::FlexStart => TaffyItemKeyword::FlexStart,
            ItemAlignment::FlexEnd => TaffyItemKeyword::FlexEnd,
            ItemAlignment::SelfStart => TaffyItemKeyword::SelfStart,
            ItemAlignment::SelfEnd => TaffyItemKeyword::SelfEnd,
            ItemAlignment::Center => TaffyItemKeyword::Center,
            ItemAlignment::Baseline => TaffyItemKeyword::Baseline,
        },
        safety: safety(value.safety),
    }
}

/// `normal` resolves per formatting context: it behaves as `flex-start` for a flex container's
/// main axis, but as `stretch` for a grid container in both axes, which is also what Taffy's grid
/// falls back to when the style leaves the value unset.
fn content_alignment(
    value: Alignment<ContentAlignment>,
    justify: bool,
    direction: FlexDirection,
    grid: bool,
) -> TaffyAlignContent {
    let keyword = match value.keyword {
        ContentAlignment::Normal if justify && !grid => TaffyContentKeyword::FlexStart,
        ContentAlignment::Normal | ContentAlignment::Stretch => TaffyContentKeyword::Stretch,
        ContentAlignment::Start => TaffyContentKeyword::Start,
        ContentAlignment::End => TaffyContentKeyword::End,
        ContentAlignment::FlexStart => TaffyContentKeyword::FlexStart,
        ContentAlignment::FlexEnd => TaffyContentKeyword::FlexEnd,
        ContentAlignment::Center => TaffyContentKeyword::Center,
        ContentAlignment::SpaceBetween => TaffyContentKeyword::SpaceBetween,
        ContentAlignment::SpaceAround => TaffyContentKeyword::SpaceAround,
        ContentAlignment::SpaceEvenly => TaffyContentKeyword::SpaceEvenly,
        ContentAlignment::Left => TaffyContentKeyword::Start,
        ContentAlignment::Right if direction.is_column() => TaffyContentKeyword::Start,
        ContentAlignment::Right => TaffyContentKeyword::End,
    };
    TaffyAlignContent {
        keyword,
        safety: safety(value.safety),
    }
}

fn margin(value: CssMargin) -> LengthPercentageAuto {
    match value {
        CssMargin::Auto => LengthPercentageAuto::auto(),
        CssMargin::Cells(value) => LengthPercentageAuto::length(value as f32),
    }
}

pub(super) fn layout_rect(col: f32, row: f32, size: TaffySize<f32>) -> LayoutRect {
    let right = (col + size.width.max(0.0)).max(0.0).round() as usize;
    let bottom = (row + size.height.max(0.0)).max(0.0).round() as usize;
    let col = col.max(0.0).round() as usize;
    let row = row.max(0.0).round() as usize;
    LayoutRect {
        col,
        row,
        width: right.saturating_sub(col),
        height: bottom.saturating_sub(row),
    }
}

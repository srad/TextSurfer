use taffy::geometry::Point as TaffyPoint;
use taffy::prelude::{
    BoxSizing as TaffyBoxSizing, Dimension, Display as TaffyDisplay, LengthPercentage,
    LengthPercentageAuto, Rect as TaffyRect, Size as TaffySize, Style as TaffyStyle,
};
use taffy::style::{
    AlignContent as TaffyAlignContent, AlignContentKeyword as TaffyContentKeyword,
    AlignItems as TaffyAlignItems, AlignItemsKeyword as TaffyItemKeyword,
    AlignmentSafety as TaffySafety, FlexDirection as TaffyFlexDirection, FlexWrap as TaffyFlexWrap,
    Overflow as TaffyOverflow, Position as TaffyPosition, TextAlign as TaffyTextAlign,
};

use crate::core::style::{
    Alignment, AlignmentSafety, BoxSizing, ContentAlignment, CssGap, CssInset, CssMargin,
    CssMaxSize, CssSize, FlexBasis, FlexDirection, FlexWrap, ItemAlignment, LegacyAlign, Overflow,
    Position,
};

use super::LayoutRect;
use super::flow::FlowBox;

pub(super) fn taffy_style(
    flow: &FlowBox,
    root: bool,
    viewport_width: usize,
    parent_direction: Option<FlexDirection>,
    calc_values: &std::cell::RefCell<Vec<crate::core::style::CssCalc>>,
) -> TaffyStyle {
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
    TaffyStyle {
        display: if matches!(
            flow.style.display.inside(),
            Some(crate::core::style::DisplayInside::Flex)
        ) {
            TaffyDisplay::Flex
        } else {
            TaffyDisplay::Block
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
        align_items: Some(item_alignment(flow.style.flex.align_items)),
        align_self: flow.style.flex.align_self.map(item_alignment),
        align_content: Some(content_alignment(
            flow.style.flex.align_content,
            false,
            flow.style.flex.direction,
        )),
        justify_content: Some(content_alignment(
            flow.style.flex.justify_content,
            true,
            flow.style.flex.direction,
        )),
        gap: TaffySize {
            width: gap(flow.style.flex.column_gap),
            height: gap(flow.style.flex.row_gap),
        },
        ..Default::default()
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

fn content_alignment(
    value: Alignment<ContentAlignment>,
    justify: bool,
    direction: FlexDirection,
) -> TaffyAlignContent {
    let keyword = match value.keyword {
        ContentAlignment::Normal if justify => TaffyContentKeyword::FlexStart,
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

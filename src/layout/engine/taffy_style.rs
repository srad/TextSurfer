use taffy::prelude::{
    BoxSizing as TaffyBoxSizing, Dimension, Display as TaffyDisplay, LengthPercentage,
    LengthPercentageAuto, Rect as TaffyRect, Size as TaffySize, Style as TaffyStyle,
};
use taffy::style::TextAlign as TaffyTextAlign;

use crate::core::style::{BoxSizing, CssMargin, CssWidth, LegacyAlign};

use super::LayoutRect;
use super::flow::FlowBox;

pub(super) fn taffy_style(flow: &FlowBox, root: bool, viewport_width: usize) -> TaffyStyle {
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
    TaffyStyle {
        display: TaffyDisplay::Block,
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
        size: TaffySize {
            width: match flow.style.width {
                CssWidth::Auto => Dimension::auto(),
                CssWidth::Cells(value) => Dimension::length(value as f32),
                CssWidth::Percent(value) => {
                    Dimension::percent(value.basis_points() as f32 / 10_000.0)
                }
            },
            height: if flow.rule {
                Dimension::length(1.0)
            } else {
                Dimension::auto()
            },
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
        ..Default::default()
    }
}

fn margin(value: CssMargin) -> LengthPercentageAuto {
    match value {
        CssMargin::Auto => LengthPercentageAuto::auto(),
        CssMargin::Cells(value) => LengthPercentageAuto::length(value as f32),
    }
}

pub(super) fn layout_rect(col: f32, row: f32, size: TaffySize<f32>) -> LayoutRect {
    LayoutRect {
        col: col.max(0.0).round() as usize,
        row: row.max(0.0).round() as usize,
        width: size.width.max(0.0).round() as usize,
        height: size.height.max(0.0).round() as usize,
    }
}

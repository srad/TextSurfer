use crate::core::dom::NodeId;
use crate::core::style::{CaptionSide, ComputedStyle};
use crate::layout::text_flow::{format_inline, formatted_height};
use crate::layout::{BorderStroke, LayoutBox, LayoutRect};

use super::content::{CellLayout, append_cell_content, resolve_items};
use super::geometry::{EdgeInsets, add_fill};
use super::model::TableModel;
use super::{TableFormatter, TableLimits, TableOutput};

pub(super) struct CaptionLayout {
    pub(super) node: NodeId,
    pub(super) style: ComputedStyle,
    pub(super) layout: CellLayout,
    pub(super) border_rect: LayoutRect,
    pub(super) content_rect: LayoutRect,
}

pub(super) struct CaptionBands {
    pub(super) top: Vec<CaptionLayout>,
    pub(super) bottom: Vec<CaptionLayout>,
}

impl CaptionBands {
    pub(super) fn top_height(&self) -> usize {
        self.top
            .iter()
            .map(|caption| caption.border_rect.height)
            .sum()
    }

    pub(super) fn bottom_height(&self) -> usize {
        self.bottom
            .iter()
            .map(|caption| caption.border_rect.height)
            .sum()
    }
}

pub(super) fn natural_width(
    formatter: &TableFormatter<'_>,
    model: &TableModel,
    limits: TableLimits,
    nesting: usize,
) -> usize {
    model
        .captions
        .iter()
        .map(|caption| {
            formatter
                .cell_metrics(&[*caption], formatter.styles.get(*caption), limits, nesting)
                .maximum
        })
        .max()
        .unwrap_or(0)
}

pub(super) fn layout_captions(
    formatter: &TableFormatter<'_>,
    model: &TableModel,
    caption_width: usize,
    limits: TableLimits,
    nesting: usize,
) -> CaptionBands {
    let mut top = Vec::new();
    let mut bottom = Vec::new();
    for caption in &model.captions {
        let style = formatter.styles.get(*caption);
        let edges = EdgeInsets::from_border(style.border);
        let padding = formatter.padding(style, caption_width);
        let content_width = caption_width
            .saturating_sub(edges.left + edges.right + padding.left + padding.right)
            .max(1);
        let metrics = formatter.cell_metrics(&[*caption], style, limits, nesting);
        let pieces = resolve_items(&metrics.items, |node| {
            formatter.format(node, content_width, limits, nesting.saturating_add(1))
        });
        let lines = format_inline(&pieces, content_width);
        let content_height = formatted_height(&lines, &pieces);
        let height = edges.top + edges.bottom + padding.top + padding.bottom + content_height;
        let layout = CaptionLayout {
            node: *caption,
            style,
            layout: CellLayout {
                pieces,
                lines,
                text_align: style.text_align,
            },
            border_rect: LayoutRect {
                col: 0,
                row: 0,
                width: caption_width,
                height,
            },
            content_rect: LayoutRect {
                col: edges.left + padding.left,
                row: edges.top + padding.top,
                width: content_width,
                height: content_height,
            },
        };
        if style.caption_side == CaptionSide::Top {
            top.push(layout);
        } else {
            bottom.push(layout);
        }
    }
    CaptionBands { top, bottom }
}

pub(super) fn append_captions(
    output: &mut TableOutput,
    captions: &[CaptionLayout],
    start_row: usize,
    _width: usize,
) {
    let mut row = start_row;
    for (index, caption) in captions.iter().enumerate() {
        let mut border_rect = caption.border_rect;
        let mut content_rect = caption.content_rect;
        border_rect.row = border_rect.row.saturating_add(row);
        content_rect.row = content_rect.row.saturating_add(row);
        if !caption.style.visibility.is_hidden() {
            output.boxes.push(LayoutBox {
                node: caption.node,
                border_rect,
                content_rect,
                depth: 1,
                style: caption.style.cell_style(),
            });
        }
        add_fill(&mut output.fills, border_rect, caption.style, 1);
        if !caption.style.visibility.is_hidden() && caption.style.border.is_visible() {
            output.strokes.push(BorderStroke {
                rect: border_rect,
                edges: caption.style.border,
                style: caption.style.cell_style(),
                depth: 1,
                merge_group: 900_000usize.saturating_add(index),
            });
        }
        append_cell_content(
            output,
            &caption.layout,
            content_rect,
            2,
            900_000usize.saturating_add(index),
        );
        row = row.saturating_add(border_rect.height);
    }
}

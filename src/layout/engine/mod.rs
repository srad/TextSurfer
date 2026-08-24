mod flow;
mod links;
mod tables;
mod taffy_style;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use taffy::prelude::{AvailableSpace, Size as TaffySize, TaffyTree};
use unicode_width::UnicodeWidthStr;

use crate::core::dom::{Document, NodeId};
use crate::core::geom::Size;
use crate::core::style::{BorderEdges, CellStyle, Rgb, StyleTree};
use crate::layout::table::{TableFormatter, TableLimits};
use crate::layout::text_flow::{
    format_inline, formatted_height, intrinsic_width, min_content_width,
};

use flow::{append_inline, build_flow_tree};
use links::{assign_link_rects, collect_links};
use tables::append_table_output;
use taffy_style::{layout_rect, taffy_style};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoxTree {
    pub width: usize,
    pub height: usize,
    pub boxes: Vec<LayoutBox>,
    pub fragments: Vec<TextFragment>,
    pub fills: Vec<BackgroundFill>,
    pub strokes: Vec<BorderStroke>,
    pub links: Vec<LinkBox>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutRect {
    pub col: usize,
    pub row: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutBox {
    pub node: NodeId,
    pub border_rect: LayoutRect,
    pub content_rect: LayoutRect,
    pub depth: usize,
    pub style: CellStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackgroundFill {
    pub rect: LayoutRect,
    pub color: Rgb,
    pub depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorderStroke {
    pub rect: LayoutRect,
    pub edges: BorderEdges,
    pub style: CellStyle,
    pub depth: usize,
    pub merge_group: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextFragment {
    pub node: NodeId,
    pub col: usize,
    pub row: usize,
    pub text: String,
    pub depth: usize,
    pub style: CellStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkBox {
    pub node: NodeId,
    pub href: String,
    pub rects: Vec<LayoutRect>,
}

pub trait LayoutEngine: Send + Sync {
    fn layout(&self, document: &Document, styles: &StyleTree, viewport: Size) -> BoxTree;
}

#[derive(Default)]
pub struct TaffyLayoutEngine;

impl LayoutEngine for TaffyLayoutEngine {
    fn layout(&self, document: &Document, styles: &StyleTree, viewport: Size) -> BoxTree {
        let width = usize::from(viewport.cols);
        let flow = build_flow_tree(document, styles, width);
        let mut links = Vec::new();
        for root in document.roots() {
            collect_links(document, styles, *root, &mut links);
        }
        let mut taffy: TaffyTree<usize> = TaffyTree::new();
        let mut taffy_nodes = vec![None; flow.len()];
        for index in (0..flow.len()).rev() {
            let style = taffy_style(&flow[index], index == 0, width);
            let node = if !flow[index].inline.is_empty() || flow[index].table.is_some() {
                taffy
                    .new_leaf_with_context(style, index)
                    .expect("taffy measured leaf")
            } else {
                let children: Vec<_> = flow[index]
                    .children
                    .iter()
                    .map(|child| taffy_nodes[*child].expect("taffy child"))
                    .collect();
                taffy
                    .new_with_children(style, &children)
                    .expect("taffy block")
            };
            taffy_nodes[index] = Some(node);
        }
        let root = taffy_nodes[0].expect("taffy root");
        let mut table_cache = HashMap::new();
        taffy
            .compute_layout_with_measure(
                root,
                TaffySize {
                    width: AvailableSpace::Definite(width as f32),
                    height: AvailableSpace::MaxContent,
                },
                |known, available, _, context, _| {
                    let Some(index) = context.map(|context| *context) else {
                        return TaffySize::ZERO;
                    };
                    if let Some(table) = flow[index].table {
                        let width = known.width.unwrap_or(match available.width {
                            AvailableSpace::Definite(value) => value,
                            AvailableSpace::MinContent => 1.0,
                            AvailableSpace::MaxContent => width as f32,
                        });
                        let width = width.max(1.0) as usize;
                        let output = table_cache.entry((table, width)).or_insert_with(|| {
                            TableFormatter::new(document, styles).format(
                                table,
                                width,
                                TableLimits::default(),
                                0,
                            )
                        });
                        return TaffySize {
                            width: output.width as f32,
                            height: output.height as f32,
                        };
                    }
                    let natural = intrinsic_width(&flow[index].inline);
                    let measured_width = known.width.unwrap_or_else(|| match available.width {
                        AvailableSpace::Definite(value) => value,
                        AvailableSpace::MinContent => min_content_width(&flow[index].inline) as f32,
                        AvailableSpace::MaxContent => natural as f32,
                    });
                    let lines =
                        format_inline(&flow[index].inline, measured_width.max(0.0) as usize);
                    TaffySize {
                        width: known.width.unwrap_or(measured_width),
                        height: known
                            .height
                            .unwrap_or(formatted_height(&lines, &flow[index].inline) as f32),
                    }
                },
            )
            .expect("taffy block layout");

        let root_height = taffy
            .layout(root)
            .expect("root layout")
            .size
            .height
            .max(0.0) as usize;
        let mut tree = BoxTree {
            width,
            height: root_height,
            links,
            ..Default::default()
        };
        let mut stack = vec![(0usize, 0.0f32, 0.0f32)];
        while let Some((index, parent_col, parent_row)) = stack.pop() {
            let node = taffy_nodes[index].expect("taffy node");
            let layout = *taffy.layout(node).expect("node layout");
            let absolute_col = parent_col + layout.location.x;
            let absolute_row = parent_row + layout.location.y;
            if let Some(table) = flow[index].table {
                let width = layout.size.width.max(1.0) as usize;
                let output = table_cache
                    .entry((table, width))
                    .or_insert_with(|| {
                        TableFormatter::new(document, styles).format(
                            table,
                            width,
                            TableLimits::default(),
                            0,
                        )
                    })
                    .clone();
                append_table_output(
                    &mut tree,
                    output,
                    absolute_col.max(0.0).round() as usize,
                    absolute_row.max(0.0).round() as usize,
                    flow[index].depth,
                    index.saturating_add(1),
                );
            } else if let Some(owner) = flow[index].owner {
                let border_rect = layout_rect(absolute_col, absolute_row, layout.size);
                let left = layout.border.left + layout.padding.left;
                let right = layout.border.right + layout.padding.right;
                let top = layout.border.top + layout.padding.top;
                let bottom = layout.border.bottom + layout.padding.bottom;
                let content_rect = layout_rect(
                    absolute_col + left,
                    absolute_row + top,
                    TaffySize {
                        width: (layout.size.width - left - right).max(0.0),
                        height: (layout.size.height - top - bottom).max(0.0),
                    },
                );
                tree.height = tree
                    .height
                    .max(border_rect.row.saturating_add(border_rect.height));
                let style = flow[index].style.cell_style();
                if let Some(color) = style.bg {
                    tree.fills.push(BackgroundFill {
                        rect: border_rect,
                        color,
                        depth: flow[index].depth,
                    });
                }
                if flow[index].style.border.is_visible() {
                    tree.strokes.push(BorderStroke {
                        rect: border_rect,
                        edges: flow[index].style.border,
                        style,
                        depth: flow[index].depth,
                        merge_group: index.saturating_add(1),
                    });
                }
                tree.boxes.push(LayoutBox {
                    node: owner,
                    border_rect,
                    content_rect,
                    depth: flow[index].depth,
                    style,
                });
                if flow[index].rule && content_rect.width > 0 {
                    tree.fragments.push(TextFragment {
                        node: owner,
                        col: content_rect.col,
                        row: content_rect.row,
                        text: "─".repeat(content_rect.width),
                        depth: flow[index].depth,
                        style: flow[index].style.cell_style(),
                    });
                }
                if let Some(marker) = &flow[index].marker {
                    let width = UnicodeWidthStr::width(marker.text.as_str());
                    tree.fragments.push(TextFragment {
                        node: owner,
                        col: content_rect.col.saturating_sub(width),
                        row: content_rect.row,
                        text: marker.text.clone(),
                        depth: flow[index].depth,
                        style: marker.style.cell_style(),
                    });
                }
            }
            if !flow[index].inline.is_empty() {
                let rect = layout_rect(absolute_col, absolute_row, layout.size);
                append_inline(
                    &mut tree,
                    &flow[index].inline,
                    rect.col,
                    rect.row,
                    rect.width,
                    flow[index].style.text_align,
                    index.saturating_add(1),
                );
                tree.height = tree.height.max(rect.row.saturating_add(rect.height));
            }
            for child in flow[index].children.iter().rev() {
                stack.push((*child, absolute_col, absolute_row));
            }
        }
        tree.boxes.sort_by_key(|layout_box| layout_box.depth);
        tree.fragments
            .sort_by_key(|fragment| (fragment.row, fragment.col, fragment.depth));
        assign_link_rects(document, &mut tree);
        tree
    }
}

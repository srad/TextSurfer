mod flow;
mod links;
mod tables;
mod taffy_style;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use taffy::compute_leaf_layout;
use taffy::prelude::{AvailableSpace, Size as TaffySize, TaffyTree};
use taffy::tree::Baselines;
use unicode_width::UnicodeWidthStr;

use crate::core::dom::{Document, NodeId};
use crate::core::geom::Size;
use crate::core::style::{BorderEdges, CellStyle, DisplayInside, FlexDirection, Rgb, StyleTree};
use crate::layout::table::{TableFormatter, TableLimits};
use crate::layout::text_flow::{
    Atom, format_inline, formatted_height, intrinsic_width, line_metrics, min_content_width,
};

use flow::{
    AtomicLayout, FlowBox, InlineAtom, InlineAtomSource, ResolvedInlinePiece, append_inline,
    build_flow_subtree, build_flow_tree,
};
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

impl TextFragment {
    pub fn rect(&self) -> LayoutRect {
        LayoutRect {
            col: self.col,
            row: self.row,
            width: UnicodeWidthStr::width(self.text.as_str())
                .saturating_mul(usize::from(self.style.scale)),
            height: usize::from(self.style.scale),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkBox {
    pub node: NodeId,
    pub href: String,
    pub rects: Vec<LayoutRect>,
    pub hit_nodes: Vec<NodeId>,
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
        let mut tree = layout_flow(
            document,
            styles,
            width,
            flow,
            0,
            AvailableSpace::Definite(width as f32),
            0,
        );
        tree.width = width;
        tree.links = links;
        assign_link_rects(document, &mut tree);
        tree
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MeasureWidth {
    MinContent,
    MaxContent,
    Definite(usize),
}

fn layout_flow(
    document: &Document,
    styles: &StyleTree,
    viewport_width: usize,
    mut flow: Vec<FlowBox>,
    root_index: usize,
    available_width: AvailableSpace,
    nesting: usize,
) -> BoxTree {
    let parent_direction = prepare_flow(&mut flow);
    let mut taffy: TaffyTree<usize> = TaffyTree::new();
    let mut taffy_nodes = vec![None; flow.len()];
    for index in (0..flow.len()).rev() {
        let style = taffy_style(
            &flow[index],
            index == 0 && root_index == 0,
            viewport_width,
            parent_direction[index],
        );
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
                .expect("taffy container")
        };
        taffy_nodes[index] = Some(node);
    }
    let root = taffy_nodes[root_index].expect("taffy root");
    let mut table_cache = HashMap::new();
    let mut inline_cache: HashMap<(usize, MeasureWidth), Vec<ResolvedInlinePiece>> = HashMap::new();
    taffy
        .compute_layout_with_measure(
            root,
            TaffySize {
                width: available_width,
                height: AvailableSpace::MaxContent,
            },
            |inputs, _, context, style| {
                let index = context.map(|context| *context);
                let mut baseline = None;
                let mut output = compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, available| {
                        let Some(index) = index else {
                            return TaffySize::ZERO;
                        };
                        if let Some(table) = flow[index].table {
                            let width =
                                measured_width(known.width, available.width, viewport_width)
                                    .definite(viewport_width)
                                    .max(1);
                            let output = table_cache.entry((table, width)).or_insert_with(|| {
                                TableFormatter::new(document, styles).format(
                                    table,
                                    width,
                                    TableLimits::default(),
                                    nesting,
                                )
                            });
                            baseline = Some(output.baseline() as f32);
                            return TaffySize {
                                width: output.width as f32,
                                height: output.height as f32,
                            };
                        }
                        let measure = measured_width(known.width, available.width, viewport_width);
                        let key = (index, measure);
                        inline_cache.entry(key).or_insert_with(|| {
                            resolve_inline(
                                document,
                                styles,
                                viewport_width,
                                &flow[index].inline,
                                measure,
                                nesting,
                            )
                        });
                        let pieces = inline_cache.get(&key).expect("resolved inline");
                        let natural = intrinsic_width(pieces);
                        let measured_width = known.width.unwrap_or_else(|| match available.width {
                            AvailableSpace::Definite(value) => value,
                            AvailableSpace::MinContent => min_content_width(pieces) as f32,
                            AvailableSpace::MaxContent => natural as f32,
                        });
                        let lines = format_inline(pieces, measured_width.max(0.0) as usize);
                        baseline = lines.first().map(|line| {
                            let (_, baseline) = line_metrics(line, pieces);
                            let inset = flow[index]
                                .style
                                .padding
                                .top
                                .saturating_add(flow[index].style.border.top.layout_width());
                            baseline.saturating_add(inset) as f32
                        });
                        TaffySize {
                            width: known.width.unwrap_or(measured_width),
                            height: known
                                .height
                                .unwrap_or(formatted_height(&lines, pieces) as f32),
                        }
                    },
                );
                output.baselines = Baselines::from_first(baseline);
                output
            },
        )
        .expect("taffy layout");

    let root_layout = *taffy.layout(root).expect("root layout");
    let mut tree = BoxTree {
        width: root_layout.size.width.max(0.0).round() as usize,
        height: root_layout.size.height.max(0.0).round() as usize,
        ..Default::default()
    };
    let mut stack = vec![(root_index, 0.0f32, 0.0f32)];
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
            let measure = MeasureWidth::Definite(rect.width);
            let key = (index, measure);
            inline_cache.entry(key).or_insert_with(|| {
                resolve_inline(
                    document,
                    styles,
                    viewport_width,
                    &flow[index].inline,
                    measure,
                    nesting,
                )
            });
            append_inline(
                &mut tree,
                inline_cache.get(&key).expect("resolved inline"),
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
    tree
}

fn prepare_flow(flow: &mut [FlowBox]) -> Vec<Option<FlexDirection>> {
    let mut parent_direction = vec![None; flow.len()];
    for index in 0..flow.len() {
        if matches!(
            flow[index].style.display.inside(),
            Some(DisplayInside::Flex)
        ) {
            let direction = flow[index].style.flex.direction;
            let mut children = flow[index].children.clone();
            children.sort_by_key(|child| flow[*child].style.flex.order);
            for child in &children {
                parent_direction[*child] = Some(direction);
            }
            flow[index].children = children;
        }
    }
    parent_direction
}

impl MeasureWidth {
    fn definite(self, viewport_width: usize) -> usize {
        match self {
            Self::MinContent => 1,
            Self::MaxContent => viewport_width,
            Self::Definite(width) => width,
        }
    }
}

fn measured_width(
    known: Option<f32>,
    available: AvailableSpace,
    viewport_width: usize,
) -> MeasureWidth {
    if let Some(width) = known {
        return MeasureWidth::Definite(width.max(0.0).round() as usize);
    }
    match available {
        AvailableSpace::MinContent => MeasureWidth::MinContent,
        AvailableSpace::MaxContent => MeasureWidth::MaxContent,
        AvailableSpace::Definite(width) => {
            MeasureWidth::Definite(width.max(0.0).round() as usize).max_viewport(viewport_width)
        }
    }
}

impl MeasureWidth {
    fn max_viewport(self, viewport_width: usize) -> Self {
        match self {
            Self::Definite(width) => Self::Definite(width.min(viewport_width)),
            other => other,
        }
    }
}

fn resolve_inline(
    document: &Document,
    styles: &StyleTree,
    viewport_width: usize,
    pieces: &[flow::InlinePiece],
    measure: MeasureWidth,
    nesting: usize,
) -> Vec<ResolvedInlinePiece> {
    let available = measure.definite(viewport_width).max(1);
    pieces
        .iter()
        .map(|piece| {
            let atom = piece.atom.as_ref().map(|source| match source {
                InlineAtomSource::Ready(output) => InlineAtom::Table(output.clone()),
                InlineAtomSource::Node(node)
                    if matches!(
                        styles.get(*node).display.inside(),
                        Some(DisplayInside::Flex)
                    ) && nesting < TableLimits::default().max_nesting =>
                {
                    let flow = build_flow_subtree(document, styles, viewport_width, *node);
                    let root = flow[0].children.first().copied().unwrap_or(0);
                    let width = match measure {
                        MeasureWidth::MinContent => AvailableSpace::MinContent,
                        MeasureWidth::MaxContent => AvailableSpace::MaxContent,
                        MeasureWidth::Definite(width) => AvailableSpace::Definite(width as f32),
                    };
                    let tree = layout_flow(
                        document,
                        styles,
                        viewport_width,
                        flow,
                        root,
                        width,
                        nesting.saturating_add(1),
                    );
                    let baseline = tree
                        .fragments
                        .iter()
                        .map(|fragment| {
                            fragment
                                .row
                                .saturating_add(usize::from(fragment.style.scale).saturating_sub(1))
                        })
                        .min()
                        .unwrap_or_else(|| tree.height.saturating_sub(1));
                    InlineAtom::Layout(Box::new(AtomicLayout { tree, baseline }))
                }
                InlineAtomSource::Node(node) => InlineAtom::Table(Box::new(
                    TableFormatter::new(document, styles).format_inline_atom(
                        *node,
                        available,
                        TableLimits::default(),
                        nesting,
                    ),
                )),
            });
            ResolvedInlinePiece {
                node: piece.node,
                text: piece.text.clone(),
                white_space: piece.white_space,
                depth: piece.depth,
                style: piece.style,
                atom,
            }
        })
        .collect()
}

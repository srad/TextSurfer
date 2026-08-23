use std::collections::HashMap;

use taffy::prelude::{
    AvailableSpace, BoxSizing as TaffyBoxSizing, Dimension, Display as TaffyDisplay,
    LengthPercentage, LengthPercentageAuto, Rect as TaffyRect, Size as TaffySize,
    Style as TaffyStyle, TaffyTree,
};
use unicode_width::UnicodeWidthStr;

use crate::core::dom::{AttrNs, Document, ElementNs, Node, NodeId};
use crate::core::geom::Size;
use crate::core::style::{
    BorderEdges, BoxSizing, CellStyle, ComputedStyle, CssWidth, Display, ListStylePosition, Marker,
    PseudoElement, Rgb, StyleTree, WhiteSpace,
};
use crate::layout::table::{TableFormatter, TableLimits, TableOutput};
use crate::layout::text_flow::{
    Piece, format_inline, formatted_height, intrinsic_width, line_height, min_content_width,
    normalize_segment_breaks,
};

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
                    // Right-aligned in the reserved field: every marker's right edge meets the
                    // content column, so `9.` and `10.` share one text column.
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

fn append_table_output(
    tree: &mut BoxTree,
    output: TableOutput,
    col: usize,
    row: usize,
    depth: usize,
    merge_base: usize,
) {
    tree.height = tree.height.max(row.saturating_add(output.height));
    for mut layout_box in output.boxes {
        offset_rect(&mut layout_box.border_rect, col, row);
        offset_rect(&mut layout_box.content_rect, col, row);
        layout_box.depth += depth;
        tree.boxes.push(layout_box);
    }
    for mut fill in output.fills {
        offset_rect(&mut fill.rect, col, row);
        fill.depth += depth;
        tree.fills.push(fill);
    }
    for mut stroke in output.strokes {
        offset_rect(&mut stroke.rect, col, row);
        stroke.depth += depth;
        stroke.merge_group = merge_base
            .saturating_mul(1_000_000)
            .saturating_add(stroke.merge_group);
        tree.strokes.push(stroke);
    }
    for fragment in output.fragments {
        tree.fragments.push(TextFragment {
            node: fragment.node,
            col: col.saturating_add(fragment.col),
            row: row.saturating_add(fragment.row),
            text: fragment.text,
            depth: depth.saturating_add(fragment.depth),
            style: fragment.style,
        });
    }
}

fn offset_rect(rect: &mut LayoutRect, col: usize, row: usize) {
    rect.col = rect.col.saturating_add(col);
    rect.row = rect.row.saturating_add(row);
}

fn assign_link_rects(document: &Document, tree: &mut BoxTree) {
    if tree.links.is_empty() {
        return;
    }
    let owners: HashMap<NodeId, usize> = tree
        .links
        .iter()
        .enumerate()
        .map(|(index, link)| (link.node, index))
        .collect();
    let mut resolved: HashMap<NodeId, Option<usize>> = HashMap::new();
    let mut rects: Vec<Vec<LayoutRect>> = vec![Vec::new(); tree.links.len()];
    for fragment in &tree.fragments {
        let Some(index) = *resolved
            .entry(fragment.node)
            .or_insert_with(|| nearest_link(document, fragment.node, &owners))
        else {
            continue;
        };
        let rect = LayoutRect {
            col: fragment.col,
            row: fragment.row,
            width: UnicodeWidthStr::width(fragment.text.as_str()),
            height: 1,
        };
        if rect.width == 0 {
            continue;
        }
        match rects[index].last_mut() {
            Some(last) if last.row == rect.row && last.col + last.width == rect.col => {
                last.width += rect.width;
            }
            _ => rects[index].push(rect),
        }
    }
    for (index, link) in tree.links.iter_mut().enumerate() {
        link.rects = std::mem::take(&mut rects[index]);
    }
}

fn nearest_link(
    document: &Document,
    node: NodeId,
    owners: &HashMap<NodeId, usize>,
) -> Option<usize> {
    let mut current = Some(node);
    while let Some(id) = current {
        if let Some(index) = owners.get(&id) {
            return Some(*index);
        }
        current = document.parent(id);
    }
    None
}

#[derive(Clone)]
struct FlowBox {
    owner: Option<NodeId>,
    style: ComputedStyle,
    depth: usize,
    inline: Vec<InlinePiece>,
    children: Vec<usize>,
    rule: bool,
    table: Option<NodeId>,
    /// An outside list marker. Its `reserve` widens the box's left padding so the marker field
    /// sits inside the border box, and the marker paints right-aligned in that field.
    marker: Option<Marker>,
}

type InlinePiece = Piece<TableOutput>;

#[derive(Clone, Copy)]
struct InlineContext {
    white_space: WhiteSpace,
    style: CellStyle,
    depth: usize,
}

enum FlowEvent {
    Enter(NodeId, InlineContext),
    RightEdge(NodeId, ComputedStyle, InlineContext),
}

fn build_flow_tree(document: &Document, styles: &StyleTree, viewport_width: usize) -> Vec<FlowBox> {
    let mut flow = vec![FlowBox {
        owner: None,
        style: ComputedStyle {
            display: Display::Block,
            ..Default::default()
        },
        depth: 0,
        inline: Vec::new(),
        children: Vec::new(),
        rule: false,
        table: None,
        marker: None,
    }];
    let mut tasks = vec![(0usize, None)];
    while let Some((flow_index, container)) = tasks.pop() {
        let depth = container.map(|_| flow[flow_index].depth + 1).unwrap_or(0);
        let inherited = InlineContext {
            white_space: container
                .map(|node| styles.get(node).white_space)
                .unwrap_or_default(),
            style: container
                .map(|node| styles.get(node).cell_style())
                .unwrap_or_default(),
            depth,
        };
        let roots = container
            .map(|node| document.children(node))
            .unwrap_or_else(|| document.roots().to_vec());
        let mut events: Vec<_> = roots
            .into_iter()
            .rev()
            .map(|node| FlowEvent::Enter(node, inherited))
            .collect();
        let mut buffer = Vec::new();
        if let Some(node) = container {
            let own = InlineContext {
                depth: flow[flow_index].depth,
                ..inherited
            };
            if let Some(marker) = styles.marker(node)
                && marker.position == ListStylePosition::Inside
            {
                buffer.push(InlinePiece {
                    node,
                    text: marker.text.clone(),
                    white_space: marker.style.white_space,
                    depth: own.depth,
                    style: marker.style.cell_style(),
                    atom: None,
                });
            }
            append_pseudo(styles, node, PseudoElement::Before, own, &mut buffer);
        }
        let mut children = Vec::new();
        let mut nested_tasks = Vec::new();
        while let Some(event) = events.pop() {
            match event {
                FlowEvent::RightEdge(node, style, context) => {
                    append_pseudo(
                        styles,
                        node,
                        PseudoElement::After,
                        InlineContext {
                            white_space: style.white_space,
                            style: style.cell_style(),
                            depth: context.depth + 1,
                        },
                        &mut buffer,
                    );
                    append_edge(node, style, false, context, &mut buffer);
                }
                FlowEvent::Enter(node, context) => match document.node(node) {
                    Some(Node::Text { data }) => buffer.push(InlinePiece {
                        node,
                        text: normalize_segment_breaks(data),
                        white_space: context.white_space,
                        depth: context.depth,
                        style: context.style,
                        atom: None,
                    }),
                    Some(Node::Element { name, ns, attrs }) => {
                        let style = styles.get(node);
                        if style.display == Display::None {
                            continue;
                        }
                        let element_context = InlineContext {
                            white_space: style.white_space,
                            style: style.cell_style(),
                            depth: context.depth + 1,
                        };
                        if style.display == Display::Table {
                            flush_inline(&mut flow, &mut children, &mut buffer, context.depth);
                            let child = flow.len();
                            flow.push(FlowBox {
                                owner: Some(node),
                                style,
                                depth: context.depth,
                                inline: Vec::new(),
                                children: Vec::new(),
                                rule: false,
                                table: Some(node),
                                marker: None,
                            });
                            children.push(child);
                        } else if style.display == Display::InlineTable {
                            buffer.push(InlinePiece {
                                node,
                                text: String::new(),
                                white_space: context.white_space,
                                depth: context.depth,
                                style: style.cell_style(),
                                atom: Some(TableFormatter::new(document, styles).format(
                                    node,
                                    viewport_width,
                                    TableLimits::default(),
                                    0,
                                )),
                            });
                        } else if style.display.is_block_container() {
                            flush_inline(&mut flow, &mut children, &mut buffer, context.depth);
                            let child = flow.len();
                            flow.push(FlowBox {
                                owner: Some(node),
                                style,
                                depth: context.depth,
                                inline: Vec::new(),
                                children: Vec::new(),
                                rule: *ns == ElementNs::Html && name == "hr",
                                table: None,
                                marker: styles
                                    .marker(node)
                                    .filter(|marker| {
                                        marker.position == ListStylePosition::Outside
                                            && marker.reserve > 0
                                    })
                                    .cloned(),
                            });
                            children.push(child);
                            nested_tasks.push((child, Some(node)));
                        } else if *ns == ElementNs::Html && name == "br" {
                            buffer.push(InlinePiece {
                                node,
                                text: "\n".to_string(),
                                white_space: WhiteSpace::Pre,
                                depth: context.depth,
                                style: context.style,
                                atom: None,
                            });
                        } else if *ns == ElementNs::Html && name == "img" {
                            if let Some(text) = super::image_fallback(attrs) {
                                buffer.push(InlinePiece {
                                    node,
                                    text,
                                    white_space: context.white_space,
                                    depth: context.depth,
                                    style: style.cell_style(),
                                    atom: None,
                                });
                            }
                        } else {
                            append_edge(node, style, true, context, &mut buffer);
                            events.push(FlowEvent::RightEdge(node, style, context));
                            append_pseudo(
                                styles,
                                node,
                                PseudoElement::Before,
                                element_context,
                                &mut buffer,
                            );
                            events.extend(
                                document
                                    .children(node)
                                    .into_iter()
                                    .rev()
                                    .map(|child| FlowEvent::Enter(child, element_context)),
                            );
                        }
                    }
                    Some(Node::DocumentFragment) => events.extend(
                        document
                            .children(node)
                            .into_iter()
                            .rev()
                            .map(|child| FlowEvent::Enter(child, context)),
                    ),
                    _ => {}
                },
            }
        }
        if let Some(node) = container {
            append_pseudo(
                styles,
                node,
                PseudoElement::After,
                InlineContext {
                    depth: flow[flow_index].depth,
                    ..inherited
                },
                &mut buffer,
            );
        }
        flush_inline(&mut flow, &mut children, &mut buffer, depth);
        flow[flow_index].children = children;
        tasks.extend(nested_tasks.into_iter().rev());
    }
    flow
}

fn flush_inline(
    flow: &mut Vec<FlowBox>,
    children: &mut Vec<usize>,
    buffer: &mut Vec<InlinePiece>,
    depth: usize,
) {
    if buffer.is_empty()
        || buffer.iter().all(|piece| {
            piece.atom.is_none()
                && matches!(
                    piece.white_space,
                    WhiteSpace::Normal | WhiteSpace::NoWrap | WhiteSpace::PreLine
                )
                && piece.text.chars().all(char::is_whitespace)
        })
    {
        buffer.clear();
        return;
    }
    let child = flow.len();
    flow.push(FlowBox {
        owner: None,
        style: ComputedStyle {
            display: Display::Block,
            ..Default::default()
        },
        depth,
        inline: std::mem::take(buffer),
        children: Vec::new(),
        rule: false,
        table: None,
        marker: None,
    });
    children.push(child);
}

/// Pushes the generated content the cascade resolved for one pseudo-element, tagged with the
/// originating element so hit-testing, link rects and search keep working through it.
fn append_pseudo(
    styles: &StyleTree,
    node: NodeId,
    which: PseudoElement,
    context: InlineContext,
    buffer: &mut Vec<InlinePiece>,
) {
    let Some(pseudo) = styles.pseudo(node, which) else {
        return;
    };
    buffer.push(InlinePiece {
        node,
        text: pseudo.text.clone(),
        white_space: pseudo.style.white_space,
        depth: context.depth,
        style: pseudo.style.cell_style(),
        atom: None,
    });
}

fn append_edge(
    node: NodeId,
    style: ComputedStyle,
    left: bool,
    context: InlineContext,
    buffer: &mut Vec<InlinePiece>,
) {
    let cells = if left {
        style.margin.left.saturating_add(style.padding.left)
    } else {
        style.margin.right.saturating_add(style.padding.right)
    };
    if cells > 0 {
        buffer.push(InlinePiece {
            node,
            text: " ".repeat(cells),
            white_space: WhiteSpace::BreakSpaces,
            depth: context.depth,
            style: context.style,
            atom: None,
        });
    }
}

fn collect_links(document: &Document, styles: &StyleTree, id: NodeId, links: &mut Vec<LinkBox>) {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        if styles.get(id).display == Display::None {
            continue;
        }
        match document.node(id) {
            Some(Node::Element { name, ns, attrs }) => {
                if *ns == ElementNs::Html
                    && name == "a"
                    && let Some(href) = attrs
                        .iter()
                        .find(|attr| attr.ns == AttrNs::None && attr.name == "href")
                {
                    links.push(LinkBox {
                        node: id,
                        href: href.value.clone(),
                        rects: Vec::new(),
                    });
                }
                let children = document.children(id);
                stack.extend(children.into_iter().rev());
            }
            Some(Node::DocumentFragment) => {
                let children = document.children(id);
                stack.extend(children.into_iter().rev());
            }
            _ => {}
        }
    }
}

fn taffy_style(flow: &FlowBox, root: bool, viewport_width: usize) -> TaffyStyle {
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
            left: LengthPercentageAuto::length(flow.style.margin.left as f32),
            right: LengthPercentageAuto::length(flow.style.margin.right as f32),
            top: LengthPercentageAuto::length(flow.style.margin.top as f32),
            bottom: LengthPercentageAuto::length(flow.style.margin.bottom as f32),
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

fn layout_rect(col: f32, row: f32, size: TaffySize<f32>) -> LayoutRect {
    LayoutRect {
        col: col.max(0.0).round() as usize,
        row: row.max(0.0).round() as usize,
        width: size.width.max(0.0).round() as usize,
        height: size.height.max(0.0).round() as usize,
    }
}

fn append_inline(
    tree: &mut BoxTree,
    pieces: &[InlinePiece],
    col: usize,
    row: usize,
    width: usize,
    merge_base: usize,
) {
    let mut current_row = row;
    for line in format_inline(pieces, width) {
        let line_height = line_height(&line, pieces);
        let mut current_col = col;
        for glyph in line {
            if let Some(index) = glyph.atom {
                if let Some(table) = &pieces[index].atom {
                    append_table_output(
                        tree,
                        table.clone(),
                        current_col,
                        current_row + line_height.saturating_sub(table.height),
                        pieces[index].depth,
                        merge_base.saturating_mul(1_000).saturating_add(index),
                    );
                }
                current_col = current_col.saturating_add(glyph.width);
                continue;
            }
            let baseline = current_row + line_height - 1;
            if let Some(last) = tree.fragments.last_mut()
                && last.node == glyph.node
                && last.row == baseline
                && last.depth == glyph.depth
                && last.style == glyph.style
                && last.col + UnicodeWidthStr::width(last.text.as_str()) == current_col
            {
                last.text.push_str(&glyph.text);
            } else {
                tree.fragments.push(TextFragment {
                    node: glyph.node,
                    col: current_col,
                    row: baseline,
                    text: glyph.text,
                    depth: glyph.depth,
                    style: glyph.style,
                });
            }
            current_col = current_col.saturating_add(glyph.width);
        }
        current_row += line_height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dom::ElementNs;
    use crate::core::style::{BorderEdges, BorderLineStyle, BorderSide, Palette};
    use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};
    use crate::paint::{BasicPainter, Painter};
    use proptest::prelude::*;

    fn paragraph(text: &str) -> (Document, StyleTree) {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        document.insert_text(Some(p), text);
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        (document, styles)
    }

    fn fragment_text(tree: &BoxTree) -> Vec<&str> {
        tree.fragments
            .iter()
            .map(|fragment| fragment.text.as_str())
            .collect()
    }

    fn nested_document(text: &str, depth: usize) -> (Document, StyleTree) {
        let mut document = Document::new();
        let body = document.insert_element(None, "body", ElementNs::Html, vec![]);
        let mut parent = body;
        for _ in 0..depth {
            parent = document.insert_element(
                Some(parent),
                "div",
                ElementNs::Html,
                vec![crate::core::dom::Attr::plain("style", "padding-left: 1px")],
            );
        }
        let p = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
        document.insert_text(Some(p), text);
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        (document, styles)
    }

    fn covers(outer: LayoutRect, inner: LayoutRect) -> bool {
        inner.col >= outer.col
            && inner.col + inner.width <= outer.col + outer.width
            && inner.row >= outer.row
            && inner.row + inner.height <= outer.row + outer.height
    }

    fn disjoint(a: LayoutRect, b: LayoutRect) -> bool {
        a.col + a.width <= b.col
            || b.col + b.width <= a.col
            || a.row + a.height <= b.row
            || b.row + b.height <= a.row
    }

    proptest! {
        #[test]
        fn wider_viewports_never_increase_single_block_height(
            text in "[a-z ]{0,200}",
            narrow in 1u16..40,
            extra in 0u16..40,
        ) {
            let (document, styles) = paragraph(&text);
            let narrow_tree = TaffyLayoutEngine.layout(
                &document,
                &styles,
                Size { cols: narrow, rows: 5 },
            );
            let wide_tree = TaffyLayoutEngine.layout(
                &document,
                &styles,
                Size { cols: narrow + extra, rows: 5 },
            );
            prop_assert!(wide_tree.height <= narrow_tree.height);
        }

        #[test]
        fn painted_rows_never_exceed_the_viewport(
            chars in prop::collection::vec(any::<char>(), 0..100),
            width in 1u16..80,
        ) {
            let text: String = chars.into_iter().collect();
            let (document, styles) = paragraph(&text);
            let tree = TaffyLayoutEngine.layout(
                &document,
                &styles,
                Size { cols: width, rows: 5 },
            );
            let display = BasicPainter.paint(&tree, Palette::default());
            for line in display.text_lines() {
                prop_assert!(UnicodeWidthStr::width(line.as_str()) <= usize::from(width));
            }
        }

        #[test]
        fn leaf_text_cells_never_overlap_within_a_row(
            text in "[a-z ]{0,120}",
            width in 4u16..40,
        ) {
            let (document, styles) = paragraph(&text);
            let tree = TaffyLayoutEngine.layout(
                &document,
                &styles,
                Size { cols: width, rows: 5 },
            );
            let mut occupied: Vec<(usize, usize, usize)> = Vec::new();
            for fragment in &tree.fragments {
                let span = UnicodeWidthStr::width(fragment.text.as_str());
                for (row, from, to) in &occupied {
                    if *row == fragment.row {
                        prop_assert!(
                            fragment.col >= *to || fragment.col + span <= *from,
                            "fragments overlap at row {row}"
                        );
                    }
                }
                occupied.push((fragment.row, fragment.col, fragment.col + span));
            }
        }

        #[test]
        fn boxes_form_a_laminar_family(
            text in "[a-z ]{0,80}",
            depth in 0usize..6,
            width in 6u16..40,
        ) {
            let (document, styles) = nested_document(&text, depth);
            let tree = TaffyLayoutEngine.layout(
                &document,
                &styles,
                Size { cols: width, rows: 5 },
            );
            for (index, outer) in tree.boxes.iter().enumerate() {
                for inner in &tree.boxes[index + 1..] {
                    let a = outer.border_rect;
                    let b = inner.border_rect;
                    if a.width == 0 || a.height == 0 || b.width == 0 || b.height == 0 {
                        continue;
                    }
                    prop_assert!(
                        covers(a, b) || covers(b, a) || disjoint(a, b),
                        "boxes {a:?} and {b:?} partially overlap"
                    );
                }
            }
        }

        #[test]
        fn hit_testing_round_trips_to_the_deepest_engine_box(
            text in "[a-z ]{1,80}",
            depth in 0usize..5,
            width in 8u16..40,
        ) {
            let (document, styles) = nested_document(&text, depth);
            let tree = TaffyLayoutEngine.layout(
                &document,
                &styles,
                Size { cols: width, rows: 5 },
            );
            let display = BasicPainter.paint(&tree, Palette::default());
            for layout_box in &tree.boxes {
                let rect = layout_box.border_rect;
                if rect.width == 0 || rect.height == 0 {
                    continue;
                }
                let deepest = tree
                    .boxes
                    .iter()
                    .filter(|candidate| {
                        let other = candidate.border_rect;
                        rect.col >= other.col
                            && rect.col < other.col + other.width
                            && rect.row >= other.row
                            && rect.row < other.row + other.height
                    })
                    .max_by_key(|candidate| candidate.depth)
                    .expect("the probed box contains its own corner");
                prop_assert_eq!(
                    display.hit_test(rect.col, rect.row),
                    Some(deepest.node)
                );
            }
        }

    }

    #[test]
    fn block_text_wraps_to_the_terminal_width_and_hidden_nodes_disappear() {
        let mut document = Document::new();
        let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
        let head = document.insert_element(Some(html), "head", ElementNs::Html, vec![]);
        document.insert_text(Some(head), "hidden");
        let body = document.insert_element(Some(html), "body", ElementNs::Html, vec![]);
        let p = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
        document.insert_text(Some(p), "one two three four");
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 9, rows: 5 });
        assert_eq!(fragment_text(&tree), vec!["one two", "three", "four"]);
        assert!(
            !fragment_text(&tree)
                .iter()
                .any(|line| line.contains("hidden"))
        );
    }

    #[test]
    fn long_words_break_to_terminal_width_even_when_author_css_requests_normal_wrapping() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        document.insert_text(Some(p), "abcdefghijk");
        let sheet = CssparserParser.parse("p { overflow-wrap: normal }");
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 4, rows: 5 });
        assert_eq!(fragment_text(&tree), vec!["abcd", "efgh", "ijk"]);
    }

    #[test]
    fn preformatted_text_preserves_spaces_and_line_breaks() {
        let mut document = Document::new();
        let pre = document.insert_element(None, "pre", ElementNs::Html, vec![]);
        document.insert_text(Some(pre), "  a\n b");
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 4, rows: 5 });
        assert_eq!(fragment_text(&tree), vec!["  a", " b"]);
    }

    #[test]
    fn images_render_their_alt_text_and_rules_span_the_content_width() {
        let mut document = Document::new();
        let body = document.insert_element(None, "body", ElementNs::Html, vec![]);
        let described = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
        document.insert_element(
            Some(described),
            "img",
            ElementNs::Html,
            vec![crate::core::dom::Attr::plain("alt", "a cat")],
        );
        let empty = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
        document.insert_text(Some(empty), "before");
        document.insert_element(
            Some(empty),
            "img",
            ElementNs::Html,
            vec![crate::core::dom::Attr::plain("alt", "")],
        );
        document.insert_text(Some(empty), "after");
        let whitespace = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
        document.insert_text(Some(whitespace), "left");
        document.insert_element(
            Some(whitespace),
            "img",
            ElementNs::Html,
            vec![crate::core::dom::Attr::plain("alt", "   ")],
        );
        document.insert_text(Some(whitespace), "right");
        document.insert_element(Some(body), "hr", ElementNs::Html, vec![]);
        let bare = document.insert_element(Some(body), "p", ElementNs::Html, vec![]);
        document.insert_element(Some(bare), "img", ElementNs::Html, vec![]);
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 12, rows: 5 });
        let lines = BasicPainter.paint(&tree, Palette::default()).text_lines();
        assert!(lines.iter().any(|line| line == "[a cat]"));
        assert!(lines.iter().any(|line| line == "beforeafter"));
        assert!(lines.iter().any(|line| line == "leftright"));
        assert_eq!(lines.iter().filter(|line| *line == "[img]").count(), 1);
        assert!(
            lines.iter().any(|line| line == "────────────"),
            "an <hr> must draw a rule across the content width, not a single glyph: {lines:?}"
        );
    }

    #[test]
    fn accented_words_never_break_mid_word_while_wide_scripts_still_wrap() {
        let (document, styles) = paragraph("Grüße Grüße");
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 6, rows: 5 });
        let lines = BasicPainter.paint(&tree, Palette::default()).text_lines();
        assert_eq!(
            &lines[..2],
            ["Grüße", "Grüße"],
            "non-ASCII letters are not break opportunities"
        );

        let (document, styles) = paragraph("日本語のテキスト");
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 6, rows: 5 });
        let lines = BasicPainter.paint(&tree, Palette::default()).text_lines();
        assert!(
            lines.len() > 1,
            "wide scripts still break between characters: {lines:?}"
        );
    }

    #[test]
    fn inline_styles_travel_with_the_text_fragments() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        document.insert_text(Some(p), "plain ");
        let strong = document.insert_element(Some(p), "strong", ElementNs::Html, vec![]);
        document.insert_text(Some(strong), "loud");
        let sheet = CssparserParser.parse("p { color: #112233 }");
        let styles = BasicCascade.apply(&[sheet], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
        let plain = tree
            .fragments
            .iter()
            .find(|fragment| fragment.text.starts_with("plain"))
            .expect("plain fragment");
        let loud = tree
            .fragments
            .iter()
            .find(|fragment| fragment.text == "loud")
            .expect("bold fragment");
        assert_eq!(
            plain.style.fg,
            Some(crate::core::style::Rgba::new(17, 34, 51, 255))
        );
        assert!(!plain.style.bold);
        assert!(loud.style.bold, "inline weight must reach the fragment");
        assert_eq!(
            loud.style.fg, plain.style.fg,
            "colour inherits into <strong>"
        );
    }

    #[test]
    fn links_are_discovered_inside_inline_content() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let link = document.insert_element(
            Some(p),
            "a",
            ElementNs::Html,
            vec![crate::core::dom::Attr::plain("href", "/next")],
        );
        document.insert_text(Some(link), "next");
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
        assert_eq!(tree.links.len(), 1);
        assert_eq!(tree.links[0].node, link);
        assert_eq!(tree.links[0].href, "/next");
        assert_eq!(
            tree.links[0].rects,
            vec![LayoutRect {
                col: 0,
                row: 0,
                width: 4,
                height: 1
            }],
            "link geometry must reach the box tree for keyboard and mouse targeting"
        );
    }

    #[test]
    fn deeply_nested_inline_content_is_walked_iteratively() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let mut parent = p;
        for _ in 0..4096 {
            parent = document.insert_element(Some(parent), "span", ElementNs::Html, vec![]);
        }
        document.insert_text(Some(parent), "deep");
        let mut styles = StyleTree::default();
        styles.insert(
            p,
            ComputedStyle {
                display: Display::Block,
                ..Default::default()
            },
        );
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 5 });
        assert_eq!(fragment_text(&tree), vec!["deep"]);
    }

    #[test]
    fn bordered_wide_text_keeps_the_right_border_in_its_cell() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        document.insert_text(Some(p), "界x");
        let mut styles = StyleTree::default();
        styles.insert(
            p,
            ComputedStyle {
                display: Display::Block,
                border: BorderEdges::uniform(BorderSide {
                    style: BorderLineStyle::Solid,
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 6, rows: 5 });
        let lines = BasicPainter.paint(&tree, Palette::default()).text_lines();
        assert_eq!(lines[1], "│界x │");
        assert_eq!(UnicodeWidthStr::width(lines[1].as_str()), 6);
    }

    #[test]
    fn mixed_inline_and_block_children_create_ordered_anonymous_runs() {
        let mut document = Document::new();
        let outer = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let before = document.insert_text(Some(outer), "before");
        let inner = document.insert_element(Some(outer), "p", ElementNs::Html, vec![]);
        let inside = document.insert_text(Some(inner), "inside");
        let after = document.insert_text(Some(outer), "after");
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 1 });
        assert_eq!(
            tree.fragments
                .iter()
                .map(|fragment| fragment.node)
                .collect::<Vec<_>>(),
            vec![before, inside, after]
        );
        assert_eq!(fragment_text(&tree), vec!["before", "inside", "after"]);
        let outer_box = tree
            .boxes
            .iter()
            .find(|layout_box| layout_box.node == outer)
            .unwrap();
        let inner_box = tree
            .boxes
            .iter()
            .find(|layout_box| layout_box.node == inner)
            .unwrap();
        assert!(inner_box.border_rect.row >= outer_box.content_rect.row);
        assert!(
            inner_box.border_rect.row + inner_box.border_rect.height
                <= outer_box.content_rect.row + outer_box.content_rect.height
        );
    }

    #[test]
    fn box_sizing_changes_fixed_width_border_geometry() {
        let mut document = Document::new();
        let content = document.insert_element(
            None,
            "div",
            ElementNs::Html,
            vec![crate::core::dom::Attr::plain(
                "style",
                "width: 6px; padding: 1px; border: solid; box-sizing: content-box",
            )],
        );
        document.insert_text(Some(content), "x");
        let border = document.insert_element(
            None,
            "div",
            ElementNs::Html,
            vec![crate::core::dom::Attr::plain(
                "style",
                "width: 6px; padding: 1px; border: solid; box-sizing: border-box",
            )],
        );
        document.insert_text(Some(border), "x");
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 1 });
        let content_box = tree
            .boxes
            .iter()
            .find(|layout_box| layout_box.node == content)
            .unwrap();
        let border_box = tree
            .boxes
            .iter()
            .find(|layout_box| layout_box.node == border)
            .unwrap();
        assert_eq!(content_box.border_rect.width, 10);
        assert_eq!(content_box.content_rect.width, 6);
        assert_eq!(border_box.border_rect.width, 6);
        assert_eq!(border_box.content_rect.width, 2);
    }

    #[test]
    fn intrinsic_height_is_not_limited_by_viewport_rows() {
        let mut document = Document::new();
        let pre = document.insert_element(None, "pre", ElementNs::Html, vec![]);
        let text = (0..300).map(|_| "x").collect::<Vec<_>>().join("\n");
        document.insert_text(Some(pre), &text);
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 4, rows: 1 });
        assert_eq!(tree.fragments.len(), 300);
        assert!(tree.height >= 300);
    }

    #[test]
    fn all_white_space_modes_apply_their_collapse_break_and_wrap_rules() {
        let cases = [
            ("normal", " a  b c ", 4, vec!["a b", "c"]),
            ("nowrap", " a  b c ", 4, vec!["a b c"]),
            ("pre", " a\nb ", 2, vec![" a", "b "]),
            ("pre-wrap", "ab cd", 3, vec!["ab ", "cd"]),
            ("pre-line", " a  b\n c ", 4, vec!["a b", "c"]),
            ("break-spaces", "a  b", 2, vec!["a ", " b"]),
        ];
        for (mode, text, width, expected) in cases {
            let mut document = Document::new();
            let declaration = format!("white-space: {mode}");
            let p = document.insert_element(
                None,
                "p",
                ElementNs::Html,
                vec![crate::core::dom::Attr::plain("style", &declaration)],
            );
            document.insert_text(Some(p), text);
            let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
            let tree = TaffyLayoutEngine.layout(
                &document,
                &styles,
                Size {
                    cols: width,
                    rows: 1,
                },
            );
            assert_eq!(fragment_text(&tree), expected, "{mode}");
        }
    }

    #[test]
    fn segment_breaks_tabs_forced_breaks_and_cross_node_graphemes_stay_owned() {
        let mut document = Document::new();
        let pre = document.insert_element(None, "pre", ElementNs::Html, vec![]);
        let first = document.insert_text(Some(pre), "a\tX\r\nb\u{000c}c\n");
        let br = document.insert_element(Some(pre), "br", ElementNs::Html, vec![]);
        let base = document.insert_text(Some(pre), "e");
        document.insert_text(Some(pre), "\u{301}");
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 20, rows: 1 });
        assert_eq!(
            tree.fragments
                .iter()
                .map(|fragment| (fragment.node, fragment.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (first, "a       X"),
                (first, "b"),
                (first, "c"),
                (base, "e\u{301}")
            ]
        );
        assert_eq!(tree.fragments.last().unwrap().row, 4);
        assert!(tree.fragments.iter().all(|fragment| fragment.node != br));
    }

    #[test]
    fn zero_width_keeps_zero_geometry_but_text_layout_makes_progress() {
        let (document, styles) = paragraph("ab");
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 0, rows: 1 });
        assert_eq!(tree.width, 0);
        assert_eq!(tree.boxes[0].border_rect.width, 0);
        assert!(tree.height >= 2);
        assert!(
            BasicPainter
                .paint(&tree, Palette::default())
                .text_lines()
                .iter()
                .all(String::is_empty)
        );
    }

    /// Lays `source` out at `cols` and returns the painted rows, so marker geometry can be
    /// asserted as the columns a reader actually sees.
    fn rendered_rows(source: &str, cols: u16) -> Vec<String> {
        crate::app::render::render_html(source, Size { cols, rows: 24 }, Palette::DEFAULT, false)
            .painted
            .text_lines()
    }

    #[test]
    fn the_marker_field_is_shared_by_every_sibling_so_numbers_share_one_text_column() {
        let rows = rendered_rows(
            "<ol start='9'><li>nine</li><li>ten</li><li>eleven</li></ol>",
            30,
        );
        assert_eq!(rows, [" 9. nine", "10. ten", "11. eleven", ""]);
    }

    #[test]
    fn outside_markers_hang_so_wrapped_lines_align_under_the_item_text() {
        let rows = rendered_rows("<ul><li>alpha beta gamma delta</li></ul>", 12);
        assert_eq!(rows, ["• alpha beta", "  gamma", "  delta", ""]);
    }

    #[test]
    fn inside_markers_keep_the_marker_inline_with_no_hanging_indent() {
        let rows = rendered_rows(
            "<ul style='list-style-position: inside'><li>alpha beta gamma</li></ul>",
            12,
        );
        assert_eq!(rows, ["• alpha beta", "gamma", ""]);
    }

    #[test]
    fn a_suppressed_marker_reserves_no_field_at_all() {
        let rows = rendered_rows("<ul style='list-style-type: none'><li>alpha</li></ul>", 12);
        assert_eq!(rows, ["alpha", ""]);
    }

    #[test]
    fn generated_content_and_markers_render_inside_table_cells() {
        let rows = rendered_rows(
            "<style>td::before { content: '> ' }</style>\
             <table><tr><td><ul><li>cell item</li></ul></td></tr></table>",
            24,
        );
        let text = rows.join("\n");
        assert!(
            text.contains('>'),
            "generated content reaches the cell: {text}"
        );
        assert!(
            text.contains('\u{2022}'),
            "the marker reaches the cell: {text}"
        );
    }

    #[test]
    fn generated_content_is_tagged_with_its_originating_element_for_hit_testing() {
        use crate::html::HtmlParser;
        let outcome = crate::html::Html5everParser::new(false)
            .parse_document("<style>a::after { content: ' (link)' }</style><a href='/x'>go</a>");
        let document = outcome.document.borrow();
        let sheets = crate::app::render::embedded_style_sheets(&document);
        let styles = BasicCascade.apply(&sheets, &document, MediaContext::screen());
        let tree = TaffyLayoutEngine.layout(&document, &styles, Size { cols: 40, rows: 24 });
        let link = tree.links.first().expect("the anchor is a link");
        let painted: usize = link.rects.iter().map(|rect| rect.width).sum();
        assert_eq!(
            painted,
            "go (link)".len(),
            "generated content joins the link's own rects"
        );
    }
}

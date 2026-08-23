use unicode_width::UnicodeWidthStr;

use crate::core::dom::{Document, ElementNs, Node, NodeId};
use crate::core::style::{
    CellStyle, ComputedStyle, Display, ListStylePosition, Marker, PseudoElement, StyleTree,
    WhiteSpace,
};
use crate::layout::table::{TableFormatter, TableLimits, TableOutput};
use crate::layout::text_flow::{Piece, format_inline, line_height, normalize_segment_breaks};

use super::tables::append_table_output;
use super::{BoxTree, TextFragment};

#[derive(Clone)]
pub(super) struct FlowBox {
    pub(super) owner: Option<NodeId>,
    pub(super) style: ComputedStyle,
    pub(super) depth: usize,
    pub(super) inline: Vec<InlinePiece>,
    pub(super) children: Vec<usize>,
    pub(super) rule: bool,
    pub(super) table: Option<NodeId>,
    pub(super) marker: Option<Marker>,
}

pub(super) type InlinePiece = Piece<TableOutput>;

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

pub(super) fn build_flow_tree(
    document: &Document,
    styles: &StyleTree,
    viewport_width: usize,
) -> Vec<FlowBox> {
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
                            if let Some(text) = crate::layout::replaced::image_fallback(attrs) {
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

pub(super) fn append_inline(
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

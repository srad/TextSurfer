use unicode_width::UnicodeWidthStr;

use crate::core::dom::{Document, ElementNs, Node, NodeId};
use crate::core::style::{
    CellStyle, ComputedStyle, Display, ListStylePosition, Marker, PseudoElement, StyleTree,
    TextAlign, WhiteSpace,
};
use crate::layout::table::{TableFormatter, TableLimits, TableOutput};
use crate::layout::text_flow::{
    Atom, Piece, format_inline, line_metrics, normalize_segment_breaks,
};

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

#[derive(Clone)]
pub(super) enum InlineAtomSource {
    Ready(Box<TableOutput>),
    Node(NodeId),
}

impl Atom for InlineAtomSource {
    fn width(&self) -> usize {
        match self {
            Self::Ready(output) => output.width,
            Self::Node(_) => 0,
        }
    }

    fn height(&self) -> usize {
        match self {
            Self::Ready(output) => output.height,
            Self::Node(_) => 0,
        }
    }

    fn baseline(&self) -> usize {
        match self {
            Self::Ready(output) => output.baseline(),
            Self::Node(_) => 0,
        }
    }
}

#[derive(Clone)]
pub(super) enum InlineAtom {
    Table(Box<TableOutput>),
    Layout(Box<AtomicLayout>),
}

#[derive(Clone)]
pub(super) struct AtomicLayout {
    pub(super) tree: BoxTree,
    pub(super) baseline: usize,
}

impl Atom for InlineAtom {
    fn width(&self) -> usize {
        match self {
            Self::Table(output) => output.width,
            Self::Layout(output) => output.tree.width,
        }
    }

    fn height(&self) -> usize {
        match self {
            Self::Table(output) => output.height,
            Self::Layout(output) => output.tree.height,
        }
    }

    fn baseline(&self) -> usize {
        match self {
            Self::Table(output) => output.baseline(),
            Self::Layout(output) => output.baseline,
        }
    }
}

pub(super) type InlinePiece = Piece<InlineAtomSource>;
pub(super) type ResolvedInlinePiece = Piece<InlineAtom>;

#[derive(Clone, Copy)]
struct InlineContext {
    white_space: WhiteSpace,
    style: CellStyle,
    computed: ComputedStyle,
    depth: usize,
    inline_parent: bool,
}

enum FlowEvent {
    Enter(NodeId, InlineContext),
    Exit(NodeId, ComputedStyle, Box<InlineContext>, bool),
    AnonymousTable(Vec<NodeId>, InlineContext),
}

pub(super) fn build_flow_tree(
    document: &Document,
    styles: &StyleTree,
    viewport_width: usize,
) -> Vec<FlowBox> {
    build_flow_tree_from(
        document,
        styles,
        viewport_width,
        document.roots().to_vec(),
        None,
    )
}

pub(super) fn build_flow_subtree(
    document: &Document,
    styles: &StyleTree,
    viewport_width: usize,
    root: NodeId,
) -> Vec<FlowBox> {
    build_flow_tree_from(document, styles, viewport_width, vec![root], Some(root))
}

fn build_flow_tree_from(
    document: &Document,
    styles: &StyleTree,
    viewport_width: usize,
    roots: Vec<NodeId>,
    atomic_root: Option<NodeId>,
) -> Vec<FlowBox> {
    let mut flow = vec![FlowBox {
        owner: None,
        style: ComputedStyle {
            display: Display::BLOCK,
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
            computed: container.map(|node| styles.get(node)).unwrap_or_default(),
            depth,
            inline_parent: false,
        };
        let roots = container
            .map(|node| document.children(node))
            .unwrap_or_else(|| roots.clone());
        let mut events = Vec::new();
        push_children(&mut events, roots, document, styles, inherited);
        let mut buffer = Vec::new();
        let mut children = Vec::new();
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
            if matches!(
                flow[flow_index].style.display.inside(),
                Some(crate::core::style::DisplayInside::Flex)
            ) {
                let pseudo_depth = flow[flow_index].depth.saturating_add(1);
                append_flex_pseudo(
                    &mut flow,
                    &mut children,
                    styles,
                    node,
                    PseudoElement::Before,
                    pseudo_depth,
                );
            } else {
                append_pseudo(styles, node, PseudoElement::Before, own, &mut buffer);
            }
        }
        let mut nested_tasks = Vec::new();
        while let Some(event) = events.pop() {
            match event {
                FlowEvent::Exit(node, style, context, has_edge) => {
                    append_pseudo(
                        styles,
                        node,
                        PseudoElement::After,
                        InlineContext {
                            white_space: style.white_space,
                            style: style.cell_style(),
                            computed: style,
                            depth: context.depth + 1,
                            inline_parent: context.inline_parent,
                        },
                        &mut buffer,
                    );
                    if has_edge {
                        append_edge(node, style, false, *context, &mut buffer);
                    }
                }
                FlowEvent::AnonymousTable(roots, context) => {
                    let node = roots[0];
                    let atom = TableFormatter::new(document, styles).format_anonymous(
                        roots,
                        context.computed,
                        context.inline_parent,
                        viewport_width,
                        TableLimits::default(),
                        0,
                    );
                    let piece = InlinePiece {
                        node,
                        text: String::new(),
                        white_space: context.white_space,
                        depth: context.depth,
                        style: context.style,
                        atom: Some(InlineAtomSource::Ready(Box::new(atom))),
                    };
                    if context.inline_parent {
                        buffer.push(piece);
                    } else {
                        flush_inline(
                            &mut flow,
                            &mut children,
                            &mut buffer,
                            context.depth,
                            context.computed,
                        );
                        let child = flow.len();
                        flow.push(FlowBox {
                            owner: None,
                            style: ComputedStyle::anonymous_inheriting(
                                context.computed,
                                Display::BLOCK,
                            ),
                            depth: context.depth,
                            inline: vec![piece],
                            children: Vec::new(),
                            rule: false,
                            table: None,
                            marker: None,
                        });
                        children.push(child);
                    }
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
                        let mut style = styles.get(node);
                        if atomic_root == Some(node) {
                            style.display = style.display.blockify();
                        }
                        if style.display.is_none() {
                            continue;
                        }
                        let mut inline_style = style.cell_style();
                        if style.display.is_contents() {
                            inline_style.bg = None;
                        }
                        let element_context = InlineContext {
                            white_space: style.white_space,
                            style: inline_style,
                            computed: style,
                            depth: context.depth + 1,
                            inline_parent: if style.display.is_contents() {
                                context.inline_parent
                            } else {
                                style.display.is_inline_flow()
                            },
                        };
                        if style.display.is_contents() {
                            events.push(FlowEvent::Exit(node, style, Box::new(context), false));
                            append_pseudo(
                                styles,
                                node,
                                PseudoElement::Before,
                                element_context,
                                &mut buffer,
                            );
                            push_children(
                                &mut events,
                                document.children(node),
                                document,
                                styles,
                                element_context,
                            );
                        } else if style.display == Display::TABLE {
                            flush_inline(
                                &mut flow,
                                &mut children,
                                &mut buffer,
                                context.depth,
                                context.computed,
                            );
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
                        } else if style.display.is_atomic_inline() {
                            append_horizontal_margin(
                                node,
                                style.margin.left.cells(),
                                context,
                                &mut buffer,
                            );
                            buffer.push(InlinePiece {
                                node,
                                text: String::new(),
                                white_space: context.white_space,
                                depth: context.depth,
                                style: style.cell_style(),
                                atom: Some(InlineAtomSource::Node(node)),
                            });
                            append_horizontal_margin(
                                node,
                                style.margin.right.cells(),
                                context,
                                &mut buffer,
                            );
                        } else if style.display.is_block_container() {
                            flush_inline(
                                &mut flow,
                                &mut children,
                                &mut buffer,
                                context.depth,
                                context.computed,
                            );
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
                            if style.display.is_list_item()
                                && let Some(marker) = styles.marker(node)
                            {
                                buffer.push(InlinePiece {
                                    node,
                                    text: marker.text.clone(),
                                    white_space: marker.style.white_space,
                                    depth: context.depth,
                                    style: marker.style.cell_style(),
                                    atom: None,
                                });
                            }
                            append_edge(node, style, true, context, &mut buffer);
                            events.push(FlowEvent::Exit(node, style, Box::new(context), true));
                            append_pseudo(
                                styles,
                                node,
                                PseudoElement::Before,
                                element_context,
                                &mut buffer,
                            );
                            push_children(
                                &mut events,
                                document.children(node),
                                document,
                                styles,
                                element_context,
                            );
                        }
                    }
                    Some(Node::DocumentFragment) => {
                        push_children(
                            &mut events,
                            document.children(node),
                            document,
                            styles,
                            context,
                        );
                    }
                    _ => {}
                },
            }
        }
        if let Some(node) = container {
            if matches!(
                flow[flow_index].style.display.inside(),
                Some(crate::core::style::DisplayInside::Flex)
            ) {
                let pseudo_depth = flow[flow_index].depth.saturating_add(1);
                append_flex_pseudo(
                    &mut flow,
                    &mut children,
                    styles,
                    node,
                    PseudoElement::After,
                    pseudo_depth,
                );
            } else {
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
        }
        flush_inline(
            &mut flow,
            &mut children,
            &mut buffer,
            depth,
            inherited.computed,
        );
        flow[flow_index].children = children;
        tasks.extend(nested_tasks.into_iter().rev());
    }
    flow
}

fn push_children(
    events: &mut Vec<FlowEvent>,
    children: Vec<NodeId>,
    document: &Document,
    styles: &StyleTree,
    context: InlineContext,
) {
    let mut pending: Vec<_> = children
        .into_iter()
        .rev()
        .map(|node| (node, context))
        .collect();
    let mut flattened = Vec::new();
    while let Some((node, node_context)) = pending.pop() {
        match document.node(node) {
            Some(Node::Element { .. })
                if styles.get(node).display.is_contents()
                    && styles.pseudo(node, PseudoElement::Before).is_none()
                    && styles.pseudo(node, PseudoElement::After).is_none() =>
            {
                let style = styles.get(node);
                let mut inline_style = style.cell_style();
                inline_style.bg = None;
                let child_context = InlineContext {
                    white_space: style.white_space,
                    style: inline_style,
                    computed: style,
                    depth: node_context.depth + 1,
                    inline_parent: node_context.inline_parent,
                };
                pending.extend(
                    document
                        .children(node)
                        .into_iter()
                        .rev()
                        .map(|child| (child, child_context)),
                );
            }
            Some(Node::DocumentFragment) => pending.extend(
                document
                    .children(node)
                    .into_iter()
                    .rev()
                    .map(|child| (child, node_context)),
            ),
            _ => flattened.push((node, node_context)),
        }
    }
    let mut ordered = Vec::new();
    let mut table_run = Vec::new();
    for (child, child_context) in flattened {
        if styles.get(child).display.is_table_internal() {
            table_run.push(child);
        } else {
            let does_not_generate_a_box = styles.get(child).display.is_none()
                || matches!(
                    document.node(child),
                    Some(Node::Comment { .. } | Node::Pi { .. } | Node::Doctype { .. })
                )
                || (!table_run.is_empty()
                    && matches!(document.node(child), Some(Node::Text { data }) if data.chars().all(char::is_whitespace)));
            if does_not_generate_a_box {
                continue;
            }
            if !table_run.is_empty() {
                ordered.push(FlowEvent::AnonymousTable(
                    std::mem::take(&mut table_run),
                    context,
                ));
            }
            ordered.push(FlowEvent::Enter(child, child_context));
        }
    }
    if !table_run.is_empty() {
        ordered.push(FlowEvent::AnonymousTable(table_run, context));
    }
    events.extend(ordered.into_iter().rev());
}

fn flush_inline(
    flow: &mut Vec<FlowBox>,
    children: &mut Vec<usize>,
    buffer: &mut Vec<InlinePiece>,
    depth: usize,
    parent_style: ComputedStyle,
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
        style: ComputedStyle::anonymous_inheriting(parent_style, Display::BLOCK),
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

fn append_flex_pseudo(
    flow: &mut Vec<FlowBox>,
    children: &mut Vec<usize>,
    styles: &StyleTree,
    node: NodeId,
    which: PseudoElement,
    depth: usize,
) {
    let Some(pseudo) = styles.pseudo(node, which) else {
        return;
    };
    let child = flow.len();
    flow.push(FlowBox {
        owner: Some(node),
        style: pseudo.style,
        depth,
        inline: vec![InlinePiece {
            node,
            text: pseudo.text.clone(),
            white_space: pseudo.style.white_space,
            depth,
            style: pseudo.style.cell_style(),
            atom: None,
        }],
        children: Vec::new(),
        rule: false,
        table: None,
        marker: None,
    });
    children.push(child);
}

fn append_edge(
    node: NodeId,
    style: ComputedStyle,
    left: bool,
    context: InlineContext,
    buffer: &mut Vec<InlinePiece>,
) {
    let cells = if left {
        style.margin.left.cells().saturating_add(style.padding.left)
    } else {
        style
            .margin
            .right
            .cells()
            .saturating_add(style.padding.right)
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

fn append_horizontal_margin(
    node: NodeId,
    cells: usize,
    context: InlineContext,
    buffer: &mut Vec<InlinePiece>,
) {
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
    pieces: &[ResolvedInlinePiece],
    col: usize,
    row: usize,
    width: usize,
    text_align: TextAlign,
    merge_base: usize,
) {
    let mut current_row = row;
    for line in format_inline(pieces, width) {
        let (line_height, baseline) = line_metrics(&line, pieces);
        let line_width = line.iter().map(|glyph| glyph.width).sum::<usize>();
        let remaining = width.saturating_sub(line_width);
        let offset = match text_align {
            TextAlign::Right => remaining,
            TextAlign::Center => remaining.div_ceil(2),
            TextAlign::Start | TextAlign::Left | TextAlign::Justify => 0,
        };
        let mut current_col = col.saturating_add(offset);
        for glyph in line {
            if let Some(index) = glyph.atom {
                if let Some(atom) = &pieces[index].atom {
                    let row = current_row + baseline.saturating_sub(atom.baseline());
                    match atom {
                        InlineAtom::Table(table) => append_table_output(
                            tree,
                            table.as_ref().clone(),
                            current_col,
                            row,
                            pieces[index].depth,
                            merge_base.saturating_mul(1_000).saturating_add(index),
                        ),
                        InlineAtom::Layout(layout) => append_atomic_layout(
                            tree,
                            &layout.tree,
                            current_col,
                            row,
                            pieces[index].depth,
                            merge_base.saturating_mul(1_000).saturating_add(index),
                        ),
                    }
                }
                current_col = current_col.saturating_add(glyph.width);
                continue;
            }
            if glyph.style.scale == 0 {
                continue;
            }
            let baseline = current_row + baseline;
            let glyph_row =
                baseline.saturating_sub(usize::from(glyph.style.scale).saturating_sub(1));
            if let Some(last) = tree.fragments.last_mut()
                && last.node == glyph.node
                && last.row == glyph_row
                && last.depth == glyph.depth
                && last.style == glyph.style
                && last.col
                    + UnicodeWidthStr::width(last.text.as_str()) * usize::from(last.style.scale)
                    == current_col
            {
                last.text.push_str(&glyph.text);
            } else {
                tree.fragments.push(TextFragment {
                    node: glyph.node,
                    col: current_col,
                    row: glyph_row,
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

fn append_atomic_layout(
    tree: &mut BoxTree,
    nested: &BoxTree,
    col: usize,
    row: usize,
    depth: usize,
    merge_base: usize,
) {
    tree.height = tree.height.max(row.saturating_add(nested.height));
    for layout_box in &nested.boxes {
        let mut layout_box = layout_box.clone();
        offset_rect(&mut layout_box.border_rect, col, row);
        offset_rect(&mut layout_box.content_rect, col, row);
        layout_box.depth = layout_box.depth.saturating_add(depth);
        tree.boxes.push(layout_box);
    }
    for fill in &nested.fills {
        let mut fill = *fill;
        offset_rect(&mut fill.rect, col, row);
        fill.depth = fill.depth.saturating_add(depth);
        tree.fills.push(fill);
    }
    for stroke in &nested.strokes {
        let mut stroke = *stroke;
        offset_rect(&mut stroke.rect, col, row);
        stroke.depth = stroke.depth.saturating_add(depth);
        stroke.merge_group = merge_base
            .saturating_mul(1_000_000)
            .saturating_add(stroke.merge_group);
        tree.strokes.push(stroke);
    }
    for fragment in &nested.fragments {
        let mut fragment = fragment.clone();
        fragment.col = fragment.col.saturating_add(col);
        fragment.row = fragment.row.saturating_add(row);
        fragment.depth = fragment.depth.saturating_add(depth);
        tree.fragments.push(fragment);
    }
}

fn offset_rect(rect: &mut super::LayoutRect, col: usize, row: usize) {
    rect.col = rect.col.saturating_add(col);
    rect.row = rect.row.saturating_add(row);
}

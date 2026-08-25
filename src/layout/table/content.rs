use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::core::dom::{ElementNs, Node, NodeId};
use crate::core::style::{
    BorderSide, CellStyle, ComputedStyle, Display, PseudoElement, TextAlign, WhiteSpace,
};
use crate::layout::LayoutRect;
use crate::layout::text_flow::{
    Atom, Glyph, Piece, format_inline, formatted_height, intrinsic_width, line_metrics,
    min_content_width, normalize_segment_breaks,
};

use super::geometry::{TableGeometry, intersect_rect, offset_table_rect};
use super::model::TableModel;
use super::sizing::grow_span;
use super::{TableFormatter, TableFragment, TableLimits, TableOutput};

#[derive(Clone)]
pub(super) struct TextRun {
    node: NodeId,
    text: String,
    white_space: WhiteSpace,
    style: CellStyle,
    hidden: bool,
}

#[derive(Clone)]
pub(super) enum CellItem {
    Text(TextRun),
    Table(NodeId),
    Boundary(NodeId, CellStyle),
    Break(NodeId, CellStyle),
}

pub(super) struct CellMetrics {
    pub(super) items: Vec<CellItem>,
    pub(super) minimum: usize,
    pub(super) maximum: usize,
}

pub(super) struct CellLayout {
    pub(super) pieces: Vec<Piece<TableOutput>>,
    pub(super) lines: Vec<Vec<Glyph>>,
    pub(super) text_align: TextAlign,
}

pub(super) struct CellGrid {
    pub(super) layouts: Vec<CellLayout>,
    pub(super) row_heights: Vec<usize>,
    pub(super) row_baselines: Vec<usize>,
}

impl CellLayout {
    pub(super) fn height(&self) -> usize {
        formatted_height(&self.lines, &self.pieces)
    }

    pub(super) fn baseline(&self) -> usize {
        self.lines
            .first()
            .map(|line| line_metrics(line, &self.pieces).1)
            .unwrap_or_else(|| self.height().saturating_sub(1))
    }
}

enum ContentEvent {
    Enter(NodeId, bool),
    Exit(NodeId, bool),
}

#[derive(Clone, Copy)]
pub(super) struct MetricAtom {
    width: usize,
    height: usize,
}

impl Atom for MetricAtom {
    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }
}

impl TableFormatter<'_> {
    pub(super) fn measure_cells(
        &self,
        model: &TableModel,
        limits: TableLimits,
        nesting: usize,
    ) -> Vec<CellMetrics> {
        model
            .cells
            .iter()
            .map(|cell| self.cell_metrics(&cell.roots, self.cell_style(cell), limits, nesting))
            .collect()
    }

    pub(super) fn layout_cells(
        &self,
        model: &TableModel,
        metrics: &[CellMetrics],
        columns: &[usize],
        geometry: &TableGeometry,
        limits: TableLimits,
        nesting: usize,
    ) -> CellGrid {
        let mut layouts = Vec::with_capacity(model.cells.len());
        for (cell, metric) in model.cells.iter().zip(metrics) {
            let span_width = columns[cell.col..cell.col + cell.col_span]
                .iter()
                .sum::<usize>()
                .saturating_add(if geometry.collapsed {
                    geometry
                        .grid
                        .saturating_mul(cell.col_span.saturating_sub(1))
                } else {
                    geometry
                        .spacing_x
                        .saturating_mul(cell.col_span.saturating_sub(1))
                });
            let style = self.cell_style(cell);
            let edges = if geometry.collapsed {
                super::geometry::EdgeInsets {
                    top: geometry.grid,
                    right: geometry.grid,
                    bottom: geometry.grid,
                    left: geometry.grid,
                }
            } else {
                super::geometry::EdgeInsets::from_border(style.border)
            };
            let inner = span_width
                .saturating_sub(edges.left + edges.right + style.padding.left + style.padding.right)
                .max(1);
            let pieces = resolve_items(&metric.items, |node| {
                self.format_inline_atom(node, inner, limits, nesting.saturating_add(1))
            });
            let lines = format_inline(&pieces, inner);
            layouts.push(CellLayout {
                pieces,
                lines,
                text_align: style.text_align,
            });
        }
        let mut row_heights = vec![0usize; model.rows.len()];
        let mut row_baselines = vec![0usize; model.rows.len()];
        let mut row_descents = vec![0usize; model.rows.len()];
        for (cell, layout) in model.cells.iter().zip(&layouts) {
            let style = self.cell_style(cell);
            if cell.row_span == 1 {
                let vertical = style.padding.top
                    + style.padding.bottom
                    + if geometry.collapsed {
                        0
                    } else {
                        style.border.top.layout_width() + style.border.bottom.layout_width()
                    };
                row_heights[cell.row] =
                    row_heights[cell.row].max(layout.height().saturating_add(vertical));
            }
            if style.vertical_align == crate::core::style::VerticalAlign::Baseline {
                let top = style.padding.top
                    + if geometry.collapsed {
                        0
                    } else {
                        style.border.top.layout_width()
                    };
                let bottom = style.padding.bottom
                    + if geometry.collapsed {
                        0
                    } else {
                        style.border.bottom.layout_width()
                    };
                let baseline = top.saturating_add(layout.baseline());
                let descent = bottom.saturating_add(
                    layout
                        .height()
                        .saturating_sub(layout.baseline().saturating_add(1)),
                );
                row_baselines[cell.row] = row_baselines[cell.row].max(baseline);
                if cell.row_span == 1 {
                    row_descents[cell.row] = row_descents[cell.row].max(descent);
                }
            }
        }
        for row in 0..row_heights.len() {
            row_heights[row] = row_heights[row].max(
                row_baselines[row]
                    .saturating_add(row_descents[row])
                    .saturating_add(1),
            );
        }
        for (cell, layout) in model.cells.iter().zip(&layouts) {
            if cell.row_span > 1 {
                let style = self.cell_style(cell);
                let required = layout.height()
                    + style.padding.top
                    + style.padding.bottom
                    + if geometry.collapsed {
                        0
                    } else {
                        style.border.top.layout_width() + style.border.bottom.layout_width()
                    };
                grow_span(&mut row_heights, cell.row, cell.row_span, required);
            }
        }
        CellGrid {
            layouts,
            row_heights,
            row_baselines,
        }
    }

    pub(super) fn cell_metrics(
        &self,
        roots: &[NodeId],
        style: ComputedStyle,
        limits: TableLimits,
        nesting: usize,
    ) -> CellMetrics {
        let items = self.collect_items(roots, true, true);
        let pieces = resolve_items(&items, |node| {
            self.nested_metric(node, limits, nesting.saturating_add(1))
        });
        let minimum = min_content_width(&pieces).max(1);
        let maximum = intrinsic_width(&pieces).max(1);
        let horizontal = style.padding.left
            + style.padding.right
            + style.border.left.layout_width()
            + style.border.right.layout_width();
        CellMetrics {
            items,
            minimum: minimum + horizontal,
            maximum: maximum + horizontal,
        }
    }

    fn nested_metric(&self, table: NodeId, limits: TableLimits, nesting: usize) -> MetricAtom {
        let key = (table, nesting, limits.max_width, limits.max_nesting);
        if let Some(metric) = self.metric_cache.borrow().get(&key).copied() {
            return metric;
        }
        let metric = if nesting >= limits.max_nesting {
            let items = self.collect_items(&[table], false, false);
            let pieces: Vec<Piece<MetricAtom>> = resolve_items(&items, |_| unreachable!());
            let lines = format_inline(&pieces, limits.max_width.max(1));
            MetricAtom {
                width: intrinsic_width(&pieces).max(1),
                height: formatted_height(&lines, &pieces),
            }
        } else {
            let output = self.format_inline_atom(table, limits.max_width, limits, nesting);
            MetricAtom {
                width: output.width,
                height: output.height,
            }
        };
        self.metric_cache.borrow_mut().insert(key, metric);
        metric
    }

    fn push_pseudo(&self, items: &mut Vec<CellItem>, node: NodeId, which: PseudoElement) {
        let Some(pseudo) = self.styles.pseudo(node, which) else {
            return;
        };
        items.push(CellItem::Text(TextRun {
            node,
            text: pseudo.text.clone(),
            white_space: pseudo.style.white_space,
            style: pseudo.style.cell_style(),
            hidden: pseudo.style.visibility.is_hidden(),
        }));
    }

    fn collect_items(
        &self,
        roots: &[NodeId],
        atomize_tables: bool,
        atomize_roots: bool,
    ) -> Vec<CellItem> {
        let mut items = Vec::new();
        let mut stack: Vec<_> = roots
            .iter()
            .rev()
            .copied()
            .map(|node| ContentEvent::Enter(node, true))
            .collect();
        while let Some(event) = stack.pop() {
            let (node, root) = match event {
                ContentEvent::Exit(node, root) => {
                    self.push_pseudo(&mut items, node, PseudoElement::After);
                    if !root && is_blockish(self.styles.get(node).display) {
                        items.push(CellItem::Boundary(node, self.styles.get(node).cell_style()));
                    }
                    continue;
                }
                ContentEvent::Enter(node, root) => (node, root),
            };
            match self.document.node(node) {
                Some(Node::Text { data }) => {
                    let parent = self.document.parent(node).unwrap_or(node);
                    let style = self.styles.get(parent);
                    let mut cell_style = style.cell_style();
                    if style.display.is_contents() {
                        cell_style.bg = None;
                    }
                    items.push(CellItem::Text(TextRun {
                        node,
                        text: normalize_segment_breaks(data),
                        white_space: style.white_space,
                        style: cell_style,
                        hidden: style.visibility.is_hidden(),
                    }));
                }
                Some(Node::Element { name, ns, attrs }) => {
                    let style = self.styles.get(node);
                    if style.display.is_none() {
                        continue;
                    }
                    if atomize_tables
                        && (style.display.is_table() || style.display.is_atomic_inline())
                        && (!root || (atomize_roots && style.display.is_table()))
                    {
                        if style.display == Display::TABLE {
                            items.push(CellItem::Boundary(node, style.cell_style()));
                        }
                        items.push(CellItem::Table(node));
                        if style.display == Display::TABLE {
                            items.push(CellItem::Boundary(node, style.cell_style()));
                        }
                        continue;
                    }
                    if !root && is_blockish(style.display) {
                        items.push(CellItem::Boundary(node, style.cell_style()));
                    }
                    if let Some(marker) = self.styles.marker(node) {
                        items.push(CellItem::Text(TextRun {
                            node,
                            text: marker.text.clone(),
                            white_space: marker.style.white_space,
                            style: marker.style.cell_style(),
                            hidden: marker.style.visibility.is_hidden(),
                        }));
                    }
                    self.push_pseudo(&mut items, node, PseudoElement::Before);
                    if *ns == ElementNs::Html && name == "br" {
                        items.push(CellItem::Break(node, style.cell_style()));
                        continue;
                    }
                    if *ns == ElementNs::Html
                        && name == "img"
                        && let Some(text) = crate::layout::replaced::image_fallback(attrs)
                    {
                        items.push(CellItem::Text(TextRun {
                            node,
                            text,
                            white_space: style.white_space,
                            style: style.cell_style(),
                            hidden: style.visibility.is_hidden(),
                        }));
                    }
                    stack.push(ContentEvent::Exit(node, root));
                    let children = self.document.children(node);
                    stack.extend(
                        children
                            .into_iter()
                            .rev()
                            .map(|child| ContentEvent::Enter(child, false)),
                    );
                }
                Some(Node::DocumentFragment) => {
                    let children = self.document.children(node);
                    stack.extend(
                        children
                            .into_iter()
                            .rev()
                            .map(|child| ContentEvent::Enter(child, false)),
                    );
                }
                _ => {}
            }
        }
        items
    }

    pub(super) fn degraded(&self, table: NodeId, width: usize) -> TableOutput {
        self.degraded_roots(&[table], width)
    }

    pub(super) fn degraded_roots(&self, roots: &[NodeId], width: usize) -> TableOutput {
        let items = self.collect_items(roots, false, false);
        let pieces: Vec<Piece<TableOutput>> = resolve_items(&items, |_| unreachable!());
        let lines = format_inline(&pieces, width.max(1));
        let mut output = TableOutput {
            width: width.max(1),
            height: formatted_height(&lines, &pieces),
            #[cfg(test)]
            degraded: true,
            ..Default::default()
        };
        let output_rect = LayoutRect {
            col: 0,
            row: 0,
            width: output.width,
            height: output.height,
        };
        append_cell_content(
            &mut output,
            &CellLayout {
                pieces,
                lines,
                text_align: self.styles.get(roots[0]).text_align,
            },
            output_rect,
            0,
            1,
        );
        output
    }
}

fn is_blockish(display: Display) -> bool {
    display.is_block_level() || display.is_table_internal()
}

pub(super) fn resolve_items<A: Atom>(
    items: &[CellItem],
    mut resolve_table: impl FnMut(NodeId) -> A,
) -> Vec<Piece<A>> {
    let mut pieces = Vec::new();
    let mut has_content = false;
    let mut pending_boundary = None;
    for item in items {
        match item {
            CellItem::Boundary(node, style) => {
                if has_content {
                    pending_boundary = Some((*node, *style));
                }
            }
            CellItem::Break(node, style) => {
                pieces.push(Piece {
                    node: *node,
                    text: "\n".to_string(),
                    white_space: WhiteSpace::Pre,
                    depth: 0,
                    style: *style,
                    hidden: false,
                    atom: None,
                });
                has_content = true;
                pending_boundary = None;
            }
            CellItem::Text(run) => {
                let visible = !run.text.chars().all(char::is_whitespace)
                    || matches!(
                        run.white_space,
                        WhiteSpace::Pre | WhiteSpace::PreWrap | WhiteSpace::BreakSpaces
                    );
                if visible && let Some((node, style)) = pending_boundary.take() {
                    pieces.push(Piece {
                        node,
                        text: "\n".to_string(),
                        white_space: WhiteSpace::Pre,
                        depth: 0,
                        style,
                        hidden: false,
                        atom: None,
                    });
                }
                pieces.push(Piece {
                    node: run.node,
                    text: run.text.clone(),
                    white_space: run.white_space,
                    depth: 0,
                    style: run.style,
                    hidden: run.hidden,
                    atom: None,
                });
                has_content |= visible;
            }
            CellItem::Table(node) => {
                if let Some((boundary_node, style)) = pending_boundary.take() {
                    pieces.push(Piece {
                        node: boundary_node,
                        text: "\n".to_string(),
                        white_space: WhiteSpace::Pre,
                        depth: 0,
                        style,
                        hidden: false,
                        atom: None,
                    });
                }
                pieces.push(Piece {
                    node: *node,
                    text: String::new(),
                    white_space: WhiteSpace::Normal,
                    depth: 0,
                    style: CellStyle::default(),
                    hidden: false,
                    atom: Some(resolve_table(*node)),
                });
                has_content = true;
            }
        }
    }
    pieces
}

fn append_line(
    fragments: &mut Vec<TableFragment>,
    line: &[Glyph],
    start: usize,
    row: usize,
    clip_right: usize,
    depth: usize,
) {
    let mut col = start;
    for glyph in line {
        if glyph.style.scale == 0 {
            continue;
        }
        if glyph.hidden {
            col = col.saturating_add(glyph.width);
            continue;
        }
        if col.saturating_add(glyph.width) > clip_right {
            break;
        }
        match fragments.last_mut() {
            Some(fragment)
                if fragment.node == glyph.node
                    && fragment.row == row
                    && fragment.col
                        + UnicodeWidthStr::width(fragment.text.as_str())
                            * usize::from(fragment.style.scale)
                        == col
                    && fragment.style == glyph.style
                    && fragment.clip_right == clip_right =>
            {
                fragment.text.push_str(&glyph.text);
            }
            _ => fragments.push(TableFragment {
                node: glyph.node,
                col,
                row,
                text: glyph.text.clone(),
                depth: depth.saturating_add(glyph.depth),
                style: glyph.style,
                clip_right,
            }),
        }
        col += glyph.width;
    }
}

pub(super) fn append_cell_content(
    output: &mut TableOutput,
    layout: &CellLayout,
    clip: LayoutRect,
    depth: usize,
    merge_base: usize,
) {
    let clip_bottom = clip.row.saturating_add(clip.height);
    let clip_right = clip.col.saturating_add(clip.width);
    let mut row = clip.row;
    for line in &layout.lines {
        if row >= clip_bottom {
            break;
        }
        let (height, baseline) = line_metrics(line, &layout.pieces);
        let line_width = line.iter().map(|glyph| glyph.width).sum::<usize>();
        let remaining = clip.width.saturating_sub(line_width);
        let offset = match layout.text_align {
            TextAlign::Right => remaining,
            TextAlign::Center => remaining.div_ceil(2),
            TextAlign::Start | TextAlign::Left | TextAlign::Justify => 0,
        };
        let mut col = clip.col.saturating_add(offset);
        for glyph in line {
            if let Some(index) = glyph.atom {
                if let Some(table) = &layout.pieces[index].atom {
                    append_nested_output(
                        output,
                        table,
                        col,
                        row.saturating_add(baseline.saturating_sub(table.baseline())),
                        clip,
                        depth.saturating_add(layout.pieces[index].depth),
                        merge_base.saturating_add(index),
                    );
                }
            } else {
                append_line(
                    &mut output.fragments,
                    std::slice::from_ref(glyph),
                    col,
                    row.saturating_add(baseline)
                        .saturating_sub(usize::from(glyph.style.scale).saturating_sub(1)),
                    clip_right,
                    depth,
                );
            }
            col = col.saturating_add(glyph.width);
        }
        row = row.saturating_add(height);
    }
}

#[allow(clippy::too_many_arguments)]
fn append_nested_output(
    output: &mut TableOutput,
    nested: &TableOutput,
    col: usize,
    row: usize,
    clip: LayoutRect,
    depth: usize,
    merge_base: usize,
) {
    for layout_box in &nested.boxes {
        let mut layout_box = layout_box.clone();
        offset_table_rect(&mut layout_box.border_rect, col, row);
        offset_table_rect(&mut layout_box.content_rect, col, row);
        let Some(border_rect) = intersect_rect(layout_box.border_rect, clip) else {
            continue;
        };
        layout_box.border_rect = border_rect;
        layout_box.content_rect = intersect_rect(layout_box.content_rect, clip).unwrap_or_default();
        layout_box.depth += depth;
        output.boxes.push(layout_box);
    }
    for fill in &nested.fills {
        let mut fill = *fill;
        offset_table_rect(&mut fill.rect, col, row);
        let Some(rect) = intersect_rect(fill.rect, clip) else {
            continue;
        };
        fill.rect = rect;
        fill.depth += depth;
        output.fills.push(fill);
    }
    for stroke in &nested.strokes {
        let mut stroke = *stroke;
        offset_table_rect(&mut stroke.rect, col, row);
        let unclipped = stroke.rect;
        let Some(rect) = intersect_rect(stroke.rect, clip) else {
            continue;
        };
        if rect.row > unclipped.row {
            stroke.edges.top = BorderSide::default();
        }
        if rect.col > unclipped.col {
            stroke.edges.left = BorderSide::default();
        }
        if rect.row.saturating_add(rect.height) < unclipped.row.saturating_add(unclipped.height) {
            stroke.edges.bottom = BorderSide::default();
        }
        if rect.col.saturating_add(rect.width) < unclipped.col.saturating_add(unclipped.width) {
            stroke.edges.right = BorderSide::default();
        }
        stroke.rect = rect;
        stroke.depth += depth;
        stroke.merge_group = merge_base
            .saturating_mul(1_000_000)
            .saturating_add(stroke.merge_group);
        output.strokes.push(stroke);
    }
    for fragment in &nested.fragments {
        let fragment_col = col.saturating_add(fragment.col);
        let fragment_row = row.saturating_add(fragment.row);
        if fragment_col >= clip.col.saturating_add(clip.width)
            || fragment_row >= clip.row.saturating_add(clip.height)
        {
            continue;
        }
        let right = clip.col.saturating_add(clip.width);
        let text = clip_text(&fragment.text, right.saturating_sub(fragment_col));
        if !text.is_empty() {
            output.fragments.push(TableFragment {
                node: fragment.node,
                col: fragment_col,
                row: fragment_row,
                text,
                depth: depth + fragment.depth,
                style: fragment.style,
                clip_right: right,
            });
        }
    }
}

fn clip_text(text: &str, width: usize) -> String {
    let mut clipped = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let glyph_width = UnicodeWidthStr::width(grapheme);
        if used.saturating_add(glyph_width) > width {
            break;
        }
        clipped.push_str(grapheme);
        used += glyph_width;
    }
    clipped
}

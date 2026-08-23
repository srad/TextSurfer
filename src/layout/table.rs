use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::core::dom::{AttrNs, Document, ElementNs, Node, NodeId};
use crate::core::style::{
    BorderCollapse, BorderEdges, BorderLineStyle, BorderSide, CellStyle, ComputedStyle, CssWidth,
    Display, PseudoElement, StyleTree, TableLayoutMode, WhiteSpace,
};
use crate::layout::text_flow::{
    Atom, Glyph, Piece, format_inline, formatted_height, intrinsic_width, line_height,
    min_content_width, normalize_segment_breaks,
};
use crate::layout::{BackgroundFill, BorderStroke, LayoutBox, LayoutRect};

#[derive(Clone, Copy)]
pub(super) struct TableLimits {
    pub max_rows: usize,
    pub max_cells: usize,
    pub max_columns: usize,
    pub max_width: usize,
    pub max_nesting: usize,
}

impl Default for TableLimits {
    fn default() -> Self {
        Self {
            max_rows: 65_535,
            max_cells: 65_535,
            max_columns: 4_096,
            max_width: 65_535,
            max_nesting: 64,
        }
    }
}

#[derive(Clone)]
pub(super) struct TableFragment {
    pub node: NodeId,
    pub col: usize,
    pub row: usize,
    pub text: String,
    pub depth: usize,
    pub style: CellStyle,
    pub clip_right: usize,
}

#[derive(Clone, Default)]
pub(super) struct TableOutput {
    pub width: usize,
    pub height: usize,
    pub boxes: Vec<LayoutBox>,
    pub fills: Vec<BackgroundFill>,
    pub strokes: Vec<BorderStroke>,
    pub fragments: Vec<TableFragment>,
    pub model: TableModel,
    #[cfg(test)]
    pub degraded: bool,
}

impl TableOutput {
    #[cfg(test)]
    pub fn plain_text(&self) -> String {
        let mut fragments = self.fragments.clone();
        fragments.sort_by_key(|fragment| (fragment.row, fragment.col));
        fragments
            .into_iter()
            .map(|fragment| fragment.text)
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Atom for TableOutput {
    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }
}

#[derive(Clone, Default)]
pub(super) struct TableModel {
    pub rows: Vec<TableRow>,
    pub cells: Vec<TableCell>,
    pub columns: usize,
    captions: Vec<NodeId>,
    column_nodes: Vec<ColumnTrack>,
}

#[derive(Clone)]
pub(super) struct TableRow {
    pub node: Option<NodeId>,
    group_node: Option<NodeId>,
}

#[derive(Clone)]
pub(super) struct TableCell {
    pub owner: Option<NodeId>,
    roots: Vec<NodeId>,
    pub row: usize,
    pub col: usize,
    pub row_span: usize,
    pub col_span: usize,
}

pub(super) struct TableFormatter<'a> {
    document: &'a Document,
    styles: &'a StyleTree,
    metric_cache: RefCell<HashMap<(NodeId, usize, usize, usize), MetricAtom>>,
}

impl<'a> TableFormatter<'a> {
    pub fn new(document: &'a Document, styles: &'a StyleTree) -> Self {
        Self {
            document,
            styles,
            metric_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn format(
        &self,
        table: NodeId,
        available_width: usize,
        limits: TableLimits,
        nesting: usize,
    ) -> TableOutput {
        let available_width = available_width.min(limits.max_width);
        if nesting >= limits.max_nesting {
            return self.degraded(table, available_width);
        }
        let Some(model) = self.build_model(table, limits) else {
            return self.degraded(table, available_width);
        };
        if model.rows.is_empty() || model.columns == 0 {
            return self.empty_table(table, model);
        }
        let output = self.layout_model(table, model, available_width, limits, nesting);
        if output.width > limits.max_width {
            self.degraded(table, available_width)
        } else {
            output
        }
    }

    fn build_model(&self, table: NodeId, limits: TableLimits) -> Option<TableModel> {
        let mut sections = Vec::new();
        let mut captions = Vec::new();
        let mut column_nodes = Vec::new();
        let mut anonymous_cells = Vec::new();
        let mut anonymous_rows = Vec::new();
        let mut group = 0usize;
        for child in self.document.children(table) {
            match self.styles.get(child).display {
                Display::TableCaption => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    captions.push(child);
                }
                Display::TableColumn => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    self.expand_column(child, None, &mut column_nodes);
                }
                Display::TableColumnGroup => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    let before = column_nodes.len();
                    for column in self.document.children(child) {
                        if self.styles.get(column).display == Display::TableColumn {
                            self.expand_column(column, Some(child), &mut column_nodes);
                        }
                    }
                    if column_nodes.len() == before {
                        self.expand_column(child, Some(child), &mut column_nodes);
                    }
                }
                Display::TableHeaderGroup => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    sections.push((SectionKind::Header, self.rows_in_group(child, group)));
                    group += 1;
                }
                Display::TableFooterGroup => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    sections.push((SectionKind::Footer, self.rows_in_group(child, group)));
                    group += 1;
                }
                Display::TableRowGroup => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    sections.push((SectionKind::Body, self.rows_in_group(child, group)));
                    group += 1;
                }
                Display::TableRow => {
                    if !anonymous_cells.is_empty() {
                        anonymous_rows.push(RowSeed {
                            node: None,
                            group,
                            group_node: None,
                            cells: self.cell_seeds(std::mem::take(&mut anonymous_cells)),
                        });
                    }
                    anonymous_rows.push(self.row_seed(child, group, None));
                }
                Display::TableCell => anonymous_cells.push(child),
                Display::None => {}
                _ if self.is_ignorable(child) => {}
                _ => anonymous_cells.push(child),
            }
        }
        self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
        if !anonymous_rows.is_empty() {
            sections.push((SectionKind::Body, anonymous_rows));
        }

        let header = sections
            .iter()
            .position(|(kind, _)| *kind == SectionKind::Header);
        let footer = sections
            .iter()
            .position(|(kind, _)| *kind == SectionKind::Footer);
        let mut seeds = Vec::new();
        if let Some(index) = header {
            seeds.extend(sections[index].1.clone());
        }
        for (index, (_, rows)) in sections.iter().enumerate() {
            if Some(index) != header && Some(index) != footer {
                seeds.extend(rows.clone());
            }
        }
        if let Some(index) = footer {
            seeds.extend(sections[index].1.clone());
        }
        if seeds.len() > limits.max_rows {
            return None;
        }

        let mut occupied = vec![Vec::<bool>::new(); seeds.len()];
        let mut cells = Vec::new();
        let mut columns = 0usize;
        for (row, seed) in seeds.iter().enumerate() {
            for seed_cell in &seed.cells {
                if cells.len() >= limits.max_cells {
                    return None;
                }
                let col_span = seed_cell
                    .owner
                    .map(|node| self.span(node, "colspan", false))
                    .unwrap_or(1)
                    .clamp(1, 1_000);
                let raw_row_span = seed_cell
                    .owner
                    .map(|node| self.span(node, "rowspan", true))
                    .unwrap_or(1);
                let group_end = seeds
                    .iter()
                    .enumerate()
                    .skip(row + 1)
                    .find(|(_, candidate)| candidate.group != seed.group)
                    .map(|(index, _)| index)
                    .unwrap_or(seeds.len());
                let row_span = if raw_row_span == 0 {
                    group_end.saturating_sub(row).max(1)
                } else {
                    raw_row_span.clamp(1, seeds.len().saturating_sub(row).max(1))
                };
                let mut col = 0usize;
                while !range_free(&occupied, row, col, row_span, col_span) {
                    col += 1;
                    if col.saturating_add(col_span) > limits.max_columns {
                        return None;
                    }
                }
                if col.saturating_add(col_span) > limits.max_columns {
                    return None;
                }
                occupy(&mut occupied, row, col, row_span, col_span);
                columns = columns.max(col + col_span);
                cells.push(TableCell {
                    owner: seed_cell.owner,
                    roots: seed_cell.roots.clone(),
                    row,
                    col,
                    row_span,
                    col_span,
                });
            }
        }
        if columns > limits.max_columns {
            return None;
        }
        column_nodes.resize(columns, ColumnTrack::default());
        Some(TableModel {
            rows: seeds
                .into_iter()
                .map(|seed| TableRow {
                    node: seed.node,
                    group_node: seed.group_node,
                })
                .collect(),
            cells,
            columns,
            captions,
            column_nodes,
        })
    }

    fn rows_in_group(&self, group_node: NodeId, group: usize) -> Vec<RowSeed> {
        let mut rows = Vec::new();
        let mut cells = Vec::new();
        for child in self.document.children(group_node) {
            match self.styles.get(child).display {
                Display::TableRow => {
                    if !cells.is_empty() {
                        rows.push(RowSeed {
                            node: None,
                            group,
                            group_node: Some(group_node),
                            cells: self.cell_seeds(std::mem::take(&mut cells)),
                        });
                    }
                    rows.push(self.row_seed(child, group, Some(group_node)));
                }
                Display::TableCell => cells.push(child),
                Display::None => {}
                _ if self.is_ignorable(child) => {}
                _ => cells.push(child),
            }
        }
        if !cells.is_empty() {
            rows.push(RowSeed {
                node: None,
                group,
                group_node: Some(group_node),
                cells: self.cell_seeds(cells),
            });
        }
        rows
    }

    fn row_seed(&self, row: NodeId, group: usize, group_node: Option<NodeId>) -> RowSeed {
        let cells = self.cell_seeds(self.document.children(row));
        RowSeed {
            node: Some(row),
            group,
            group_node,
            cells,
        }
    }

    fn flush_anonymous(&self, group: usize, cells: &mut Vec<NodeId>, rows: &mut Vec<RowSeed>) {
        if !cells.is_empty() {
            rows.push(RowSeed {
                node: None,
                group,
                group_node: None,
                cells: self.cell_seeds(std::mem::take(cells)),
            });
        }
    }

    fn cell_seeds(&self, children: Vec<NodeId>) -> Vec<CellSeed> {
        let mut cells = Vec::new();
        let mut anonymous = Vec::new();
        for child in children {
            let display = self.styles.get(child).display;
            if display == Display::None || self.is_ignorable(child) {
                continue;
            }
            if display == Display::TableCell {
                if !anonymous.is_empty() {
                    cells.push(CellSeed {
                        owner: None,
                        roots: std::mem::take(&mut anonymous),
                    });
                }
                cells.push(CellSeed {
                    owner: Some(child),
                    roots: vec![child],
                });
            } else {
                anonymous.push(child);
            }
        }
        if !anonymous.is_empty() {
            cells.push(CellSeed {
                owner: None,
                roots: anonymous,
            });
        }
        cells
    }

    fn expand_column(&self, node: NodeId, group: Option<NodeId>, columns: &mut Vec<ColumnTrack>) {
        let span = self.span(node, "span", false).clamp(1, 1_000);
        columns.extend(std::iter::repeat_n(
            ColumnTrack {
                group,
                column: Some(node),
            },
            span,
        ));
    }

    fn span(&self, node: NodeId, name: &str, allow_zero: bool) -> usize {
        let Some(Node::Element {
            name: tag,
            ns: ElementNs::Html,
            attrs,
        }) = self.document.node(node)
        else {
            return 1;
        };
        let allowed = match name {
            "colspan" | "rowspan" => matches!(tag.as_str(), "td" | "th"),
            "span" => matches!(tag.as_str(), "col" | "colgroup"),
            _ => false,
        };
        if !allowed {
            return 1;
        }
        attrs
            .iter()
            .find(|attr| attr.ns == AttrNs::None && attr.name == name)
            .and_then(|attr| attr.value.trim().parse::<usize>().ok())
            .filter(|value| allow_zero || *value > 0)
            .unwrap_or(1)
    }

    fn is_ignorable(&self, node: NodeId) -> bool {
        matches!(
            self.document.node(node),
            Some(Node::Text { data }) if data.chars().all(char::is_whitespace)
        ) || matches!(
            self.document.node(node),
            Some(Node::Comment { .. } | Node::Pi { .. } | Node::Doctype { .. })
        )
    }

    fn cell_style(&self, cell: &TableCell) -> ComputedStyle {
        cell.owner
            .map(|node| self.styles.get(node))
            .unwrap_or_default()
    }

    fn layout_model(
        &self,
        table: NodeId,
        model: TableModel,
        available_width: usize,
        limits: TableLimits,
        nesting: usize,
    ) -> TableOutput {
        let table_style = self.styles.get(table);
        let metrics: Vec<_> = model
            .cells
            .iter()
            .map(|cell| self.cell_metrics(&cell.roots, self.cell_style(cell), limits, nesting))
            .collect();
        let collapsed = table_style.border_collapse == BorderCollapse::Collapse;
        let grid = usize::from(
            collapsed
                && (table_style.border.has_layout()
                    || model
                        .cells
                        .iter()
                        .any(|cell| self.cell_style(cell).border.has_layout())
                    || model.rows.iter().any(|row| {
                        row.node
                            .is_some_and(|node| self.styles.get(node).border.has_layout())
                            || row
                                .group_node
                                .is_some_and(|node| self.styles.get(node).border.has_layout())
                    })
                    || model.column_nodes.iter().any(|track| {
                        track
                            .column
                            .is_some_and(|node| self.styles.get(node).border.has_layout())
                            || track
                                .group
                                .is_some_and(|node| self.styles.get(node).border.has_layout())
                    })),
        );
        let spacing_x = if collapsed {
            0
        } else {
            table_style.border_spacing.horizontal
        };
        let spacing_y = if collapsed {
            0
        } else {
            table_style.border_spacing.vertical
        };
        let table_edges = if collapsed {
            EdgeInsets::default()
        } else {
            EdgeInsets::from_border(table_style.border)
        };
        let table_padding = if collapsed {
            crate::core::style::EdgeSizes::default()
        } else {
            table_style.padding
        };
        let fixed_overhead = table_edges.left
            + table_edges.right
            + table_padding.left
            + table_padding.right
            + if collapsed {
                grid.saturating_mul(model.columns + 1)
            } else {
                spacing_x.saturating_mul(model.columns + 1)
            };
        let mut minimum = vec![1usize; model.columns];
        let mut maximum = vec![1usize; model.columns];
        let specified = resolved_width(table_style.width, available_width);
        let percentage_basis = specified.unwrap_or(available_width);
        for (cell, metric) in model.cells.iter().zip(&metrics) {
            if cell.col_span == 1 {
                minimum[cell.col] = minimum[cell.col].max(metric.minimum);
                maximum[cell.col] = maximum[cell.col].max(metric.maximum);
            }
        }
        for (cell, metric) in model.cells.iter().zip(&metrics) {
            if cell.col_span > 1 {
                grow_span(&mut minimum, cell.col, cell.col_span, metric.minimum);
                grow_span(&mut maximum, cell.col, cell.col_span, metric.maximum);
            }
            if let Some(width) = cell_width_hint(self.cell_style(cell), percentage_basis) {
                grow_span(&mut minimum, cell.col, cell.col_span, width);
                grow_span(&mut maximum, cell.col, cell.col_span, width);
            }
        }
        for (col, track) in model.column_nodes.iter().enumerate() {
            if let Some(group) = track.group {
                apply_width_hint(
                    self.styles.get(group).width,
                    available_width,
                    &mut minimum[col],
                );
                maximum[col] = maximum[col].max(minimum[col]);
            }
            if let Some(node) = track.column {
                apply_width_hint(
                    self.styles.get(node).width,
                    available_width,
                    &mut minimum[col],
                );
                maximum[col] = maximum[col].max(minimum[col]);
            }
        }
        let mut columns =
            if table_style.table_layout == TableLayoutMode::Fixed && specified.is_some() {
                let mut fixed = vec![0usize; model.columns];
                for (col, track) in model.column_nodes.iter().enumerate() {
                    if let Some(group) = track.group {
                        apply_width_hint(
                            self.styles.get(group).width,
                            available_width,
                            &mut fixed[col],
                        );
                    }
                    if let Some(node) = track.column {
                        apply_width_hint(
                            self.styles.get(node).width,
                            available_width,
                            &mut fixed[col],
                        );
                    }
                }
                for cell in model.cells.iter().filter(|cell| cell.row == 0) {
                    if let Some(width) = cell_width_hint(self.cell_style(cell), percentage_basis) {
                        let each = width.div_ceil(cell.col_span);
                        for value in &mut fixed[cell.col..cell.col + cell.col_span] {
                            *value = (*value).max(each);
                        }
                    }
                }
                let target = specified.unwrap_or_default().saturating_sub(fixed_overhead);
                distribute_remainder(&mut fixed, target);
                fixed
            } else {
                let natural = maximum.iter().sum::<usize>();
                let target = specified
                    .unwrap_or_else(|| natural.saturating_add(fixed_overhead).min(available_width))
                    .saturating_sub(fixed_overhead)
                    .max(minimum.iter().sum());
                let mut auto = minimum.clone();
                grow_towards(&mut auto, &maximum, target);
                auto
            };
        for value in &mut columns {
            *value = (*value).max(1);
        }

        let caption_natural = model
            .captions
            .iter()
            .map(|caption| {
                self.cell_metrics(&[*caption], self.styles.get(*caption), limits, nesting)
                    .maximum
            })
            .max()
            .unwrap_or(0);
        let mut table_width = columns.iter().sum::<usize>().saturating_add(fixed_overhead);
        if caption_natural > table_width {
            distribute_remainder(&mut columns, caption_natural.saturating_sub(fixed_overhead));
            table_width = columns.iter().sum::<usize>().saturating_add(fixed_overhead);
        }
        table_width = table_width.max(1);

        let mut cell_layouts = Vec::with_capacity(model.cells.len());
        for (cell, metric) in model.cells.iter().zip(&metrics) {
            let span_width = columns[cell.col..cell.col + cell.col_span]
                .iter()
                .sum::<usize>()
                .saturating_add(if collapsed {
                    grid.saturating_mul(cell.col_span.saturating_sub(1))
                } else {
                    spacing_x.saturating_mul(cell.col_span.saturating_sub(1))
                });
            let style = self.cell_style(cell);
            let edges = if collapsed {
                EdgeInsets {
                    top: grid,
                    right: grid,
                    bottom: grid,
                    left: grid,
                }
            } else {
                EdgeInsets::from_border(style.border)
            };
            let inner = span_width
                .saturating_sub(edges.left + edges.right + style.padding.left + style.padding.right)
                .max(1);
            let pieces = resolve_items(&metric.items, |node| {
                self.format(node, inner, limits, nesting.saturating_add(1))
            });
            let lines = format_inline(&pieces, inner);
            cell_layouts.push(CellLayout { pieces, lines });
        }
        let mut row_heights = vec![0usize; model.rows.len()];
        for (cell, layout) in model.cells.iter().zip(&cell_layouts) {
            if cell.row_span == 1 {
                let style = self.cell_style(cell);
                let vertical = style.padding.top
                    + style.padding.bottom
                    + if collapsed {
                        0
                    } else {
                        style.border.top.layout_width() + style.border.bottom.layout_width()
                    };
                row_heights[cell.row] =
                    row_heights[cell.row].max(layout.height().saturating_add(vertical));
            }
        }
        for (cell, layout) in model.cells.iter().zip(&cell_layouts) {
            if cell.row_span > 1 {
                let style = self.cell_style(cell);
                let required = layout.height()
                    + style.padding.top
                    + style.padding.bottom
                    + if collapsed {
                        0
                    } else {
                        style.border.top.layout_width() + style.border.bottom.layout_width()
                    };
                grow_span(&mut row_heights, cell.row, cell.row_span, required);
            }
        }

        let caption_width = table_width;
        let mut top_captions = Vec::new();
        let mut bottom_captions = Vec::new();
        for caption in &model.captions {
            let style = self.styles.get(*caption);
            let edges = EdgeInsets::from_border(style.border);
            let content_width = caption_width
                .saturating_sub(edges.left + edges.right + style.padding.left + style.padding.right)
                .max(1);
            let metrics = self.cell_metrics(&[*caption], style, limits, nesting);
            let pieces = resolve_items(&metrics.items, |node| {
                self.format(node, content_width, limits, nesting.saturating_add(1))
            });
            let lines = format_inline(&pieces, content_width);
            let content_height = formatted_height(&lines, &pieces);
            let height = edges.top
                + edges.bottom
                + style.padding.top
                + style.padding.bottom
                + content_height;
            let layout = CaptionLayout {
                node: *caption,
                style,
                layout: CellLayout { pieces, lines },
                border_rect: LayoutRect {
                    col: 0,
                    row: 0,
                    width: caption_width,
                    height,
                },
                content_rect: LayoutRect {
                    col: edges.left + style.padding.left,
                    row: edges.top + style.padding.top,
                    width: content_width,
                    height: content_height,
                },
            };
            if style.caption_side == crate::core::style::CaptionSide::Top {
                top_captions.push(layout);
            } else {
                bottom_captions.push(layout);
            }
        }
        let top_height: usize = top_captions
            .iter()
            .map(|caption| caption.border_rect.height)
            .sum();
        let bottom_height: usize = bottom_captions
            .iter()
            .map(|caption| caption.border_rect.height)
            .sum();
        let grid_height = table_edges.top
            + table_edges.bottom
            + table_padding.top
            + table_padding.bottom
            + row_heights.iter().sum::<usize>()
            + if collapsed {
                grid.saturating_mul(model.rows.len() + 1)
            } else {
                spacing_y.saturating_mul(model.rows.len() + 1)
            };
        let mut output = TableOutput {
            width: table_width,
            height: top_height + grid_height + bottom_height,
            model,
            ..Default::default()
        };
        let table_rect = LayoutRect {
            col: 0,
            row: top_height,
            width: table_width,
            height: grid_height,
        };
        output.boxes.push(LayoutBox {
            node: table,
            border_rect: table_rect,
            content_rect: table_rect,
            depth: 0,
            style: table_style.cell_style(),
        });
        add_fill(&mut output.fills, table_rect, table_style, 0);
        if !collapsed && table_style.border.is_visible() {
            output.strokes.push(BorderStroke {
                rect: table_rect,
                edges: table_style.border,
                style: table_style.cell_style(),
                depth: 0,
                merge_group: 1,
            });
        }

        let x_positions = track_positions(
            &columns,
            table_edges.left + table_padding.left + if collapsed { 0 } else { spacing_x },
            if collapsed { grid } else { spacing_x },
        );
        let y_positions = track_positions(
            &row_heights,
            top_height
                + table_edges.top
                + table_padding.top
                + if collapsed { 0 } else { spacing_y },
            if collapsed { grid } else { spacing_y },
        );
        for (column, track) in output.model.column_nodes.iter().enumerate() {
            let rect = LayoutRect {
                col: x_positions[column],
                row: table_rect.row,
                width: columns[column],
                height: table_rect.height,
            };
            if let Some(group) = track.group {
                add_fill(&mut output.fills, rect, self.styles.get(group), 1);
            }
            if let Some(node) = track.column {
                add_fill(&mut output.fills, rect, self.styles.get(node), 2);
            }
        }
        let mut painted_groups = Vec::new();
        for (row_index, row) in output.model.rows.iter().enumerate() {
            let Some(group) = row.group_node else {
                continue;
            };
            if painted_groups.contains(&group) {
                continue;
            }
            painted_groups.push(group);
            let end = output
                .model
                .rows
                .iter()
                .enumerate()
                .skip(row_index + 1)
                .find(|(_, candidate)| candidate.group_node != Some(group))
                .map(|(index, _)| index)
                .unwrap_or(output.model.rows.len());
            let rect = LayoutRect {
                col: table_rect.col,
                row: y_positions[row_index],
                width: table_rect.width,
                height: y_positions[end]
                    .saturating_sub(y_positions[row_index])
                    .saturating_sub(if collapsed { 0 } else { spacing_y }),
            };
            let style = self.styles.get(group);
            add_fill(&mut output.fills, rect, style, 3);
        }
        for (row_index, row) in output.model.rows.iter().enumerate() {
            if let Some(node) = row.node {
                let rect = LayoutRect {
                    col: x_positions[0],
                    row: y_positions[row_index],
                    width: table_width.saturating_sub(x_positions[0]),
                    height: row_heights[row_index],
                };
                add_fill(&mut output.fills, rect, self.styles.get(node), 4);
            }
        }

        let mut collapse_segments = BTreeMap::new();
        if collapsed {
            add_collapsed_candidates(
                &mut collapse_segments,
                table_rect,
                table_style.border,
                table_style.cell_style(),
                0,
            );
            for (column, track) in output.model.column_nodes.iter().enumerate() {
                let rect = LayoutRect {
                    col: x_positions[column],
                    row: table_rect.row,
                    width: x_positions[column + 1]
                        .saturating_sub(x_positions[column])
                        .saturating_add(grid),
                    height: table_rect.height,
                };
                if let Some(group) = track.group {
                    let style = self.styles.get(group);
                    add_collapsed_candidates(
                        &mut collapse_segments,
                        rect,
                        style.border,
                        style.cell_style(),
                        1,
                    );
                }
                if let Some(node) = track.column {
                    let style = self.styles.get(node);
                    add_collapsed_candidates(
                        &mut collapse_segments,
                        rect,
                        style.border,
                        style.cell_style(),
                        2,
                    );
                }
            }
            for (row_index, row) in output.model.rows.iter().enumerate() {
                let rect = LayoutRect {
                    col: table_rect.col,
                    row: y_positions[row_index],
                    width: table_rect.width,
                    height: y_positions[row_index + 1]
                        .saturating_sub(y_positions[row_index])
                        .saturating_add(grid),
                };
                if let Some(group) = row.group_node {
                    let style = self.styles.get(group);
                    add_collapsed_candidates(
                        &mut collapse_segments,
                        rect,
                        style.border,
                        style.cell_style(),
                        3,
                    );
                }
                if let Some(node) = row.node {
                    let style = self.styles.get(node);
                    add_collapsed_candidates(
                        &mut collapse_segments,
                        rect,
                        style.border,
                        style.cell_style(),
                        4,
                    );
                }
            }
        }
        let placed_cells = output.model.cells.clone();
        for (index, cell) in placed_cells.iter().enumerate() {
            let style = self.cell_style(cell);
            let left = x_positions[cell.col];
            let top = y_positions[cell.row];
            let right = x_positions[cell.col + cell.col_span].saturating_sub(if collapsed {
                0
            } else {
                spacing_x
            });
            let bottom = y_positions[cell.row + cell.row_span].saturating_sub(if collapsed {
                0
            } else {
                spacing_y
            });
            let rect = LayoutRect {
                col: left,
                row: top,
                width: right.saturating_sub(left).saturating_add(grid),
                height: bottom.saturating_sub(top).saturating_add(grid),
            };
            let edge = if collapsed {
                EdgeInsets {
                    top: grid,
                    right: grid,
                    bottom: grid,
                    left: grid,
                }
            } else {
                EdgeInsets::from_border(style.border)
            };
            let content_col = rect.col + edge.left + style.padding.left;
            let content_row = rect.row + edge.top + style.padding.top;
            let content_width = rect
                .width
                .saturating_sub(edge.left + edge.right + style.padding.left + style.padding.right);
            let content_height = rect
                .height
                .saturating_sub(edge.top + edge.bottom + style.padding.top + style.padding.bottom);
            let content_rect = LayoutRect {
                col: content_col,
                row: content_row,
                width: content_width,
                height: content_height,
            };
            let mut hit_rect = rect;
            if collapsed && cell.col + cell.col_span < output.model.columns {
                hit_rect.width = hit_rect.width.saturating_sub(grid);
            }
            if collapsed && cell.row + cell.row_span < output.model.rows.len() {
                hit_rect.height = hit_rect.height.saturating_sub(grid);
            }
            if let Some(node) = cell.owner {
                output.boxes.push(LayoutBox {
                    node,
                    border_rect: hit_rect,
                    content_rect,
                    depth: 5,
                    style: style.cell_style(),
                });
            }
            add_fill(&mut output.fills, rect, style, 5);
            if collapsed {
                add_collapsed_candidates(
                    &mut collapse_segments,
                    rect,
                    style.border,
                    style.cell_style(),
                    5,
                );
            } else if style.border.is_visible() {
                output.strokes.push(BorderStroke {
                    rect,
                    edges: style.border,
                    style: style.cell_style(),
                    depth: 5,
                    merge_group: index + 2,
                });
            }
            append_cell_content(
                &mut output,
                &cell_layouts[index],
                content_rect,
                6,
                (index + 1).saturating_mul(1_000),
            );
        }
        if collapsed {
            output
                .strokes
                .extend(collapse_segments.into_values().filter_map(|candidate| {
                    candidate.side.is_visible().then_some(BorderStroke {
                        rect: candidate.rect,
                        edges: candidate.edges,
                        style: candidate.style,
                        depth: candidate.depth,
                        merge_group: 1,
                    })
                }));
        }
        append_captions(&mut output, &top_captions, 0, caption_width);
        append_captions(
            &mut output,
            &bottom_captions,
            top_height + grid_height,
            caption_width,
        );
        output
    }

    fn cell_metrics(
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
            let output = self.format(table, limits.max_width, limits, nesting);
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
                    items.push(CellItem::Text(TextRun {
                        node,
                        text: normalize_segment_breaks(data),
                        white_space: style.white_space,
                        style: style.cell_style(),
                    }));
                }
                Some(Node::Element { name, ns, attrs }) => {
                    let style = self.styles.get(node);
                    if style.display == Display::None {
                        continue;
                    }
                    if atomize_tables
                        && matches!(style.display, Display::Table | Display::InlineTable)
                        && (!root || atomize_roots)
                    {
                        if style.display == Display::Table {
                            items.push(CellItem::Boundary(node, style.cell_style()));
                        }
                        items.push(CellItem::Table(node));
                        if style.display == Display::Table {
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
                        }));
                    }
                    self.push_pseudo(&mut items, node, PseudoElement::Before);
                    if *ns == ElementNs::Html && name == "br" {
                        items.push(CellItem::Break(node, style.cell_style()));
                        continue;
                    }
                    if *ns == ElementNs::Html
                        && name == "img"
                        && let Some(text) = super::image_fallback(attrs)
                    {
                        items.push(CellItem::Text(TextRun {
                            node,
                            text,
                            white_space: style.white_space,
                            style: style.cell_style(),
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

    fn degraded(&self, table: NodeId, width: usize) -> TableOutput {
        let items = self.collect_items(&[table], false, false);
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
            &CellLayout { pieces, lines },
            output_rect,
            0,
            1,
        );
        output
    }

    fn empty_table(&self, table: NodeId, model: TableModel) -> TableOutput {
        let style = self.styles.get(table);
        let rect = LayoutRect {
            col: 0,
            row: 0,
            width: 1,
            height: 1,
        };
        TableOutput {
            width: 1,
            height: 1,
            boxes: vec![LayoutBox {
                node: table,
                border_rect: rect,
                content_rect: rect,
                depth: 0,
                style: style.cell_style(),
            }],
            model,
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SectionKind {
    Header,
    Body,
    Footer,
}

#[derive(Clone)]
struct RowSeed {
    node: Option<NodeId>,
    group: usize,
    group_node: Option<NodeId>,
    cells: Vec<CellSeed>,
}

#[derive(Clone)]
struct CellSeed {
    owner: Option<NodeId>,
    roots: Vec<NodeId>,
}

#[derive(Clone, Copy, Default)]
struct ColumnTrack {
    group: Option<NodeId>,
    column: Option<NodeId>,
}

#[derive(Clone)]
struct TextRun {
    node: NodeId,
    text: String,
    white_space: WhiteSpace,
    style: CellStyle,
}

#[derive(Clone)]
enum CellItem {
    Text(TextRun),
    Table(NodeId),
    Boundary(NodeId, CellStyle),
    Break(NodeId, CellStyle),
}

struct CellMetrics {
    items: Vec<CellItem>,
    minimum: usize,
    maximum: usize,
}

struct CellLayout {
    pieces: Vec<Piece<TableOutput>>,
    lines: Vec<Vec<Glyph>>,
}

impl CellLayout {
    fn height(&self) -> usize {
        formatted_height(&self.lines, &self.pieces)
    }
}

struct CaptionLayout {
    node: NodeId,
    style: ComputedStyle,
    layout: CellLayout,
    border_rect: LayoutRect,
    content_rect: LayoutRect,
}

enum ContentEvent {
    Enter(NodeId, bool),
    Exit(NodeId, bool),
}

#[derive(Clone, Copy)]
struct MetricAtom {
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

#[derive(Clone, Copy, Default)]
struct EdgeInsets {
    top: usize,
    right: usize,
    bottom: usize,
    left: usize,
}

impl EdgeInsets {
    fn from_border(border: BorderEdges) -> Self {
        Self {
            top: border.top.layout_width(),
            right: border.right.layout_width(),
            bottom: border.bottom.layout_width(),
            left: border.left.layout_width(),
        }
    }
}

#[derive(Clone, Copy)]
struct BorderCandidate {
    rect: LayoutRect,
    edges: BorderEdges,
    side: BorderSide,
    style: CellStyle,
    depth: usize,
}

fn range_free(
    occupied: &[Vec<bool>],
    row: usize,
    col: usize,
    row_span: usize,
    col_span: usize,
) -> bool {
    occupied[row..row + row_span]
        .iter()
        .all(|line| (col..col + col_span).all(|column| !line.get(column).copied().unwrap_or(false)))
}

fn occupy(occupied: &mut [Vec<bool>], row: usize, col: usize, row_span: usize, col_span: usize) {
    for line in &mut occupied[row..row + row_span] {
        line.resize(line.len().max(col + col_span), false);
        line[col..col + col_span].fill(true);
    }
}

fn grow_span(values: &mut [usize], start: usize, span: usize, required: usize) {
    let current = values[start..start + span].iter().sum::<usize>();
    distribute_remainder(&mut values[start..start + span], required.max(current));
}

fn distribute_remainder(values: &mut [usize], target: usize) {
    if values.is_empty() {
        return;
    }
    let current = values.iter().sum::<usize>();
    let mut extra = target.saturating_sub(current);
    let mut index = 0usize;
    while extra > 0 {
        values[index % values.len()] += 1;
        index += 1;
        extra -= 1;
    }
}

fn grow_towards(values: &mut [usize], maximum: &[usize], target: usize) {
    let mut remaining = target.saturating_sub(values.iter().sum());
    while remaining > 0 {
        let mut changed = false;
        for (value, max) in values.iter_mut().zip(maximum) {
            if *value < *max && remaining > 0 {
                *value += 1;
                remaining -= 1;
                changed = true;
            }
        }
        if !changed {
            distribute_remainder(values, values.iter().sum::<usize>() + remaining);
            break;
        }
    }
}

fn apply_width_hint(width: CssWidth, basis: usize, target: &mut usize) {
    if let Some(value) = resolved_width(width, basis) {
        *target = (*target).max(value);
    }
}

fn resolved_width(width: CssWidth, basis: usize) -> Option<usize> {
    match width {
        CssWidth::Auto => None,
        CssWidth::Cells(value) => Some(value),
        CssWidth::Percent(value) => Some(value.resolve(basis)),
    }
}

fn cell_width_hint(style: ComputedStyle, basis: usize) -> Option<usize> {
    resolved_width(style.width, basis).map(|width| {
        if style.box_sizing == crate::core::style::BoxSizing::ContentBox {
            width
                + style.padding.left
                + style.padding.right
                + style.border.left.layout_width()
                + style.border.right.layout_width()
        } else {
            width
        }
    })
}

fn track_positions(values: &[usize], start: usize, gap: usize) -> Vec<usize> {
    let mut positions = Vec::with_capacity(values.len() + 1);
    positions.push(start);
    for value in values {
        positions.push(positions.last().copied().unwrap_or(start) + value + gap);
    }
    positions
}

fn is_blockish(display: Display) -> bool {
    matches!(
        display,
        Display::Block
            | Display::ListItem
            | Display::Table
            | Display::TableHeaderGroup
            | Display::TableRowGroup
            | Display::TableFooterGroup
            | Display::TableRow
            | Display::TableCell
            | Display::TableCaption
    )
}

fn resolve_items<A: Atom>(
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
                        atom: None,
                    });
                }
                pieces.push(Piece {
                    node: run.node,
                    text: run.text.clone(),
                    white_space: run.white_space,
                    depth: 0,
                    style: run.style,
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
                        atom: None,
                    });
                }
                pieces.push(Piece {
                    node: *node,
                    text: String::new(),
                    white_space: WhiteSpace::Normal,
                    depth: 0,
                    style: CellStyle::default(),
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
        if col.saturating_add(glyph.width) > clip_right {
            break;
        }
        match fragments.last_mut() {
            Some(fragment)
                if fragment.node == glyph.node
                    && fragment.row == row
                    && fragment.col + UnicodeWidthStr::width(fragment.text.as_str()) == col
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

fn append_cell_content(
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
        let height = line_height(line, &layout.pieces);
        let mut col = clip.col;
        for glyph in line {
            if let Some(index) = glyph.atom {
                if let Some(table) = &layout.pieces[index].atom {
                    append_nested_output(
                        output,
                        table,
                        col,
                        row.saturating_add(height.saturating_sub(table.height)),
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
                    row.saturating_add(height - 1),
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

fn offset_table_rect(rect: &mut LayoutRect, col: usize, row: usize) {
    rect.col = rect.col.saturating_add(col);
    rect.row = rect.row.saturating_add(row);
}

fn intersect_rect(left: LayoutRect, right: LayoutRect) -> Option<LayoutRect> {
    let col = left.col.max(right.col);
    let row = left.row.max(right.row);
    let far_col = left
        .col
        .saturating_add(left.width)
        .min(right.col.saturating_add(right.width));
    let far_row = left
        .row
        .saturating_add(left.height)
        .min(right.row.saturating_add(right.height));
    (far_col > col && far_row > row).then_some(LayoutRect {
        col,
        row,
        width: far_col - col,
        height: far_row - row,
    })
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

fn add_fill(fills: &mut Vec<BackgroundFill>, rect: LayoutRect, style: ComputedStyle, depth: usize) {
    if let Some(color) = style.background {
        fills.push(BackgroundFill { rect, color, depth });
    }
}

fn add_collapsed_candidates(
    segments: &mut BTreeMap<(bool, usize, usize), BorderCandidate>,
    rect: LayoutRect,
    edges: BorderEdges,
    style: CellStyle,
    depth: usize,
) {
    let right = rect.col + rect.width.saturating_sub(1);
    let bottom = rect.row + rect.height.saturating_sub(1);
    for col in rect.col..right {
        choose_candidate(
            segments,
            (true, rect.row, col),
            horizontal_candidate(col, rect.row, edges.top, style, depth),
        );
        choose_candidate(
            segments,
            (true, bottom, col),
            horizontal_candidate(col, bottom, edges.bottom, style, depth),
        );
    }
    for row in rect.row..bottom {
        choose_candidate(
            segments,
            (false, row, rect.col),
            vertical_candidate(rect.col, row, edges.left, style, depth),
        );
        choose_candidate(
            segments,
            (false, row, right),
            vertical_candidate(right, row, edges.right, style, depth),
        );
    }
}

fn horizontal_candidate(
    col: usize,
    row: usize,
    side: BorderSide,
    style: CellStyle,
    depth: usize,
) -> BorderCandidate {
    BorderCandidate {
        rect: LayoutRect {
            col,
            row,
            width: 2,
            height: 1,
        },
        edges: BorderEdges {
            top: side,
            ..Default::default()
        },
        side,
        style,
        depth,
    }
}

fn vertical_candidate(
    col: usize,
    row: usize,
    side: BorderSide,
    style: CellStyle,
    depth: usize,
) -> BorderCandidate {
    BorderCandidate {
        rect: LayoutRect {
            col,
            row,
            width: 1,
            height: 2,
        },
        edges: BorderEdges {
            left: side,
            ..Default::default()
        },
        side,
        style,
        depth,
    }
}

fn choose_candidate(
    segments: &mut BTreeMap<(bool, usize, usize), BorderCandidate>,
    key: (bool, usize, usize),
    candidate: BorderCandidate,
) {
    match segments.get(&key) {
        Some(current)
            if border_priority(current.side, current.depth)
                >= border_priority(candidate.side, candidate.depth) => {}
        _ => {
            segments.insert(key, candidate);
        }
    }
}

fn border_priority(side: BorderSide, origin: usize) -> (u8, u8, usize) {
    if side.style == BorderLineStyle::Hidden {
        return (3, u8::MAX, origin);
    }
    if side.style == BorderLineStyle::None || side.width == 0 {
        return (0, 0, origin);
    }
    (2, side.style as u8, origin)
}

fn append_captions(
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
        output.boxes.push(LayoutBox {
            node: caption.node,
            border_rect,
            content_rect,
            depth: 1,
            style: caption.style.cell_style(),
        });
        add_fill(&mut output.fills, border_rect, caption.style, 1);
        if caption.style.border.is_visible() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geom::Size;
    use crate::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};
    use crate::html::{Html5everParser, HtmlParser};
    use proptest::prelude::*;

    fn formatted(source: &str, width: usize) -> TableOutput {
        let outcome = Html5everParser::new(false).parse_document(source);
        let document = outcome.document.borrow();
        let sheets = vec![
            CssparserParser.parse(
                document
                    .element_by_id("css")
                    .and_then(|id| document.first_child(id))
                    .and_then(|id| match document.node(id) {
                        Some(crate::core::dom::Node::Text { data }) => Some(data.as_str()),
                        _ => None,
                    })
                    .unwrap_or_default(),
            ),
        ];
        let styles = BasicCascade.apply(
            &sheets,
            &document,
            MediaContext::screen().with_viewport(Size {
                cols: width as u16,
                rows: 24,
            }),
        );
        let table = document.element_by_id("table").unwrap();
        TableFormatter::new(&document, &styles).format(table, width, TableLimits::default(), 0)
    }

    #[test]
    fn spans_form_a_sparse_non_overlapping_grid() {
        let output = formatted(
            "<table id=table><tbody><tr><td rowspan=2>A</td><td>B</td></tr>
             <tr><td>C</td></tr><tr><td colspan=2>D</td></tr></tbody></table>",
            30,
        );
        assert_eq!(output.model.rows.len(), 3);
        assert_eq!(output.model.columns, 2);
        assert_eq!(output.model.cells[0].row_span, 2);
        assert_eq!(output.model.cells[3].col_span, 2);
        for (index, left) in output.model.cells.iter().enumerate() {
            for right in output.model.cells.iter().skip(index + 1) {
                assert!(
                    left.row + left.row_span <= right.row
                        || right.row + right.row_span <= left.row
                        || left.col + left.col_span <= right.col
                        || right.col + right.col_span <= left.col
                );
            }
        }
    }

    #[test]
    fn image_alt_fallbacks_match_inside_table_cells() {
        let output = formatted(
            "<table id=table><tr><td>A<img alt=''>B<img alt='   '>C<img alt='cat'><img></td></tr></table>",
            40,
        );
        let mut fragments: Vec<_> = output.fragments.iter().collect();
        fragments.sort_by_key(|fragment| (fragment.row, fragment.col));
        let text = fragments
            .into_iter()
            .map(|fragment| fragment.text.as_str())
            .collect::<String>();
        assert_eq!(text, "ABC[cat][img]");
    }

    #[test]
    fn fixed_layout_clips_wide_graphemes_at_the_cell_inner_edge() {
        let output = formatted(
            "<style id=css>#table { table-layout: fixed; width: 5px } td { border: solid }</style>
             <table id=table><tr><td>ab界z</td><td>x</td></tr></table>",
            30,
        );
        assert!(output.width >= 5);
        assert!(output.fragments.iter().all(|fragment| {
            fragment.col + unicode_width::UnicodeWidthStr::width(fragment.text.as_str())
                <= fragment.clip_right
        }));
        assert!(
            output
                .fragments
                .iter()
                .all(|fragment| !fragment.text.contains('…'))
        );
    }

    #[test]
    fn collapsed_spans_share_one_connected_border_group() {
        let output = formatted(
            "<style id=css>#table { border-collapse: collapse } td { border: solid red }</style>
             <table id=table><tr><td colspan=2>A</td></tr><tr><td>B</td><td>C</td></tr></table>",
            30,
        );
        assert!(!output.strokes.is_empty());
        assert!(
            output
                .strokes
                .iter()
                .all(|stroke| stroke.merge_group == output.strokes[0].merge_group)
        );
    }

    #[test]
    fn resource_limit_falls_back_without_losing_text() {
        let output = formatted("<table id=table><tr><td>A</td><td>B</td></tr></table>", 30);
        let outcome = Html5everParser::new(false)
            .parse_document("<table id=table><tr><td>A</td><td>B</td></tr></table>");
        let document = outcome.document.borrow();
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let table = document.element_by_id("table").unwrap();
        let limited = TableFormatter::new(&document, &styles).format(
            table,
            30,
            TableLimits {
                max_columns: 1,
                ..Default::default()
            },
            0,
        );
        assert!(!output.fragments.is_empty());
        assert!(limited.degraded);
        assert_eq!(limited.plain_text(), "A B");

        let outcome = Html5everParser::new(false).parse_document(
            "<table id=table><tr><td>before<table><tr><td>middle<table><tr><td>deep</td></tr></table>after-middle</td></tr></table>after</td></tr></table>",
        );
        let document = outcome.document.borrow();
        let styles = BasicCascade.apply(&[], &document, MediaContext::screen());
        let table = document.element_by_id("table").unwrap();
        let nested = TableFormatter::new(&document, &styles).format(
            table,
            30,
            TableLimits {
                max_nesting: 1,
                ..Default::default()
            },
            0,
        );
        assert_eq!(nested.plain_text(), "before middle deep after-middle after");
    }

    #[test]
    fn authored_table_cells_receive_anonymous_row_fixup() {
        let output = formatted(
            "<style id=css>#table { display: table } .cell { display: table-cell }</style>
             <div id=table><span class=cell>A</span><span class=cell>B</span></div>",
            30,
        );
        assert_eq!(output.model.rows.len(), 1);
        assert_eq!(output.model.columns, 2);
        assert_eq!(output.plain_text(), "A B");
    }

    #[test]
    fn zero_rowspan_stops_at_the_explicit_group_boundary() {
        let output = formatted(
            "<table id=table><tbody><tr><td rowspan=0>A</td><td>B</td></tr><tr><td>C</td></tr></tbody>
             <tbody><tr><td>D</td><td>E</td></tr></tbody></table>",
            30,
        );
        assert_eq!(output.model.cells[0].row_span, 2);
        assert_eq!(output.model.rows.len(), 3);
    }

    #[test]
    fn hidden_collapsed_edge_suppresses_the_competing_visible_edge() {
        let output = formatted(
            "<style id=css>#table { border-collapse: collapse }
             td { border: solid } .left { border-right-style: hidden }</style>
             <table id=table><tr><td class=left>A</td><td>B</td></tr></table>",
            30,
        );
        let left_node = output.model.cells[0].owner.unwrap();
        let left_box = output
            .boxes
            .iter()
            .find(|layout_box| layout_box.node == left_node)
            .unwrap();
        let boundary = left_box.border_rect.col + left_box.border_rect.width - 1;
        assert!(
            !output
                .strokes
                .iter()
                .any(|stroke| stroke.rect.col == boundary && stroke.rect.width == 1)
        );
    }

    #[test]
    fn table_hit_boxes_are_laminar_and_text_cells_are_disjoint() {
        let output = formatted(
            "<style id=css>#table { border-collapse: collapse } td { border: solid }</style>
             <table id=table><tr><td rowspan=2>A</td><td>B</td></tr><tr><td>C</td></tr></table>",
            30,
        );
        for (index, left) in output.boxes.iter().enumerate() {
            for right in output.boxes.iter().skip(index + 1) {
                if rects_intersect(left.border_rect, right.border_rect) {
                    assert!(
                        rect_contains(left.border_rect, right.border_rect)
                            || rect_contains(right.border_rect, left.border_rect)
                    );
                }
            }
        }
        let mut occupied = std::collections::HashSet::new();
        for fragment in &output.fragments {
            for col in fragment.col..fragment.col + UnicodeWidthStr::width(fragment.text.as_str()) {
                assert!(occupied.insert((fragment.row, col)));
            }
        }
    }

    proptest! {
        #[test]
        fn generated_spans_never_overlap(
            spans in prop::collection::vec(1usize..=4, 1..24),
            width in 8usize..80,
        ) {
            let cells = spans
                .iter()
                .enumerate()
                .map(|(index, span)| format!("<td colspan={span}>{index}</td>"))
                .collect::<String>();
            let source = format!("<table id=table><tr>{cells}</tr></table>");
            let output = formatted(&source, width);
            for (index, left) in output.model.cells.iter().enumerate() {
                for right in output.model.cells.iter().skip(index + 1) {
                    prop_assert!(
                        left.row + left.row_span <= right.row
                            || right.row + right.row_span <= left.row
                            || left.col + left.col_span <= right.col
                            || right.col + right.col_span <= left.col
                    );
                }
            }
        }

        #[test]
        fn wider_table_viewports_do_not_increase_height(
            narrow in 8usize..40,
            extra in 0usize..40,
        ) {
            let source = "<table id=table><tr><td>alpha beta gamma delta</td><td>one two three four</td></tr></table>";
            let narrow_output = formatted(source, narrow);
            let wide_output = formatted(source, narrow + extra);
            prop_assert!(wide_output.height <= narrow_output.height);
        }
    }

    fn rects_intersect(left: LayoutRect, right: LayoutRect) -> bool {
        left.col < right.col + right.width
            && right.col < left.col + left.width
            && left.row < right.row + right.height
            && right.row < left.row + left.height
    }

    fn rect_contains(outer: LayoutRect, inner: LayoutRect) -> bool {
        outer.col <= inner.col
            && outer.row <= inner.row
            && outer.col + outer.width >= inner.col + inner.width
            && outer.row + outer.height >= inner.row + inner.height
    }
}

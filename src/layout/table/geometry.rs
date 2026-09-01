use std::collections::{BTreeMap, BTreeSet};

use crate::core::style::{BorderCollapse, BorderEdges, ComputedStyle};
use crate::layout::{BackgroundFill, BorderStroke, LayoutBox, LayoutRect};

use super::borders::add_collapsed_candidates;
use super::captions::{CaptionBands, append_captions};
use super::content::{CellGrid, append_cell_content};
use super::model::TableModel;
use super::sizing::ColumnLayout;
use super::{TableFormatter, TableOutput, TableRoot};

pub(super) struct TableGeometry {
    pub(super) collapsed: bool,
    pub(super) grid: usize,
    pub(super) spacing_x: usize,
    pub(super) spacing_y: usize,
    pub(super) table_edges: EdgeInsets,
    pub(super) table_padding: crate::core::style::EdgeSizes,
    pub(super) fixed_overhead: usize,
}

impl TableGeometry {
    pub(super) fn new(
        formatter: &TableFormatter<'_>,
        table_style: ComputedStyle,
        model: &TableModel,
        available_width: usize,
    ) -> Self {
        let collapsed = table_style.border_collapse == BorderCollapse::Collapse;
        let grid = usize::from(
            collapsed
                && (table_style.border.has_layout()
                    || model
                        .cells
                        .iter()
                        .any(|cell| formatter.cell_style(cell).border.has_layout())
                    || model.rows.iter().any(|row| {
                        row.node
                            .is_some_and(|node| formatter.styles.get(node).border.has_layout())
                            || row
                                .group_node
                                .is_some_and(|node| formatter.styles.get(node).border.has_layout())
                    })
                    || model.column_nodes.iter().any(|track| {
                        track
                            .column
                            .is_some_and(|node| formatter.styles.get(node).border.has_layout())
                            || track
                                .group
                                .is_some_and(|node| formatter.styles.get(node).border.has_layout())
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
            formatter.padding(table_style, available_width)
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
        Self {
            collapsed,
            grid,
            spacing_x,
            spacing_y,
            table_edges,
            table_padding,
            fixed_overhead,
        }
    }
}

pub(super) struct TablePlacement {
    pub(super) root: TableRoot,
    pub(super) model: TableModel,
    pub(super) table_style: ComputedStyle,
    pub(super) columns: ColumnLayout,
    pub(super) cells: CellGrid,
    pub(super) captions: CaptionBands,
    pub(super) collapsed_columns: Vec<bool>,
    pub(super) collapsed_rows: Vec<bool>,
}

pub(super) fn place_table(
    formatter: &TableFormatter<'_>,
    geometry: &TableGeometry,
    placement: TablePlacement,
) -> TableOutput {
    let TablePlacement {
        root,
        model,
        table_style,
        columns,
        cells,
        captions,
        collapsed_columns,
        collapsed_rows,
    } = placement;
    let top_height = captions.top_height();
    let bottom_height = captions.bottom_height();
    let grid_height = geometry.table_edges.top
        + geometry.table_edges.bottom
        + geometry.table_padding.top
        + geometry.table_padding.bottom
        + cells.row_heights.iter().sum::<usize>()
        + if geometry.collapsed {
            geometry.grid.saturating_mul(
                collapsed_rows
                    .iter()
                    .filter(|collapsed| !**collapsed)
                    .count()
                    + 1,
            )
        } else {
            geometry.spacing_y.saturating_mul(
                collapsed_rows
                    .iter()
                    .filter(|collapsed| !**collapsed)
                    .count()
                    + 1,
            )
        };
    let mut output = TableOutput {
        width: columns.table_width,
        height: top_height + grid_height + bottom_height,
        minimum_width: columns.minimum_width,
        model,
        ..Default::default()
    };
    let table_rect = LayoutRect {
        col: 0,
        row: top_height,
        width: columns.table_width,
        height: grid_height,
    };
    if let Some(node) = root.owner
        && !table_style.visibility.is_hidden()
    {
        output.boxes.push(LayoutBox {
            node,
            paint_source: crate::layout::engine::PaintStyleSource::Element(node),
            background_handled: true,
            border_rect: table_rect,
            content_rect: table_rect,
            depth: 0,
            style: table_style.cell_style(),
        });
    }
    add_fill(&mut output.fills, table_rect, table_style, 0, root.owner);
    if !table_style.visibility.is_hidden() && !geometry.collapsed && table_style.border.has_layout()
    {
        output.strokes.push(BorderStroke {
            rect: table_rect,
            edges: table_style.border,
            current_color: table_style.color,
            source_node: None,
            source_edge: None,
            depth: 0,
            merge_group: 1,
        });
    }

    let physical_widths = if table_style.direction == crate::core::style::Direction::Rtl {
        columns.widths.iter().rev().copied().collect::<Vec<_>>()
    } else {
        columns.widths.clone()
    };
    let physical_collapsed_columns = if table_style.direction == crate::core::style::Direction::Rtl
    {
        collapsed_columns.iter().rev().copied().collect::<Vec<_>>()
    } else {
        collapsed_columns.clone()
    };
    let x_positions = track_positions_collapsing(
        &physical_widths,
        &physical_collapsed_columns,
        geometry.table_edges.left
            + geometry.table_padding.left
            + if geometry.collapsed {
                0
            } else {
                geometry.spacing_x
            },
        if geometry.collapsed {
            geometry.grid
        } else {
            geometry.spacing_x
        },
    );
    let y_positions = track_positions_collapsing(
        &cells.row_heights,
        &collapsed_rows,
        top_height
            + geometry.table_edges.top
            + geometry.table_padding.top
            + if geometry.collapsed {
                0
            } else {
                geometry.spacing_y
            },
        if geometry.collapsed {
            geometry.grid
        } else {
            geometry.spacing_y
        },
    );
    if let Some(row) = collapsed_rows.iter().position(|collapsed| !*collapsed) {
        output.baseline = Some(y_positions[row].saturating_add(cells.row_baselines[row]));
    }
    let mut collapse_segments = BTreeMap::new();
    if geometry.collapsed {
        add_collapsed_candidates(
            &mut collapse_segments,
            table_rect,
            table_style.border,
            table_style.color,
            root.owner,
            0,
        );
        for (column, track) in output.model.column_nodes.iter().enumerate() {
            if collapsed_columns[column] {
                continue;
            }
            let column = physical_column(column, output.model.columns, table_style.direction);
            let rect = LayoutRect {
                col: x_positions[column],
                row: table_rect.row,
                width: x_positions[column + 1]
                    .saturating_sub(x_positions[column])
                    .saturating_add(geometry.grid),
                height: table_rect.height,
            };
            if let Some(group) = track.group {
                let style = formatter.styles.get(group);
                add_collapsed_candidates(
                    &mut collapse_segments,
                    rect,
                    style.border,
                    style.color,
                    Some(group),
                    1,
                );
            }
            if let Some(node) = track.column {
                let style = formatter.styles.get(node);
                add_collapsed_candidates(
                    &mut collapse_segments,
                    rect,
                    style.border,
                    style.color,
                    Some(node),
                    2,
                );
            }
        }
        for (row_index, row) in output.model.rows.iter().enumerate() {
            if collapsed_rows[row_index] {
                continue;
            }
            let rect = LayoutRect {
                col: table_rect.col,
                row: y_positions[row_index],
                width: table_rect.width,
                height: y_positions[row_index + 1]
                    .saturating_sub(y_positions[row_index])
                    .saturating_add(geometry.grid),
            };
            if let Some(group) = row.group_node {
                let style = formatter.styles.get(group);
                add_collapsed_candidates(
                    &mut collapse_segments,
                    rect,
                    style.border,
                    style.color,
                    Some(group),
                    3,
                );
            }
            if let Some(node) = row.node {
                let style = formatter.styles.get(node);
                add_collapsed_candidates(
                    &mut collapse_segments,
                    rect,
                    style.border,
                    style.color,
                    Some(node),
                    4,
                );
            }
        }
    }
    let placed_cells = output.model.cells.clone();
    for (index, cell) in placed_cells.iter().enumerate() {
        let style = formatter.cell_style(cell);
        let collapsed_cell = collapsed_columns[cell.col..cell.col + cell.col_span]
            .iter()
            .all(|collapsed| *collapsed)
            || collapsed_rows[cell.row..cell.row + cell.row_span]
                .iter()
                .all(|collapsed| *collapsed);
        let (physical_start, physical_end) = physical_span(
            cell.col,
            cell.col_span,
            output.model.columns,
            table_style.direction,
        );
        let left = x_positions[physical_start];
        let top = y_positions[cell.row];
        let right = x_positions[physical_end].saturating_sub(if geometry.collapsed {
            0
        } else {
            geometry.spacing_x
        });
        let bottom = y_positions[cell.row + cell.row_span].saturating_sub(if geometry.collapsed {
            0
        } else {
            geometry.spacing_y
        });
        let rect = LayoutRect {
            col: left,
            row: top,
            width: right.saturating_sub(left).saturating_add(geometry.grid),
            height: bottom.saturating_sub(top).saturating_add(geometry.grid),
        };
        let fill_rect = collapsed_background_rect(
            rect,
            geometry,
            cell.row > 0,
            cell.col > 0,
            table_style.direction,
        );
        let column = output.model.column_nodes[cell.col];
        let row = &output.model.rows[cell.row];
        if let Some(group) = column.group {
            add_fill(
                &mut output.fills,
                fill_rect,
                formatter.styles.get(group),
                1,
                Some(group),
            );
        }
        if let Some(node) = column.column {
            add_fill(
                &mut output.fills,
                fill_rect,
                formatter.styles.get(node),
                2,
                Some(node),
            );
        }
        if let Some(group) = row.group_node {
            add_fill(
                &mut output.fills,
                fill_rect,
                formatter.styles.get(group),
                3,
                Some(group),
            );
        }
        if let Some(node) = row.node {
            add_fill(
                &mut output.fills,
                fill_rect,
                formatter.styles.get(node),
                4,
                Some(node),
            );
        }
        let edge = if geometry.collapsed {
            EdgeInsets {
                top: geometry.grid,
                right: geometry.grid,
                bottom: geometry.grid,
                left: geometry.grid,
            }
        } else {
            EdgeInsets::from_border(style.border)
        };
        let padding = formatter.padding(style, rect.width);
        let content_col = rect.col + edge.left + padding.left;
        let content_row = rect.row + edge.top + padding.top;
        let content_width = rect
            .width
            .saturating_sub(edge.left + edge.right + padding.left + padding.right);
        let content_height = rect
            .height
            .saturating_sub(edge.top + edge.bottom + padding.top + padding.bottom);
        let mut content_rect = LayoutRect {
            col: content_col,
            row: content_row,
            width: content_width,
            height: content_height,
        };
        let free = content_height.saturating_sub(cells.layouts[index].height());
        let offset = match style.vertical_align {
            crate::core::style::VerticalAlign::Top => 0,
            crate::core::style::VerticalAlign::Middle => free.div_ceil(2),
            crate::core::style::VerticalAlign::Bottom => free,
            crate::core::style::VerticalAlign::Baseline => y_positions[cell.row]
                .saturating_add(cells.row_baselines[cell.row])
                .saturating_sub(content_row.saturating_add(cells.layouts[index].baseline()))
                .min(free),
        };
        content_rect.row = content_rect.row.saturating_add(offset);
        content_rect.height = content_rect.height.saturating_sub(offset);
        let mut hit_rect = rect;
        if geometry.collapsed && cell.col + cell.col_span < output.model.columns {
            hit_rect.width = hit_rect.width.saturating_sub(geometry.grid);
        }
        if geometry.collapsed && cell.row + cell.row_span < output.model.rows.len() {
            hit_rect.height = hit_rect.height.saturating_sub(geometry.grid);
        }
        let hide_empty = !geometry.collapsed
            && style.empty_cells == crate::core::style::EmptyCells::Hide
            && cells.layouts[index].is_empty();
        if let Some(node) = cell.owner
            && !style.visibility.is_hidden()
            && !hide_empty
            && !collapsed_cell
        {
            output.boxes.push(LayoutBox {
                node,
                paint_source: crate::layout::engine::PaintStyleSource::Element(node),
                background_handled: true,
                border_rect: hit_rect,
                content_rect,
                depth: 5,
                style: style.cell_style(),
            });
        }
        if !hide_empty && !collapsed_cell {
            add_fill(&mut output.fills, fill_rect, style, 5, cell.owner);
        }
        if !style.visibility.is_hidden() && !hide_empty && !collapsed_cell {
            if geometry.collapsed {
                add_collapsed_candidates(
                    &mut collapse_segments,
                    rect,
                    style.border,
                    style.color,
                    cell.owner,
                    5,
                );
            } else if style.border.has_layout() {
                output.strokes.push(BorderStroke {
                    rect,
                    edges: style.border,
                    current_color: style.color,
                    source_node: None,
                    source_edge: None,
                    depth: 5,
                    merge_group: index + 2,
                });
            }
        }
        if !style.visibility.is_hidden() && !collapsed_cell {
            append_cell_content(
                &mut output,
                &cells.layouts[index],
                content_rect,
                6,
                (index + 1).saturating_mul(1_000),
            );
        }
    }
    if geometry.collapsed {
        clip_table_layer_fills(&mut output.fills, &collapse_segments);
        output
            .strokes
            .extend(collapse_segments.into_values().filter_map(|candidate| {
                (candidate.side.layout_width() > 0).then_some(BorderStroke {
                    rect: candidate.rect,
                    edges: candidate.edges,
                    current_color: candidate.current_color,
                    source_node: candidate.source_node,
                    source_edge: Some(candidate.source_edge),
                    depth: candidate.depth,
                    merge_group: 1,
                })
            }));
    }
    append_captions(&mut output, &captions.top, 0, columns.table_width);
    append_captions(
        &mut output,
        &captions.bottom,
        top_height + grid_height,
        columns.table_width,
    );
    output
}

fn physical_column(
    column: usize,
    columns: usize,
    direction: crate::core::style::Direction,
) -> usize {
    match direction {
        crate::core::style::Direction::Ltr => column,
        crate::core::style::Direction::Rtl => columns.saturating_sub(column + 1),
    }
}

fn physical_span(
    column: usize,
    span: usize,
    columns: usize,
    direction: crate::core::style::Direction,
) -> (usize, usize) {
    match direction {
        crate::core::style::Direction::Ltr => (column, column.saturating_add(span)),
        crate::core::style::Direction::Rtl => (
            columns.saturating_sub(column.saturating_add(span)),
            columns.saturating_sub(column),
        ),
    }
}

fn collapsed_background_rect(
    mut rect: LayoutRect,
    geometry: &TableGeometry,
    after_block_start: bool,
    after_inline_start: bool,
    direction: crate::core::style::Direction,
) -> LayoutRect {
    if !geometry.collapsed {
        return rect;
    }
    if after_block_start {
        rect.row = rect.row.saturating_add(geometry.grid);
        rect.height = rect.height.saturating_sub(geometry.grid);
    }
    if after_inline_start {
        rect.width = rect.width.saturating_sub(geometry.grid);
        if direction == crate::core::style::Direction::Ltr {
            rect.col = rect.col.saturating_add(geometry.grid);
        }
    }
    rect
}

#[derive(Clone, Copy, Default)]
pub(super) struct EdgeInsets {
    pub(super) top: usize,
    pub(super) right: usize,
    pub(super) bottom: usize,
    pub(super) left: usize,
}

impl EdgeInsets {
    pub(super) fn from_border(border: BorderEdges) -> Self {
        Self {
            top: border.top.layout_width(),
            right: border.right.layout_width(),
            bottom: border.bottom.layout_width(),
            left: border.left.layout_width(),
        }
    }
}

impl TableFormatter<'_> {
    pub(super) fn empty_table(&self, root: TableRoot, model: TableModel) -> TableOutput {
        let style = root.style;
        let rect = LayoutRect {
            col: 0,
            row: 0,
            width: 1,
            height: 1,
        };
        TableOutput {
            width: 1,
            height: 1,
            minimum_width: 1,
            boxes: root
                .owner
                .map(|node| LayoutBox {
                    node,
                    paint_source: crate::layout::engine::PaintStyleSource::Element(node),
                    background_handled: false,
                    border_rect: rect,
                    content_rect: rect,
                    depth: 0,
                    style: style.cell_style(),
                })
                .into_iter()
                .collect(),
            model,
            ..Default::default()
        }
    }
}

fn clip_table_layer_fills(
    fills: &mut Vec<BackgroundFill>,
    segments: &BTreeMap<(bool, usize, usize), super::borders::BorderCandidate>,
) {
    let mut border_cells = BTreeMap::<usize, BTreeSet<usize>>::new();
    for candidate in segments
        .values()
        .filter(|candidate| candidate.side.paints_ink(candidate.current_color))
    {
        for row in candidate.rect.row..candidate.rect.row.saturating_add(candidate.rect.height) {
            let columns = border_cells.entry(row).or_default();
            columns.extend(
                candidate.rect.col..candidate.rect.col.saturating_add(candidate.rect.width),
            );
        }
    }
    if border_cells.is_empty() {
        return;
    }
    let mut clipped = Vec::with_capacity(fills.len());
    for fill in fills.drain(..) {
        if !(1..=5).contains(&fill.depth) {
            clipped.push(fill);
            continue;
        }
        clipped.extend(
            subtract_border_cells(fill.rect, &border_cells)
                .into_iter()
                .map(|rect| BackgroundFill { rect, ..fill }),
        );
    }
    *fills = clipped;
}

fn subtract_border_cells(
    rect: LayoutRect,
    border_cells: &BTreeMap<usize, BTreeSet<usize>>,
) -> Vec<LayoutRect> {
    if rect.width == 0 || rect.height == 0 {
        return vec![rect];
    }
    let bottom = rect.row.saturating_add(rect.height);
    let right = rect.col.saturating_add(rect.width);
    let mut output = Vec::new();
    let mut active = Vec::<LayoutRect>::new();
    let mut next_row = rect.row;
    for (&row, columns) in border_cells.range(rect.row..bottom) {
        if row > next_row {
            output.append(&mut active);
            output.push(LayoutRect {
                col: rect.col,
                row: next_row,
                width: rect.width,
                height: row - next_row,
            });
        }
        let mut runs = Vec::new();
        let mut start = rect.col;
        for column in columns.range(rect.col..right).copied() {
            if start < column {
                runs.push((start, column - start));
            }
            start = column.saturating_add(1);
        }
        if start < right {
            runs.push((start, right - start));
        }
        if active.len() == runs.len()
            && active.iter().zip(&runs).all(|(active, (col, width))| {
                active.col == *col
                    && active.width == *width
                    && active.row.saturating_add(active.height) == row
            })
        {
            for active in &mut active {
                active.height = active.height.saturating_add(1);
            }
        } else {
            output.append(&mut active);
            active = runs
                .into_iter()
                .map(|(col, width)| LayoutRect {
                    col,
                    row,
                    width,
                    height: 1,
                })
                .collect();
        }
        next_row = row.saturating_add(1);
    }
    output.append(&mut active);
    if next_row < bottom {
        output.push(LayoutRect {
            col: rect.col,
            row: next_row,
            width: rect.width,
            height: bottom - next_row,
        });
    }
    output
}

fn track_positions_collapsing(
    values: &[usize],
    collapsed: &[bool],
    start: usize,
    gap: usize,
) -> Vec<usize> {
    let mut positions = Vec::with_capacity(values.len() + 1);
    positions.push(start);
    for (value, collapsed) in values.iter().zip(collapsed) {
        positions.push(
            positions.last().copied().unwrap_or(start)
                + if *collapsed {
                    0
                } else {
                    value.saturating_add(gap)
                },
        );
    }
    positions
}

pub(super) fn offset_table_rect(rect: &mut LayoutRect, col: usize, row: usize) {
    rect.col = rect.col.saturating_add(col);
    rect.row = rect.row.saturating_add(row);
}

pub(super) fn intersect_rect(left: LayoutRect, right: LayoutRect) -> Option<LayoutRect> {
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
    (far_col > col && far_row > row).then(|| LayoutRect {
        col,
        row,
        width: far_col - col,
        height: far_row - row,
    })
}

pub(super) fn add_fill(
    fills: &mut Vec<BackgroundFill>,
    rect: LayoutRect,
    style: ComputedStyle,
    depth: usize,
    source: Option<crate::core::dom::NodeId>,
) {
    if !style.visibility.is_hidden()
        && let Some(source) = source
    {
        fills.push(BackgroundFill {
            rect,
            color: style.background,
            depth,
            paint_source: crate::layout::engine::PaintStyleSource::Element(source),
        });
    }
}

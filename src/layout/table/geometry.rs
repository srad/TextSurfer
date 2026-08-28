use std::collections::BTreeMap;

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
    } = placement;
    let top_height = captions.top_height();
    let bottom_height = captions.bottom_height();
    let grid_height = geometry.table_edges.top
        + geometry.table_edges.bottom
        + geometry.table_padding.top
        + geometry.table_padding.bottom
        + cells.row_heights.iter().sum::<usize>()
        + if geometry.collapsed {
            geometry.grid.saturating_mul(model.rows.len() + 1)
        } else {
            geometry.spacing_y.saturating_mul(model.rows.len() + 1)
        };
    let mut output = TableOutput {
        width: columns.table_width,
        height: top_height + grid_height + bottom_height,
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
            border_rect: table_rect,
            content_rect: table_rect,
            depth: 0,
            style: table_style.cell_style(),
        });
    }
    add_fill(&mut output.fills, table_rect, table_style, 0);
    if !table_style.visibility.is_hidden() && !geometry.collapsed && table_style.border.has_layout()
    {
        output.strokes.push(BorderStroke {
            rect: table_rect,
            edges: table_style.border,
            style: table_style.cell_style(),
            depth: 0,
            merge_group: 1,
        });
    }

    let x_positions = track_positions(
        &columns.widths,
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
    let y_positions = track_positions(
        &cells.row_heights,
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
    if !output.model.rows.is_empty() {
        output.baseline = Some(y_positions[0].saturating_add(cells.row_baselines[0]));
    }
    for (column, track) in output.model.column_nodes.iter().enumerate() {
        let rect = LayoutRect {
            col: x_positions[column],
            row: table_rect.row,
            width: columns.widths[column],
            height: table_rect.height,
        };
        if let Some(group) = track.group {
            add_fill(&mut output.fills, rect, formatter.styles.get(group), 1);
        }
        if let Some(node) = track.column {
            add_fill(&mut output.fills, rect, formatter.styles.get(node), 2);
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
                .saturating_sub(if geometry.collapsed {
                    0
                } else {
                    geometry.spacing_y
                }),
        };
        add_fill(&mut output.fills, rect, formatter.styles.get(group), 3);
    }
    for (row_index, row) in output.model.rows.iter().enumerate() {
        if let Some(node) = row.node {
            let rect = LayoutRect {
                col: x_positions[0],
                row: y_positions[row_index],
                width: columns.table_width.saturating_sub(x_positions[0]),
                height: cells.row_heights[row_index],
            };
            add_fill(&mut output.fills, rect, formatter.styles.get(node), 4);
        }
    }

    let mut collapse_segments = BTreeMap::new();
    if geometry.collapsed {
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
                    .saturating_add(geometry.grid),
                height: table_rect.height,
            };
            if let Some(group) = track.group {
                let style = formatter.styles.get(group);
                add_collapsed_candidates(
                    &mut collapse_segments,
                    rect,
                    style.border,
                    style.cell_style(),
                    1,
                );
            }
            if let Some(node) = track.column {
                let style = formatter.styles.get(node);
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
                    .saturating_add(geometry.grid),
            };
            if let Some(group) = row.group_node {
                let style = formatter.styles.get(group);
                add_collapsed_candidates(
                    &mut collapse_segments,
                    rect,
                    style.border,
                    style.cell_style(),
                    3,
                );
            }
            if let Some(node) = row.node {
                let style = formatter.styles.get(node);
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
        let style = formatter.cell_style(cell);
        let left = x_positions[cell.col];
        let top = y_positions[cell.row];
        let right = x_positions[cell.col + cell.col_span].saturating_sub(if geometry.collapsed {
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
        if let Some(node) = cell.owner
            && !style.visibility.is_hidden()
        {
            output.boxes.push(LayoutBox {
                node,
                paint_source: crate::layout::engine::PaintStyleSource::Element(node),
                border_rect: hit_rect,
                content_rect,
                depth: 5,
                style: style.cell_style(),
            });
        }
        add_fill(&mut output.fills, rect, style, 5);
        if !style.visibility.is_hidden() {
            if geometry.collapsed {
                add_collapsed_candidates(
                    &mut collapse_segments,
                    rect,
                    style.border,
                    style.cell_style(),
                    5,
                );
            } else if style.border.has_layout() {
                output.strokes.push(BorderStroke {
                    rect,
                    edges: style.border,
                    style: style.cell_style(),
                    depth: 5,
                    merge_group: index + 2,
                });
            }
        }
        append_cell_content(
            &mut output,
            &cells.layouts[index],
            content_rect,
            6,
            (index + 1).saturating_mul(1_000),
        );
    }
    if geometry.collapsed {
        output
            .strokes
            .extend(collapse_segments.into_values().filter_map(|candidate| {
                (candidate.side.layout_width() > 0).then_some(BorderStroke {
                    rect: candidate.rect,
                    edges: candidate.edges,
                    style: candidate.style,
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
            boxes: root
                .owner
                .map(|node| LayoutBox {
                    node,
                    paint_source: crate::layout::engine::PaintStyleSource::Element(node),
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

pub(super) fn track_positions(values: &[usize], start: usize, gap: usize) -> Vec<usize> {
    let mut positions = Vec::with_capacity(values.len() + 1);
    positions.push(start);
    for value in values {
        positions.push(positions.last().copied().unwrap_or(start) + value + gap);
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
) {
    if !style.visibility.is_hidden()
        && let Some(color) = style.background
    {
        fills.push(BackgroundFill {
            rect,
            color: Some(color),
            depth,
        });
    }
}

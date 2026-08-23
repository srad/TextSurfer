use crate::core::style::{ComputedStyle, CssWidth, TableLayoutMode};

use super::TableFormatter;
use super::content::CellMetrics;
use super::geometry::TableGeometry;
use super::model::TableModel;

pub(super) struct ColumnLayout {
    pub(super) widths: Vec<usize>,
    pub(super) table_width: usize,
}

pub(super) fn size_columns(
    formatter: &TableFormatter<'_>,
    model: &TableModel,
    metrics: &[CellMetrics],
    table_style: ComputedStyle,
    available_width: usize,
    geometry: &TableGeometry,
    caption_natural: usize,
) -> ColumnLayout {
    let mut minimum = vec![1usize; model.columns];
    let mut maximum = vec![1usize; model.columns];
    let specified = resolved_width(table_style.width, available_width);
    let percentage_basis = specified.unwrap_or(available_width);
    for (cell, metric) in model.cells.iter().zip(metrics) {
        if cell.col_span == 1 {
            minimum[cell.col] = minimum[cell.col].max(metric.minimum);
            maximum[cell.col] = maximum[cell.col].max(metric.maximum);
        }
    }
    for (cell, metric) in model.cells.iter().zip(metrics) {
        if cell.col_span > 1 {
            grow_span(&mut minimum, cell.col, cell.col_span, metric.minimum);
            grow_span(&mut maximum, cell.col, cell.col_span, metric.maximum);
        }
        if let Some(width) = cell_width_hint(formatter.cell_style(cell), percentage_basis) {
            grow_span(&mut minimum, cell.col, cell.col_span, width);
            grow_span(&mut maximum, cell.col, cell.col_span, width);
        }
    }
    for (col, track) in model.column_nodes.iter().enumerate() {
        if let Some(group) = track.group {
            apply_width_hint(
                formatter.styles.get(group).width,
                available_width,
                &mut minimum[col],
            );
            maximum[col] = maximum[col].max(minimum[col]);
        }
        if let Some(node) = track.column {
            apply_width_hint(
                formatter.styles.get(node).width,
                available_width,
                &mut minimum[col],
            );
            maximum[col] = maximum[col].max(minimum[col]);
        }
    }
    let mut widths = if table_style.table_layout == TableLayoutMode::Fixed && specified.is_some() {
        let mut fixed = vec![0usize; model.columns];
        for (col, track) in model.column_nodes.iter().enumerate() {
            if let Some(group) = track.group {
                apply_width_hint(
                    formatter.styles.get(group).width,
                    available_width,
                    &mut fixed[col],
                );
            }
            if let Some(node) = track.column {
                apply_width_hint(
                    formatter.styles.get(node).width,
                    available_width,
                    &mut fixed[col],
                );
            }
        }
        for cell in model.cells.iter().filter(|cell| cell.row == 0) {
            if let Some(width) = cell_width_hint(formatter.cell_style(cell), percentage_basis) {
                let each = width.div_ceil(cell.col_span);
                for value in &mut fixed[cell.col..cell.col + cell.col_span] {
                    *value = (*value).max(each);
                }
            }
        }
        let target = specified
            .unwrap_or_default()
            .saturating_sub(geometry.fixed_overhead);
        distribute_remainder(&mut fixed, target);
        fixed
    } else {
        let natural = maximum.iter().sum::<usize>();
        let target = specified
            .unwrap_or_else(|| {
                natural
                    .saturating_add(geometry.fixed_overhead)
                    .min(available_width)
            })
            .saturating_sub(geometry.fixed_overhead)
            .max(minimum.iter().sum());
        let mut auto = minimum;
        grow_towards(&mut auto, &maximum, target);
        auto
    };
    for value in &mut widths {
        *value = (*value).max(1);
    }
    let mut table_width = widths
        .iter()
        .sum::<usize>()
        .saturating_add(geometry.fixed_overhead);
    if caption_natural > table_width {
        distribute_remainder(
            &mut widths,
            caption_natural.saturating_sub(geometry.fixed_overhead),
        );
        table_width = widths
            .iter()
            .sum::<usize>()
            .saturating_add(geometry.fixed_overhead);
    }
    ColumnLayout {
        widths,
        table_width: table_width.max(1),
    }
}

pub(super) fn grow_span(values: &mut [usize], start: usize, span: usize, required: usize) {
    let current = values[start..start + span].iter().sum::<usize>();
    distribute_remainder(&mut values[start..start + span], required.max(current));
}

pub(super) fn distribute_remainder(values: &mut [usize], target: usize) {
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

pub(super) fn grow_towards(values: &mut [usize], maximum: &[usize], target: usize) {
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

pub(super) fn apply_width_hint(width: CssWidth, basis: usize, target: &mut usize) {
    if let Some(value) = resolved_width(width, basis) {
        *target = (*target).max(value);
    }
}

pub(super) fn resolved_width(width: CssWidth, basis: usize) -> Option<usize> {
    match width {
        CssWidth::Auto => None,
        CssWidth::Cells(value) => Some(value),
        CssWidth::Percent(value) => Some(value.resolve(basis)),
    }
}

pub(super) fn cell_width_hint(style: ComputedStyle, basis: usize) -> Option<usize> {
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

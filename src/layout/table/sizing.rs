use crate::core::style::{ComputedStyle, CssPercentage, CssWidth, TableLayoutMode};

use super::TableFormatter;
use super::captions::CaptionWidths;
use super::content::CellMetrics;
use super::geometry::TableGeometry;
use super::model::TableModel;

pub(super) struct ColumnLayout {
    pub(super) widths: Vec<usize>,
    pub(super) table_width: usize,
    pub(super) minimum_width: usize,
}

pub(super) fn size_columns(
    formatter: &TableFormatter<'_>,
    model: &TableModel,
    metrics: &[CellMetrics],
    table_style: ComputedStyle,
    available_width: usize,
    geometry: &TableGeometry,
    caption_widths: CaptionWidths,
) -> ColumnLayout {
    let mut minimum = vec![1usize; model.columns];
    let mut maximum = vec![1usize; model.columns];
    let mut percentages = vec![0u32; model.columns];
    let specified = resolved_width(table_style.width, available_width, formatter.styles);
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
    }
    let mut intrinsic_minimum = minimum.clone();
    for cell in &model.cells {
        if let Some(width) = non_percentage_cell_width_hint(
            formatter.cell_style(cell),
            percentage_basis,
            formatter.styles,
        ) {
            grow_span(&mut minimum, cell.col, cell.col_span, width);
            grow_span(&mut maximum, cell.col, cell.col_span, width);
        }
        if let Some(width) = intrinsic_cell_width_hint(formatter.cell_style(cell), formatter.styles)
        {
            grow_span(&mut intrinsic_minimum, cell.col, cell.col_span, width);
        }
    }
    collect_intrinsic_percentages(formatter, model, &maximum, &mut percentages);
    for (col, track) in model.column_nodes.iter().enumerate() {
        if let Some(group) = track.group {
            apply_intrinsic_width_hint(
                formatter.styles.get(group).width,
                &mut intrinsic_minimum[col],
                formatter.styles,
            );
            apply_non_percentage_width_hint(
                formatter.styles.get(group).width,
                available_width,
                &mut minimum[col],
                formatter.styles,
            );
            apply_percentage_hint(formatter.styles.get(group).width, &mut percentages[col]);
            maximum[col] = maximum[col].max(minimum[col]);
        }
        if let Some(node) = track.column {
            apply_intrinsic_width_hint(
                formatter.styles.get(node).width,
                &mut intrinsic_minimum[col],
                formatter.styles,
            );
            apply_non_percentage_width_hint(
                formatter.styles.get(node).width,
                available_width,
                &mut minimum[col],
                formatter.styles,
            );
            apply_percentage_hint(formatter.styles.get(node).width, &mut percentages[col]);
            maximum[col] = maximum[col].max(minimum[col]);
        }
    }
    clamp_percentages(&mut percentages);
    let intrinsic_minimum = intrinsic_minimum
        .iter()
        .sum::<usize>()
        .saturating_add(geometry.fixed_overhead)
        .max(caption_widths.minimum);
    let mut widths = if table_style.table_layout == TableLayoutMode::Fixed && specified.is_some() {
        let mut fixed = vec![0usize; model.columns];
        for (col, track) in model.column_nodes.iter().enumerate() {
            if let Some(group) = track.group {
                apply_width_hint(
                    formatter.styles.get(group).width,
                    available_width,
                    &mut fixed[col],
                    formatter.styles,
                );
            }
            if let Some(node) = track.column {
                apply_width_hint(
                    formatter.styles.get(node).width,
                    available_width,
                    &mut fixed[col],
                    formatter.styles,
                );
            }
        }
        for cell in model.cells.iter().filter(|cell| cell.row == 0) {
            if let Some(width) = cell_width_hint(
                formatter.cell_style(cell),
                percentage_basis,
                formatter.styles,
            ) {
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
            .saturating_sub(geometry.fixed_overhead);
        let mut auto = minimum;
        for (width, percentage) in auto.iter_mut().zip(&percentages) {
            *width = (*width).max(CssPercentage::new(*percentage).resolve(target));
        }
        if auto.iter().sum::<usize>() > target && specified.is_some() {
            shrink_to(&mut auto, target);
        } else if auto.iter().sum::<usize>() <= target {
            for (preferred, width) in maximum.iter_mut().zip(&auto) {
                *preferred = (*preferred).max(*width);
            }
            grow_towards(&mut auto, &maximum, target);
        }
        auto
    };
    for value in &mut widths {
        *value = (*value).max(1);
    }
    let mut table_width = widths
        .iter()
        .sum::<usize>()
        .saturating_add(geometry.fixed_overhead);
    let caption_target = caption_widths
        .maximum
        .min(specified.unwrap_or(available_width))
        .max(caption_widths.minimum);
    if caption_target > table_width {
        distribute_remainder(
            &mut widths,
            caption_target.saturating_sub(geometry.fixed_overhead),
        );
        table_width = widths
            .iter()
            .sum::<usize>()
            .saturating_add(geometry.fixed_overhead);
    }
    let minimum_width = if intrinsic_resolved_width(table_style.width, formatter.styles).is_some() {
        table_width
    } else {
        intrinsic_minimum.min(table_width)
    };
    ColumnLayout {
        widths,
        table_width: table_width.max(1),
        minimum_width: minimum_width.max(1),
    }
}

fn shrink_to(values: &mut [usize], target: usize) {
    if values.is_empty() {
        return;
    }
    let target = target.max(values.len());
    let total = values.iter().sum::<usize>().max(1);
    for value in values.iter_mut() {
        *value = value
            .saturating_mul(target)
            .checked_div(total)
            .unwrap_or(0)
            .max(1);
    }
    while values.iter().sum::<usize>() > target {
        if let Some(value) = values.iter_mut().filter(|value| **value > 1).max() {
            *value -= 1;
        } else {
            break;
        }
    }
    distribute_remainder(values, target);
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

pub(super) fn apply_width_hint(
    width: CssWidth,
    basis: usize,
    target: &mut usize,
    styles: &crate::core::style::StyleTree,
) {
    if let Some(value) = resolved_width(width, basis, styles) {
        *target = (*target).max(value);
    }
}

fn apply_non_percentage_width_hint(
    width: CssWidth,
    basis: usize,
    target: &mut usize,
    styles: &crate::core::style::StyleTree,
) {
    if !matches!(width, CssWidth::Percent(_)) {
        apply_width_hint(width, basis, target, styles);
    }
}

fn apply_percentage_hint(width: CssWidth, target: &mut u32) {
    if let CssWidth::Percent(value) = width {
        *target = (*target).max(value.basis_points());
    }
}

fn collect_intrinsic_percentages(
    formatter: &TableFormatter<'_>,
    model: &TableModel,
    maximum: &[usize],
    percentages: &mut [u32],
) {
    for cell in model.cells.iter().filter(|cell| cell.col_span == 1) {
        apply_percentage_hint(formatter.cell_style(cell).width, &mut percentages[cell.col]);
    }
    for span in 2..=model.columns {
        for cell in model.cells.iter().filter(|cell| cell.col_span == span) {
            let CssWidth::Percent(value) = formatter.cell_style(cell).width else {
                continue;
            };
            let tracks = &mut percentages[cell.col..cell.col + span];
            let remaining = value
                .basis_points()
                .saturating_sub(tracks.iter().sum::<u32>());
            let eligible = tracks
                .iter()
                .enumerate()
                .filter_map(|(index, value)| (*value == 0).then_some(index))
                .collect::<Vec<_>>();
            if eligible.is_empty() || remaining == 0 {
                continue;
            }
            let total_weight = eligible
                .iter()
                .map(|index| maximum[cell.col + *index])
                .sum::<usize>();
            let mut left = remaining;
            for (position, index) in eligible.iter().enumerate() {
                let contribution = if position + 1 == eligible.len() {
                    left
                } else if total_weight == 0 {
                    remaining / eligible.len() as u32
                } else {
                    remaining.saturating_mul(maximum[cell.col + *index] as u32)
                        / total_weight as u32
                };
                tracks[*index] = contribution;
                left = left.saturating_sub(contribution);
            }
        }
    }
}

fn clamp_percentages(percentages: &mut [u32]) {
    let mut remaining = 10_000u32;
    for percentage in percentages {
        *percentage = (*percentage).min(remaining);
        remaining -= *percentage;
    }
}

fn apply_intrinsic_width_hint(
    width: CssWidth,
    target: &mut usize,
    styles: &crate::core::style::StyleTree,
) {
    if let Some(value) = intrinsic_resolved_width(width, styles) {
        *target = (*target).max(value);
    }
}

fn intrinsic_resolved_width(
    width: CssWidth,
    styles: &crate::core::style::StyleTree,
) -> Option<usize> {
    match width {
        CssWidth::Auto | CssWidth::Percent(_) => None,
        CssWidth::Cells(value) => Some(value),
        CssWidth::Calc(value) if styles.calc_depends_on_basis(value)? => None,
        CssWidth::Calc(value) => Some(styles.resolve_calc(value, 0.0)?.max(0.0).round() as usize),
    }
}

pub(super) fn resolved_width(
    width: CssWidth,
    basis: usize,
    styles: &crate::core::style::StyleTree,
) -> Option<usize> {
    match width {
        CssWidth::Auto => None,
        CssWidth::Cells(value) => Some(value),
        CssWidth::Percent(value) => Some(value.resolve(basis)),
        CssWidth::Calc(value) => {
            Some(styles.resolve_calc(value, basis as f32)?.max(0.0).round() as usize)
        }
    }
}

pub(super) fn cell_width_hint(
    style: ComputedStyle,
    basis: usize,
    styles: &crate::core::style::StyleTree,
) -> Option<usize> {
    resolved_width(style.width, basis, styles).map(|width| {
        if style.box_sizing == crate::core::style::BoxSizing::ContentBox {
            let padding = styles.resolve_padding_edges(style.padding, basis);
            width
                + padding.left
                + padding.right
                + style.border.left.layout_width()
                + style.border.right.layout_width()
        } else {
            width
        }
    })
}

fn non_percentage_cell_width_hint(
    style: ComputedStyle,
    basis: usize,
    styles: &crate::core::style::StyleTree,
) -> Option<usize> {
    (!matches!(style.width, CssWidth::Percent(_)))
        .then(|| cell_width_hint(style, basis, styles))
        .flatten()
}

fn intrinsic_cell_width_hint(
    style: ComputedStyle,
    styles: &crate::core::style::StyleTree,
) -> Option<usize> {
    intrinsic_resolved_width(style.width, styles).map(|width| {
        if style.box_sizing == crate::core::style::BoxSizing::ContentBox {
            let padding = styles.resolve_padding_edges(style.padding, 0);
            width
                + padding.left
                + padding.right
                + style.border.left.layout_width()
                + style.border.right.layout_width()
        } else {
            width
        }
    })
}

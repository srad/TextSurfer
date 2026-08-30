use std::collections::{BTreeMap, HashSet};
use std::ops::Range;

use crate::core::style::Palette;
use crate::layout::BoxTree;

use super::row::RowBuffer;
use super::strokes::draw_strokes_row;
use super::{BasicPainter, DisplayList, PaintedRow, ScaledTextRun, resolve_cell_style};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayPatch {
    pub rows: Vec<(usize, PaintedRow)>,
    pub scaled_text: Vec<(usize, ScaledTextRun)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RetainedPaint {
    pub(crate) patch: DisplayPatch,
    pub(crate) contributors_visited: usize,
    pub(crate) rows_rebuilt: usize,
    pub(crate) scaled_runs_rebuilt: usize,
}

impl DisplayPatch {
    pub fn changed_rows(&self) -> Vec<Range<usize>> {
        let mut dirty = self
            .rows
            .iter()
            .map(|(row, _)| *row..row.saturating_add(1))
            .chain(
                self.scaled_text
                    .iter()
                    .map(|(_, run)| run.rect.row_range(usize::MAX)),
            )
            .collect::<Vec<_>>();
        dirty.sort_unstable_by_key(|range| range.start);
        let mut merged = Vec::new();
        for range in dirty {
            add_range(&mut merged, range);
        }
        merged
    }

    pub fn fits(&self, display: &DisplayList) -> bool {
        !self
            .rows
            .iter()
            .any(|(index, _)| *index >= display.rows.len())
            && !self
                .scaled_text
                .iter()
                .any(|(index, _)| *index >= display.scaled_text.len())
    }

    pub fn apply(self, display: &mut DisplayList) -> bool {
        if !self.fits(display) {
            return false;
        }
        for (index, row) in self.rows {
            display.rows[index] = row;
        }
        for (index, run) in self.scaled_text {
            display.scaled_text[index] = run;
        }
        true
    }
}

impl BasicPainter {
    pub(crate) fn paint_retained(
        &self,
        tree: &BoxTree,
        palette: Palette,
        baseline: &DisplayList,
        dirty: &[Range<usize>],
    ) -> Option<RetainedPaint> {
        if baseline.rows.len() != tree.height {
            return None;
        }
        let mut buffers = BTreeMap::new();
        let mut scaled_fragments = HashSet::new();
        let mut contributors_visited = 0;
        for range in dirty {
            for row in range.start.min(tree.height)..range.end.min(tree.height) {
                let mut buffer = RowBuffer::new(tree.width);
                let mut indices = Vec::new();
                tree.paint_fills_at(row, &mut indices);
                contributors_visited += indices.len();
                indices.sort_by_key(|index| {
                    (
                        tree.paint_fill_is_float(*index),
                        tree.fills[*index].depth,
                        *index,
                    )
                });
                for index in indices.drain(..) {
                    let fill = tree.fills[index];
                    let Some(color) = fill.color else {
                        continue;
                    };
                    buffer.fill_background(
                        fill.rect.col,
                        fill.rect.col.saturating_add(fill.rect.width),
                        color,
                    );
                }
                tree.paint_strokes_at(row, &mut indices);
                contributors_visited += indices.len();
                indices.sort_by_key(|index| {
                    (
                        tree.paint_stroke_is_float(*index),
                        tree.strokes[*index].depth,
                        *index,
                    )
                });
                let strokes = indices
                    .drain(..)
                    .map(|index| tree.strokes[index])
                    .collect::<Vec<_>>();
                draw_strokes_row(&mut buffer, tree.width, tree.height, &strokes, row);
                tree.paint_fragments_at(row, &mut indices);
                contributors_visited += indices.len();
                indices.sort_by_key(|index| {
                    let fragment = &tree.fragments[*index];
                    (
                        tree.paint_fragment_is_float(*index),
                        fragment.depth,
                        fragment.row,
                        fragment.col,
                        *index,
                    )
                });
                for index in indices.drain(..) {
                    let fragment = &tree.fragments[index];
                    if fragment.row >= tree.height || fragment.col >= tree.width {
                        continue;
                    }
                    if fragment.style.scale > 1 {
                        scaled_fragments.insert(index);
                        let rect = fragment.rect();
                        let mut style = fragment.style;
                        style.underline = false;
                        style.strike = false;
                        buffer.fill_style(
                            rect.col,
                            rect.width.min(tree.width.saturating_sub(rect.col)),
                            style,
                        );
                    } else if fragment.style.scale == 1 && fragment.row == row {
                        buffer.write(fragment.col, &fragment.text, fragment.style);
                    }
                }
                buffers.insert(row, buffer);
            }
        }
        let scaled_runs_rebuilt = scaled_fragments.len();
        let mut scaled_text = Vec::new();
        for fragment_index in scaled_fragments {
            let fragment = &tree.fragments[fragment_index];
            let run_index = tree.scaled_run_for_fragment(fragment_index)?;
            let mut run = ScaledTextRun {
                node: fragment.node,
                rect: fragment.rect(),
                text: fragment.text.clone(),
                style: fragment.style,
                depth: fragment.depth,
                ink: !fragment
                    .style
                    .fg
                    .is_some_and(|foreground| foreground.alpha == 0),
            };
            let underline = run.style.underline;
            let strike = run.style.strike;
            let style = buffers
                .get(&run.rect.row)
                .and_then(|row| row.style_at(run.rect.col))?;
            run.style = resolve_cell_style(style, palette);
            run.style.underline = underline;
            run.style.strike = strike;
            if baseline.scaled_text.get(run_index) != Some(&run) {
                scaled_text.push((run_index, run));
            }
        }
        scaled_text.sort_unstable_by_key(|(index, _)| *index);
        let rows_rebuilt = buffers.len();
        let rows = buffers
            .into_iter()
            .filter_map(|(index, buffer)| {
                let row = buffer.into_row(palette);
                (baseline.rows.get(index) != Some(&row)).then_some((index, row))
            })
            .collect();
        Some(RetainedPaint {
            patch: DisplayPatch { rows, scaled_text },
            contributors_visited,
            rows_rebuilt,
            scaled_runs_rebuilt,
        })
    }
}

fn add_range(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if range.is_empty() {
        return;
    }
    if let Some(last) = ranges.last_mut()
        && range.start <= last.end
    {
        last.end = last.end.max(range.end);
    } else {
        ranges.push(range);
    }
}

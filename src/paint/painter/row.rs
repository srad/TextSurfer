use std::collections::BTreeMap;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::core::style::{CellStyle, Palette, Rgb};
use crate::layout::LayoutRect;

use super::contrast::{merge_style, resolve_cell_style};
use super::{PaintedRow, PaintedSpan};

pub(super) struct RowBuffer {
    cells: Vec<Option<String>>,
    owners: Vec<Option<usize>>,
    styles: Vec<CellStyle>,
}

impl RowBuffer {
    pub(super) fn new(width: usize) -> Self {
        Self {
            cells: vec![None; width],
            owners: vec![None; width],
            styles: vec![CellStyle::default(); width],
        }
    }

    pub(super) fn write(&mut self, start: usize, text: &str, style: CellStyle) {
        write_line(
            &mut self.cells,
            &mut self.owners,
            &mut self.styles,
            start,
            text,
            style,
        );
    }

    pub(super) fn style_at(&self, col: usize) -> Option<CellStyle> {
        self.styles.get(col).copied()
    }

    pub(super) fn fill_style(&mut self, from: usize, width: usize, style: CellStyle) {
        let end = from.saturating_add(width).min(self.styles.len());
        for index in from..end {
            clear_grapheme(&mut self.cells, &mut self.owners, index);
            self.cells[index] = Some(" ".to_string());
            self.owners[index] = Some(index);
            self.styles[index] = merge_style(self.styles[index], style);
        }
    }

    fn fill_background(&mut self, from: usize, to: usize, background: Rgb) {
        for index in from..to.min(self.styles.len()) {
            if self.owners[index].is_none() {
                self.cells[index] = Some(" ".to_string());
                self.owners[index] = Some(index);
            }
            self.styles[index].bg = Some(background);
        }
    }

    pub(super) fn into_row(self, palette: Palette) -> PaintedRow {
        let mut spans: Vec<PaintedSpan> = Vec::new();
        let mut last_painted = 0usize;
        for (index, cell) in self.cells.iter().enumerate() {
            let Some(cell) = cell else { continue };
            let mut text = cell.clone();
            let source_style = self.styles[index];
            let style = resolve_cell_style(source_style, palette);
            if source_style
                .fg
                .is_some_and(|foreground| foreground.alpha == 0)
            {
                text = " ".repeat(UnicodeWidthStr::width(text.as_str()));
            }
            let blank = text.chars().all(char::is_whitespace) && style == CellStyle::default();
            match spans.last_mut() {
                Some(span)
                    if span.style == style
                        && span.col + UnicodeWidthStr::width(span.text.as_str()) == index =>
                {
                    span.text.push_str(&text);
                }
                _ => spans.push(PaintedSpan {
                    col: index,
                    text,
                    style,
                }),
            }
            if !blank {
                last_painted = spans.len();
            }
        }
        spans.truncate(last_painted);
        if let Some(span) = spans.last_mut()
            && span.style == CellStyle::default()
        {
            let trimmed = span.text.trim_end();
            if trimmed.len() != span.text.len() {
                span.text.truncate(trimmed.len());
            }
        }
        spans.retain(|span| !span.text.is_empty());
        PaintedRow { spans }
    }
}

pub(super) fn fill_background(
    rows: &mut BTreeMap<usize, RowBuffer>,
    viewport_width: usize,
    document_height: usize,
    rect: LayoutRect,
    background: Rgb,
) {
    if rect.width == 0 || rect.height == 0 || rect.col >= viewport_width {
        return;
    }
    let last_row = rect
        .row
        .saturating_add(rect.height)
        .min(document_height)
        .min(rect.row.saturating_add(rect.height));
    for row in rect.row..last_row {
        let buffer = rows
            .entry(row)
            .or_insert_with(|| RowBuffer::new(viewport_width));
        buffer.fill_background(rect.col, rect.col.saturating_add(rect.width), background);
    }
}

fn write_line(
    cells: &mut [Option<String>],
    owners: &mut [Option<usize>],
    styles: &mut [CellStyle],
    start: usize,
    text: &str,
    style: CellStyle,
) {
    let mut col = start;
    for grapheme in text.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if width == 0 {
            if let Some(previous) = col
                .checked_sub(1)
                .and_then(|index| owners.get(index))
                .copied()
                .flatten()
                .and_then(|owner| cells.get_mut(owner))
                .and_then(Option::as_mut)
            {
                previous.push_str(grapheme);
            }
            continue;
        }
        if col.saturating_add(width) > cells.len() {
            break;
        }
        for target in col..col + width {
            clear_grapheme(cells, owners, target);
        }
        cells[col] = Some(grapheme.to_string());
        owners[col] = Some(col);
        styles[col] = merge_style(styles[col], style);
        for offset in 1..width {
            cells[col + offset] = None;
            owners[col + offset] = Some(col);
            styles[col + offset] = styles[col];
        }
        col += width;
    }
}

fn clear_grapheme(cells: &mut [Option<String>], owners: &mut [Option<usize>], target: usize) {
    let Some(owner) = owners.get(target).copied().flatten() else {
        return;
    };
    let width = cells[owner]
        .as_deref()
        .map(UnicodeWidthStr::width)
        .unwrap_or(1)
        .max(1);
    for index in owner..(owner + width).min(owners.len()) {
        owners[index] = None;
        cells[index] = None;
    }
}

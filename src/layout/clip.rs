use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::{BorderStroke, LayoutRect, TextFragment};

#[derive(Clone, Copy, Debug)]
pub(in crate::layout) struct ClipRegion {
    x: Option<(usize, usize)>,
    y: Option<(usize, usize)>,
}

impl ClipRegion {
    pub(in crate::layout) const fn viewport(width: usize) -> Self {
        Self {
            x: Some((0, width)),
            y: None,
        }
    }

    pub(in crate::layout) fn intersect_axes(
        self,
        rect: LayoutRect,
        clip_x: bool,
        clip_y: bool,
    ) -> Self {
        Self {
            x: if clip_x {
                intersect_axis(self.x, rect.col, rect.col.saturating_add(rect.width))
            } else {
                self.x
            },
            y: if clip_y {
                intersect_axis(self.y, rect.row, rect.row.saturating_add(rect.height))
            } else {
                self.y
            },
        }
    }

    pub(in crate::layout) fn is_empty(self) -> bool {
        self.x.is_some_and(|(start, end)| start >= end)
            || self.y.is_some_and(|(start, end)| start >= end)
    }

    pub(in crate::layout) fn rect(self, rect: LayoutRect) -> Option<LayoutRect> {
        if self.is_empty() {
            return None;
        }
        let (col, right) = bounded_axis(self.x, rect.col, rect.col.saturating_add(rect.width))?;
        let (row, bottom) = bounded_axis(self.y, rect.row, rect.row.saturating_add(rect.height))?;
        Some(LayoutRect {
            col,
            row,
            width: right.saturating_sub(col),
            height: bottom.saturating_sub(row),
        })
    }

    pub(in crate::layout) fn box_rect(self, rect: LayoutRect) -> Option<LayoutRect> {
        if rect.width > 0 && rect.height > 0 {
            return self.rect(rect);
        }
        let inside_x = self
            .x
            .is_none_or(|(left, right)| rect.col >= left && rect.col <= right);
        let inside_y = self
            .y
            .is_none_or(|(top, bottom)| rect.row >= top && rect.row <= bottom);
        (inside_x && inside_y).then_some(rect)
    }

    pub(in crate::layout) fn vertical_end(self, rect: LayoutRect) -> Option<usize> {
        let (_, bottom) = bounded_axis(self.y, rect.row, rect.row.saturating_add(rect.height))?;
        Some(bottom)
    }

    pub(in crate::layout) fn fragment(self, fragment: &TextFragment) -> Option<TextFragment> {
        let scale = usize::from(fragment.style.scale);
        if scale == 0 {
            return None;
        }
        if let Some((top, bottom)) = self.y
            && (fragment.row < top || fragment.row.saturating_add(scale) > bottom)
        {
            return None;
        }
        let left = self.x.map_or(0, |axis| axis.0);
        let right = self.x.map_or(usize::MAX, |axis| axis.1);
        let mut col = fragment.col;
        let mut first = None;
        let mut text = String::new();
        for grapheme in fragment.text.graphemes(true) {
            let width = UnicodeWidthStr::width(grapheme).saturating_mul(scale);
            let end = col.saturating_add(width);
            if col >= left && end <= right {
                first.get_or_insert(col);
                text.push_str(grapheme);
            }
            col = end;
        }
        if text.is_empty() {
            None
        } else {
            let mut clipped = fragment.clone();
            clipped.col = first.unwrap_or(fragment.col);
            clipped.text = text;
            Some(clipped)
        }
    }

    pub(in crate::layout) fn stroke(self, stroke: BorderStroke) -> Option<BorderStroke> {
        let rect = self.rect(stroke.rect)?;
        let mut edges = stroke.edges;
        if rect.col != stroke.rect.col {
            edges.left = Default::default();
        }
        if rect.row != stroke.rect.row {
            edges.top = Default::default();
        }
        if rect.col.saturating_add(rect.width) != stroke.rect.col.saturating_add(stroke.rect.width)
        {
            edges.right = Default::default();
        }
        if rect.row.saturating_add(rect.height)
            != stroke.rect.row.saturating_add(stroke.rect.height)
        {
            edges.bottom = Default::default();
        }
        Some(BorderStroke {
            rect,
            edges,
            ..stroke
        })
    }

    pub(in crate::layout) fn translate_rect(
        rect: LayoutRect,
        col: isize,
        row: isize,
    ) -> Option<LayoutRect> {
        let left = (rect.col as isize).saturating_add(col);
        let top = (rect.row as isize).saturating_add(row);
        let right = left.saturating_add(rect.width as isize);
        let bottom = top.saturating_add(rect.height as isize);
        if right <= 0 || bottom <= 0 {
            return None;
        }
        let clipped_left = left.max(0);
        let clipped_top = top.max(0);
        Some(LayoutRect {
            col: clipped_left as usize,
            row: clipped_top as usize,
            width: right.saturating_sub(clipped_left) as usize,
            height: bottom.saturating_sub(clipped_top) as usize,
        })
    }

    pub(in crate::layout) fn translate_fragment(
        fragment: &TextFragment,
        col: isize,
        row: isize,
    ) -> Option<TextFragment> {
        let scale = usize::from(fragment.style.scale);
        let translated_row = (fragment.row as isize).saturating_add(row);
        if translated_row < 0 || scale == 0 {
            return None;
        }
        let mut translated_col = (fragment.col as isize).saturating_add(col);
        let mut first = None;
        let mut text = String::new();
        for grapheme in fragment.text.graphemes(true) {
            let width = UnicodeWidthStr::width(grapheme).saturating_mul(scale) as isize;
            let end = translated_col.saturating_add(width);
            if translated_col >= 0 {
                first.get_or_insert(translated_col as usize);
                text.push_str(grapheme);
            }
            translated_col = end;
        }
        if text.is_empty() {
            None
        } else {
            let mut translated = fragment.clone();
            translated.col = first.unwrap_or_default();
            translated.row = translated_row as usize;
            translated.text = text;
            Some(translated)
        }
    }

    pub(in crate::layout) fn translate_stroke(
        stroke: BorderStroke,
        col: isize,
        row: isize,
    ) -> Option<BorderStroke> {
        let rect = Self::translate_rect(stroke.rect, col, row)?;
        let mut edges = stroke.edges;
        if (stroke.rect.col as isize).saturating_add(col) < 0 {
            edges.left = Default::default();
        }
        if (stroke.rect.row as isize).saturating_add(row) < 0 {
            edges.top = Default::default();
        }
        Some(BorderStroke {
            rect,
            edges,
            ..stroke
        })
    }
}

fn intersect_axis(
    current: Option<(usize, usize)>,
    start: usize,
    end: usize,
) -> Option<(usize, usize)> {
    Some(current.map_or((start, end), |(old_start, old_end)| {
        (old_start.max(start), old_end.min(end))
    }))
}

fn bounded_axis(bound: Option<(usize, usize)>, start: usize, end: usize) -> Option<(usize, usize)> {
    let (start, end) = bound.map_or((start, end), |(low, high)| (start.max(low), end.min(high)));
    (start < end).then_some((start, end))
}

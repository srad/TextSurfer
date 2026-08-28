use ratatui::buffer::{Buffer, CellDiffOption};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::TextFieldState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextFieldFrame {
    #[default]
    None,
    Brackets,
}

#[derive(Clone, Copy, Debug)]
pub struct TextFieldView<'a> {
    text: &'a str,
    cursor: usize,
    selection: Option<(usize, usize)>,
    frame: TextFieldFrame,
    mask: Option<char>,
    multiline: bool,
    placeholder: Option<&'a str>,
}

impl<'a> TextFieldView<'a> {
    pub fn new(state: &'a TextFieldState) -> Self {
        Self {
            text: state.text(),
            cursor: state.cursor(),
            selection: state.selection().map(|range| (range.start, range.end)),
            frame: TextFieldFrame::None,
            mask: None,
            multiline: false,
            placeholder: state.placeholder(),
        }
    }

    pub fn display(text: &'a str) -> Self {
        Self {
            text,
            cursor: 0,
            selection: None,
            frame: TextFieldFrame::None,
            mask: None,
            multiline: false,
            placeholder: None,
        }
    }

    pub fn text(self) -> &'a str {
        self.text
    }

    pub fn is_empty(self) -> bool {
        self.text.is_empty()
    }

    pub fn frame(mut self, frame: TextFieldFrame) -> Self {
        self.frame = frame;
        self
    }

    pub fn mask(mut self, mask: char) -> Self {
        self.mask = Some(mask);
        self
    }

    pub fn multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    pub fn placeholder(mut self, placeholder: Option<&'a str>) -> Self {
        self.placeholder = placeholder;
        self
    }

    pub fn without_selection(mut self) -> Self {
        self.selection = None;
        self
    }

    pub fn cursor_in(self, area: Rect) -> Option<(u16, u16)> {
        let inner = self.inner(area)?;
        let layout = self.layout(inner);
        Some((
            inner.x + layout.cursor_col.min(inner.width.saturating_sub(1)),
            inner.y + layout.cursor_row.min(inner.height.saturating_sub(1)),
        ))
    }

    pub fn index_at(self, area: Rect, col: u16, row: u16) -> usize {
        let Some(inner) = self.inner(area) else {
            return 0;
        };
        let layout = self.layout(inner);
        let row = row
            .saturating_sub(inner.y)
            .min(inner.height.saturating_sub(1));
        let line = usize::from(layout.first_line.saturating_add(row));
        let lines = line_ranges(self.text);
        let Some(range) = lines.get(line) else {
            return self.text.graphemes(true).count();
        };
        let target = usize::from(col.saturating_sub(inner.x)) + layout.first_col;
        let mut width = 0;
        let mut index = range.start;
        for grapheme in self
            .text
            .graphemes(true)
            .skip(range.start)
            .take(range.len())
        {
            let next = width + self.grapheme_width(grapheme);
            if next > target {
                break;
            }
            width = next;
            index += 1;
        }
        index.min(range.end)
    }

    fn inner(self, area: Rect) -> Option<Rect> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        match self.frame {
            TextFieldFrame::None => Some(area),
            TextFieldFrame::Brackets if area.width >= 2 => {
                Some(Rect::new(area.x + 1, area.y, area.width - 2, area.height))
            }
            TextFieldFrame::Brackets => None,
        }
    }

    fn layout(self, inner: Rect) -> FieldLayout {
        let ranges = line_ranges(self.text);
        let cursor_line = ranges
            .iter()
            .position(|range| range.contains(&self.cursor) || range.end == self.cursor)
            .unwrap_or(ranges.len().saturating_sub(1));
        let cursor_width = self.width_between(ranges[cursor_line].start, self.cursor);
        let first_line = if self.multiline {
            cursor_line.saturating_sub(usize::from(inner.height).saturating_sub(1))
        } else {
            0
        };
        let desired_first_col =
            cursor_width.saturating_sub(usize::from(inner.width.saturating_sub(1)));
        let mut first_col = 0;
        for grapheme in self
            .text
            .graphemes(true)
            .skip(ranges[cursor_line].start)
            .take(self.cursor.saturating_sub(ranges[cursor_line].start))
        {
            if first_col >= desired_first_col {
                break;
            }
            first_col += self.grapheme_width(grapheme);
        }
        FieldLayout {
            first_line: u16::try_from(first_line).unwrap_or(u16::MAX),
            first_col,
            cursor_row: u16::try_from(cursor_line.saturating_sub(first_line)).unwrap_or(u16::MAX),
            cursor_col: u16::try_from(cursor_width.saturating_sub(first_col)).unwrap_or(u16::MAX),
        }
    }

    fn width_between(self, start: usize, end: usize) -> usize {
        self.text
            .graphemes(true)
            .skip(start)
            .take(end.saturating_sub(start))
            .map(|grapheme| self.grapheme_width(grapheme))
            .sum()
    }

    fn grapheme_width(self, grapheme: &str) -> usize {
        self.mask.map_or_else(
            || UnicodeWidthStr::width(grapheme),
            |mask| UnicodeWidthStr::width(mask.to_string().as_str()),
        )
    }
}

impl PartialEq<&str> for TextFieldView<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

struct FieldLayout {
    first_line: u16,
    first_col: usize,
    cursor_row: u16,
    cursor_col: u16,
}

pub struct TextField<'a> {
    view: TextFieldView<'a>,
    style: Style,
    selection_style: Style,
    placeholder_style: Style,
}

impl<'a> TextField<'a> {
    pub fn new(view: TextFieldView<'a>) -> Self {
        Self {
            view,
            style: Style::default(),
            selection_style: Style::default().reversed(),
            placeholder_style: Style::default().dim(),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn selection_style(mut self, style: Style) -> Self {
        self.selection_style = style;
        self
    }

    pub fn placeholder_style(mut self, style: Style) -> Self {
        self.placeholder_style = style;
        self
    }
}

impl Widget for TextField<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let Some(inner) = self.view.inner(area) else {
            return;
        };
        for row in area.y..area.bottom() {
            for col in area.x..area.right() {
                buffer[(col, row)]
                    .set_symbol(" ")
                    .set_diff_option(CellDiffOption::None);
            }
        }
        buffer.set_style(area, self.style);
        if self.view.frame == TextFieldFrame::Brackets {
            buffer.set_string(area.x, area.y, "[", self.style);
            buffer.set_string(area.right() - 1, area.y, "]", self.style);
        }
        let layout = self.view.layout(inner);
        let placeholder = self
            .view
            .text
            .is_empty()
            .then_some(self.view.placeholder)
            .flatten();
        let source = placeholder.unwrap_or(self.view.text);
        let selection = placeholder
            .is_none()
            .then_some(self.view.selection)
            .flatten();
        let ranges = line_ranges(source);
        for screen_row in 0..inner.height {
            let line = usize::from(layout.first_line.saturating_add(screen_row));
            let Some(range) = ranges.get(line) else {
                break;
            };
            let mut used = 0usize;
            let mut skipped = 0usize;
            for (offset, grapheme) in source
                .graphemes(true)
                .skip(range.start)
                .take(range.len())
                .enumerate()
            {
                let display = if placeholder.is_some() {
                    grapheme.to_string()
                } else {
                    self.view
                        .mask
                        .map_or_else(|| grapheme.to_string(), |mask| mask.to_string())
                };
                let width = UnicodeWidthStr::width(display.as_str());
                if skipped + width <= layout.first_col {
                    skipped += width;
                    continue;
                }
                if used + width > usize::from(inner.width) {
                    break;
                }
                let index = range.start + offset;
                let selected = selection.is_some_and(|(start, end)| (start..end).contains(&index));
                let style = if placeholder.is_some() {
                    self.placeholder_style
                } else if selected {
                    self.selection_style
                } else {
                    self.style
                };
                buffer.set_string(inner.x + used as u16, inner.y + screen_row, &display, style);
                used += width;
            }
        }
    }
}

fn line_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let mut ranges = Vec::new();
    let mut start = 0;
    for (index, grapheme) in graphemes.iter().enumerate() {
        if *grapheme == "\n" {
            ranges.push(start..index);
            start = index + 1;
        }
    }
    ranges.push(start..graphemes.len());
    ranges
}

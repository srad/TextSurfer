use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::TextFieldMenuAction;
use crate::core::event::{Key, KeyEvent};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardAction {
    Read,
    Write(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextFieldEdit {
    pub consumed: bool,
    pub changed: bool,
    pub clipboard: Option<ClipboardAction>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextFieldState {
    text: String,
    placeholder: Option<String>,
    cursor: usize,
    anchor: usize,
    preferred_column: Option<usize>,
}

impl TextFieldState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(text: &str) -> Self {
        let mut field = Self::new();
        field.set_text(text);
        field
    }

    pub fn with_placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = (!placeholder.is_empty()).then(|| placeholder.to_string());
        self
    }

    pub fn set_text(&mut self, text: &str) {
        self.text.clear();
        self.text.push_str(text);
        self.cursor = self.len();
        self.anchor = self.cursor;
        self.preferred_column = None;
    }

    pub fn reconcile_text(&mut self, text: &str) {
        if self.text == text {
            return;
        }
        self.text.clear();
        self.text.push_str(text);
        let len = self.len();
        self.cursor = self.cursor.min(len);
        self.anchor = self.anchor.min(len);
        self.preferred_column = None;
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn placeholder(&self) -> Option<&str> {
        self.placeholder.as_deref()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.text.graphemes(true).count()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn selection(&self) -> Option<Range<usize>> {
        (self.anchor != self.cursor)
            .then(|| self.anchor.min(self.cursor)..self.anchor.max(self.cursor))
    }

    pub fn selected_text(&self) -> Option<String> {
        let range = self.selection()?;
        Some(
            self.text
                .graphemes(true)
                .skip(range.start)
                .take(range.len())
                .collect(),
        )
    }

    pub fn set_cursor(&mut self, grapheme: usize, extend: bool) {
        self.cursor = grapheme.min(self.len());
        if !extend {
            self.anchor = self.cursor;
        }
        self.preferred_column = None;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.len();
        self.preferred_column = None;
    }

    pub fn paste(&mut self, text: &str) -> bool {
        if text.is_empty() && self.selection().is_none() {
            return false;
        }
        self.replace_selection(text);
        true
    }

    pub fn handle_key(&mut self, event: &KeyEvent, multiline: bool) -> TextFieldEdit {
        let mut edit = TextFieldEdit::default();
        if event.modifiers.alt {
            return edit;
        }
        if event.modifiers.ctrl {
            edit.consumed = true;
            match &event.code {
                Key::Char(character) if character.eq_ignore_ascii_case(&'a') => self.select_all(),
                Key::Char(character) if character.eq_ignore_ascii_case(&'c') => {
                    edit.clipboard = self.selected_text().map(ClipboardAction::Write);
                }
                Key::Char(character) if character.eq_ignore_ascii_case(&'x') => {
                    if let Some(selected) = self.selected_text() {
                        edit.clipboard = Some(ClipboardAction::Write(selected));
                        self.replace_selection("");
                        edit.changed = true;
                    }
                }
                Key::Char(character) if character.eq_ignore_ascii_case(&'v') => {
                    edit.clipboard = Some(ClipboardAction::Read);
                }
                Key::Left => self.move_word(false, event.modifiers.shift),
                Key::Right => self.move_word(true, event.modifiers.shift),
                _ => edit.consumed = false,
            }
            return edit;
        }
        edit.consumed = true;
        match event.code {
            Key::Char(character) => {
                self.replace_selection(&character.to_string());
                edit.changed = true;
            }
            Key::Backspace => edit.changed = self.erase(true),
            Key::Delete => edit.changed = self.erase(false),
            Key::Left => self.move_horizontal(false, event.modifiers.shift),
            Key::Right => self.move_horizontal(true, event.modifiers.shift),
            Key::Home => self.move_line_edge(false, multiline, event.modifiers.shift),
            Key::End => self.move_line_edge(true, multiline, event.modifiers.shift),
            Key::Up if multiline => self.move_vertical(false, event.modifiers.shift),
            Key::Down if multiline => self.move_vertical(true, event.modifiers.shift),
            Key::Enter if multiline => {
                self.replace_selection("\n");
                edit.changed = true;
            }
            _ => edit.consumed = false,
        }
        edit
    }

    pub fn apply_menu(&mut self, action: TextFieldMenuAction) -> TextFieldEdit {
        let mut edit = TextFieldEdit {
            consumed: true,
            ..Default::default()
        };
        match action {
            TextFieldMenuAction::Copy => {
                edit.clipboard = self.selected_text().map(ClipboardAction::Write);
            }
            TextFieldMenuAction::Cut => {
                if let Some(selected) = self.selected_text() {
                    edit.clipboard = Some(ClipboardAction::Write(selected));
                    self.replace_selection("");
                    edit.changed = true;
                }
            }
            TextFieldMenuAction::Paste => edit.clipboard = Some(ClipboardAction::Read),
            TextFieldMenuAction::SelectAll => self.select_all(),
        }
        edit
    }

    fn replace_selection(&mut self, replacement: &str) {
        let range = self.selection().unwrap_or(self.cursor..self.cursor);
        let start = self.byte_at(range.start);
        let end = self.byte_at(range.end);
        self.text.replace_range(start..end, replacement);
        let replacement_len = replacement.graphemes(true).count();
        self.cursor = range.start.saturating_add(replacement_len).min(self.len());
        self.anchor = self.cursor;
        self.preferred_column = None;
    }

    fn erase(&mut self, backward: bool) -> bool {
        if self.selection().is_some() {
            self.replace_selection("");
            return true;
        }
        let range = if backward && self.cursor > 0 {
            self.cursor - 1..self.cursor
        } else if !backward && self.cursor < self.len() {
            self.cursor..self.cursor + 1
        } else {
            return false;
        };
        let start = self.byte_at(range.start);
        let end = self.byte_at(range.end);
        self.text.replace_range(start..end, "");
        self.cursor = range.start;
        self.anchor = self.cursor;
        self.preferred_column = None;
        true
    }

    fn move_horizontal(&mut self, forward: bool, extend: bool) {
        if !extend && let Some(selection) = self.selection() {
            self.set_cursor(
                if forward {
                    selection.end
                } else {
                    selection.start
                },
                false,
            );
            return;
        }
        let next = if forward {
            self.cursor.saturating_add(1).min(self.len())
        } else {
            self.cursor.saturating_sub(1)
        };
        self.set_cursor(next, extend);
    }

    fn move_word(&mut self, forward: bool, extend: bool) {
        let graphemes: Vec<&str> = self.text.graphemes(true).collect();
        let mut next = self.cursor;
        if forward {
            while next < graphemes.len() && !graphemes[next].chars().any(char::is_whitespace) {
                next += 1;
            }
            while next < graphemes.len() && graphemes[next].chars().all(char::is_whitespace) {
                next += 1;
            }
        } else {
            while next > 0 && graphemes[next - 1].chars().all(char::is_whitespace) {
                next -= 1;
            }
            while next > 0 && !graphemes[next - 1].chars().any(char::is_whitespace) {
                next -= 1;
            }
        }
        self.set_cursor(next, extend);
    }

    fn move_line_edge(&mut self, end: bool, multiline: bool, extend: bool) {
        let (line, _, starts, ends) = self.line_position();
        let next = if multiline {
            if end { ends[line] } else { starts[line] }
        } else if end {
            self.len()
        } else {
            0
        };
        self.set_cursor(next, extend);
    }

    fn move_vertical(&mut self, down: bool, extend: bool) {
        let (line, column, starts, ends) = self.line_position();
        let preferred = self.preferred_column.unwrap_or(column);
        let next_line = if down {
            (line + 1).min(starts.len() - 1)
        } else {
            line.saturating_sub(1)
        };
        let next = starts[next_line]
            .saturating_add(preferred)
            .min(ends[next_line]);
        self.cursor = next;
        if !extend {
            self.anchor = next;
        }
        self.preferred_column = Some(preferred);
    }

    fn line_position(&self) -> (usize, usize, Vec<usize>, Vec<usize>) {
        let graphemes: Vec<&str> = self.text.graphemes(true).collect();
        let mut starts = vec![0];
        let mut ends = Vec::new();
        for (index, grapheme) in graphemes.iter().enumerate() {
            if *grapheme == "\n" {
                ends.push(index);
                starts.push(index + 1);
            }
        }
        ends.push(graphemes.len());
        let line = starts
            .iter()
            .rposition(|start| *start <= self.cursor)
            .unwrap_or(0);
        (line, self.cursor.saturating_sub(starts[line]), starts, ends)
    }

    fn byte_at(&self, grapheme: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .nth(grapheme)
            .map_or(self.text.len(), |(byte, _)| byte)
    }
}

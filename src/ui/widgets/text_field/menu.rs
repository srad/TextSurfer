use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Widget};

use crate::core::geom::Point;
use crate::ui::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFieldMenuAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl TextFieldMenuAction {
    pub fn is_enabled(self, can_copy: bool, can_cut: bool) -> bool {
        match self {
            Self::Cut => can_cut,
            Self::Copy => can_copy,
            Self::Paste | Self::SelectAll => true,
        }
    }
}

const ITEMS: [(TextFieldMenuAction, &str); 4] = [
    (TextFieldMenuAction::Cut, "Cut"),
    (TextFieldMenuAction::Copy, "Copy"),
    (TextFieldMenuAction::Paste, "Paste"),
    (TextFieldMenuAction::SelectAll, "Select all"),
];

pub fn menu_rect(bounds: Rect, anchor: Point) -> Rect {
    let width = 14.min(bounds.width);
    let height = 6.min(bounds.height);
    Rect::new(
        anchor.col.min(bounds.right().saturating_sub(width)),
        anchor.row.min(bounds.bottom().saturating_sub(height)),
        width,
        height,
    )
}

pub fn menu_action_at(bounds: Rect, anchor: Point, at: Point) -> Option<TextFieldMenuAction> {
    let rect = menu_rect(bounds, anchor);
    let position = Position::new(at.col, at.row);
    if !rect.contains(position)
        || at.col == rect.x
        || at.col.saturating_add(1) == rect.right()
        || at.row == rect.y
        || at.row.saturating_add(1) == rect.bottom()
    {
        return None;
    }
    ITEMS
        .get(usize::from(at.row - rect.y - 1))
        .map(|item| item.0)
}

pub struct TextFieldMenu<'a> {
    pub anchor: Point,
    pub can_copy: bool,
    pub can_cut: bool,
    pub selected: Option<TextFieldMenuAction>,
    pub theme: &'a Theme,
}

impl Widget for TextFieldMenu<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let rect = menu_rect(area, self.anchor);
        if rect.width < 2 || rect.height < 2 {
            return;
        }
        Clear.render(rect, buffer);
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.frame))
            .style(
                Style::default()
                    .fg(self.theme.bar_text)
                    .bg(self.theme.bar_bg),
            )
            .render(rect, buffer);
        for (row, (action, label)) in ITEMS.iter().enumerate() {
            if row as u16 + 2 >= rect.height {
                break;
            }
            let enabled = action.is_enabled(self.can_copy, self.can_cut);
            let selected = enabled && self.selected == Some(*action);
            let mut style = if selected {
                self.theme.selected()
            } else {
                Style::default()
                    .fg(self.theme.bar_text)
                    .bg(self.theme.bar_bg)
            };
            if !enabled {
                style = style.add_modifier(Modifier::DIM);
            }
            let y = rect.y + 1 + row as u16;
            if selected {
                buffer.set_style(Rect::new(rect.x + 1, y, rect.width - 2, 1), style);
            }
            buffer.set_string(rect.x + 1, y, label, style);
        }
    }
}

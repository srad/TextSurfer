use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, Widget};

use crate::ui::theme::Theme;

use super::{clip_width, width};

pub const MENU_TITLES: [&str; 4] = ["File", "Navigate", "View", "Help"];
pub const MENUS: [&[&str]; 4] = [
    &["New Tab", "Close Tab", "Reload", "Quit"],
    &["Back", "Forward", "Home"],
    &["Theme: Norton"],
    &["Help"],
];

pub fn title_x(menu: usize) -> u16 {
    let mut x = 1u16;
    for title in MENU_TITLES.iter().take(menu) {
        x += width(title) + 2;
    }
    x
}

/// The menu whose title covers `col` of a bar `bar_width` wide, if any.
///
/// Walks the same spans `MenuBar::render` writes, including its overflow break, so a
/// title that was never drawn is not clickable.
pub fn title_at(col: u16, bar_width: u16) -> Option<usize> {
    let mut x = 0u16;
    for (index, title) in MENU_TITLES.iter().enumerate() {
        let span = width(title) + 2;
        if x + span > bar_width {
            return None;
        }
        if col >= x && col < x + span {
            return Some(index);
        }
        x += span;
    }
    None
}

/// The item at `row` of the open popup, if the point is inside its interior.
pub fn popup_item_at(area: Rect, menu: usize, at: Position) -> Option<usize> {
    let rect = popup_rect(area, area, menu);
    if rect.width < 2 || rect.height < 2 || !rect.contains(at) {
        return None;
    }
    let index = usize::from(at.y.checked_sub(rect.y + 1)?);
    let last_row = usize::from(rect.height.saturating_sub(2));
    (index < MENUS[menu].len().min(last_row)).then_some(index)
}

pub fn popup_rect(area: Rect, bounds: Rect, menu: usize) -> Rect {
    let items = MENUS[menu];
    let max_item = items.iter().map(|item| width(item)).max().unwrap_or(1);
    Rect {
        x: area.x + title_x(menu),
        y: area.y + 1,
        width: (max_item + 5).min(bounds.width.saturating_sub(area.x + title_x(menu))),
        height: (items.len() as u16 + 2).min(bounds.height.saturating_sub(area.y + 1)),
    }
}

pub struct MenuBar<'a> {
    pub active: usize,
    pub open: bool,
    pub theme: &'a Theme,
}

impl Widget for MenuBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bar = Style::default()
            .bg(self.theme.bar_bg)
            .fg(self.theme.bar_text);
        if area.width < 2 {
            return;
        }
        buf.set_style(area, bar);
        let mut x = 0u16;
        for (index, title) in MENU_TITLES.iter().enumerate() {
            let fragment_width = width(title) + 2;
            if x + fragment_width > area.width {
                break;
            }
            if self.open && index == self.active {
                buf.set_string(
                    area.x + x,
                    area.y,
                    format!(" {title} "),
                    self.theme.selected(),
                );
            } else {
                buf.set_string(area.x + x, area.y, " ", bar);
                buf.set_string(
                    area.x + x + 1,
                    area.y,
                    &title[..1],
                    Style::default()
                        .fg(self.theme.mnemonic)
                        .bg(self.theme.bar_bg),
                );
                buf.set_string(area.x + x + 2, area.y, &title[1..], bar);
                buf.set_string(area.x + x + fragment_width - 1, area.y, " ", bar);
            }
            x += fragment_width;
        }
        let ornament = "TextSurfer";
        let ornament_width = width(ornament);
        let room = area.width.saturating_sub(x);
        if room >= ornament_width {
            let start = x + (room - ornament_width) / 2;
            buf.set_string(area.x + start, area.y, ornament, bar);
        }
    }
}

pub struct MenuPopup<'a> {
    pub menu: usize,
    pub selected: usize,
    pub theme: &'a Theme,
}

impl Widget for MenuPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let items = MENUS[self.menu];
        let rect = popup_rect(area, buf.area, self.menu);
        if rect.width < 2 || rect.height < 2 {
            return;
        }
        Clear.render(rect, buf);
        let block = Block::bordered()
            .border_style(Style::default().fg(self.theme.frame))
            .style(
                Style::default()
                    .bg(self.theme.bar_bg)
                    .fg(self.theme.bar_text),
            );
        let list_items: Vec<ListItem> = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let clipped = clip_width(item, rect.width.saturating_sub(1));
                let style = if index == self.selected {
                    self.theme.selected()
                } else {
                    Style::default()
                        .bg(self.theme.bar_bg)
                        .fg(self.theme.bar_text)
                };
                if index == self.selected || clipped.is_empty() {
                    return ListItem::new(clipped).style(style);
                }
                let mut chars = clipped.chars();
                let first = chars.next().unwrap();
                let first_len = first.len_utf8();
                let line = Line::from(vec![
                    Span::styled(
                        clipped[..first_len].to_string(),
                        Style::default()
                            .fg(self.theme.mnemonic)
                            .bg(self.theme.bar_bg),
                    ),
                    Span::styled(clipped[first_len..].to_string(), style),
                ]);
                ListItem::new(line)
            })
            .collect();
        List::new(list_items).block(block).render(rect, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::NORTON;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render_bar(active: usize, open: bool, width: u16) -> String {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                MenuBar {
                    active,
                    open,
                    theme: &NORTON,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    #[test]
    fn renders_titles_and_the_ornament() {
        insta::assert_snapshot!(render_bar(0, false, 80));
        insta::assert_snapshot!(render_bar(2, true, 80));
    }

    #[test]
    fn narrow_rows_break_before_overflowing() {
        insta::assert_snapshot!(render_bar(0, false, 16));
    }

    #[test]
    fn popup_shows_items_with_the_selected_one_inverse() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                MenuPopup {
                    menu: 0,
                    selected: 2,
                    theme: &NORTON,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        insta::assert_snapshot!(crate::ui::test_util::buffer_string(
            terminal.backend().buffer()
        ));
        let cells = terminal.backend().buffer().content();
        let selected = NORTON.selected();
        assert_eq!(
            cells[4 * 80 + 4].style().bg,
            selected.bg,
            "the selected row must be inverse"
        );
        assert_ne!(
            cells[3 * 80 + 4].style().bg,
            selected.bg,
            "unselected rows must stay on the bar background"
        );
    }
}

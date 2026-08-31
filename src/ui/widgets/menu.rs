use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, Widget};

use crate::ui::theme::Theme;

use super::{clip_width, width};
use crate::ui::theme::THEME_NAMES;

pub const MENU_TITLES: [&str; 4] = ["File", "Navigate", "View", "Help"];
pub const THEME_MENU: usize = 2;
pub const MENUS: [&[&str]; 4] = [
    &["New Tab", "Close Tab", "Reload", "Quit"],
    &["Back", "Forward", "Home"],
    &THEME_NAMES,
    &["Help"],
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuItemMask(u64);

impl MenuItemMask {
    pub fn all(count: usize) -> Self {
        Self(if count >= u64::BITS as usize {
            u64::MAX
        } else {
            (1u64 << count).saturating_sub(1)
        })
    }

    pub fn set(&mut self, index: usize, enabled: bool) {
        let Some(bit) = 1u64.checked_shl(index as u32) else {
            return;
        };
        if enabled {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }

    pub fn contains(self, index: usize) -> bool {
        1u64.checked_shl(index as u32)
            .is_some_and(|bit| self.0 & bit != 0)
    }
}

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
    if rect.width < 2
        || rect.height < 2
        || !rect.contains(at)
        || at.x == rect.x
        || at.x.saturating_add(1) == rect.right()
    {
        return None;
    }
    let index = usize::from(at.y.checked_sub(rect.y + 1)?);
    (index < popup_item_count(area, menu)).then_some(index)
}

pub fn popup_item_count(area: Rect, menu: usize) -> usize {
    let rect = popup_rect(area, area, menu);
    if rect.width < 2 || rect.height < 2 {
        return 0;
    }
    MENUS[menu]
        .len()
        .min(usize::from(rect.height.saturating_sub(2)))
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
    pub hovered: Option<usize>,
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
            let selected = if self.open {
                index == self.active
            } else {
                self.hovered == Some(index)
            };
            if selected {
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
    pub selected: Option<usize>,
    pub enabled: MenuItemMask,
    pub marked: Option<usize>,
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
        let text_width = rect
            .width
            .saturating_sub(2)
            .saturating_sub(u16::from(self.marked.is_some()));
        let list_items: Vec<ListItem> = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let clipped = clip_width(item, text_width);
                let enabled = self.enabled.contains(index);
                let selected = enabled && self.selected == Some(index);
                let style = if selected {
                    self.theme.selected()
                } else if !enabled {
                    Style::default()
                        .bg(self.theme.bar_bg)
                        .fg(self.theme.bar_text)
                        .add_modifier(Modifier::DIM)
                } else {
                    Style::default()
                        .bg(self.theme.bar_bg)
                        .fg(self.theme.bar_text)
                };
                if selected || !enabled || clipped.is_empty() {
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
        let visible = popup_item_count(buf.area, self.menu);
        for index in 0..visible {
            let style = if self.enabled.contains(index) {
                (self.selected == Some(index)).then(|| self.theme.selected())
            } else {
                Some(
                    Style::default()
                        .bg(self.theme.bar_bg)
                        .fg(self.theme.bar_text)
                        .add_modifier(Modifier::DIM),
                )
            };
            if let Some(style) = style {
                buf.set_style(
                    Rect::new(rect.x + 1, rect.y + 1 + index as u16, rect.width - 2, 1),
                    style,
                );
            }
        }
        if let Some(marked) = self.marked
            && marked < items.len()
            && marked < usize::from(rect.height.saturating_sub(2))
            && rect.width >= 3
        {
            let style = if self.enabled.contains(marked) && self.selected == Some(marked) {
                self.theme.selected()
            } else if !self.enabled.contains(marked) {
                Style::default()
                    .bg(self.theme.bar_bg)
                    .fg(self.theme.bar_text)
                    .add_modifier(Modifier::DIM)
            } else {
                Style::default()
                    .bg(self.theme.bar_bg)
                    .fg(self.theme.bar_text)
            };
            buf[(rect.right() - 2, rect.y + 1 + marked as u16)]
                .set_symbol("•")
                .set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::DEFAULT;
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
                    hovered: None,
                    theme: &DEFAULT,
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
    fn main_menu_closed_title_hover_uses_the_selected_style() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                MenuBar {
                    active: 0,
                    open: false,
                    hovered: Some(1),
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let selected = DEFAULT.selected();
        for col in title_x(1) - 1..title_x(2) - 1 {
            assert_eq!(
                terminal.backend().buffer()[(col, 0)].bg,
                selected.bg.unwrap()
            );
        }
    }

    #[test]
    fn main_menu_disabled_rows_stay_dim_and_enabled_selection_fills_the_row() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut enabled = MenuItemMask::all(MENUS[1].len());
        enabled.set(0, false);
        terminal
            .draw(|frame| {
                MenuPopup {
                    menu: 1,
                    selected: Some(0),
                    enabled,
                    marked: None,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let rect = popup_rect(
            terminal.backend().buffer().area,
            terminal.backend().buffer().area,
            1,
        );
        let disabled = &terminal.backend().buffer()[(rect.x + 1, rect.y + 1)];
        assert!(disabled.modifier.contains(Modifier::DIM));
        assert_ne!(disabled.bg, DEFAULT.selected().bg.unwrap());

        terminal
            .draw(|frame| {
                MenuPopup {
                    menu: 1,
                    selected: Some(2),
                    enabled,
                    marked: None,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        for col in rect.x + 1..rect.right() - 1 {
            assert_eq!(
                terminal.backend().buffer()[(col, rect.y + 3)].bg,
                DEFAULT.selected().bg.unwrap()
            );
        }
    }

    #[test]
    fn main_menu_popup_borders_are_not_item_targets() {
        let area = Rect::new(0, 0, 80, 24);
        let rect = popup_rect(area, area, 0);
        assert_eq!(
            popup_item_at(area, 0, Position::new(rect.x, rect.y + 1)),
            None
        );
        assert_eq!(
            popup_item_at(area, 0, Position::new(rect.right() - 1, rect.y + 1)),
            None
        );
        assert_eq!(popup_item_count(Rect::new(0, 0, 1, 24), 0), 0);
    }

    #[test]
    fn popup_shows_items_with_the_selected_one_inverse() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                MenuPopup {
                    menu: 0,
                    selected: Some(2),
                    enabled: MenuItemMask::all(MENUS[0].len()),
                    marked: None,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        insta::assert_snapshot!(crate::ui::test_util::buffer_string(
            terminal.backend().buffer()
        ));
        let cells = terminal.backend().buffer().content();
        let selected = DEFAULT.selected();
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

    #[test]
    fn view_popup_marks_the_current_theme() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                MenuPopup {
                    menu: THEME_MENU,
                    selected: Some(4),
                    enabled: MenuItemMask::all(MENUS[THEME_MENU].len()),
                    marked: Some(4),
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        insta::assert_snapshot!(crate::ui::test_util::buffer_string(
            terminal.backend().buffer()
        ));
    }

    #[test]
    fn clipped_view_popup_preserves_the_marker_and_final_item_hit_target() {
        let backend = TestBackend::new(24, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                MenuPopup {
                    menu: THEME_MENU,
                    selected: Some(4),
                    enabled: MenuItemMask::all(MENUS[THEME_MENU].len()),
                    marked: Some(4),
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let rect = popup_rect(
            terminal.backend().buffer().area,
            terminal.backend().buffer().area,
            THEME_MENU,
        );
        assert_eq!(
            terminal.backend().buffer()[(rect.right() - 2, rect.y + 5)].symbol(),
            "•"
        );
        assert_eq!(
            popup_item_at(
                terminal.backend().buffer().area,
                THEME_MENU,
                Position::new(rect.x + 1, rect.y + 5),
            ),
            Some(4)
        );
    }
}

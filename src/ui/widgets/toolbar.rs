use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::ui::theme::Theme;

use super::text_field::{TextField, TextFieldView};

pub const BUTTON_COUNT: u16 = 4;
pub const EXPANDED_MIN_WIDTH: u16 = 47;
pub const EXPANDED_MIN_ROWS: u16 = 9;
const COMPACT_BUTTON_PITCH: u16 = 4;
const EXPANDED_BUTTON_PITCH: u16 = 6;
const BUTTON_START: u16 = 2;
const EXPANDED_BUTTON_WIDTH: u16 = 5;
const EXPANDED_BUTTON_HEIGHT: u16 = 3;
pub const BUTTONS_END: u16 = BUTTON_START + BUTTON_COUNT * COMPACT_BUTTON_PITCH;
pub const FIELD_LABEL_START: u16 = BUTTONS_END + 1;
pub const FIELD_RAIL: u16 = FIELD_LABEL_START + 4 + 1;
pub const FIELD_TEXT: u16 = FIELD_RAIL + 1;
const EXPANDED_FIELD_START: u16 = 27;

const BUTTONS: [&str; 4] = ["‹", "›", "↻", "⌂"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolbarLayout {
    pub expanded: bool,
    pub buttons: [Option<Rect>; BUTTON_COUNT as usize],
    pub address: Option<Rect>,
    pub address_text: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarTarget {
    Button(usize),
    Address { col: u16 },
}

impl ToolbarLayout {
    pub fn target_at(&self, position: Position) -> Option<ToolbarTarget> {
        if let Some(index) = self
            .buttons
            .iter()
            .position(|rect| rect.is_some_and(|rect| rect.contains(position)))
        {
            return Some(ToolbarTarget::Button(index));
        }
        self.address
            .filter(|rect| rect.contains(position))
            .map(|_| ToolbarTarget::Address { col: position.x })
    }
}

pub fn layout_toolbar(area: Rect) -> ToolbarLayout {
    let expanded = area.height >= EXPANDED_BUTTON_HEIGHT;
    let button_width = if expanded { EXPANDED_BUTTON_WIDTH } else { 3 };
    let button_height = if expanded { EXPANDED_BUTTON_HEIGHT } else { 1 };
    let button_pitch = if expanded {
        EXPANDED_BUTTON_PITCH
    } else {
        COMPACT_BUTTON_PITCH
    };
    let buttons = std::array::from_fn(|index| {
        let x = area.x + BUTTON_START + index as u16 * button_pitch;
        (x.saturating_add(button_width) < area.right())
            .then(|| Rect::new(x, area.y, button_width, button_height.min(area.height)))
    });
    let address_x = area.x
        + if expanded {
            EXPANDED_FIELD_START
        } else {
            FIELD_RAIL
        };
    let address_right = area.right().saturating_sub(2);
    let address = (address_x.saturating_add(1) < address_right).then(|| {
        Rect::new(
            address_x,
            area.y,
            address_right - address_x,
            button_height.min(area.height),
        )
    });
    let address_text = address.and_then(|address| {
        let y = address.y + u16::from(expanded);
        (address.width > 2 && y < address.bottom())
            .then(|| Rect::new(address.x + 1, y, address.width - 2, 1))
    });
    ToolbarLayout {
        expanded,
        buttons,
        address,
        address_text,
    }
}

pub struct Toolbar<'a> {
    pub address: TextFieldView<'a>,
    pub focused: bool,
    pub back_enabled: bool,
    pub forward_enabled: bool,
    pub hovered_button: Option<usize>,
    pub theme: &'a Theme,
}

impl Widget for Toolbar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 2 || area.height == 0 {
            return;
        }
        let layout = layout_toolbar(area);
        let frame = Style::default().fg(self.theme.frame);
        for row in area.y..area.bottom() {
            buf.set_string(area.x, row, "│", frame);
            buf.set_string(area.right() - 1, row, "│", frame);
        }
        for (index, rect) in layout.buttons.iter().enumerate() {
            let Some(rect) = rect else {
                continue;
            };
            let enabled = match index {
                0 => self.back_enabled,
                1 => self.forward_enabled,
                _ => true,
            };
            let hovered = enabled && self.hovered_button == Some(index);
            if layout.expanded {
                let wall = if hovered {
                    Style::default().fg(self.theme.bg).bg(self.theme.hover)
                } else {
                    frame
                };
                let glyph = if hovered {
                    wall
                } else if enabled {
                    Style::default().fg(self.theme.text)
                } else {
                    Style::default().fg(self.theme.dim)
                };
                if hovered {
                    buf.set_style(*rect, wall);
                }
                buf.set_string(rect.x, rect.y, "┌───┐", wall);
                buf.set_string(rect.x, rect.y + 1, "│", wall);
                buf.set_string(rect.x + 1, rect.y + 1, " ", glyph);
                buf.set_string(rect.x + 2, rect.y + 1, BUTTONS[index], glyph);
                buf.set_string(rect.x + 3, rect.y + 1, " ", glyph);
                buf.set_string(rect.x + 4, rect.y + 1, "│", wall);
                buf.set_string(rect.x, rect.y + 2, "└───┘", wall);
            } else {
                let style = if hovered {
                    Style::default().fg(self.theme.bg).bg(self.theme.hover)
                } else if enabled {
                    Style::default().fg(self.theme.text)
                } else {
                    Style::default().fg(self.theme.dim)
                };
                let wall = if hovered { style } else { frame };
                buf.set_string(rect.x, rect.y, "[", wall);
                buf.set_string(rect.x + 1, rect.y, BUTTONS[index], style);
                buf.set_string(rect.x + 2, rect.y, "]", wall);
            }
        }
        let field_style = if self.focused {
            Style::default().fg(self.theme.text)
        } else {
            Style::default()
        };
        let field_wall = if self.focused {
            Style::default().fg(self.theme.text)
        } else {
            frame
        };
        if layout.expanded {
            if let Some(address) = layout.address {
                let horizontal = "─".repeat(usize::from(address.width.saturating_sub(2)));
                buf.set_string(address.x, address.y, "┌", field_wall);
                buf.set_string(address.x + 1, address.y, &horizontal, field_wall);
                buf.set_string(address.right() - 1, address.y, "┐", field_wall);
                if address.width >= 8 {
                    buf.set_string(
                        address.x + 2,
                        address.y,
                        " URL ",
                        Style::default().fg(self.theme.dim),
                    );
                }
                buf.set_string(address.x, address.y + 1, "│", field_wall);
                buf.set_string(address.right() - 1, address.y + 1, "│", field_wall);
                buf.set_string(address.x, address.y + 2, "└", field_wall);
                buf.set_string(address.x + 1, address.y + 2, &horizontal, field_wall);
                buf.set_string(address.right() - 1, address.y + 2, "┘", field_wall);
            }
        } else {
            buf.set_string(
                area.x + FIELD_LABEL_START,
                area.y,
                "URL:",
                Style::default().fg(self.theme.dim),
            );
            if let Some(address) = layout.address {
                buf.set_string(address.x, address.y, "│", field_wall);
                buf.set_string(address.right() - 1, address.y, "│", field_wall);
            }
        }
        if let Some(text) = layout.address_text {
            TextField::new(self.address)
                .style(field_style)
                .selection_style(self.theme.selected())
                .render(text, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::DEFAULT;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(address: &str, focused: bool, back: bool, forward: bool, width: u16) -> String {
        let height = if width >= EXPANDED_MIN_WIDTH { 3 } else { 1 };
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: TextFieldView::display(address),
                    focused,
                    back_enabled: back,
                    forward_enabled: forward,
                    hovered_button: None,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    #[test]
    fn renders_buttons_label_and_address() {
        insta::assert_snapshot!(render("https://example.com", false, true, true, 60));
    }

    #[test]
    fn back_button_marks_itself_disabled_at_history_start() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: TextFieldView::display("https://example.com"),
                    focused: false,
                    back_enabled: false,
                    forward_enabled: true,
                    hovered_button: None,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer();
        assert_eq!(cells[(4, 1)].symbol(), "‹");
        assert_eq!(
            cells[(4, 1)].style().fg,
            Some(DEFAULT.dim),
            "disabled back glyph must be dim"
        );
        assert_eq!(cells[(10, 1)].symbol(), "›");
        assert_eq!(
            cells[(10, 1)].style().fg,
            Some(DEFAULT.text),
            "enabled forward glyph must be normal text"
        );
    }

    #[test]
    fn narrowed_window_clips_the_address() {
        insta::assert_snapshot!(render(
            "https://example.com/very/long/path",
            false,
            true,
            true,
            40
        ));
    }

    #[test]
    fn focused_field_does_not_paint_unselected_text_as_selected() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: TextFieldView::display("https://example.com"),
                    focused: true,
                    back_enabled: true,
                    forward_enabled: true,
                    hovered_button: None,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer();
        let selected = DEFAULT.selected();
        assert_ne!(cells[(27, 1)].style().bg, selected.bg);
        assert_ne!(cells[(28, 1)].style().bg, selected.bg);
        assert_ne!(cells[(44, 1)].style().bg, selected.bg);
        assert_eq!(cells[(28, 1)].style().fg, Some(DEFAULT.text));
        assert_eq!(cells[(44, 1)].style().fg, Some(DEFAULT.text));
    }

    #[test]
    fn unfocused_field_leaves_the_interior_plain() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: TextFieldView::display("https://example.com"),
                    focused: false,
                    back_enabled: true,
                    forward_enabled: true,
                    hovered_button: None,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer();
        assert_eq!(
            cells[(44, 1)].style().bg,
            Some(ratatui::style::Color::Reset),
            "unfocused interior stays plain"
        );
    }

    #[test]
    fn compact_address_rails_use_the_theme_frame() {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: TextFieldView::display("about:blank"),
                    focused: false,
                    back_enabled: true,
                    forward_enabled: true,
                    hovered_button: None,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer();
        assert_eq!(cells[(24, 0)].style().fg, Some(DEFAULT.frame));
        assert_eq!(cells[(37, 0)].style().fg, Some(DEFAULT.frame));
    }

    #[test]
    fn expanded_layout_uses_three_row_buttons_and_a_framed_address() {
        let layout = layout_toolbar(Rect::new(0, 0, 60, 3));
        assert!(layout.expanded);
        assert_eq!(layout.buttons[0], Some(Rect::new(2, 0, 5, 3)));
        assert_eq!(layout.buttons[3], Some(Rect::new(20, 0, 5, 3)));
        assert_eq!(layout.address, Some(Rect::new(27, 0, 31, 3)));
        assert_eq!(layout.address_text, Some(Rect::new(28, 1, 29, 1)));
    }

    #[test]
    fn hover_fills_an_enabled_button_but_not_a_disabled_one() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: TextFieldView::display("about:blank"),
                    focused: false,
                    back_enabled: false,
                    forward_enabled: true,
                    hovered_button: Some(1),
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer();
        for row in 0..3 {
            for col in 8..13 {
                assert_eq!(cells[(col, row)].style().bg, Some(DEFAULT.hover));
            }
        }
        assert_ne!(cells[(4, 1)].style().bg, Some(DEFAULT.hover));
    }

    const _: () = {
        assert!(FIELD_RAIL > FIELD_LABEL_START);
        assert!(FIELD_TEXT == FIELD_RAIL + 1);
    };
}

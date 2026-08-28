use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::ui::theme::Theme;

use super::text_field::{TextField, TextFieldView};

pub const BUTTON_PITCH: u16 = 4;
pub const BUTTON_COUNT: u16 = 4;
pub const BUTTONS_END: u16 = 1 + BUTTON_COUNT * BUTTON_PITCH;
pub const FIELD_LABEL_START: u16 = BUTTONS_END + 1;
pub const FIELD_RAIL: u16 = FIELD_LABEL_START + 4 + 1;
pub const FIELD_TEXT: u16 = FIELD_RAIL + 1;

const BUTTONS: [&str; 4] = ["‹", "›", "↻", "⌂"];

pub struct Toolbar<'a> {
    pub address: TextFieldView<'a>,
    pub focused: bool,
    pub back_enabled: bool,
    pub forward_enabled: bool,
    pub theme: &'a Theme,
}

impl Widget for Toolbar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 2 {
            return;
        }
        let frame = Style::default().fg(self.theme.frame);
        buf.set_string(area.x, area.y, "│", frame);
        buf.set_string(area.right() - 1, area.y, "│", frame);
        for (index, glyph) in BUTTONS.iter().enumerate() {
            let x = area.x + 1 + index as u16 * BUTTON_PITCH;
            if x.saturating_add(2) >= area.right() - 1 {
                break;
            }
            buf.set_string(x, area.y, "[", frame);
            let enabled = match index {
                0 => self.back_enabled,
                1 => self.forward_enabled,
                _ => true,
            };
            let style = if enabled {
                Style::default().fg(self.theme.text)
            } else {
                Style::default().fg(self.theme.dim)
            };
            buf.set_string(x + 1, area.y, *glyph, style);
            buf.set_string(x + 2, area.y, "]", frame);
        }
        let label = "URL:";
        buf.set_string(
            area.x + FIELD_LABEL_START,
            area.y,
            label,
            Style::default().fg(self.theme.dim),
        );
        let rail_x = area.x + FIELD_RAIL;
        if rail_x >= area.right() {
            return;
        }
        let field_style = if self.focused {
            Style::default().fg(self.theme.text)
        } else {
            Style::default()
        };
        buf.set_string(area.x + FIELD_RAIL, area.y, "│", field_style);
        if rail_x + 1 < area.right() {
            TextField::new(self.address)
                .style(field_style)
                .selection_style(self.theme.selected())
                .render(
                    Rect::new(
                        area.x + FIELD_TEXT,
                        area.y,
                        area.width.saturating_sub(FIELD_TEXT + 1),
                        1,
                    ),
                    buf,
                );
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
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: TextFieldView::display(address),
                    focused,
                    back_enabled: back,
                    forward_enabled: forward,
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
        let backend = TestBackend::new(60, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: TextFieldView::display("https://example.com"),
                    focused: false,
                    back_enabled: false,
                    forward_enabled: true,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer().content();
        assert_eq!(cells[2].symbol(), "‹");
        assert_eq!(
            cells[2].style().fg,
            Some(DEFAULT.dim),
            "disabled back glyph must be dim"
        );
        assert_eq!(cells[6].symbol(), "›");
        assert_eq!(
            cells[6].style().fg,
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
        let backend = TestBackend::new(60, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: TextFieldView::display("https://example.com"),
                    focused: true,
                    back_enabled: true,
                    forward_enabled: true,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer().content();
        let selected = DEFAULT.selected();
        assert_ne!(cells[23].style().bg, selected.bg);
        assert_ne!(cells[24].style().bg, selected.bg);
        assert_ne!(cells[40].style().bg, selected.bg);
        assert_eq!(cells[24].style().fg, Some(DEFAULT.text));
        assert_eq!(cells[40].style().fg, Some(DEFAULT.text));
    }

    #[test]
    fn unfocused_field_leaves_the_interior_plain() {
        let backend = TestBackend::new(60, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: TextFieldView::display("https://example.com"),
                    focused: false,
                    back_enabled: true,
                    forward_enabled: true,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer().content();
        assert_eq!(
            cells[40].style().bg,
            Some(ratatui::style::Color::Reset),
            "unfocused interior stays plain"
        );
    }

    const _: () = {
        assert!(FIELD_RAIL > FIELD_LABEL_START);
        assert!(FIELD_TEXT == FIELD_RAIL + 1);
    };
}

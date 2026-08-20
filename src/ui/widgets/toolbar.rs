use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use super::clip_width;

pub const NAV_BUTTONS: &str = "‹  ›  ↻  ";
pub const NAV_BUTTONS_WIDTH: u16 = 9;

pub struct Toolbar<'a> {
    pub address: &'a str,
    pub focused: bool,
}

impl Widget for Toolbar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_string(
            area.x,
            area.y,
            NAV_BUTTONS,
            Style::default().fg(Color::DarkGray),
        );
        let budget = area.width.saturating_sub(NAV_BUTTONS_WIDTH);
        if budget == 0 {
            return;
        }
        let text = clip_width(self.address, budget);
        let style = if self.focused {
            Style::default().add_modifier(Modifier::UNDERLINED)
        } else {
            Style::default()
        };
        buf.set_string(area.x + NAV_BUTTONS_WIDTH, area.y, text, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(address: &str, focused: bool) -> String {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| Toolbar { address, focused }.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    #[test]
    fn renders_nav_buttons_and_address() {
        insta::assert_snapshot!(render("https://example.com", false));
    }

    #[test]
    fn clipped_address_when_narrow() {
        insta::assert_snapshot!(render("https://example.com/very/long/path", false));
    }

    #[test]
    fn focused_address_is_underlined() {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Toolbar {
                    address: "https://example.com",
                    focused: true,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer().content();
        assert!(
            cells[10]
                .style()
                .add_modifier
                .contains(ratatui::style::Modifier::UNDERLINED),
        );
    }
}

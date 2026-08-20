use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

pub struct StatusView {
    pub url: String,
    pub message: String,
}

pub struct StatusBar<'a> {
    pub view: &'a StatusView,
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let message = &self.view.message;
        let url = &self.view.url;
        let message_width = unicode_width::UnicodeWidthStr::width(message.as_str());
        let url_width = unicode_width::UnicodeWidthStr::width(url.as_str());
        let total = usize::from(area.width);
        let pad = total.saturating_sub(message_width + 1 + url_width);
        let line = Line::from(vec![
            Span::raw(format!("{message}{}", " ".repeat(pad))),
            Span::styled(url.clone(), Style::default().fg(Color::DarkGray)),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(url: &str, message: &str) -> String {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let view = StatusView {
            url: url.to_string(),
            message: message.to_string(),
        };
        terminal
            .draw(|frame| StatusBar { view: &view }.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    #[test]
    fn message_left_url_right() {
        insta::assert_snapshot!(render("https://example.com", "Loaded"));
    }

    #[test]
    fn long_message_squeezes_the_url_off_edge() {
        insta::assert_snapshot!(render(
            "https://example.com",
            "this message is much longer than the status row can possibly hold"
        ));
    }
}

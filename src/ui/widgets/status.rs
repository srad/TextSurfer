use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::ui::theme::Theme;

pub struct StatusView<'a> {
    pub url: Cow<'a, str>,
    pub message: Cow<'a, str>,
    /// The link under the pointer. It replaces the message while it lasts, the way a
    /// browser previews a target, and leaves the message untouched underneath.
    pub hover: Option<Cow<'a, str>>,
}

pub struct StatusBar<'a> {
    pub view: &'a StatusView<'a>,
    pub theme: &'a Theme,
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bar = Style::default()
            .bg(self.theme.bar_bg)
            .fg(self.theme.bar_text);
        if area.width < 2 {
            return;
        }
        buf.set_style(area, bar);
        let inner = usize::from(area.width);
        let message = self.view.hover.as_ref().unwrap_or(&self.view.message);
        let url = &self.view.url;
        let message_width = unicode_width::UnicodeWidthStr::width(message.as_ref());
        let url_width = unicode_width::UnicodeWidthStr::width(url.as_ref());
        let pad = inner.saturating_sub(message_width + 1 + url_width);
        let line = Line::from(vec![
            Span::styled(format!("{message}{}", " ".repeat(pad)), bar),
            Span::styled(url.as_ref(), bar),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::NORTON;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(url: &str, message: &str) -> String {
        render_view(url, message, None)
    }

    fn render_view(url: &str, message: &str, hover: Option<&str>) -> String {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let view = StatusView {
            url: url.to_string().into(),
            message: message.to_string().into(),
            hover: hover.map(|hover| hover.to_string().into()),
        };
        terminal
            .draw(|frame| {
                StatusBar {
                    view: &view,
                    theme: &NORTON,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    #[test]
    fn message_left_url_right() {
        insta::assert_snapshot!(render("https://example.com", "Loaded"));
    }

    #[test]
    fn a_hovered_link_previews_in_place_of_the_message() {
        insta::assert_snapshot!(render_view(
            "https://example.com",
            "Loaded",
            Some("https://a.example/x")
        ));
    }

    #[test]
    fn long_message_squeezes_the_url_off_edge() {
        insta::assert_snapshot!(render(
            "https://example.com",
            "this message is much longer than the status row can possibly hold"
        ));
    }
}

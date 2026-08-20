use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::ui::theme::Theme;

pub struct ContentLines<'a> {
    pub lines: Cow<'a, [String]>,
    pub scroll: usize,
}

pub struct Content<'a> {
    pub lines: &'a ContentLines<'a>,
    pub theme: &'a Theme,
}

impl Widget for Content<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let frame = Style::default().fg(self.theme.frame);
        if area.width < 2 || area.height == 0 {
            return;
        }
        let start = self.lines.scroll;
        for row in 0..area.height {
            let y = area.y + row;
            buf.set_string(area.x, y, "│", frame);
            buf.set_string(area.right() - 1, y, "│", frame);
            if let Some(line) = self.lines.lines.get(start + usize::from(row)) {
                let clipped = super::clip_width(line, area.width - 2);
                buf.set_string(
                    area.x + 1,
                    y,
                    &clipped,
                    Style::default().fg(self.theme.text),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::NORTON;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(lines: &[String], scroll: usize, height: u16) -> String {
        let backend = TestBackend::new(24, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Content {
                    lines: &ContentLines {
                        lines: lines.to_vec().into(),
                        scroll,
                    },
                    theme: &NORTON,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    fn lines(n: u16) -> Vec<String> {
        (1..=n).map(|i| format!("line {i}")).collect()
    }

    #[test]
    fn renders_the_visible_window() {
        insta::assert_snapshot!(render(&lines(5), 2, 3));
    }

    #[test]
    fn short_documents_render_blank_rows_below() {
        insta::assert_snapshot!(render(&lines(1), 0, 3));
    }

    #[test]
    fn long_lines_are_clipped_to_the_interior() {
        let long = vec!["x".repeat(100)];
        insta::assert_snapshot!(render(&long, 0, 1));
    }
}

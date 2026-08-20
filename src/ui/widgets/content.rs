use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Widget};

pub struct ContentLines {
    pub lines: Vec<String>,
    pub scroll: u16,
}

pub struct Content<'a> {
    pub lines: &'a ContentLines,
}

impl Widget for Content<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let start = usize::from(self.lines.scroll);
        let visible: Vec<Line<'_>> = self
            .lines
            .lines
            .iter()
            .skip(start)
            .take(usize::from(area.height))
            .map(|line| Line::from(line.as_str()))
            .collect();
        Paragraph::new(Text::from(visible)).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(lines: &[String], scroll: u16, height: u16) -> String {
        let backend = TestBackend::new(24, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Content {
                    lines: &ContentLines {
                        lines: lines.to_vec(),
                        scroll,
                    },
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
}

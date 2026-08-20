use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use super::{clip_width, width};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabChip {
    pub title: String,
    pub url: String,
}

pub struct TabBar<'a> {
    pub tabs: &'a [TabChip],
    pub active: usize,
}

impl Widget for TabBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut x = 0u16;
        for (index, chip) in self.tabs.iter().enumerate() {
            if x >= area.width {
                break;
            }
            let fragment = format!(" {} ", chip.title);
            let fragment_width = width(&fragment);
            let remaining = area.width - x;
            if fragment_width > remaining {
                if remaining >= 2 {
                    let clipped = format!("{}»", clip_width(&fragment, remaining - 1));
                    buf.set_string(x, area.y, clipped, Style::default());
                }
                break;
            }
            let style = if index == self.active {
                Style::default().reversed()
            } else {
                Style::default()
            };
            buf.set_string(x, area.y, &fragment, style);
            x += fragment_width;
        }
        if x + 1 < area.width {
            buf.set_string(x, area.y, "+", Style::default().fg(Color::DarkGray));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(tabs: &[TabChip], active: usize, width: u16) -> String {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| TabBar { tabs, active }.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    fn chip(title: &str) -> TabChip {
        TabChip {
            title: title.to_string(),
            url: format!("https://{title}"),
        }
    }

    #[test]
    fn renders_chips_with_the_active_one_reversed() {
        let tabs = vec![chip("example.com"), chip("duckduckgo"), chip("wikipedia")];
        insta::assert_snapshot!(render(&tabs, 1, 40));
    }

    #[test]
    fn overflow_is_marked_with_a_chevron() {
        let tabs = vec![
            chip("example.com"),
            chip("duckduckgo"),
            chip("wikipedia"),
            chip("bluesky"),
        ];
        let line = render(&tabs, 1, 40);
        insta::assert_snapshot!(line);
        assert!(
            line.ends_with("bl»\n"),
            "last chip must be clipped with a chevron"
        );
    }

    #[test]
    fn empty_tab_row_shows_the_new_tab_hint() {
        let line = render(&[], 0, 40);
        assert!(line.starts_with('+'));
        assert!(!line.contains("»"));
    }

    #[test]
    fn active_chip_uses_reversed_style() {
        let tabs = vec![chip("a"), chip("b")];
        let backend = TestBackend::new(20, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                TabBar {
                    tabs: &tabs,
                    active: 1,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer().content();
        let reversed = ratatui::style::Modifier::REVERSED;
        assert!(
            cells[4].style().add_modifier.contains(reversed),
            "active chip must be reversed"
        );
        assert!(
            !cells[1].style().add_modifier.contains(reversed),
            "inactive chip must be plain"
        );
    }
}

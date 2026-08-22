use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::ui::theme::Theme;

use super::{clip_width, width};

pub const MAX_TAB_TITLE: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabChip<'a> {
    pub title: Cow<'a, str>,
    pub url: Cow<'a, str>,
}

pub struct TabBox {
    pub text: String,
    pub x: u16,
    pub width: u16,
    pub closed: bool,
    pub active: bool,
}

pub struct TabBar<'a> {
    pub tabs: &'a [TabChip<'a>],
    pub active: usize,
    pub theme: &'a Theme,
}

pub fn layout_tabs(tabs: &[TabChip<'_>], active: usize, inner: u16) -> Vec<TabBox> {
    let mut boxes = Vec::new();
    let mut x = 0u16;
    for (index, chip) in tabs.iter().enumerate() {
        let inner_text = format!(" {} ", clip_title(&chip.title));
        let box_width = 2 + width(&inner_text);
        let sep = if x == 0 { 0 } else { 1 };
        if x + sep + box_width > inner {
            let room = inner.saturating_sub(x + sep);
            if room >= 4 {
                let stub = format!("{}…»", clip_width(&inner_text, room.saturating_sub(3)));
                boxes.push(TabBox {
                    text: stub,
                    x: x + sep,
                    width: room,
                    closed: false,
                    active: index == active,
                });
            }
            return boxes;
        }
        let at = x + sep;
        boxes.push(TabBox {
            text: inner_text,
            x: at,
            width: box_width,
            closed: true,
            active: index == active,
        });
        x = at + box_width;
    }
    const HINT_WIDTH: u16 = 5;
    let sep = if x == 0 { 0 } else { 1 };
    if x + sep + HINT_WIDTH <= inner {
        boxes.push(TabBox {
            text: " + ".to_string(),
            x: x + sep,
            width: HINT_WIDTH,
            closed: true,
            active: false,
        });
    }
    boxes
}

pub fn active_span(tabs: &[TabChip<'_>], active: usize, inner: u16) -> Option<(u16, u16)> {
    layout_tabs(tabs, active, inner)
        .into_iter()
        .find(|b| b.active)
        .map(|b| (b.x, b.x + b.width - 1))
}

fn clip_title(title: &str) -> String {
    if width(title) as usize <= MAX_TAB_TITLE {
        title.to_string()
    } else {
        format!(
            "{}…",
            clip_width(title, MAX_TAB_TITLE.saturating_sub(1) as u16)
        )
    }
}

impl Widget for TabBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let frame = Style::default().fg(self.theme.frame);
        let text = Style::default().fg(self.theme.text);
        if area.width < 2 {
            return;
        }
        buf.set_string(area.x, area.y, "│", frame);
        buf.set_string(area.right() - 1, area.y, "│", frame);
        for tab_box in layout_tabs(self.tabs, self.active, area.width - 2) {
            let col = area.x + 1 + tab_box.x;
            if tab_box.active {
                let selected = self.theme.selected();
                buf.set_string(col, area.y, "┌", selected);
                buf.set_string(col + 1, area.y, &tab_box.text, selected);
                if tab_box.closed {
                    buf.set_string(col + 1 + width(&tab_box.text), area.y, "┐", selected);
                }
            } else {
                buf.set_string(col, area.y, "┌", frame);
                buf.set_string(col + 1, area.y, &tab_box.text, text);
                if tab_box.closed {
                    buf.set_string(col + 1 + width(&tab_box.text), area.y, "┐", frame);
                }
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

    fn render(tabs: &[TabChip], active: usize, width: u16) -> String {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                TabBar {
                    tabs,
                    active,
                    theme: &NORTON,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    fn chip(title: &str) -> TabChip<'static> {
        TabChip {
            title: title.to_string().into(),
            url: format!("https://{title}").into(),
        }
    }

    #[test]
    fn renders_tabs_as_raised_boxes() {
        let tabs = vec![chip("a"), chip("b"), chip("c")];
        insta::assert_snapshot!(render(&tabs, 1, 40));
    }

    #[test]
    fn raised_boxes_use_blank_gaps_without_separator_bars() {
        let tabs = vec![chip("a"), chip("b")];
        let line = render(&tabs, 0, 24);
        assert!(line.contains("┐ ┌"));
        assert!(!line.contains("┐│┌"));
    }

    #[test]
    fn overflow_is_marked_with_a_chevron() {
        let tabs = vec![
            chip("example.com"),
            chip("duckduckgo"),
            chip("wikipedia"),
            chip("bluesky"),
        ];
        let line = render(&tabs, 1, 37);
        insta::assert_snapshot!(line);
        assert!(line.contains('»'), "a clipped chip must carry a chevron");
        assert!(
            line.ends_with("│\n"),
            "the framed row must close with its right rail"
        );
    }

    #[test]
    fn empty_tab_row_shows_the_new_tab_hint() {
        let line = render(&[], 0, 40);
        assert!(line.contains('+'));
        assert!(!line.contains("»"));
        assert!(line.contains("┌ + ┐"));
    }

    #[test]
    fn active_box_uses_selected_style_and_inactive_boxes_stay_plain() {
        let tabs = vec![chip("a"), chip("b")];
        let backend = TestBackend::new(20, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                TabBar {
                    tabs: &tabs,
                    active: 1,
                    theme: &NORTON,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let cells = terminal.backend().buffer().content();
        let selected = NORTON.selected();
        let active_corner = (0..20)
            .map(|i| &cells[i])
            .find(|cell| cell.symbol() == "┌" && cell.style().bg == selected.bg);
        assert!(
            active_corner.is_some(),
            "the active box corner must carry the selected background"
        );
        let plain_corner = (0..20)
            .map(|i| &cells[i])
            .find(|cell| cell.symbol() == "┌" && cell.style().fg == Some(NORTON.frame));
        assert!(
            plain_corner.is_some(),
            "inactive box corners must be framed, not selected"
        );
    }

    #[test]
    fn long_titles_are_capped_to_the_box_maximum() {
        let tabs = vec![chip(&"x".repeat(100))];
        let boxes = layout_tabs(&tabs, 0, 78);
        assert_eq!(boxes.len(), 2, "a full box plus the hint box");
        let full = &boxes[0];
        assert!(full.closed);
        assert!(
            full.width as usize <= MAX_TAB_TITLE + 4,
            "box must not exceed the title cap plus corners and padding"
        );
        assert!(
            full.text.contains('…'),
            "a capped title carries an ellipsis"
        );
        assert!(full.active, "the single tab must be the active box");
    }

    #[test]
    fn partial_overflow_boxes_report_a_active_span() {
        let tabs = vec![chip(&"y".repeat(60)), chip("b")];
        let boxes = layout_tabs(&tabs, 1, 12);
        assert!(!boxes.is_empty());
        assert!(!boxes[0].closed, "the overflowing box is left open");
        assert!(!boxes[0].active, "tab 0 is not the active tab");
    }

    #[test]
    fn active_span_is_the_active_boxes_wall_columns() {
        let tabs = vec![chip("a"), chip("b"), chip("c")];
        assert_eq!(active_span(&tabs, 0, 40), Some((0, 4)));
        assert_eq!(active_span(&tabs, 1, 40), Some((6, 10)));
    }
}

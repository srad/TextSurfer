use std::borrow::Cow;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::ui::theme::Theme;

use super::{clip_width, width};

pub const MAX_TAB_TITLE: usize = 24;

/// The close box a chip carries, in the DOS window-control tradition.
const CLOSE: &str = "[■]";
const CLOSE_WIDTH: u16 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabChip<'a> {
    pub title: Cow<'a, str>,
    pub url: Cow<'a, str>,
}

/// What a laid-out box stands for. The strip ends with a new-tab hint that is not a
/// tab, so the position of a box in the vector is not its tab index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabSlot {
    Tab(usize),
    Close(usize),
    NewTab,
}

pub struct TabBox {
    pub text: String,
    pub x: u16,
    pub width: u16,
    /// Whether the box draws its own right corner, or runs off the end of the strip.
    pub capped: bool,
    /// The columns the close box covers, when the chip is whole enough to carry one.
    pub close: Option<std::ops::Range<u16>>,
    pub active: bool,
    pub slot: TabSlot,
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
        let text_width = width(&inner_text);
        let box_width = 2 + text_width + CLOSE_WIDTH;
        let sep = if x == 0 { 0 } else { 1 };
        if x + sep + box_width > inner {
            let room = inner.saturating_sub(x + sep);
            if room >= 4 {
                let stub = format!("{}…»", clip_width(&inner_text, room.saturating_sub(3)));
                boxes.push(TabBox {
                    text: stub,
                    x: x + sep,
                    width: room,
                    capped: false,
                    close: None,
                    active: index == active,
                    slot: TabSlot::Tab(index),
                });
            }
            return boxes;
        }
        let at = x + sep;
        let close_at = at + 1 + text_width;
        boxes.push(TabBox {
            text: inner_text,
            x: at,
            width: box_width,
            capped: true,
            close: Some(close_at..close_at + CLOSE_WIDTH),
            active: index == active,
            slot: TabSlot::Tab(index),
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
            capped: true,
            close: None,
            active: false,
            slot: TabSlot::NewTab,
        });
    }
    boxes
}

/// The slot covering `col`, measured from the strip's interior left edge.
///
/// A chip's close box is tested first: it lies inside the chip, and clicking it means
/// something other than clicking the chip.
pub fn tab_at(tabs: &[TabChip<'_>], active: usize, inner: u16, col: u16) -> Option<TabSlot> {
    layout_tabs(tabs, active, inner)
        .into_iter()
        .find(|tab_box| col >= tab_box.x && col < tab_box.x + tab_box.width)
        .map(|tab_box| match (&tab_box.close, tab_box.slot) {
            (Some(close), TabSlot::Tab(index)) if close.contains(&col) => TabSlot::Close(index),
            _ => tab_box.slot,
        })
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
        let dim = Style::default().fg(self.theme.dim);
        let selected = self.theme.selected();
        for tab_box in layout_tabs(self.tabs, self.active, area.width - 2) {
            let col = area.x + 1 + tab_box.x;
            let (wall, label, close_style) = if tab_box.active {
                (selected, selected, selected)
            } else {
                (frame, text, dim)
            };
            buf.set_string(col, area.y, "┌", wall);
            buf.set_string(col + 1, area.y, &tab_box.text, label);
            if let Some(close) = tab_box.close.as_ref() {
                buf.set_string(area.x + 1 + close.start, area.y, CLOSE, close_style);
            }
            if tab_box.capped {
                let right = tab_box
                    .close
                    .as_ref()
                    .map_or(col + 1 + width(&tab_box.text), |close| {
                        area.x + 1 + close.end
                    });
                buf.set_string(right, area.y, "┐", wall);
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
        let line = render(&tabs, 1, 44);
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
        assert!(full.capped);
        assert!(
            full.width as usize <= MAX_TAB_TITLE + 4 + usize::from(CLOSE_WIDTH),
            "box must not exceed the title cap plus corners, padding and the close box"
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
        assert!(!boxes[0].capped, "the overflowing box is left open");
        assert!(!boxes[0].active, "tab 0 is not the active tab");
    }

    #[test]
    fn active_span_is_the_active_boxes_wall_columns() {
        let tabs = vec![chip("a"), chip("b"), chip("c")];
        assert_eq!(active_span(&tabs, 0, 40), Some((0, 7)));
        assert_eq!(active_span(&tabs, 1, 40), Some((9, 16)));
    }

    #[test]
    fn every_whole_chip_carries_a_close_box_and_nothing_else_does() {
        let tabs = vec![chip("a"), chip("b")];
        let boxes = layout_tabs(&tabs, 0, 40);
        for (index, tab_box) in boxes.iter().enumerate() {
            let close = tab_box.close.as_ref();
            match tab_box.slot {
                TabSlot::Tab(_) => {
                    let close = close.expect("a whole chip carries a close box");
                    assert_eq!(close.end - close.start, CLOSE_WIDTH);
                    assert!(
                        close.end < tab_box.x + tab_box.width,
                        "the close box sits inside the chip's right wall",
                    );
                }
                _ => assert!(close.is_none(), "box {index} must not close a tab"),
            }
        }
    }

    #[test]
    fn a_clipped_chip_has_no_close_box_to_press() {
        let tabs = vec![chip(&"y".repeat(60)), chip("b")];
        let boxes = layout_tabs(&tabs, 0, 20);
        let stub = boxes.last().expect("an overflow stub");
        assert!(!stub.capped);
        assert!(stub.close.is_none());
    }

    #[test]
    fn the_close_box_resolves_to_its_own_slot() {
        let tabs = vec![chip("a"), chip("b")];
        let boxes = layout_tabs(&tabs, 0, 40);
        let close = boxes[1].close.clone().expect("a close box on the second");
        for col in close.clone() {
            assert_eq!(
                tab_at(&tabs, 0, 40, col),
                Some(TabSlot::Close(1)),
                "column {col}",
            );
        }
        assert_eq!(tab_at(&tabs, 0, 40, close.start - 1), Some(TabSlot::Tab(1)));
        assert_eq!(tab_at(&tabs, 0, 40, close.end), Some(TabSlot::Tab(1)));
    }

    #[test]
    fn the_close_box_is_drawn_dim_on_an_inactive_chip_and_selected_on_the_active_one() {
        let tabs = vec![chip("a"), chip("b")];
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                TabBar {
                    tabs: &tabs,
                    active: 0,
                    theme: &NORTON,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        let boxes = layout_tabs(&tabs, 0, 38);
        let buffer = terminal.backend().buffer();
        let mark = |index: usize| {
            let close = boxes[index].close.clone().expect("a close box");
            buffer[(1 + close.start + 1, 0)].clone()
        };
        assert_eq!(mark(0).symbol(), "■");
        assert_eq!(mark(0).style().bg, NORTON.selected().bg);
        assert_eq!(mark(1).symbol(), "■");
        assert_eq!(mark(1).style().fg, Some(NORTON.dim));
    }
}

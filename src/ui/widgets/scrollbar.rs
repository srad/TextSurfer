use std::ops::Range;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::ui::theme::Theme;

const ARROW_UP: &str = "▲";
const ARROW_DOWN: &str = "▼";
const TRACK: &str = "▒";
const THUMB: &str = "█";

/// The shortest bar with room for a cap at each end and a track between them.
const CAPPED_ROWS: u16 = 3;

/// How much document there is, how much of it shows, and where the window sits.
///
/// The three numbers travel together because two of them are `usize` and a call site
/// that took them apart could swap them silently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollExtent {
    pub rows: u16,
    pub doc_rows: usize,
    pub scroll: usize,
}

/// Where the bar's parts land, in rows measured from the top of the bar.
///
/// `max_scroll` rides along so the inverse map needs no second argument: a
/// `(metrics, extent)` pair at a call site is a pair that can disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metrics {
    pub track: Range<u16>,
    pub thumb: Range<u16>,
    pub max_scroll: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarPart {
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    Thumb,
}

pub struct Scrollbar<'a> {
    pub extent: ScrollExtent,
    pub theme: &'a Theme,
}

impl ScrollExtent {
    /// Resolve the bar's geometry.
    ///
    /// A document that fits — including one that has not been painted yet, which is a
    /// real state and would otherwise divide by zero — gets a thumb filling the track.
    pub fn metrics(self) -> Metrics {
        let track = if self.rows >= CAPPED_ROWS {
            1..self.rows - 1
        } else {
            0..self.rows
        };
        let track_len = track.end - track.start;
        let visible = usize::from(self.rows);
        let max_scroll = self.doc_rows.saturating_sub(visible);
        if max_scroll == 0 || track_len == 0 || self.doc_rows == 0 {
            return Metrics {
                thumb: track.clone(),
                track,
                max_scroll: 0,
            };
        }
        let thumb_len = ((usize::from(track_len) * visible) / self.doc_rows)
            .clamp(1, usize::from(track_len)) as u16;
        let travel = track_len - thumb_len;
        if travel == 0 {
            return Metrics {
                thumb: track.clone(),
                track,
                max_scroll,
            };
        }
        let offset = self.scroll.min(max_scroll) * usize::from(travel) / max_scroll;
        let start = track.start + offset as u16;
        Metrics {
            thumb: start..start + thumb_len,
            track,
            max_scroll,
        }
    }
}

impl Metrics {
    pub fn part_at(&self, row: u16) -> ScrollbarPart {
        if row < self.track.start {
            ScrollbarPart::LineUp
        } else if row >= self.track.end {
            ScrollbarPart::LineDown
        } else if row < self.thumb.start {
            ScrollbarPart::PageUp
        } else if row >= self.thumb.end {
            ScrollbarPart::PageDown
        } else {
            ScrollbarPart::Thumb
        }
    }

    /// The scroll offset that puts the thumb's top edge on `top`.
    ///
    /// Placement in [`ScrollExtent::metrics`] rounds down, so several scroll offsets
    /// share a thumb row. Rounding *up* here picks the first of them, which is the only
    /// choice that round-trips: the thumb lands back on the row the pointer named.
    pub fn scroll_for_thumb_top(&self, top: u16) -> usize {
        let travel = (self.track.end - self.track.start) - (self.thumb.end - self.thumb.start);
        if travel == 0 || self.max_scroll == 0 {
            return 0;
        }
        let offset = usize::from(top.saturating_sub(self.track.start).min(travel));
        (offset * self.max_scroll).div_ceil(usize::from(travel))
    }
}

impl Widget for Scrollbar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let metrics = self.extent.metrics();
        let frame = Style::default().fg(self.theme.frame).bg(self.theme.bg);
        let thumb = Style::default().fg(self.theme.text).bg(self.theme.bg);
        for row in 0..area.height {
            let (symbol, style) = match metrics.part_at(row) {
                ScrollbarPart::LineUp => (ARROW_UP, frame),
                ScrollbarPart::LineDown => (ARROW_DOWN, frame),
                ScrollbarPart::Thumb => (THUMB, thumb),
                ScrollbarPart::PageUp | ScrollbarPart::PageDown => (TRACK, frame),
            };
            buf.set_string(area.x, area.y + row, symbol, style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::DEFAULT;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn extent(rows: u16, doc_rows: usize, scroll: usize) -> ScrollExtent {
        ScrollExtent {
            rows,
            doc_rows,
            scroll,
        }
    }

    #[test]
    fn a_document_that_fits_fills_the_track() {
        let metrics = extent(18, 10, 0).metrics();
        assert_eq!(metrics.track, 1..17);
        assert_eq!(metrics.thumb, metrics.track);
        assert_eq!(metrics.max_scroll, 0);
    }

    #[test]
    fn an_unpainted_document_does_not_divide_by_zero() {
        let metrics = extent(18, 0, 0).metrics();
        assert_eq!(metrics.thumb, metrics.track);
        assert_eq!(metrics.max_scroll, 0);
    }

    #[test]
    fn the_thumb_sits_flush_at_both_ends_of_the_track() {
        let top = extent(18, 100, 0).metrics();
        assert_eq!(top.track.start, top.thumb.start);
        let bottom = extent(18, 100, 82).metrics();
        assert_eq!(bottom.max_scroll, 82);
        assert_eq!(bottom.track.end, bottom.thumb.end);
    }

    #[test]
    fn the_thumb_never_leaves_the_track_and_never_shrinks_away() {
        for doc_rows in [19usize, 40, 500, 10_000] {
            for scroll in 0..=doc_rows - 18 {
                let metrics = extent(18, doc_rows, scroll).metrics();
                assert!(metrics.thumb.start >= metrics.track.start);
                assert!(metrics.thumb.end <= metrics.track.end);
                assert!(metrics.thumb.end > metrics.thumb.start);
            }
        }
    }

    #[test]
    fn the_thumb_advances_monotonically_with_the_scroll() {
        let mut previous = 0;
        for scroll in 0..=82 {
            let start = extent(18, 100, scroll).metrics().thumb.start;
            assert!(
                start >= previous,
                "thumb moved backwards at scroll {scroll}"
            );
            previous = start;
        }
    }

    #[test]
    fn thumb_travel_never_exceeds_the_scrollable_distance() {
        // At most one thumb row per scrollable row, so no reachable thumb position is
        // skipped as the wheel walks the document.
        for rows in [3u16, 8, 18, 60] {
            for doc_rows in [usize::from(rows) + 1, usize::from(rows) * 3, 10_000] {
                let metrics = extent(rows, doc_rows, 0).metrics();
                let travel = (metrics.track.end - metrics.track.start)
                    - (metrics.thumb.end - metrics.thumb.start);
                assert!(
                    usize::from(travel) <= metrics.max_scroll,
                    "rows {rows}, doc {doc_rows}: travel {travel} > max {}",
                    metrics.max_scroll,
                );
            }
        }
    }

    #[test]
    fn dragging_to_a_row_and_reading_it_back_lands_on_the_same_row() {
        for scroll in 0..=82usize {
            let metrics = extent(18, 100, scroll).metrics();
            let read_back = metrics.scroll_for_thumb_top(metrics.thumb.start);
            assert_eq!(
                extent(18, 100, read_back).metrics().thumb.start,
                metrics.thumb.start,
                "scroll {scroll} did not round-trip",
            );
        }
    }

    #[test]
    fn a_drag_past_either_end_clamps_to_the_document() {
        let metrics = extent(18, 100, 0).metrics();
        assert_eq!(metrics.scroll_for_thumb_top(0), 0);
        assert_eq!(metrics.scroll_for_thumb_top(u16::MAX), 82);
    }

    #[test]
    fn parts_split_into_caps_trough_and_thumb() {
        let metrics = extent(18, 100, 41).metrics();
        assert_eq!(metrics.part_at(0), ScrollbarPart::LineUp);
        assert_eq!(metrics.part_at(17), ScrollbarPart::LineDown);
        assert_eq!(metrics.part_at(metrics.thumb.start), ScrollbarPart::Thumb);
        assert_eq!(
            metrics.part_at(metrics.thumb.start - 1),
            ScrollbarPart::PageUp
        );
        assert_eq!(metrics.part_at(metrics.thumb.end), ScrollbarPart::PageDown);
    }

    #[test]
    fn a_bar_too_short_for_caps_is_all_track() {
        let metrics = extent(2, 100, 0).metrics();
        assert_eq!(metrics.track, 0..2);
        assert_eq!(metrics.part_at(0), ScrollbarPart::Thumb);
        assert_ne!(metrics.part_at(1), ScrollbarPart::LineDown);
    }

    fn render(extent: ScrollExtent) -> String {
        let backend = TestBackend::new(1, extent.rows);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                Scrollbar {
                    extent,
                    theme: &DEFAULT,
                }
                .render(frame.area(), frame.buffer_mut())
            })
            .unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    #[test]
    fn renders_caps_track_and_thumb() {
        insta::assert_snapshot!(render(extent(10, 40, 12)));
    }

    #[test]
    fn a_page_that_fits_renders_a_full_thumb_between_the_caps() {
        insta::assert_snapshot!(render(extent(10, 4, 0)));
    }
}

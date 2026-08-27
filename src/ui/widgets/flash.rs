//! The flash notice: one line in front of the page, for a moment.
//!
//! It sits over the content rather than in the status bar because the status bar is where
//! a page's own load messages go, and something the reader just asked for — a saved
//! screenshot, or the reason there is none — has to be seen even when it says the same
//! thing twice in a row.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Clear, Widget};

use crate::ui::theme::Theme;

use super::{clip_width, width};

pub struct Flash<'a> {
    pub message: &'a str,
    pub theme: &'a Theme,
}

/// The box a flash takes inside `content`, or `None` when the content is too small to
/// hold one. Centred across the page, one row down from its top.
pub fn flash_rect(content: Rect, message: &str) -> Option<Rect> {
    if content.width < 6 || content.height < 3 {
        return None;
    }
    let text = width(message).clamp(1, content.width - 4);
    let box_width = text + 4;
    Some(Rect {
        x: content.x + (content.width - box_width) / 2,
        y: content.y + u16::from(content.height >= 4),
        width: box_width,
        height: 3,
    })
}

impl Widget for Flash<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 3 {
            return;
        }
        let style = self.theme.selected();
        let block = Block::bordered().border_style(style).style(style);
        let inner = block.inner(area);
        Clear.render(area, buf);
        block.render(area, buf);
        let text = clip_width(self.message, inner.width);
        let left = inner.x + (inner.width - width(&text)) / 2;
        buf.set_string(left, inner.y, &text, style);
    }
}

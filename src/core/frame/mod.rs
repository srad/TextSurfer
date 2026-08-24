use std::ops::Range;

mod scheduler;

pub use scheduler::{EVENTS_PER_FRAME, FRAME_INTERVAL, FrameScheduler};

const MAX_ROW_RANGES: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChromeDamage {
    #[default]
    None,
    Status,
    Full,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RowDamage {
    #[default]
    None,
    Ranges(Vec<Range<usize>>),
    Full,
}

impl RowDamage {
    pub fn add(&mut self, range: Range<usize>) {
        if range.is_empty() || matches!(self, Self::Full) {
            return;
        }
        let Self::Ranges(ranges) = self else {
            *self = Self::Ranges(vec![range]);
            return;
        };
        ranges.push(range);
        ranges.sort_unstable_by_key(|range| range.start);
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
        for range in ranges.drain(..) {
            if let Some(previous) = merged.last_mut()
                && range.start <= previous.end
            {
                previous.end = previous.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        if merged.len() > MAX_ROW_RANGES {
            *self = Self::Full;
        } else {
            *ranges = merged;
        }
    }

    pub fn merge(&mut self, other: Self) {
        match other {
            Self::None => {}
            Self::Full => *self = Self::Full,
            Self::Ranges(ranges) => {
                for range in ranges {
                    self.add(range);
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentDamage {
    pub scroll_rows: i32,
    pub repaint: RowDamage,
    pub full: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameDamage {
    pub chrome: ChromeDamage,
    pub content: ContentDamage,
}

impl FrameDamage {
    pub const fn full() -> Self {
        Self {
            chrome: ChromeDamage::Full,
            content: ContentDamage {
                scroll_rows: 0,
                repaint: RowDamage::Full,
                full: true,
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.chrome == ChromeDamage::None
            && self.content.scroll_rows == 0
            && self.content.repaint == RowDamage::None
            && !self.content.full
    }

    pub fn damage_chrome(&mut self, damage: ChromeDamage) {
        self.chrome = self.chrome.max(damage);
    }

    pub fn scroll(&mut self, rows: i32) {
        if !self.content.full {
            self.content.scroll_rows = self.content.scroll_rows.saturating_add(rows);
        }
    }

    pub fn repaint_rows(&mut self, range: Range<usize>) {
        if !self.content.full {
            self.content.repaint.add(range);
        }
    }

    pub fn repaint_content(&mut self) {
        self.content.full = true;
        self.content.scroll_rows = 0;
        self.content.repaint = RowDamage::None;
    }

    pub fn repaint_all(&mut self) {
        self.damage_chrome(ChromeDamage::Full);
        self.repaint_content();
    }

    pub fn merge(&mut self, other: Self) {
        self.damage_chrome(other.chrome);
        if other.content.full {
            self.repaint_content();
        } else if !self.content.full {
            self.scroll(other.content.scroll_rows);
            self.content.repaint.merge(other.content.repaint);
        }
    }
}

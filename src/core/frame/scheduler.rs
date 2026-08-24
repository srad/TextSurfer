use std::time::Duration;

use crate::core::event::{InputBatch, InputEvent};

pub const FRAME_INTERVAL: Duration = Duration::from_micros(16_667);
pub const EVENTS_PER_FRAME: usize = 256;

#[derive(Debug, Default)]
pub struct FrameScheduler {
    pending: InputBatch,
    due: Option<Duration>,
    last_frame: Option<Duration>,
}

impl FrameScheduler {
    pub fn push(&mut self, event: InputEvent, now: Duration) {
        if self.pending.is_empty() {
            self.due = Some(match self.last_frame {
                Some(last) => now.max(last.saturating_add(FRAME_INTERVAL)),
                None => now,
            });
        }
        self.pending.push(event);
    }

    pub fn is_due(&self, now: Duration) -> bool {
        self.due.is_some_and(|due| due <= now)
    }

    pub fn take_due(&mut self, now: Duration) -> Option<InputBatch> {
        if !self.is_due(now) {
            return None;
        }
        let batch = self.pending.take_prefix(EVENTS_PER_FRAME);
        self.last_frame = Some(now);
        self.due = (!self.pending.is_empty()).then_some(now.saturating_add(FRAME_INTERVAL));
        Some(batch)
    }

    pub fn next_deadline(&self, app_deadline: Option<Duration>) -> Option<Duration> {
        match (self.due, app_deadline) {
            (Some(input), Some(app)) => Some(input.min(app)),
            (Some(input), None) => Some(input),
            (None, app) => app,
        }
    }

    pub fn has_pending_input(&self) -> bool {
        !self.pending.is_empty()
    }
}

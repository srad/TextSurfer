//! The flash notice: what a frontend has to tell the reader right now.
//!
//! The controller can neither draw nor write files, so a frontend that has just saved a
//! screenshot — or failed to — reports through here. The notice carries its own deadline
//! rather than a frame count, because the two frontends redraw on entirely different
//! schedules; `next_wake` treats that deadline like any other and wakes the loop to take
//! the box down.

use std::time::Duration;

use super::App;

/// How long a notice stays in front of the page.
pub const FLASH: Duration = Duration::from_secs(3);

pub(super) struct FlashNotice {
    pub message: String,
    pub until: Duration,
}

impl App {
    /// Show `message` in front of the page for [`FLASH`], and leave it on the status bar
    /// as the record once the box is gone.
    pub fn flash(&mut self, message: String) {
        self.tabs.active_mut().message = message.clone();
        self.flash = Some(FlashNotice {
            message,
            until: self.now.saturating_add(FLASH),
        });
        self.touch();
    }

    /// The live notice, for the view to draw.
    pub fn flash_message(&self) -> Option<&str> {
        self.flash.as_ref().map(|flash| flash.message.as_str())
    }

    pub(super) fn flash_deadline(&self) -> Option<Duration> {
        self.flash.as_ref().map(|flash| flash.until)
    }

    pub(super) fn expire_flash(&mut self, now: Duration) {
        if self.flash.as_ref().is_some_and(|flash| now >= flash.until) {
            self.flash = None;
            self.touch();
        }
    }
}

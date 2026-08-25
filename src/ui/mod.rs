pub mod chrome;
pub mod editing;
pub mod frame;
pub mod keymap;
pub mod mouse;
pub mod theme;
pub mod widgets;

#[cfg(test)]
pub mod test_util;

pub use chrome::ChromeView;
pub use editing::EditBuffer;
pub use keymap::{Action, DefaultKeymap, Keymap};
pub use mouse::{ChromeGeometry, MouseZone};
pub use theme::{
    AMBER_CRT, DEFAULT, DEFAULT_THEME_INDEX, GREEN_PHOSPHOR, NORTON, PAPER_WHITE, THEME_NAMES,
    THEMES, TURBO_VISION, Theme, ThemeAppearance,
};
pub use widgets::content::ContentLines;
pub use widgets::status::StatusView;
pub use widgets::tabs::TabChip;

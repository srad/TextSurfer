//! The framebuffer frontend: the chrome rendered into a window with our own font.
//!
//! This is the default interactive frontend; the terminal remains a compatibility fallback. It exists
//! because the DOS look is mostly the *font*, and inside a terminal emulator the font
//! belongs to the user: `ui::Theme` fixes the palette, but every glyph renders in
//! whatever face the terminal was configured with. Owning a framebuffer means owning
//! the face, the cell metric and the palette together.
//!
//! The seam it plugs into already existed. `ui::chrome::draw` takes a backend-agnostic
//! ratatui `Frame`, and `app` never imports a terminal library, so this module reaches
//! the whole existing UI by implementing ratatui's `Backend` and mapping window events
//! onto `core::event` types — exactly what `main.rs` does for crossterm.
//!
//! Everything here except `window`, and `capture`'s writing half, is pure and tested
//! without opening a window.

pub mod backend;
pub mod capture;
pub mod font;
mod image;
pub mod input;
mod surface;
mod window;

#[cfg(test)]
mod tests;

pub use backend::VgaBackend;
pub use surface::{CellState, Surface, SurfaceConfig};
pub use window::{VgaOptions, run, run_with_scripts};

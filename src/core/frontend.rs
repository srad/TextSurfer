//! Which frontend is presenting the browser.
//!
//! The same page does not rasterise the same way in both: the terminal has no scaled
//! headings and no true raster images, the window has both. A screenshot therefore
//! carries the frontend that produced it in its name, so two captures of one page never
//! collide or get mistaken for each other.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frontend {
    Terminal,
    Vga,
}

impl Frontend {
    pub const fn slug(self) -> &'static str {
        match self {
            Frontend::Terminal => "terminal",
            Frontend::Vga => "vga",
        }
    }
}

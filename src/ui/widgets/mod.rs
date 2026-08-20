pub mod content;
pub mod menu;
pub mod status;
pub mod tabs;
pub mod toolbar;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn clip_width(s: &str, max: u16) -> String {
    let mut out = String::new();
    let mut width = 0u16;
    for ch in s.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
        if width + ch_width > max {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out
}

pub fn width(s: &str) -> u16 {
    UnicodeWidthStr::width(s) as u16
}

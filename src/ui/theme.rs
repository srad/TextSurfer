use ratatui::style::{Color, Style};

use crate::core::style::{Palette, Rgb};

pub struct Theme {
    pub frame: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub hover: Color,
    pub bg: Color,
    pub bar_bg: Color,
    pub bar_text: Color,
    pub mnemonic: Color,
}

impl Theme {
    pub fn selected(&self) -> Style {
        Style::default().fg(Color::Black).bg(Color::White)
    }

    pub fn palette(&self) -> Palette {
        Palette {
            text: rgb_of(self.text),
            background: rgb_of(self.bg),
            link: rgb_of(self.accent),
            link_hover: rgb_of(self.hover),
        }
    }
}

pub fn rgb_of(color: Color) -> Rgb {
    match color {
        Color::Rgb(r, g, b) => Rgb::new(r, g, b),
        Color::Black | Color::Reset => Rgb::new(0, 0, 0),
        Color::Red => Rgb::new(128, 0, 0),
        Color::Green => Rgb::new(0, 128, 0),
        Color::Yellow => Rgb::new(128, 128, 0),
        Color::Blue => Rgb::new(0, 0, 128),
        Color::Magenta => Rgb::new(128, 0, 128),
        Color::Cyan => Rgb::new(0, 128, 128),
        Color::Gray => Rgb::new(192, 192, 192),
        Color::DarkGray => Rgb::new(128, 128, 128),
        Color::LightRed => Rgb::new(255, 0, 0),
        Color::LightGreen => Rgb::new(0, 255, 0),
        Color::LightYellow => Rgb::new(255, 255, 0),
        Color::LightBlue => Rgb::new(0, 0, 255),
        Color::LightMagenta => Rgb::new(255, 0, 255),
        Color::LightCyan => Rgb::new(0, 255, 255),
        Color::White => Rgb::new(255, 255, 255),
        Color::Indexed(index) => indexed_rgb(index),
    }
}

fn indexed_rgb(index: u8) -> Rgb {
    match index {
        0..=15 => rgb_of(match index {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::Gray,
            8 => Color::DarkGray,
            9 => Color::LightRed,
            10 => Color::LightGreen,
            11 => Color::LightYellow,
            12 => Color::LightBlue,
            13 => Color::LightMagenta,
            14 => Color::LightCyan,
            _ => Color::White,
        }),
        16..=231 => {
            let value = index - 16;
            let level = |component: u8| match component {
                0 => 0,
                other => 55 + other * 40,
            };
            Rgb::new(level(value / 36), level((value % 36) / 6), level(value % 6))
        }
        _ => {
            let grey = 8 + (index - 232) * 10;
            Rgb::new(grey, grey, grey)
        }
    }
}

pub const NORTON: Theme = Theme {
    frame: Color::LightBlue,
    text: Color::White,
    dim: Color::Cyan,
    accent: Color::Yellow,
    hover: Color::LightCyan,
    bg: Color::Rgb(0, 0, 128),
    bar_bg: Color::Gray,
    bar_text: Color::Black,
    mnemonic: Color::Red,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norton_palette_is_congruent() {
        assert_ne!(NORTON.frame, NORTON.text);
        assert_ne!(NORTON.text, NORTON.dim);
        assert_ne!(NORTON.accent, NORTON.text);
        assert_ne!(NORTON.hover, NORTON.text);
        assert_ne!(NORTON.hover, NORTON.bg);
        assert_ne!(NORTON.hover, NORTON.accent);
        assert_ne!(NORTON.bg, NORTON.text);
        assert_ne!(NORTON.bar_bg, NORTON.bg);
        assert_ne!(NORTON.bar_text, NORTON.bar_bg);
        assert_ne!(NORTON.mnemonic, NORTON.bar_text);
        assert_ne!(NORTON.mnemonic, NORTON.bar_bg);
    }

    #[test]
    fn selected_is_explicit_black_on_white() {
        let style = NORTON.selected();
        assert_eq!(style.fg, Some(Color::Black));
        assert_eq!(style.bg, Some(Color::White));
    }
}

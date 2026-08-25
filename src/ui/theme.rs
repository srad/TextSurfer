use ratatui::style::{Color, Style};

use crate::core::style::{Palette, Rgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub appearance: ThemeAppearance,
    pub frame: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub hover: Color,
    pub bg: Color,
    pub bar_bg: Color,
    pub bar_text: Color,
    pub mnemonic: Color,
    pub selected_text: Color,
    pub selected_bg: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeAppearance {
    Dark,
    Light,
}

impl Theme {
    pub fn selected(&self) -> Style {
        Style::default().fg(self.selected_text).bg(self.selected_bg)
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
    name: "Norton",
    appearance: ThemeAppearance::Dark,
    frame: Color::LightBlue,
    text: Color::White,
    dim: Color::Cyan,
    accent: Color::Yellow,
    hover: Color::LightCyan,
    bg: Color::Rgb(0, 0, 128),
    bar_bg: Color::Gray,
    bar_text: Color::Black,
    mnemonic: Color::Red,
    selected_text: Color::Black,
    selected_bg: Color::White,
};

pub const TURBO_VISION: Theme = Theme {
    name: "Turbo Vision",
    appearance: ThemeAppearance::Dark,
    frame: Color::Rgb(85, 255, 255),
    text: Color::Rgb(255, 255, 85),
    dim: Color::Rgb(0, 170, 170),
    accent: Color::Rgb(85, 255, 85),
    hover: Color::Rgb(255, 255, 255),
    bg: Color::Rgb(0, 0, 170),
    bar_bg: Color::Rgb(170, 170, 170),
    bar_text: Color::Rgb(0, 0, 0),
    mnemonic: Color::Rgb(170, 0, 0),
    selected_text: Color::Rgb(0, 0, 0),
    selected_bg: Color::Rgb(0, 170, 0),
};

pub const AMBER_CRT: Theme = Theme {
    name: "Amber CRT",
    appearance: ThemeAppearance::Dark,
    frame: Color::Rgb(255, 209, 102),
    text: Color::Rgb(255, 176, 0),
    dim: Color::Rgb(128, 88, 0),
    accent: Color::Rgb(255, 224, 102),
    hover: Color::Rgb(255, 255, 255),
    bg: Color::Rgb(0, 0, 0),
    bar_bg: Color::Rgb(255, 176, 0),
    bar_text: Color::Rgb(0, 0, 0),
    mnemonic: Color::Rgb(92, 46, 0),
    selected_text: Color::Rgb(0, 0, 0),
    selected_bg: Color::Rgb(255, 241, 184),
};

pub const GREEN_PHOSPHOR: Theme = Theme {
    name: "Green Phosphor",
    appearance: ThemeAppearance::Dark,
    frame: Color::Rgb(102, 255, 102),
    text: Color::Rgb(51, 255, 51),
    dim: Color::Rgb(22, 138, 22),
    accent: Color::Rgb(176, 255, 176),
    hover: Color::Rgb(255, 255, 255),
    bg: Color::Rgb(0, 16, 0),
    bar_bg: Color::Rgb(51, 255, 51),
    bar_text: Color::Rgb(0, 16, 0),
    mnemonic: Color::Rgb(0, 59, 0),
    selected_text: Color::Rgb(0, 16, 0),
    selected_bg: Color::Rgb(176, 255, 176),
};

pub const PAPER_WHITE: Theme = Theme {
    name: "Paper White",
    appearance: ThemeAppearance::Light,
    frame: Color::Rgb(32, 56, 100),
    text: Color::Rgb(16, 16, 16),
    dim: Color::Rgb(102, 98, 90),
    accent: Color::Rgb(0, 71, 171),
    hover: Color::Rgb(139, 26, 26),
    bg: Color::Rgb(232, 228, 216),
    bar_bg: Color::Rgb(31, 54, 92),
    bar_text: Color::Rgb(255, 255, 255),
    mnemonic: Color::Rgb(255, 215, 95),
    selected_text: Color::Rgb(255, 255, 255),
    selected_bg: Color::Rgb(0, 90, 156),
};

pub const THEMES: [Theme; 5] = [TURBO_VISION, NORTON, AMBER_CRT, GREEN_PHOSPHOR, PAPER_WHITE];
pub const THEME_NAMES: [&str; 5] = [
    "Turbo Vision",
    "Norton",
    "Amber CRT",
    "Green Phosphor",
    "Paper White",
];
pub const DEFAULT_THEME_INDEX: usize = 0;

pub const DEFAULT: Theme = THEMES[DEFAULT_THEME_INDEX];

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_congruent(theme: &Theme) {
        assert_ne!(theme.frame, theme.text);
        assert_ne!(theme.text, theme.dim);
        assert_ne!(theme.accent, theme.text);
        assert_ne!(theme.hover, theme.text);
        assert_ne!(theme.hover, theme.bg);
        assert_ne!(theme.hover, theme.accent);
        assert_ne!(theme.bg, theme.text);
        assert_ne!(theme.bar_bg, theme.bg);
        assert_ne!(theme.bar_text, theme.bar_bg);
        assert_ne!(theme.mnemonic, theme.bar_text);
        assert_ne!(theme.mnemonic, theme.bar_bg);
        assert_ne!(theme.selected_text, theme.selected_bg);
        assert_ne!(theme.selected_bg, theme.bg);
    }

    #[test]
    fn norton_palette_is_congruent() {
        assert_congruent(&NORTON);
    }

    #[test]
    fn turbo_vision_palette_is_congruent() {
        assert_congruent(&TURBO_VISION);
    }

    #[test]
    fn every_registered_theme_is_congruent_and_renderer_safe() {
        assert_eq!(THEMES.map(|theme| theme.name), THEME_NAMES);
        assert_eq!(THEMES[DEFAULT_THEME_INDEX], DEFAULT);
        for theme in THEMES {
            assert_congruent(&theme);
            let palette = theme.palette();
            for foreground in [palette.text, palette.link, palette.link_hover] {
                assert!(
                    foreground.contrast_ratio(palette.background) >= 3.0,
                    "{}: {foreground:?} on {:?}",
                    theme.name,
                    palette.background
                );
            }
        }
    }

    #[test]
    fn new_themes_keep_enhanced_text_and_link_contrast() {
        for theme in [TURBO_VISION, AMBER_CRT, GREEN_PHOSPHOR, PAPER_WHITE] {
            let palette = theme.palette();
            for foreground in [palette.text, palette.link, palette.link_hover] {
                assert!(
                    foreground.contrast_ratio(palette.background) >= 4.5,
                    "{}: {foreground:?} on {:?}",
                    theme.name,
                    palette.background
                );
            }
        }
    }

    #[test]
    fn only_paper_white_requests_light_page_colors() {
        for theme in THEMES {
            assert_eq!(
                theme.appearance == ThemeAppearance::Light,
                theme.name == "Paper White"
            );
        }
    }

    #[test]
    fn selected_comes_from_the_theme() {
        let style = NORTON.selected();
        assert_eq!(style.fg, Some(Color::Black));
        assert_eq!(style.bg, Some(Color::White));
        let turbo = TURBO_VISION.selected();
        assert_eq!(turbo.fg, Some(TURBO_VISION.selected_text));
        assert_eq!(turbo.bg, Some(TURBO_VISION.selected_bg));
    }

    #[test]
    fn turbo_vision_text_and_links_are_legible_on_the_desktop() {
        let palette = TURBO_VISION.palette();
        for foreground in [palette.text, palette.link, palette.link_hover] {
            assert!(
                foreground.contrast_ratio(palette.background) >= 4.5,
                "{foreground:?} on {:?}",
                palette.background
            );
        }
    }

    #[test]
    fn default_is_turbo_vision() {
        assert_eq!(DEFAULT.name, "Turbo Vision");
        assert_eq!(DEFAULT.bg, TURBO_VISION.bg);
        assert_eq!(DEFAULT.text, TURBO_VISION.text);
    }
}

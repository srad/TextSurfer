use ratatui::style::{Color, Style};

pub struct Theme {
    pub frame: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub bg: Color,
    pub bar_bg: Color,
    pub bar_text: Color,
    pub mnemonic: Color,
}

impl Theme {
    pub fn selected(&self) -> Style {
        Style::default().fg(Color::Black).bg(Color::White)
    }
}

pub const NORTON: Theme = Theme {
    frame: Color::LightBlue,
    text: Color::White,
    dim: Color::Cyan,
    accent: Color::Yellow,
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

use crate::css::ColorScheme;
use crate::ui::theme::{THEMES, Theme, ThemeAppearance};

use super::{App, apply_rendered_page};

impl App {
    pub fn theme(&self) -> &Theme {
        &THEMES[self.theme_index]
    }

    pub fn theme_index(&self) -> usize {
        self.theme_index
    }

    pub(super) fn color_scheme(&self) -> ColorScheme {
        match self.theme().appearance {
            ThemeAppearance::Dark => ColorScheme::Dark,
            ThemeAppearance::Light => ColorScheme::Light,
        }
    }

    pub(super) fn set_theme(&mut self, index: usize) {
        let Some(theme) = THEMES.get(index).copied() else {
            return;
        };
        if index == self.theme_index {
            self.tabs.active_mut().message = format!("theme: {}", theme.name);
            self.touch_status();
            return;
        }

        self.theme_index = index;
        let palette = theme.palette();
        let color_scheme = self.color_scheme();
        let active_index = self.tabs.active_index();
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        for (tab_index, tab) in self.tabs.tabs_mut().iter_mut().enumerate() {
            let Some(load) = tab.load.as_mut() else {
                continue;
            };
            if tab_index == active_index {
                if let Some(page) = load.recolor(palette, color_scheme) {
                    apply_rendered_page(tab, page, width, rows);
                }
            } else if load.set_color_context(palette, color_scheme) && load.has_painted() {
                tab.render_dirty = true;
            }
        }
        self.tabs.active_mut().message = format!("theme: {}", theme.name);
        self.refresh_hover();
        self.touch();
    }
}

use crate::core::event::{Key, KeyEvent};
use crate::core::focus::Focus;
use crate::core::frame::ChromeDamage;
use crate::ui::keymap::{Action, DefaultKeymap, Keymap};
use crate::ui::widgets::menu::MENUS;

use super::App;

impl App {
    pub fn handle_key(&mut self, event: KeyEvent) {
        match DefaultKeymap.resolve(&event, self.focus) {
            Some(action) => self.apply(action),
            None => self.edit(event),
        }
        if self.sync_dynamic_state() {
            self.touch();
        }
    }

    pub(super) fn apply(&mut self, action: Action) {
        match action {
            Action::Quit => self.quit = true,
            Action::NewTab => self.new_tab(),
            Action::CloseTab => self.close_tab_at(self.tabs.active_index()),
            Action::NextTab => {
                self.tabs.next();
                self.activate_current();
                self.pressed = None;
                self.refresh_hover();
                self.touch();
            }
            Action::PrevTab => {
                self.tabs.prev();
                self.activate_current();
                self.pressed = None;
                self.refresh_hover();
                self.touch();
            }
            Action::FocusAddress => {
                self.address.set_text(&self.tabs.active().url);
                self.focus = Focus::Address;
                self.touch();
            }
            Action::FocusContent => {
                self.focus = Focus::Content;
                self.touch();
            }
            Action::SubmitAddress => self.submit_address(),
            Action::ScrollDown => self.scroll(1),
            Action::ScrollUp => self.scroll(-1),
            Action::ScrollPageDown => self.scroll(self.page_step()),
            Action::ScrollPageUp => self.scroll(-self.page_step()),
            Action::ScrollTop => self.set_scroll(0),
            Action::ScrollBottom => self.set_scroll(usize::MAX),
            Action::ActivateLink => self.pending("link activation arrives in M2"),
            Action::NextLink => self.pending("tab-cycle link navigation arrives in M2"),
            Action::PrevLink => self.pending("shift-tab link navigation arrives in M2"),
            Action::Help => self.pending("the help overlay arrives in M2"),
            Action::Back => self.go_back(),
            Action::Forward => self.go_forward(),
            Action::Reload => self.reload(),
            Action::Home => self.home(),
            Action::FocusMenu => self.focus_menu(),
            Action::MenuOpen(menu) => self.open_menu(menu),
            Action::MenuClose => self.close_menu(),
            Action::MenuNext => {
                let len = MENUS[self.menu_active].len();
                if len > 0 {
                    self.menu_item = (self.menu_item + 1).min(len - 1);
                }
                self.touch();
            }
            Action::MenuPrev => {
                self.menu_item = self.menu_item.saturating_sub(1);
                self.touch();
            }
            Action::MenuLeft => {
                self.menu_active = if self.menu_active == 0 {
                    MENUS.len() - 1
                } else {
                    self.menu_active - 1
                };
                self.menu_item = 0;
                self.touch();
            }
            Action::MenuRight => {
                self.menu_active = (self.menu_active + 1) % MENUS.len();
                self.menu_item = 0;
                self.touch();
            }
            Action::MenuSelect => self.menu_select(),
            Action::ThemeInfo => {
                self.tabs.active_mut().message = "theme: Norton".to_string();
                self.touch();
            }
        }
    }

    fn focus_menu(&mut self) {
        if self.menu_open && self.focus == Focus::Menu {
            self.close_menu();
        } else {
            self.focus_before_menu = self.focus;
            self.menu_active = 0;
            self.menu_item = 0;
            self.menu_open = true;
            self.focus = Focus::Menu;
            self.touch();
        }
    }

    fn open_menu(&mut self, menu: usize) {
        if self.focus != Focus::Menu {
            self.focus_before_menu = self.focus;
        }
        self.focus = Focus::Menu;
        self.menu_open = true;
        self.menu_active = menu.min(MENUS.len() - 1);
        self.menu_item = 0;
        self.touch();
    }

    fn close_menu(&mut self) {
        self.menu_open = false;
        self.focus = self.focus_before_menu;
        self.touch();
    }

    fn menu_select(&mut self) {
        let action = menu_item_action(self.menu_active, self.menu_item);
        self.menu_open = false;
        self.focus = self.focus_before_menu;
        self.touch();
        if let Some(action) = action {
            self.apply(action);
        }
    }

    fn edit(&mut self, event: KeyEvent) {
        if self.focus != Focus::Address || event.modifiers.ctrl || event.modifiers.alt {
            return;
        }
        match event.code {
            Key::Char(ch) => self.address.insert(ch),
            Key::Backspace => self.address.backspace(),
            Key::Delete => self.address.delete(),
            Key::Left => self.address.left(),
            Key::Right => self.address.right(),
            Key::Home => self.address.home(),
            Key::End => self.address.end(),
            _ => return,
        }
        self.touch();
    }

    pub(super) fn pending(&mut self, notice: &str) {
        self.tabs.active_mut().message = notice.to_string();
        self.touch_status();
    }

    pub(super) fn touch(&mut self) {
        self.damage.repaint_all();
    }

    pub(super) fn touch_status(&mut self) {
        self.damage.damage_chrome(ChromeDamage::Status);
    }
}

fn menu_item_action(menu: usize, item: usize) -> Option<Action> {
    match (menu, item) {
        (0, 0) => Some(Action::NewTab),
        (0, 1) => Some(Action::CloseTab),
        (0, 2) => Some(Action::Reload),
        (0, 3) => Some(Action::Quit),
        (1, 0) => Some(Action::Back),
        (1, 1) => Some(Action::Forward),
        (1, 2) => Some(Action::Home),
        (2, 0) => Some(Action::ThemeInfo),
        (3, 0) => Some(Action::Help),
        _ => None,
    }
}

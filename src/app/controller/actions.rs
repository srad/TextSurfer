use crate::core::event::{Key, KeyEvent};
use crate::core::focus::Focus;
use crate::core::frame::ChromeDamage;
use crate::ui::keymap::{Action, DefaultKeymap, Keymap};

use super::App;

impl App {
    pub fn handle_key(&mut self, event: KeyEvent) {
        if self.text_context.take().is_some() {
            self.touch();
            if event.code == Key::Esc {
                return;
            }
        }
        if self.handle_form_key(&event) {
            if self.sync_dynamic_state() {
                self.touch();
            }
            return;
        }
        if self.use_main_menu_keyboard() {
            self.touch();
        }
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
            Action::ActivateFocused => self.activate_focused(),
            Action::NextFocusable => self.cycle_page_focus(false),
            Action::PrevFocusable => self.cycle_page_focus(true),
            Action::Help => self.pending("the help overlay arrives in M2"),
            Action::Back => self.go_back(),
            Action::Forward => self.go_forward(),
            Action::Reload => self.reload(),
            Action::Home => self.home(),
            Action::FocusMenu => self.focus_menu(),
            Action::MenuOpen(menu) => self.open_menu(menu),
            Action::MenuClose => self.close_menu(),
            Action::MenuNext => self.menu_next(),
            Action::MenuPrev => self.menu_prev(),
            Action::MenuLeft => self.menu_left(),
            Action::MenuRight => self.menu_right(),
            Action::MenuSelect => self.menu_select(),
            Action::SetTheme(index) => self.set_theme(index),
            Action::Screenshot => self.screenshot_request = true,
        }
    }

    fn edit(&mut self, event: KeyEvent) {
        if self.focus != Focus::Address || event.modifiers.alt {
            return;
        }
        let edit = self.address.handle_key(&event, false);
        if !edit.consumed {
            return;
        }
        self.clipboard_request = edit.clipboard;
        self.touch();
    }

    pub fn take_clipboard_request(
        &mut self,
    ) -> Option<crate::ui::widgets::text_field::ClipboardAction> {
        self.clipboard_request.take()
    }

    pub fn deliver_clipboard_text(&mut self, text: &str) {
        if self.focus == Focus::Address {
            let text = text.replace(['\r', '\n'], "");
            if self.address.paste(&text) {
                self.touch();
            }
        } else {
            self.paste_form_text(text);
        }
    }

    pub(super) fn pending(&mut self, notice: &str) {
        self.tabs.active_mut().message = notice.to_string();
        self.touch_status();
    }

    /// Whether a screenshot was asked for since this was last called.
    ///
    /// The controller cannot take one itself — `app` writes no files, and only the
    /// frontend knows which frontend it is — so the key press becomes a request the
    /// frontend picks up and answers with [`App::flash`].
    pub fn take_screenshot_request(&mut self) -> bool {
        std::mem::take(&mut self.screenshot_request)
    }

    pub(super) fn touch(&mut self) {
        self.damage.repaint_all();
    }

    pub(super) fn touch_status(&mut self) {
        self.damage.damage_chrome(ChromeDamage::STATUS);
    }
}

use ratatui::layout::{Position, Rect};

use crate::core::event::{MouseButton, MouseKind};
use crate::core::focus::Focus;
use crate::core::frame::ChromeDamage;
use crate::ui::keymap::Action;
use crate::ui::mouse::ChromeTarget;
use crate::ui::widgets::menu::{MENUS, MenuItemMask, THEME_MENU, popup_item_count, popup_rect};

use super::App;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuInput {
    Keyboard,
    Pointer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MenuPress {
    close_title: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MainMenuState {
    open: bool,
    active: usize,
    keyboard_item: Option<usize>,
    input: MenuInput,
    press: Option<MenuPress>,
}

impl Default for MainMenuState {
    fn default() -> Self {
        Self {
            open: false,
            active: 0,
            keyboard_item: Some(0),
            input: MenuInput::Keyboard,
            press: None,
        }
    }
}

impl MainMenuState {
    pub(super) fn is_open(self) -> bool {
        self.open
    }

    pub(super) fn active(self) -> usize {
        self.active
    }

    fn keyboard_item(self) -> Option<usize> {
        self.keyboard_item
    }

    fn uses_pointer(self) -> bool {
        self.input == MenuInput::Pointer
    }

    fn open_keyboard(&mut self, menu: usize, item: Option<usize>) {
        self.open = true;
        self.active = menu;
        self.keyboard_item = item;
        self.input = MenuInput::Keyboard;
        self.press = None;
    }

    fn switch_pointer(&mut self, menu: usize, item: Option<usize>) {
        self.active = menu;
        self.keyboard_item = item;
        self.input = MenuInput::Pointer;
    }

    fn use_keyboard(&mut self, selected: Option<usize>) {
        if self.input == MenuInput::Pointer
            && let Some(selected) = selected
        {
            self.keyboard_item = Some(selected);
        }
        self.input = MenuInput::Keyboard;
        self.press = None;
    }

    fn use_pointer(&mut self) {
        self.input = MenuInput::Pointer;
    }

    fn set_keyboard_item(&mut self, item: Option<usize>) {
        self.keyboard_item = item;
        self.input = MenuInput::Keyboard;
    }

    fn begin_press(&mut self, close_title: Option<usize>) {
        self.input = MenuInput::Pointer;
        self.press = Some(MenuPress { close_title });
    }

    fn cancel_close_if_pointer_left(&mut self, target: ChromeTarget) {
        let Some(press) = self.press.as_mut() else {
            return;
        };
        if press
            .close_title
            .is_some_and(|menu| target != ChromeTarget::MenuTitle(menu))
        {
            press.close_title = None;
        }
    }

    fn take_press(&mut self) -> Option<MenuPress> {
        self.press.take()
    }

    pub(super) fn cancel_press(&mut self) {
        self.press = None;
    }

    fn close(&mut self) {
        self.open = false;
        self.input = MenuInput::Keyboard;
        self.press = None;
    }
}

impl App {
    pub(super) fn focus_menu(&mut self) {
        if self.main_menu.is_open() && self.focus == Focus::Menu {
            self.close_menu();
        } else {
            self.open_menu(0);
        }
    }

    pub(super) fn open_menu(&mut self, menu: usize) {
        let menu = menu.min(MENUS.len() - 1);
        let item = self.initial_menu_item(menu);
        self.enter_menu_focus();
        self.main_menu.open_keyboard(menu, item);
        self.touch();
    }

    pub(super) fn open_menu_from_pointer(&mut self, menu: usize) {
        self.open_menu(menu);
        self.main_menu.use_pointer();
        self.main_menu.begin_press(None);
    }

    pub(super) fn close_menu(&mut self) {
        if !self.main_menu.is_open() {
            return;
        }
        self.main_menu.close();
        self.focus = self.focus_before_menu;
        self.refresh_hover();
        self.touch();
    }

    pub(super) fn use_main_menu_keyboard(&mut self) -> bool {
        if !self.main_menu.is_open() {
            return false;
        }
        let previous = self.selected_main_menu_item();
        self.main_menu.use_keyboard(previous);
        previous != self.selected_main_menu_item()
    }

    pub(super) fn menu_next(&mut self) {
        self.move_menu_item(true);
    }

    pub(super) fn menu_prev(&mut self) {
        self.move_menu_item(false);
    }

    pub(super) fn menu_left(&mut self) {
        let active = self.main_menu.active();
        let menu = if active == 0 {
            MENUS.len() - 1
        } else {
            active - 1
        };
        self.switch_menu_keyboard(menu);
    }

    pub(super) fn menu_right(&mut self) {
        self.switch_menu_keyboard((self.main_menu.active() + 1) % MENUS.len());
    }

    pub(super) fn menu_select(&mut self) {
        let Some(item) = self.main_menu.keyboard_item() else {
            return;
        };
        self.select_menu_item(item);
    }

    pub(super) fn selected_main_menu_item(&self) -> Option<usize> {
        if !self.main_menu.is_open() {
            return None;
        }
        let item = if self.main_menu.uses_pointer() {
            let at = self.pointer?;
            let ChromeTarget::MenuItem(item) = self.target_at(at) else {
                return None;
            };
            item
        } else {
            self.main_menu.keyboard_item()?
        };
        (item < self.visible_menu_items(self.main_menu.active())
            && self.menu_item_enabled(self.main_menu.active(), item))
        .then_some(item)
    }

    pub(super) fn hovered_main_menu_title(&self) -> Option<usize> {
        let at = self.pointer?;
        let ChromeTarget::MenuTitle(menu) = self.target_at(at) else {
            return None;
        };
        Some(menu)
    }

    pub(super) fn enabled_main_menu_items(&self) -> MenuItemMask {
        let menu = self.main_menu.active();
        let mut enabled = MenuItemMask::all(MENUS[menu].len());
        for item in 0..MENUS[menu].len() {
            enabled.set(item, self.menu_item_enabled(menu, item));
        }
        enabled
    }

    pub(super) fn pointer_uses_main_menu_cursor(&self) -> bool {
        self.main_menu.is_open() || self.hovered_main_menu_title().is_some()
    }

    pub(super) fn damage_main_menu_title_hover(&mut self, previous: Option<usize>) {
        if !self.main_menu.is_open() && previous != self.hovered_main_menu_title() {
            self.damage.damage_chrome(ChromeDamage::MENU_BAR);
        }
    }

    pub(super) fn handle_open_main_menu_mouse(
        &mut self,
        kind: MouseKind,
        target: ChromeTarget,
        previous_active: usize,
        previous_selected: Option<usize>,
    ) {
        match kind {
            MouseKind::Move => {
                self.main_menu.use_pointer();
                self.main_menu.cancel_close_if_pointer_left(target);
                if let ChromeTarget::MenuTitle(menu) = target
                    && menu != self.main_menu.active()
                {
                    self.switch_menu_pointer(menu);
                }
                self.repaint_main_menu_if_changed(previous_active, previous_selected);
            }
            MouseKind::Press(button) => {
                self.press_main_menu(target, button, previous_active, previous_selected)
            }
            MouseKind::Release(MouseButton::Left) => {
                self.release_main_menu(target, previous_active, previous_selected)
            }
            MouseKind::Release(_) | MouseKind::Wheel { .. } => {}
        }
    }

    fn enter_menu_focus(&mut self) {
        if self.focus != Focus::Menu {
            self.focus_before_menu = self.focus;
        }
        self.focus = Focus::Menu;
        self.pressed = None;
        self.scroll_drag = None;
        self.text_drag = None;
        self.text_context = None;
        self.hover = None;
        self.sync_dynamic_state();
    }

    fn switch_menu_keyboard(&mut self, menu: usize) {
        let item = self.initial_menu_item(menu);
        self.main_menu.open_keyboard(menu, item);
        self.touch();
    }

    fn switch_menu_pointer(&mut self, menu: usize) {
        let menu = menu.min(MENUS.len() - 1);
        let item = self.initial_menu_item(menu);
        self.main_menu.switch_pointer(menu, item);
    }

    fn move_menu_item(&mut self, forward: bool) {
        let menu = self.main_menu.active();
        let count = self.visible_menu_items(menu);
        if count == 0 {
            self.main_menu.set_keyboard_item(None);
            self.touch();
            return;
        }
        let current = self.main_menu.keyboard_item();
        let next = current
            .and_then(|current| {
                (1..=count)
                    .map(|step| {
                        if forward {
                            (current + step) % count
                        } else {
                            (current + count - step % count) % count
                        }
                    })
                    .find(|item| self.menu_item_enabled(menu, *item))
            })
            .or_else(|| self.initial_menu_item(menu));
        self.main_menu.set_keyboard_item(next);
        self.touch();
    }

    fn initial_menu_item(&self, menu: usize) -> Option<usize> {
        let visible = self.visible_menu_items(menu);
        if menu == THEME_MENU
            && self.theme_index < visible
            && self.menu_item_enabled(menu, self.theme_index)
        {
            return Some(self.theme_index);
        }
        (0..visible).find(|item| self.menu_item_enabled(menu, *item))
    }

    fn visible_menu_items(&self, menu: usize) -> usize {
        popup_item_count(
            Rect::new(0, 0, self.geometry.size.cols, self.geometry.size.rows),
            menu,
        )
    }

    fn menu_item_enabled(&self, menu: usize, item: usize) -> bool {
        match menu_item_action(menu, item) {
            Some(Action::Back) => self.tabs.active().history_pos > 0,
            Some(Action::Forward) => {
                self.tabs.active().history_pos + 1 < self.tabs.active().history.len()
            }
            Some(_) => true,
            None => false,
        }
    }

    fn press_main_menu(
        &mut self,
        target: ChromeTarget,
        button: MouseButton,
        previous_active: usize,
        previous_selected: Option<usize>,
    ) {
        let on_surface = self.pointer_on_main_menu_surface(target);
        if button != MouseButton::Left {
            if !on_surface {
                self.close_menu();
            }
            return;
        }
        match target {
            ChromeTarget::MenuTitle(menu) => {
                let close_title = (menu == self.main_menu.active()).then_some(menu);
                if menu != self.main_menu.active() {
                    self.switch_menu_pointer(menu);
                } else {
                    self.main_menu.use_pointer();
                }
                self.main_menu.begin_press(close_title);
                self.repaint_main_menu_if_changed(previous_active, previous_selected);
            }
            ChromeTarget::MenuItem(_) => {
                self.main_menu.use_pointer();
                self.main_menu.begin_press(None);
                self.repaint_main_menu_if_changed(previous_active, previous_selected);
            }
            _ if on_surface => {
                self.main_menu.use_pointer();
                self.main_menu.begin_press(None);
                self.repaint_main_menu_if_changed(previous_active, previous_selected);
            }
            _ => self.close_menu(),
        }
    }

    fn release_main_menu(
        &mut self,
        target: ChromeTarget,
        previous_active: usize,
        previous_selected: Option<usize>,
    ) {
        let Some(press) = self.main_menu.take_press() else {
            return;
        };
        match target {
            ChromeTarget::MenuItem(item)
                if self.menu_item_enabled(self.main_menu.active(), item) =>
            {
                self.select_menu_item(item);
            }
            ChromeTarget::MenuItem(_) => {
                self.main_menu.use_pointer();
                self.repaint_main_menu_if_changed(previous_active, previous_selected);
            }
            ChromeTarget::MenuTitle(menu) => {
                if menu != self.main_menu.active() {
                    self.switch_menu_pointer(menu);
                }
                if press.close_title == Some(menu) {
                    self.close_menu();
                } else {
                    self.repaint_main_menu_if_changed(previous_active, previous_selected);
                }
            }
            _ if self.pointer_on_main_menu_surface(target) => {
                self.repaint_main_menu_if_changed(previous_active, previous_selected);
            }
            _ => self.close_menu(),
        }
    }

    fn select_menu_item(&mut self, item: usize) {
        let menu = self.main_menu.active();
        if item >= self.visible_menu_items(menu) || !self.menu_item_enabled(menu, item) {
            return;
        }
        let action = menu_item_action(menu, item);
        self.close_menu();
        if let Some(action) = action {
            self.apply(action);
        }
    }

    fn pointer_on_main_menu_surface(&self, target: ChromeTarget) -> bool {
        if matches!(
            target,
            ChromeTarget::MenuTitle(_) | ChromeTarget::MenuItem(_)
        ) {
            return true;
        }
        let Some(at) = self.pointer else {
            return false;
        };
        let area = Rect::new(0, 0, self.geometry.size.cols, self.geometry.size.rows);
        popup_rect(area, area, self.main_menu.active()).contains(Position::new(at.col, at.row))
    }

    fn repaint_main_menu_if_changed(
        &mut self,
        previous_active: usize,
        previous_selected: Option<usize>,
    ) {
        if previous_active != self.main_menu.active()
            || previous_selected != self.selected_main_menu_item()
        {
            self.touch();
        }
    }
}

pub(super) fn menu_item_action(menu: usize, item: usize) -> Option<Action> {
    match (menu, item) {
        (0, 0) => Some(Action::NewTab),
        (0, 1) => Some(Action::CloseTab),
        (0, 2) => Some(Action::Reload),
        (0, 3) => Some(Action::Quit),
        (1, 0) => Some(Action::Back),
        (1, 1) => Some(Action::Forward),
        (1, 2) => Some(Action::Home),
        (THEME_MENU, item) if item < MENUS[THEME_MENU].len() => Some(Action::SetTheme(item)),
        (3, 0) => Some(Action::Help),
        _ => None,
    }
}

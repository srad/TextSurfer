use url::Url;

use crate::core::dom::{Node, NodeId, attr_value};
use crate::core::event::{MouseButton, MouseEvent, MouseKind, WheelDirection};
use crate::core::focus::Focus;
use crate::core::geom::Point;
use crate::ui::chrome::address_index_at;
use crate::ui::keymap::Action;
use crate::ui::mouse::{ChromeState, ChromeTarget, WHEEL_ROWS};

use super::App;

/// What the pointer is over, so the status bar and the window cursor can follow it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HoverTarget {
    pub(super) node: NodeId,
    pub(super) href: String,
}

impl App {
    pub fn handle_mouse(&mut self, event: MouseEvent) {
        self.pointer = Some(event.at);
        let target = self.target_at(event.at);
        match event.kind {
            MouseKind::Move => self.update_hover(target),
            MouseKind::Wheel(direction) => self.wheel(target, direction),
            MouseKind::Press(button) => self.press(target, button),
            MouseKind::Release(button) => self.release(target, button),
        }
    }

    /// Whether the pointer is resting on a link, so a window frontend can show a hand.
    pub fn hovers_link(&self) -> bool {
        self.hover.is_some()
    }

    pub(super) fn hovered_href(&self) -> Option<&str> {
        self.hover.as_ref().map(|hover| hover.href.as_str())
    }

    /// Re-derive the hover after the content moved under a pointer that did not.
    pub(super) fn refresh_hover(&mut self) {
        match self.pointer {
            Some(at) => {
                let target = self.target_at(at);
                self.update_hover(target);
            }
            None => {
                if self.hover.take().is_some() {
                    self.touch();
                }
            }
        }
    }

    /// The pointer left the window, so nothing is hovered and no press is outstanding.
    pub fn pointer_left(&mut self) {
        self.pointer = None;
        self.pressed = None;
        self.refresh_hover();
    }

    fn target_at(&self, at: Point) -> ChromeTarget {
        let tabs = self.tab_chips();
        self.geometry.target_at(
            at,
            ChromeState {
                tabs: &tabs,
                active_tab: self.tabs.active_index(),
                open_menu: self.menu_open.then_some(self.menu_active),
            },
        )
    }

    fn press(&mut self, target: ChromeTarget, button: MouseButton) {
        match button {
            MouseButton::Back => return self.apply(Action::Back),
            MouseButton::Forward => return self.apply(Action::Forward),
            MouseButton::Right => return,
            MouseButton::Left | MouseButton::Middle => {}
        }
        if self.menu_open {
            match target {
                ChromeTarget::MenuItem(item) => {
                    self.menu_item = item;
                    self.apply(Action::MenuSelect);
                }
                ChromeTarget::MenuTitle(menu) if menu == self.menu_active => {
                    self.apply(Action::MenuClose);
                }
                ChromeTarget::MenuTitle(menu) => self.apply(Action::MenuOpen(menu)),
                _ => self.apply(Action::MenuClose),
            }
            return;
        }
        match target {
            ChromeTarget::MenuTitle(menu) => self.apply(Action::MenuOpen(menu)),
            ChromeTarget::Tab(index) => self.select_tab(index),
            ChromeTarget::NewTab => self.apply(Action::NewTab),
            ChromeTarget::ToolbarButton(button) => match button {
                0 => self.apply(Action::Back),
                1 => self.apply(Action::Forward),
                2 => self.apply(Action::Reload),
                _ => self.apply(Action::Home),
            },
            ChromeTarget::Address { col } => self.focus_address_at(col),
            ChromeTarget::Content { col, row } => {
                if self.focus != Focus::Content {
                    self.focus = Focus::Content;
                    self.touch();
                }
                self.pressed = self.link_at(col, row).map(|link| link.node);
            }
            ChromeTarget::MenuItem(_) | ChromeTarget::Inert => {}
        }
    }

    fn release(&mut self, target: ChromeTarget, button: MouseButton) {
        let Some(pressed) = self.pressed.take() else {
            return;
        };
        if !matches!(button, MouseButton::Left | MouseButton::Middle) {
            return;
        }
        let ChromeTarget::Content { col, row } = target else {
            return;
        };
        let Some((node, href)) = self
            .link_at(col, row)
            .map(|link| (link.node, link.href.clone()))
        else {
            return;
        };
        if node != pressed {
            return;
        }
        let new_tab = button == MouseButton::Middle || self.link_opens_a_new_tab(node);
        self.activate_link(&href, new_tab);
    }

    fn wheel(&mut self, target: ChromeTarget, direction: WheelDirection) {
        if self.menu_open || !matches!(target, ChromeTarget::Content { .. }) {
            return;
        }
        match direction {
            WheelDirection::Up => self.scroll(-WHEEL_ROWS),
            WheelDirection::Down => self.scroll(WHEEL_ROWS),
        }
        self.refresh_hover();
    }

    fn update_hover(&mut self, target: ChromeTarget) {
        let next = match target {
            ChromeTarget::Content { col, row } => self.link_at(col, row).map(|link| HoverTarget {
                node: link.node,
                href: link.href.clone(),
            }),
            _ => None,
        };
        if next != self.hover {
            self.hover = next;
            self.touch();
        }
    }

    fn link_at(&self, col: u16, row: u16) -> Option<&crate::paint::PaintedLink> {
        let tab = self.tabs.active();
        let row = usize::from(row).checked_add(tab.scroll)?;
        tab.painted.link_at(usize::from(col), row)
    }

    fn link_opens_a_new_tab(&self, node: NodeId) -> bool {
        let Some(document) = self.tabs.active().document.as_ref() else {
            return false;
        };
        let document = document.borrow();
        match document.node(node) {
            Some(Node::Element { attrs, .. }) => attr_value(attrs, "target")
                .is_some_and(|target| target.eq_ignore_ascii_case("_blank")),
            _ => false,
        }
    }

    fn activate_link(&mut self, href: &str, new_tab: bool) {
        let tab = self.tabs.active();
        let base = tab
            .base
            .clone()
            .or_else(|| Url::parse(&tab.url).ok())
            .or_else(|| Url::parse("about:blank").ok());
        let resolved = match base {
            Some(base) => base.join(href),
            None => Url::parse(href),
        };
        let Ok(resolved) = resolved else {
            return self.pending(&format!("cannot follow link: {href}"));
        };
        if !new_tab && same_document_fragment(&resolved, &self.tabs.active().url) {
            return self.pending("anchor links arrive in M2");
        }
        let url = resolved.to_string();
        if new_tab {
            self.new_tab();
            self.focus = Focus::Content;
        }
        self.pressed = None;
        self.open_url(&url);
        self.refresh_hover();
    }

    fn select_tab(&mut self, index: usize) {
        if !self.tabs.activate(index) {
            return;
        }
        self.activate_current();
        self.focus = Focus::Content;
        self.pressed = None;
        self.refresh_hover();
        self.touch();
    }

    fn focus_address_at(&mut self, col: u16) {
        if self.focus != Focus::Address {
            self.address.set_text(&self.tabs.active().url);
            self.focus = Focus::Address;
        }
        let index = address_index_at(self.address.text(), self.geometry.size.cols, col);
        self.address.set_cursor(index);
        self.touch();
    }
}

/// Whether following this link would only move within the page already loaded.
fn same_document_fragment(resolved: &Url, current: &str) -> bool {
    if resolved.fragment().is_none() {
        return false;
    }
    let Ok(current) = Url::parse(current) else {
        return false;
    };
    let mut target = resolved.clone();
    let mut here = current;
    target.set_fragment(None);
    here.set_fragment(None);
    target == here
}

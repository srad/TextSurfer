use url::Url;

use crate::core::dom::{Node, NodeId, attr_value};
use crate::core::event::{MouseButton, MouseEvent, MouseKind};
use crate::core::focus::Focus;
use crate::core::form::{ControlKind, control_kind};
use crate::core::frame::ChromeDamage;
use crate::core::geom::Point;
use crate::ui::chrome::address_index_at;
use crate::ui::keymap::Action;
use crate::ui::mouse::{ChromeState, ChromeTarget};
use crate::ui::widgets::scrollbar::{ScrollExtent, ScrollbarPart};
use crate::ui::widgets::text_field::{TextFieldMenuAction, menu_action_at, menu_rect};

use super::App;

/// What the pointer is over, so the status bar and the window cursor can follow it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HoverTarget {
    pub(super) node: NodeId,
    pub(super) href: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PressedTarget {
    pub(super) button: MouseButton,
    pub(super) node: NodeId,
    pub(super) link: Option<NodeId>,
}

/// A scrollbar thumb being dragged, holding where inside the thumb it was grabbed so
/// the thumb does not jump under the pointer on the first move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScrollDrag {
    pub(super) grab: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextFieldTarget {
    Address,
    Form(NodeId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TextFieldContext {
    pub(super) target: TextFieldTarget,
    pub(super) anchor: Point,
}

impl App {
    pub fn handle_mouse(&mut self, event: MouseEvent) {
        let previous_toolbar = self.hovered_toolbar_button();
        if let Some(context) = self.text_context
            && matches!(event.kind, MouseKind::Press(MouseButton::Left))
        {
            let bounds =
                ratatui::layout::Rect::new(0, 0, self.geometry.size.cols, self.geometry.size.rows);
            if let Some(action) = menu_action_at(bounds, context.anchor, event.at) {
                self.apply_text_field_menu(context.target, action);
                self.text_context = None;
                self.touch();
                return;
            }
            self.text_context = None;
            self.touch();
        }
        if event.kind == MouseKind::Move && self.text_context.is_some() {
            let previous = self.selected_text_field_menu_action();
            self.pointer = Some(event.at);
            if previous != self.selected_text_field_menu_action() {
                self.touch();
            }
            return;
        }
        if event.kind == MouseKind::Move
            && let Some(target) = self.text_drag
        {
            self.pointer = Some(event.at);
            self.drag_text_selection(target, event.at);
            return;
        }
        if matches!(event.kind, MouseKind::Release(MouseButton::Left)) {
            self.text_drag = None;
        }
        if event.kind == MouseKind::Move && self.pointer == Some(event.at) {
            return;
        }
        // A thumb drag owns the pointer until the button comes back up: it must keep
        // tracking off the bar, and it must not light up whatever it passes over.
        if event.kind == MouseKind::Move
            && let Some(drag) = self.scroll_drag
        {
            self.pointer = Some(event.at);
            self.drag_thumb(drag, event.at.row);
            return;
        }
        let previous_href = self.hovered_href().map(str::to_string);
        self.pointer = Some(event.at);
        let target = self.target_at(event.at);
        self.update_hover(target);
        match event.kind {
            MouseKind::Move => {}
            MouseKind::Wheel { rows } => self.wheel(target, rows),
            MouseKind::Press(button) => self.press(target, button, event.at),
            MouseKind::Release(button) => self.release(target, button),
        }
        let painted_changed = self.sync_dynamic_state();
        self.damage_toolbar_hover(previous_toolbar);
        if painted_changed {
            self.touch();
        } else if previous_href.as_deref() != self.hovered_href() {
            self.touch_status();
        }
    }

    /// Whether the pointer is resting on a link, so a window frontend can show a hand.
    pub fn hovers_link(&self) -> bool {
        self.hover
            .as_ref()
            .is_some_and(|hover| hover.href.is_some())
    }

    pub(super) fn hovered_toolbar_button(&self) -> Option<usize> {
        if self.menu_open
            || self.text_context.is_some()
            || self.text_drag.is_some()
            || self.scroll_drag.is_some()
        {
            return None;
        }
        let at = self.pointer?;
        let ChromeTarget::ToolbarButton(button) = self.target_at(at) else {
            return None;
        };
        match button {
            0 => (self.tabs.active().history_pos > 0).then_some(button),
            1 => (self.tabs.active().history_pos + 1 < self.tabs.active().history.len())
                .then_some(button),
            2 | 3 => Some(button),
            _ => None,
        }
    }

    fn damage_toolbar_hover(&mut self, previous: Option<usize>) {
        if previous != self.hovered_toolbar_button() {
            self.damage.damage_chrome(ChromeDamage::TOOLBAR);
        }
    }

    pub(super) fn hovered_href(&self) -> Option<&str> {
        self.hover.as_ref().and_then(|hover| hover.href.as_deref())
    }

    /// Re-derive the hover after the content moved under a pointer that did not.
    pub(super) fn refresh_hover(&mut self) {
        let previous_href = self.hovered_href().map(str::to_string);
        self.rederive_hover();
        let painted_changed = self.sync_dynamic_state();
        if painted_changed {
            self.touch();
        } else if previous_href.as_deref() != self.hovered_href() {
            self.touch_status();
        }
    }

    pub(super) fn rederive_hover(&mut self) {
        let next = self.pointer.map(|at| self.target_at(at));
        self.hover = next.and_then(|target| self.hover_for_target(target));
    }

    /// The pointer left the window, so nothing is hovered and no press is outstanding.
    ///
    /// A thumb drag ends here too. Nothing captures the pointer, so a button released
    /// outside the window is never reported to us and the drag would otherwise stick.
    pub fn pointer_left(&mut self) {
        let previous_toolbar = self.hovered_toolbar_button();
        let had_href = self.hovered_href().is_some();
        let had_text_field_selection = self.selected_text_field_menu_action().is_some();
        self.pointer = None;
        self.pressed = None;
        self.scroll_drag = None;
        self.text_drag = None;
        self.hover = None;
        self.damage_toolbar_hover(previous_toolbar);
        let painted_changed = self.sync_dynamic_state();
        if painted_changed || had_text_field_selection {
            self.touch();
        } else if had_href {
            self.touch_status();
        }
    }

    pub(super) fn text_field_menu_capabilities(&self, target: TextFieldTarget) -> (bool, bool) {
        let has_selection = match target {
            TextFieldTarget::Address => self.address.selection().is_some(),
            TextFieldTarget::Form(node) => self
                .tabs
                .active()
                .text_fields
                .get(&node)
                .is_some_and(|field| field.selection().is_some()),
        };
        (has_selection, has_selection)
    }

    pub(super) fn selected_text_field_menu_action(&self) -> Option<TextFieldMenuAction> {
        let context = self.text_context?;
        let at = self.pointer?;
        let bounds =
            ratatui::layout::Rect::new(0, 0, self.geometry.size.cols, self.geometry.size.rows);
        let action = menu_action_at(bounds, context.anchor, at)?;
        let (can_copy, can_cut) = self.text_field_menu_capabilities(context.target);
        action.is_enabled(can_copy, can_cut).then_some(action)
    }

    pub(super) fn pointer_over_text_field_menu(&self) -> bool {
        let (Some(context), Some(at)) = (self.text_context, self.pointer) else {
            return false;
        };
        let bounds =
            ratatui::layout::Rect::new(0, 0, self.geometry.size.cols, self.geometry.size.rows);
        let menu = menu_rect(bounds, context.anchor);
        at.col >= menu.x
            && at.col < menu.x.saturating_add(menu.width)
            && at.row >= menu.y
            && at.row < menu.y.saturating_add(menu.height)
    }

    pub(super) fn target_at(&self, at: Point) -> ChromeTarget {
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

    fn press(&mut self, target: ChromeTarget, button: MouseButton, at: Point) {
        self.scroll_drag = None;
        match button {
            MouseButton::Back => return self.apply(Action::Back),
            MouseButton::Forward => return self.apply(Action::Forward),
            MouseButton::Right => return self.open_text_field_menu(target, at),
            MouseButton::Left | MouseButton::Middle => {}
        }
        self.pressed = None;
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
            // No `Action` names a tab, so this goes straight to the session the way
            // `Tab(index)` already does, rather than growing a second command set.
            ChromeTarget::TabClose(index) => self.close_tab_at(index),
            ChromeTarget::NewTab => self.apply(Action::NewTab),
            ChromeTarget::Scrollbar { row } => self.press_scrollbar(row),
            ChromeTarget::ToolbarButton(button) => match button {
                0 => self.apply(Action::Back),
                1 => self.apply(Action::Forward),
                2 => self.apply(Action::Reload),
                _ => self.apply(Action::Home),
            },
            ChromeTarget::Address { col } => {
                self.focus_address_at(col);
                self.text_drag = Some(TextFieldTarget::Address);
            }
            ChromeTarget::Content { col, row } => {
                let node = self.hit_at(col, row);
                let link = self.link_at(col, row).map(|link| link.node);
                match button {
                    MouseButton::Left => {
                        if self.focus != Focus::Content {
                            self.focus = Focus::Content;
                            self.touch();
                        }
                        let focus = node.and_then(|node| self.focusable_ancestor(node));
                        self.set_pointer_focus(focus);
                        if let Some(focus) = focus {
                            self.initialize_form_cursor(focus);
                            self.position_form_cursor(
                                focus,
                                usize::from(col),
                                usize::from(row).saturating_add(self.tabs.active().scroll),
                            );
                            let is_text = self
                                .tabs
                                .active()
                                .document
                                .as_ref()
                                .and_then(|document| control_kind(&document.borrow(), focus))
                                .is_some_and(ControlKind::is_text_entry);
                            if is_text {
                                self.text_drag = Some(TextFieldTarget::Form(focus));
                            }
                        }
                        self.pressed =
                            focus
                                .or(node)
                                .map(|node| PressedTarget { button, node, link });
                    }
                    MouseButton::Middle => {
                        self.pressed = link.map(|link| PressedTarget {
                            button,
                            node: node.unwrap_or(link),
                            link: Some(link),
                        });
                    }
                    _ => {}
                }
            }
            ChromeTarget::MenuItem(_) | ChromeTarget::Inert => {}
        }
    }

    fn open_text_field_menu(&mut self, target: ChromeTarget, at: Point) {
        let field = match target {
            ChromeTarget::Address { col } => {
                let index = address_index_at(
                    crate::ui::widgets::text_field::TextFieldView::new(&self.address),
                    self.geometry,
                    col,
                );
                let keep_selection = self.focus == Focus::Address
                    && self
                        .address
                        .selection()
                        .is_some_and(|selection| selection.contains(&index));
                if !keep_selection {
                    self.focus_address_at(col);
                }
                Some(TextFieldTarget::Address)
            }
            ChromeTarget::Content { col, row } => {
                let Some(node) = self
                    .hit_at(col, row)
                    .and_then(|node| self.focusable_ancestor(node))
                else {
                    return;
                };
                let is_text = self
                    .tabs
                    .active()
                    .document
                    .as_ref()
                    .and_then(|document| control_kind(&document.borrow(), node))
                    .is_some_and(ControlKind::is_text_entry);
                if !is_text {
                    return;
                }
                self.focus = Focus::Content;
                self.set_pointer_focus(Some(node));
                self.initialize_form_cursor(node);
                self.position_form_context_cursor(
                    node,
                    usize::from(col),
                    usize::from(row).saturating_add(self.tabs.active().scroll),
                );
                Some(TextFieldTarget::Form(node))
            }
            _ => None,
        };
        if let Some(target) = field {
            self.text_context = Some(TextFieldContext { target, anchor: at });
            self.touch();
        }
    }

    fn apply_text_field_menu(&mut self, target: TextFieldTarget, action: TextFieldMenuAction) {
        match target {
            TextFieldTarget::Address => {
                let edit = self.address.apply_menu(action);
                self.clipboard_request = edit.clipboard;
            }
            TextFieldTarget::Form(node) => {
                let edit = self
                    .tabs
                    .active_mut()
                    .text_fields
                    .get_mut(&node)
                    .map(|field| field.apply_menu(action))
                    .unwrap_or_default();
                self.clipboard_request = edit.clipboard;
                if edit.changed {
                    self.sync_text_field_value(node);
                }
                self.damage_text_field(node);
            }
        }
    }

    fn drag_text_selection(&mut self, target: TextFieldTarget, at: Point) {
        match target {
            TextFieldTarget::Address => {
                let index = address_index_at(
                    crate::ui::widgets::text_field::TextFieldView::new(&self.address),
                    self.geometry,
                    at.col,
                );
                self.address.set_cursor(index, true);
                self.touch();
            }
            TextFieldTarget::Form(node) => {
                let Some(view) = self.geometry.content_view() else {
                    return;
                };
                let col = usize::from(at.col.saturating_sub(view.origin.col));
                let row = usize::from(at.row.saturating_sub(view.origin.row))
                    .saturating_add(self.tabs.active().scroll);
                self.extend_form_selection(node, col, row);
            }
        }
    }

    /// Act on a press somewhere on the page scrollbar.
    ///
    /// The caps and the trough are the existing scroll actions — the mouse is not a
    /// second command set. Only the thumb is new, because no action can name an
    /// absolute position.
    fn press_scrollbar(&mut self, row: u16) {
        let Some(metrics) = self.scroll_extent().map(ScrollExtent::metrics) else {
            return;
        };
        match metrics.part_at(row) {
            ScrollbarPart::LineUp => self.apply(Action::ScrollUp),
            ScrollbarPart::LineDown => self.apply(Action::ScrollDown),
            ScrollbarPart::PageUp => self.apply(Action::ScrollPageUp),
            ScrollbarPart::PageDown => self.apply(Action::ScrollPageDown),
            ScrollbarPart::Thumb => {
                self.scroll_drag = Some(ScrollDrag {
                    grab: row - metrics.thumb.start,
                });
            }
        }
    }

    fn drag_thumb(&mut self, drag: ScrollDrag, row: u16) {
        let Some(extent) = self.scroll_extent() else {
            return;
        };
        let top = self
            .geometry
            .scrollbar_rect()
            .map_or(row, |rect| row.saturating_sub(rect.y));
        let scroll = extent
            .metrics()
            .scroll_for_thumb_top(top.saturating_sub(drag.grab));
        self.set_scroll(scroll);
    }

    fn release(&mut self, target: ChromeTarget, button: MouseButton) {
        self.scroll_drag = None;
        let Some(pressed) = self.pressed.as_ref().copied() else {
            return;
        };
        if pressed.button != button {
            return;
        }
        self.pressed = None;
        let ChromeTarget::Content { col, row } = target else {
            return;
        };
        if let Some((node, href)) = self
            .link_at(col, row)
            .map(|link| (link.node, link.href.clone()))
        {
            if Some(node) != pressed.link {
                return;
            }
            let new_tab = button == MouseButton::Middle || self.link_opens_a_new_tab(node);
            self.activate_link(&href, new_tab);
            return;
        };
        if button != MouseButton::Left || pressed.link.is_some() {
            return;
        }
        let released = self
            .hit_at(col, row)
            .and_then(|node| self.focusable_ancestor(node));
        if released == Some(pressed.node) {
            self.set_pointer_focus(released);
            self.activate_focused();
        }
    }

    fn wheel(&mut self, target: ChromeTarget, rows: i32) {
        if self.menu_open
            || !matches!(
                target,
                ChromeTarget::Content { .. } | ChromeTarget::Scrollbar { .. }
            )
        {
            return;
        }
        self.scroll(rows);
    }

    fn update_hover(&mut self, target: ChromeTarget) {
        self.hover = self.hover_for_target(target);
    }

    fn hover_for_target(&self, target: ChromeTarget) -> Option<HoverTarget> {
        let ChromeTarget::Content { col, row } = target else {
            return None;
        };
        let node = self.hit_at(col, row)?;
        let href = self.link_at(col, row).map(|link| link.href.clone());
        Some(HoverTarget { node, href })
    }

    fn hit_at(&self, col: u16, row: u16) -> Option<NodeId> {
        let tab = self.tabs.active();
        let row = usize::from(row).checked_add(tab.scroll)?;
        tab.painted.hit_test(usize::from(col), row)
    }

    fn link_at(&self, col: u16, row: u16) -> Option<&crate::paint::PaintedLink> {
        let tab = self.tabs.active();
        let row = usize::from(row).checked_add(tab.scroll)?;
        tab.painted.link_at(usize::from(col), row)
    }

    pub(super) fn link_opens_a_new_tab(&self, node: NodeId) -> bool {
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

    pub(super) fn activate_link(&mut self, href: &str, new_tab: bool) {
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
            self.address.set_cursor(0, false);
            self.focus = Focus::Address;
        }
        let index = address_index_at(
            crate::ui::widgets::text_field::TextFieldView::new(&self.address),
            self.geometry,
            col,
        );
        self.address.set_cursor(index, false);
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

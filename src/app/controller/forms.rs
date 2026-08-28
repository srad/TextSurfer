use crate::core::dom::{Node, NodeId, attr_value, has_attr};
use crate::core::event::{Key, KeyEvent};
use crate::core::focus::Focus;
use crate::core::form::{
    ControlKind, checkedness, control_kind, form_owner, options, selected_index, text_value,
};
use crate::css::{FocusSource, FocusedNode};
use crate::layout::LayoutRect;
use crate::paint::HitKind;
use crate::pipeline::render::RenderedPage;
use crate::ui::widgets::content::ContentTextField;
use crate::ui::widgets::text_field::{TextFieldFrame, TextFieldState, TextFieldView};

use super::{App, apply_rendered_page};

impl App {
    pub(super) fn cycle_page_focus(&mut self, reverse: bool) {
        let ring = self.page_focus_ring();
        if ring.is_empty() {
            self.tabs.active_mut().dom_focus = None;
            return;
        }
        let current = self
            .tabs
            .active()
            .dom_focus
            .and_then(|focus| ring.iter().position(|node| *node == focus.node));
        let index = match (current, reverse) {
            (Some(0), true) | (None, true) => ring.len() - 1,
            (Some(index), true) => index - 1,
            (Some(index), false) => (index + 1) % ring.len(),
            (None, false) => 0,
        };
        let node = ring[index];
        self.focus = Focus::Content;
        self.tabs.active_mut().dom_focus = Some(FocusedNode {
            node,
            source: FocusSource::Keyboard,
        });
        self.initialize_form_cursor(node);
        if let Some(row) = self.focus_row(node) {
            let visible = self.geometry.content_rows().max(1);
            let scroll = self.tabs.active().scroll;
            if row < scroll {
                self.set_scroll(row);
            } else if row >= scroll.saturating_add(visible) {
                self.set_scroll(row.saturating_sub(visible - 1));
            }
        }
        self.touch();
    }

    pub(super) fn activate_focused(&mut self) {
        let Some(node) = self.tabs.active().dom_focus.map(|focus| focus.node) else {
            return;
        };
        if let Some(href) = self
            .tabs
            .active()
            .painted
            .links
            .iter()
            .find(|link| link.node == node)
            .map(|link| link.href.clone())
        {
            let new_tab = self.link_opens_a_new_tab(node);
            self.activate_link(&href, new_tab);
            return;
        }
        let kind = self
            .tabs
            .active()
            .document
            .as_ref()
            .and_then(|document| control_kind(&document.borrow(), node));
        match kind {
            Some(ControlKind::Checkbox) => self.toggle_control(node),
            Some(ControlKind::Radio) => self.set_control_checked(node, true),
            Some(ControlKind::Select) => self.cycle_select(node, 1),
            Some(ControlKind::Reset) => self.reset_control_form(node),
            Some(ControlKind::Submit) => self.submit_control_form(node),
            _ => {}
        }
    }

    pub(super) fn handle_form_key(&mut self, event: &KeyEvent) -> bool {
        if self.focus != Focus::Content || event.modifiers.alt {
            return false;
        }
        let Some(node) = self.tabs.active().dom_focus.map(|focus| focus.node) else {
            return false;
        };
        let kind = self
            .tabs
            .active()
            .document
            .as_ref()
            .and_then(|document| control_kind(&document.borrow(), node));
        if kind.is_some_and(ControlKind::is_text_entry) {
            return self.edit_text_control(node, kind.unwrap(), event);
        }
        match (kind, &event.code) {
            (Some(ControlKind::Checkbox | ControlKind::Radio), Key::Char(' '))
            | (
                Some(ControlKind::Submit | ControlKind::Reset | ControlKind::Button),
                Key::Char(' '),
            )
            | (Some(ControlKind::Select), Key::Enter | Key::Char(' ')) => {
                self.activate_focused();
                true
            }
            (Some(ControlKind::Select), Key::Down | Key::Right) => {
                self.cycle_select(node, 1);
                true
            }
            (Some(ControlKind::Select), Key::Up | Key::Left) => {
                self.cycle_select(node, -1);
                true
            }
            (Some(ControlKind::Select), Key::Home) => {
                self.select_option(node, 0);
                true
            }
            (Some(ControlKind::Select), Key::End) => {
                let last = self
                    .tabs
                    .active()
                    .document
                    .as_ref()
                    .map(|document| options(&document.borrow(), node).len().saturating_sub(1))
                    .unwrap_or(0);
                self.select_option(node, last);
                true
            }
            (_, Key::Enter) => {
                self.activate_focused();
                true
            }
            _ => false,
        }
    }

    fn edit_text_control(&mut self, node: NodeId, kind: ControlKind, event: &KeyEvent) -> bool {
        if event.code == Key::Enter && kind != ControlKind::TextArea {
            self.submit_implicit_form(node);
            return true;
        }
        self.initialize_form_cursor(node);
        let edit = self
            .tabs
            .active_mut()
            .text_fields
            .get_mut(&node)
            .map(|field| field.handle_key(event, kind == ControlKind::TextArea))
            .unwrap_or_default();
        if !edit.consumed {
            return false;
        }
        self.clipboard_request = edit.clipboard;
        if edit.changed {
            self.sync_text_field_value(node);
        }
        self.damage_text_field(node);
        true
    }

    fn page_focus_ring(&self) -> Vec<NodeId> {
        let tab = self.tabs.active();
        let Some(document) = tab.document.as_ref() else {
            return Vec::new();
        };
        let document = document.borrow();
        let mut ring = Vec::new();
        let mut stack: Vec<NodeId> = document.roots().iter().rev().copied().collect();
        while let Some(node) = stack.pop() {
            stack.extend(document.children(node).into_iter().rev());
            let painted_link = tab
                .painted
                .links
                .iter()
                .any(|link| link.node == node && !link.rects.is_empty());
            let painted_control = tab.painted.hits.iter().any(|hit| hit.node == node);
            if painted_link || painted_control && self.operable_control(&document, node) {
                ring.push(node);
            }
        }
        ring
    }

    fn operable_control(&self, document: &crate::core::dom::Document, node: NodeId) -> bool {
        let Some(kind) = control_kind(document, node) else {
            return false;
        };
        if matches!(kind, ControlKind::Hidden | ControlKind::Unsupported)
            || crate::core::form::is_disabled(document, node)
        {
            return false;
        }
        !matches!(document.node(node), Some(Node::Element { attrs, .. })
            if kind == ControlKind::Select && has_attr(attrs, "multiple"))
    }

    pub(super) fn initialize_form_cursor(&mut self, node: NodeId) {
        let Some(document) = self.tabs.active().document.clone() else {
            return;
        };
        let (value, placeholder) = {
            let document = document.borrow();
            let Some(load) = self.tabs.active().load.as_ref() else {
                return;
            };
            if !control_kind(&document, node).is_some_and(ControlKind::is_text_entry) {
                return;
            }
            let placeholder = match document.node(node) {
                Some(Node::Element { attrs, .. }) => attr_value(attrs, "placeholder").unwrap_or(""),
                _ => "",
            };
            (
                text_value(&document, node, load.form_state()),
                placeholder.to_string(),
            )
        };
        self.tabs
            .active_mut()
            .text_fields
            .entry(node)
            .or_insert_with(|| TextFieldState::with_text(&value).with_placeholder(&placeholder));
    }

    pub(super) fn position_form_cursor(&mut self, node: NodeId, col: usize, row: usize) {
        self.position_form_selection(node, col, row, false);
    }

    pub(super) fn extend_form_selection(&mut self, node: NodeId, col: usize, row: usize) {
        self.position_form_selection(node, col, row, true);
    }

    pub(super) fn position_form_context_cursor(&mut self, node: NodeId, col: usize, row: usize) {
        self.initialize_form_cursor(node);
        let Some(index) = self.text_field_index_at(node, col, row) else {
            return;
        };
        let keep_selection = self.tabs.active().text_fields[&node]
            .selection()
            .is_some_and(|selection| selection.contains(&index));
        if !keep_selection {
            self.tabs
                .active_mut()
                .text_fields
                .get_mut(&node)
                .unwrap()
                .set_cursor(index, false);
        }
        self.damage_text_field(node);
    }

    fn position_form_selection(&mut self, node: NodeId, col: usize, row: usize, extend: bool) {
        self.initialize_form_cursor(node);
        let Some(index) = self.text_field_index_at(node, col, row) else {
            return;
        };
        self.tabs
            .active_mut()
            .text_fields
            .get_mut(&node)
            .unwrap()
            .set_cursor(index, extend);
        self.damage_text_field(node);
    }

    fn text_field_index_at(&self, node: NodeId, col: usize, row: usize) -> Option<usize> {
        let (rect, frame, kind) = self.text_field_geometry(node)?;
        let field = self.tabs.active().text_fields.get(&node)?;
        let mut view = TextFieldView::new(field)
            .frame(frame)
            .multiline(kind == ControlKind::TextArea);
        if kind == ControlKind::Password {
            view = view.mask('*');
        }
        Some(view.index_at(layout_rect(rect), col as u16, row as u16))
    }

    fn focus_row(&self, node: NodeId) -> Option<usize> {
        let tab = self.tabs.active();
        tab.painted
            .links
            .iter()
            .find(|link| link.node == node)
            .and_then(|link| link.rects.first())
            .map(|rect| rect.row)
            .or_else(|| {
                tab.painted
                    .hits
                    .iter()
                    .find(|hit| hit.node == node)
                    .map(|hit| hit.rect.row)
            })
    }

    fn toggle_control(&mut self, node: NodeId) {
        let checked = self
            .tabs
            .active()
            .document
            .as_ref()
            .zip(self.tabs.active().load.as_ref())
            .is_some_and(|(document, load)| {
                checkedness(&document.borrow(), node, load.form_state())
            });
        self.set_control_checked(node, !checked);
    }

    fn set_control_checked(&mut self, node: NodeId, checked: bool) {
        let page = self
            .tabs
            .active_mut()
            .load
            .as_mut()
            .and_then(|load| load.set_form_checked(node, checked).ok().flatten());
        self.apply_form_page(page);
    }

    fn cycle_select(&mut self, node: NodeId, delta: isize) {
        let next = self
            .tabs
            .active()
            .document
            .as_ref()
            .zip(self.tabs.active().load.as_ref())
            .map(|(document, load)| {
                let document = document.borrow();
                let len = options(&document, node).len().max(1);
                let current = selected_index(&document, node, load.form_state()).unwrap_or(0);
                if delta < 0 {
                    current.checked_sub(1).unwrap_or(len - 1)
                } else {
                    (current + 1) % len
                }
            })
            .unwrap_or(0);
        self.select_option(node, next);
    }

    fn select_option(&mut self, node: NodeId, index: usize) {
        let page = self
            .tabs
            .active_mut()
            .load
            .as_mut()
            .and_then(|load| load.select_form_option(node, index).ok().flatten());
        self.apply_form_page(page);
    }

    fn reset_control_form(&mut self, node: NodeId) {
        let form = self
            .tabs
            .active()
            .document
            .as_ref()
            .and_then(|document| form_owner(&document.borrow(), node));
        let page = form.and_then(|form| {
            self.tabs
                .active_mut()
                .load
                .as_mut()
                .and_then(|load| load.reset_form(form))
        });
        self.apply_form_page(page);
    }

    fn submit_implicit_form(&mut self, node: NodeId) {
        let submitter = self.tabs.active().document.as_ref().and_then(|document| {
            let document = document.borrow();
            let form = form_owner(&document, node)?;
            let mut stack: Vec<NodeId> = document.roots().iter().rev().copied().collect();
            while let Some(candidate) = stack.pop() {
                stack.extend(document.children(candidate).into_iter().rev());
                if form_owner(&document, candidate) == Some(form)
                    && control_kind(&document, candidate) == Some(ControlKind::Submit)
                    && !crate::core::form::is_disabled(&document, candidate)
                {
                    return Some(candidate);
                }
            }
            None
        });
        if let Some(submitter) = submitter {
            self.submit_control_form(submitter);
            return;
        }
        let blocking_fields = self
            .tabs
            .active()
            .document
            .as_ref()
            .map(|document| {
                let document = document.borrow();
                let Some(form) = form_owner(&document, node) else {
                    return 0;
                };
                let mut count = 0;
                let mut stack: Vec<NodeId> = document.roots().iter().rev().copied().collect();
                while let Some(candidate) = stack.pop() {
                    stack.extend(document.children(candidate).into_iter().rev());
                    if form_owner(&document, candidate) == Some(form)
                        && matches!(
                            control_kind(&document, candidate),
                            Some(ControlKind::Text | ControlKind::Password)
                        )
                        && !crate::core::form::is_disabled(&document, candidate)
                    {
                        count += 1;
                    }
                }
                count
            })
            .unwrap_or(0);
        if blocking_fields > 1 {
            return;
        }
        let result = self
            .tabs
            .active()
            .load
            .as_ref()
            .map(|load| load.implicit_form_submission(node));
        match result {
            Some(Ok(submission)) => self.submit_form_submission(submission),
            Some(Err(error)) => self.pending(&error.to_string()),
            None => {}
        }
    }

    fn submit_control_form(&mut self, node: NodeId) {
        let result = self
            .tabs
            .active()
            .load
            .as_ref()
            .map(|load| load.form_submission(node));
        match result {
            Some(Ok(submission)) => self.submit_form_submission(submission),
            Some(Err(error)) => self.pending(&error.to_string()),
            None => {}
        }
    }

    fn apply_form_page(&mut self, page: Option<RenderedPage>) {
        if let Some(page) = page {
            let width = self.geometry.content_cols();
            let rows = self.geometry.content_rows();
            let tab = self.tabs.active_mut();
            apply_rendered_page(tab, page, width, rows);
        } else if !self.advance_render_queue() {
            return;
        }
        self.tabs.active_mut().text_fields.clear();
        if self
            .tabs
            .active()
            .dom_focus
            .is_some_and(|focus| !self.page_focus_ring().contains(&focus.node))
        {
            self.tabs.active_mut().dom_focus = None;
        }
        self.rederive_hover();
        self.touch();
    }

    pub(super) fn content_cursor(&self) -> Option<(usize, usize)> {
        if self.focus != Focus::Content {
            return None;
        }
        let tab = self.tabs.active();
        let node = tab.dom_focus?.node;
        let state = tab.text_fields.get(&node)?;
        let (rect, frame, kind) = self.text_field_geometry(node)?;
        let mut view = TextFieldView::new(state)
            .frame(frame)
            .multiline(kind == ControlKind::TextArea);
        if kind == ControlKind::Password {
            view = view.mask('*');
        }
        let (col, row) = view.cursor_in(layout_rect(rect))?;
        Some((usize::from(col), usize::from(row)))
    }

    pub(super) fn content_text_fields(&self) -> Vec<ContentTextField<'_>> {
        let tab = self.tabs.active();
        tab.text_fields
            .iter()
            .filter_map(|(&node, state)| {
                let (rect, frame, kind) = self.text_field_geometry(node)?;
                let mut view = TextFieldView::new(state)
                    .frame(frame)
                    .multiline(kind == ControlKind::TextArea);
                if kind == ControlKind::Password {
                    view = view.mask('*');
                }
                if tab.dom_focus.is_none_or(|focus| focus.node != node) {
                    view = view.without_selection();
                }
                Some(ContentTextField {
                    rect,
                    view,
                    style: field_style(tab, rect),
                })
            })
            .collect()
    }

    fn text_field_geometry(
        &self,
        node: NodeId,
    ) -> Option<(LayoutRect, TextFieldFrame, ControlKind)> {
        let tab = self.tabs.active();
        let document = tab.document.as_ref()?.borrow();
        let kind = control_kind(&document, node).filter(|kind| kind.is_text_entry())?;
        let mut rects = tab
            .painted
            .hits
            .iter()
            .filter(|hit| hit.node == node && hit.kind == HitKind::Text)
            .map(|hit| hit.rect);
        let first = rects.next()?;
        let rect = rects.fold(first, |union, rect| LayoutRect {
            col: union.col.min(rect.col),
            row: union.row.min(rect.row),
            width: union
                .col
                .saturating_add(union.width)
                .max(rect.col.saturating_add(rect.width))
                .saturating_sub(union.col.min(rect.col)),
            height: union
                .row
                .saturating_add(union.height)
                .max(rect.row.saturating_add(rect.height))
                .saturating_sub(union.row.min(rect.row)),
        });
        let bracketed = kind != ControlKind::TextArea
            && tab
                .painted
                .hits
                .iter()
                .find(|hit| hit.node == node && hit.kind == HitKind::Box)
                .is_some_and(|hit| hit.rect == rect);
        Some((
            rect,
            if bracketed {
                TextFieldFrame::Brackets
            } else {
                TextFieldFrame::None
            },
            kind,
        ))
    }

    pub(super) fn damage_text_field(&mut self, node: NodeId) {
        if let Some((rect, _, _)) = self.text_field_geometry(node) {
            self.damage
                .repaint_rows(rect.row..rect.row.saturating_add(rect.height));
        }
    }

    pub(super) fn sync_text_field_value(&mut self, node: NodeId) {
        let Some(value) = self
            .tabs
            .active()
            .text_fields
            .get(&node)
            .map(|field| field.text().to_string())
        else {
            return;
        };
        let document = self.tabs.active().document.clone();
        if let Some(load) = self.tabs.active_mut().load.as_mut() {
            let _ = load.set_form_text(node, value);
        }
        let accepted = document
            .as_ref()
            .zip(self.tabs.active().load.as_ref())
            .map(|(document, load)| text_value(&document.borrow(), node, load.form_state()));
        if let Some(accepted) = accepted
            && let Some(field) = self.tabs.active_mut().text_fields.get_mut(&node)
        {
            field.reconcile_text(&accepted);
        }
    }

    pub(super) fn paste_form_text(&mut self, text: &str) {
        let Some(node) = self.tabs.active().dom_focus.map(|focus| focus.node) else {
            return;
        };
        let Some((_, _, kind)) = self.text_field_geometry(node) else {
            return;
        };
        let text = if kind == ControlKind::TextArea {
            text.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            text.replace(['\r', '\n'], "")
        };
        let changed = self
            .tabs
            .active_mut()
            .text_fields
            .get_mut(&node)
            .is_some_and(|field| field.paste(&text));
        if changed {
            self.sync_text_field_value(node);
            self.damage_text_field(node);
        }
    }
}

fn layout_rect(rect: LayoutRect) -> ratatui::layout::Rect {
    ratatui::layout::Rect::new(
        u16::try_from(rect.col).unwrap_or(u16::MAX),
        u16::try_from(rect.row).unwrap_or(u16::MAX),
        u16::try_from(rect.width).unwrap_or(u16::MAX),
        u16::try_from(rect.height).unwrap_or(u16::MAX),
    )
}

fn field_style(tab: &crate::app::tab::Tab, rect: LayoutRect) -> crate::core::style::CellStyle {
    use unicode_width::UnicodeWidthStr;

    tab.painted
        .row(rect.row)
        .and_then(|row| {
            row.spans.iter().find(|span| {
                span.col <= rect.col
                    && rect.col
                        < span
                            .col
                            .saturating_add(UnicodeWidthStr::width(span.text.as_str()))
            })
        })
        .map_or_else(crate::core::style::CellStyle::default, |span| span.style)
}

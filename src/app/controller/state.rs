use crate::core::dom::{AttrNs, ElementNs, Node, NodeId};
use crate::core::focus::Focus;
use crate::core::form::{ControlKind, control_kind, is_disabled};
use crate::core::style::Cursor;
use crate::css::{DynamicState, FocusSource, FocusedNode};

use super::{App, apply_rendered_page};

impl App {
    pub(super) fn settle_dynamic_state(&mut self, now: std::time::Duration) {
        let Some(deadline) = self.dynamic_settle else {
            return;
        };
        if now < deadline {
            return;
        }
        self.dynamic_settle = None;
        self.dynamic_pending = false;
        if self.sync_dynamic_state() {
            self.touch();
        }
    }

    pub(super) fn dynamic_state(&self) -> DynamicState {
        DynamicState {
            hover: self.hover.as_ref().map(|hover| hover.node),
            focus: (self.focus == Focus::Content)
                .then_some(self.tabs.active().dom_focus)
                .flatten(),
            active: self
                .pressed
                .as_ref()
                .filter(|pressed| pressed.button == crate::core::event::MouseButton::Left)
                .map(|pressed| pressed.node),
        }
    }

    pub fn pointer_cursor(&self) -> Cursor {
        let Some(mut node) = self.hover.as_ref().map(|hover| hover.node) else {
            return Cursor::Auto;
        };
        let tab = self.tabs.active();
        let (Some(document), Some(styles)) = (&tab.document, &tab.styles) else {
            return Cursor::Auto;
        };
        let document = document.borrow();
        loop {
            if matches!(document.node(node), Some(Node::Element { .. })) {
                return styles.get(node).cursor;
            }
            let Some(parent) = document.parent(node) else {
                return Cursor::Auto;
            };
            node = parent;
        }
    }

    pub(super) fn sync_dynamic_state(&mut self) -> bool {
        if self.input_transaction {
            self.dynamic_pending = true;
            return false;
        }
        let state = self.dynamic_state();
        let (rendered, mut painted_changed) = self.apply_dynamic_state(state);
        if !rendered {
            return painted_changed;
        }
        self.rederive_hover();
        let next = self.dynamic_state();
        if next != state {
            let (_, changed) = self.apply_dynamic_state(next);
            painted_changed |= changed;
        }
        painted_changed
    }

    fn apply_dynamic_state(&mut self, state: DynamicState) -> (bool, bool) {
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        let tab = self.tabs.active_mut();
        let page = tab
            .load
            .as_mut()
            .and_then(|load| load.set_dynamic_state(state));
        if let Some(page) = page {
            let applied = apply_rendered_page(tab, page, width, rows);
            for range in applied.rows {
                self.damage.repaint_rows(range);
            }
            return (true, applied.full);
        }
        let advance = self.advance_render_queue_result();
        (advance.published, advance.painted_changed)
    }

    pub(super) fn focusable_ancestor(&self, mut node: NodeId) -> Option<NodeId> {
        let document = self.tabs.active().document.as_ref()?.borrow();
        loop {
            if let Some(Node::Element { name, ns, attrs }) = document.node(node)
                && *ns == ElementNs::Html
            {
                let focusable = name == "a"
                    && attrs
                        .iter()
                        .any(|attr| attr.ns == AttrNs::None && attr.name == "href")
                    || control_kind(&document, node).is_some_and(|kind| {
                        !matches!(kind, ControlKind::Hidden | ControlKind::Unsupported)
                            && !is_disabled(&document, node)
                    });
                if focusable {
                    return Some(node);
                }
            }
            node = document.parent(node)?;
        }
    }

    pub(super) fn set_pointer_focus(&mut self, node: Option<NodeId>) {
        self.tabs.active_mut().dom_focus = node.map(|node| FocusedNode {
            node,
            source: FocusSource::Pointer,
        });
    }
}

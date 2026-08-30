use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use style::context::{SharedStyleContext, StyleContext};
use style::dom::TNode;
use style::traversal::{DomTraversal, PerLevelTraversalData, recalc_style_at};

use super::dom::{StyloElement, StyloNode};

/// The style-only traversal: resolve every element top-down, do nothing on the way up.
///
/// `DomTraversal` exposes `shared_context(&self)`, so the traversal *owns* the shared context rather
/// than receiving one. The read guard that context borrows has to outlive it, which is why the
/// cascade entry point builds guard → context → traversal in one scope instead of returning any of
/// them.
pub(super) struct RecalcStyle<'a> {
    context: SharedStyleContext<'a>,
    /// Elements this traversal actually recomputed.
    ///
    /// Atomic rather than a `Cell` because `DomTraversal` is `Sync`. This is the number that
    /// distinguishes a restyle which visited three elements from one which quietly walked the whole
    /// document in a similar time; timing alone cannot tell those apart.
    visited: AtomicUsize,
    visited_nodes: Mutex<Vec<crate::core::dom::NodeId>>,
}

impl<'a> RecalcStyle<'a> {
    pub(super) fn new(context: SharedStyleContext<'a>) -> Self {
        Self {
            context,
            visited: AtomicUsize::new(0),
            visited_nodes: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn visited(&self) -> usize {
        self.visited.load(Ordering::Relaxed)
    }

    pub(super) fn visited_nodes(&self) -> Vec<crate::core::dom::NodeId> {
        self.visited_nodes.lock().unwrap().clone()
    }
}

impl<'a, 'dom> DomTraversal<StyloElement<'dom>> for RecalcStyle<'a> {
    fn process_preorder<F>(
        &self,
        traversal_data: &PerLevelTraversalData,
        context: &mut StyleContext<StyloElement<'dom>>,
        node: StyloNode<'dom>,
        note_child: F,
    ) where
        F: FnMut(StyloNode<'dom>),
    {
        let Some(element) = node.as_element() else {
            // Text nodes take their parent's style at layout time; Stylo resolves nothing for them.
            return;
        };
        self.visited.fetch_add(1, Ordering::Relaxed);
        if let Some(id) = element.dom_id() {
            self.visited_nodes.lock().unwrap().push(id);
        }
        let mut data = element.ensure_style_data();
        recalc_style_at(
            self,
            traversal_data,
            context,
            element,
            &mut data,
            note_child,
        );
    }

    /// Nothing bubbles up: the postorder pass exists for Servo's flow construction, and the terminal
    /// layout builds its boxes its own way.
    fn process_postorder(
        &self,
        _context: &mut StyleContext<StyloElement<'dom>>,
        _node: StyloNode<'dom>,
    ) {
    }

    fn needs_postorder_traversal() -> bool {
        false
    }

    fn shared_context(&self) -> &SharedStyleContext<'_> {
        &self.context
    }
}

use style::dom::{TElement, TNode};
use style::invalidation::element::state_and_attributes::propagate_dirty_bit_up_to;
use style::selector_parser::{ServoElementSnapshot, SnapshotMap};
use stylo_dom::ElementState;

use super::dom::{StyleDom, StyloElement};

/// A pending dynamic-state change: what the elements looked like before, so Stylo can diff.
///
/// Stylo never clears `has_snapshot` — it only ever sets `handled_snapshot` — so a tree reused
/// across restyles has to be reset between them. [`StateChange::apply`] does the reset, the
/// snapshotting and the state write together, which is the only order that leaves the tree
/// consistent.
pub(super) struct StateChange {
    snapshots: SnapshotMap,
}

impl StateChange {
    /// Record `state` onto `elements`, snapshotting whatever they held before.
    ///
    /// Hover is a chain, not a single element: `css/ua.rs` resolves it with `on_chain`, so the
    /// hovered node and all its ancestors are hovered, exactly as a browser would. Callers pass that
    /// whole chain rather than one element, and every member is snapshotted.
    pub(super) fn apply<'dom>(
        dom: &StyleDom<'dom>,
        elements: &[StyloElement<'dom>],
        state: ElementState,
    ) -> Self {
        // Any snapshot left over from an earlier restyle would be diffed against this one.
        for element in dom.elements() {
            element.forget_snapshot();
        }

        let root = dom.root_element();
        let mut snapshots = SnapshotMap::new();
        for element in elements {
            let previous = element.state();
            snapshots.insert(
                TElement::as_node(element).opaque(),
                ServoElementSnapshot {
                    state: Some(previous),
                    ..ServoElementSnapshot::new()
                },
            );
            element.note_snapshot();
            element.set_state(previous | state);

            // The traversal only descends into a child `element_needs_traversal` approves of, and an
            // ancestor with current styles, no hint and no snapshot is not approved — so without
            // this the snapshot on a deep element is never reached and the restyle silently does
            // nothing. A hover chain happens to mark its own ancestors, but `:focus` on one deep
            // element would not.
            if let Some(root) = root
                && *element != root
            {
                // The helper walks up from the *parent*, so calling it on the root itself would fall
                // through to its own `debug_assert!(false, "Should've found … as an ancestor")`.
                propagate_dirty_bit_up_to(root, *element);
            }
        }
        Self { snapshots }
    }

    pub(super) fn snapshots(&self) -> &SnapshotMap {
        &self.snapshots
    }
}

/// The hover chain for an element: itself and every ancestor, matching `css::ua`'s `on_chain`.
pub(super) fn hover_chain<'dom>(element: StyloElement<'dom>) -> Vec<StyloElement<'dom>> {
    let mut chain = vec![element];
    let mut current = element;
    while let Some(parent) = TElement::traversal_parent(&current) {
        chain.push(parent);
        current = parent;
    }
    chain
}

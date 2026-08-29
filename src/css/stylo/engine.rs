use style::Atom;
use style::context::{
    QuirksMode, RegisteredSpeculativePainter, RegisteredSpeculativePainters, SharedStyleContext,
    StyleContext, StyleSystemOptions, ThreadLocalStyleContext,
};
use style::dom::TElement;
use style::driver;
use style::selector_parser::SnapshotMap;
use style::servo::animation::DocumentAnimationSet;
use style::shared_lock::{SharedRwLock, StylesheetGuards};
use style::style_resolver::{PseudoElementResolution, StyleResolverForElement};
use style::stylist::{RuleInclusion, Stylist};
use style::thread_state::{self, ThreadState};
use style::traversal::DomTraversal;
use style::traversal_flags::TraversalFlags;

use crate::core::dom::Document;
use crate::core::geom::Size;
use crate::core::style::{CellMetric, Palette};

use super::device::device;
use super::dom::{StyleArena, StyleDom, StyloElement};
use super::sheets;
use super::traversal::RecalcStyle;

/// The terminal has no paint worklets.
#[derive(Debug)]
struct NoPainters;

impl RegisteredSpeculativePainters for NoPainters {
    fn get(&self, _name: &Atom) -> Option<&dyn RegisteredSpeculativePainter> {
        None
    }
}

/// Marks the calling thread as a layout thread.
///
/// `SequentialTaskList::drop` asserts this in debug builds and `ThreadLocalStyleContext` owns one,
/// so anything that touches the style system panics on drop without it. `initialize` is idempotent
/// for a repeated identical value, unlike `enter`, which asserts on re-entry — so this is safe to
/// call once per test and once per worker thread.
pub(super) fn mark_layout_thread() {
    thread_state::initialize(ThreadState::LAYOUT);
}

/// A retained Stylo cascade: the rule database plus the device it was built against.
///
/// This type is `!Send` by construction — `Device` owns a `Box<dyn FontMetricsProvider>`, which is
/// `Sync` but not `Send` outside Gecko — so it cannot cross a channel even by accident. That is the
/// architecture invariant, enforced by the type system rather than by convention.
pub(super) struct StyloEngine {
    lock: SharedRwLock,
    stylist: Stylist,
    painters: NoPainters,
}

impl StyloEngine {
    /// Build an engine over the user-agent sheet plus any author sheets.
    ///
    /// The stylist is flushed here, so no caller can observe one whose `cascade_data` has not been
    /// rebuilt — forgetting that flush produces an empty rule database and no error at all.
    pub(super) fn new(
        metric: CellMetric,
        viewport: Size,
        quirks_mode: QuirksMode,
        palette: Palette,
        author_css: &[&str],
    ) -> Self {
        Self::with_metrics(
            viewport,
            quirks_mode,
            palette,
            author_css,
            device(metric, viewport, quirks_mode),
        )
    }

    pub(super) fn with_metrics(
        _viewport: Size,
        quirks_mode: QuirksMode,
        palette: Palette,
        author_css: &[&str],
        device: style::device::Device,
    ) -> Self {
        mark_layout_thread();
        // Before the first `Stylesheet::from_str`: Stylo reads its preferences at parse time.
        super::prefs::enable();
        let lock = SharedRwLock::new();
        let mut stylist = Stylist::new(device, quirks_mode);
        {
            let guard = lock.read();
            stylist.append_stylesheet(sheets::user_agent_sheet(&lock, quirks_mode), &guard);
            stylist.append_stylesheet(sheets::user_sheet(palette, &lock, quirks_mode), &guard);
            for css in author_css {
                stylist.append_stylesheet(sheets::author_sheet(css, &lock, quirks_mode), &guard);
            }
            stylist.flush(&StylesheetGuards::same(&guard));
        }
        Self {
            lock,
            stylist,
            painters: NoPainters,
        }
    }

    /// Mirror `document` for this engine.
    ///
    /// The only way to build a mirror that will be cascaded, because two invariants tie the mirror
    /// to the engine and neither can be checked at a call site:
    ///
    /// - **One lock.** An element's `style` attribute is a `Locked<PropertyDeclarationBlock>`, and
    ///   `Locked::read_with` asserts — in every profile, not just debug — that the guard came from
    ///   the same `SharedRwLock`. Stylo reads it through `guards.author` while collecting rules and
    ///   again in the style sharing cache's `have_same_style_attribute`, which every same-tag
    ///   sibling pair reaches, so a mirror built with its own lock panics on the first such page.
    /// - **One order.** [`Self::with_metrics`] enables Stylo's preferences, and Stylo reads them at
    ///   *parse* time. A mirror built before the engine parses its style attributes with the
    ///   defaults, which silently drops every grid longhand.
    pub(super) fn mirror<'a>(
        &self,
        arena: &'a StyleArena<'a>,
        document: &Document,
    ) -> StyleDom<'a> {
        StyleDom::build(arena, document, self.lock.clone())
    }

    /// Run `f` with a fully assembled style context.
    ///
    /// A closure rather than a constructor because `SharedStyleContext` borrows both the stylist and
    /// the stylesheet read guard; a function returning one cannot be written.
    pub(super) fn with_style_context<'dom, R>(
        &self,
        f: impl FnOnce(&mut StyleContext<'_, StyloElement<'dom>>) -> R,
    ) -> R {
        let guard = self.lock.read();
        let snapshots = SnapshotMap::new();
        let shared = SharedStyleContext {
            stylist: &self.stylist,
            visited_styles_enabled: false,
            options: StyleSystemOptions::default(),
            guards: StylesheetGuards::same(&guard),
            current_time_for_animations: 0.0,
            traversal_flags: TraversalFlags::empty(),
            snapshot_map: &snapshots,
            animations: DocumentAnimationSet::default(),
            registered_speculative_painters: &self.painters,
        };
        let mut thread_local = ThreadLocalStyleContext::new();
        let mut context = StyleContext {
            shared: &shared,
            thread_local: &mut thread_local,
        };
        f(&mut context)
    }

    /// Cascade the whole tree, returning how many elements were styled.
    pub(super) fn cascade(&self, dom: &StyleDom<'_>) -> usize {
        self.traverse(dom, &SnapshotMap::new());
        dom.styled_element_count()
    }

    /// Re-run the traversal against a populated snapshot map, returning how many elements Stylo
    /// actually recomputed.
    ///
    /// There is no explicit invalidation call: `DomTraversal::pre_traverse` runs
    /// `ElementData::invalidate_style_if_needed` on the root, and `note_children` runs it for every
    /// child the traversal walks past, so the invalidator is driven for us. What the caller owes is
    /// the snapshot bookkeeping in `super::invalidate`.
    pub(super) fn restyle(&self, dom: &StyleDom<'_>, snapshots: &SnapshotMap) -> usize {
        self.traverse(dom, snapshots)
    }

    /// The guard, the shared context and the traversal are all built here because each borrows the
    /// one before it; none can be returned separately.
    fn traverse(&self, dom: &StyleDom<'_>, snapshots: &SnapshotMap) -> usize {
        let Some(root) = dom.root_element() else {
            return 0;
        };
        let guard = self.lock.read();
        let shared = SharedStyleContext {
            stylist: &self.stylist,
            visited_styles_enabled: false,
            options: StyleSystemOptions::default(),
            guards: StylesheetGuards::same(&guard),
            current_time_for_animations: 0.0,
            traversal_flags: TraversalFlags::empty(),
            snapshot_map: snapshots,
            animations: DocumentAnimationSet::default(),
            registered_speculative_painters: &self.painters,
        };
        let token = RecalcStyle::pre_traverse(root, &shared);
        if !token.should_traverse() {
            // `traverse_dom` panics rather than no-opping on a token that says otherwise.
            return 0;
        }
        let traversal = RecalcStyle::new(shared);
        // `None` keeps this the sequential breadth-first walk: no rayon pool is ever built.
        driver::traverse_dom(&traversal, token, None);
        traversal.visited()
    }

    /// Resolve `element` and every unstyled ancestor above it, nearest-root first.
    ///
    /// Stylo reads a parent's computed style out of the parent's own `ElementData`, so a child
    /// cannot be resolved before its ancestors; doing so silently resolves against the initial
    /// style instead of the inherited one.
    pub(super) fn resolve_with_ancestors(&self, element: StyloElement<'_>) {
        let mut chain = vec![element];
        let mut current = element;
        while let Some(parent) = TElement::traversal_parent(&current) {
            chain.push(parent);
            current = parent;
        }
        for element in chain.into_iter().rev() {
            if TElement::has_data(&element) {
                continue;
            }
            self.resolve_one(element);
        }
    }

    fn resolve_one(&self, element: StyloElement<'_>) {
        self.with_style_context(|context| {
            let resolved = StyleResolverForElement::new(
                element,
                context,
                RuleInclusion::All,
                PseudoElementResolution::IfApplicable,
            )
            .resolve_style_with_default_parents();
            element.set_styles(resolved.into());
        });
    }
}

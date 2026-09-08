//! `ComputedValues` → [`ComputedStyle`] (M7 S4b).
//!
//! Stylo computes in CSS pixels against the `Device`'s viewport; `core::style` holds cell-quantised
//! values. This module is the whole of the conversion, and the only place in the crate outside
//! `css::stylo` that would otherwise need to name a `style::` type.

mod alignment;
mod color;
mod display;
mod edges;
mod flex;
mod generated;
mod grid;
mod image;
mod length;
mod policy;
mod sizing;
mod text;
mod transform;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use servo_arc::Arc as ServoArc;
use style::dom::{TDocument, TNode};
use style::properties::ComputedValues;
use style::values::computed::text::TextDecorationLine;

use crate::core::style::{
    ComputedStyle, LegacyAlign, LengthAxis, RenderContext, StyleStore, StyleTree,
};

pub(super) use color::SENTINEL_CSS;

use super::dom::StyleDom;
use super::engine::StyloEngine;
use length::Lengths;
use policy::ElementPolicy;

/// Map every styled element of `dom` into a [`StyleTree`].
///
/// `context` must be the one the engine's `Device` was built from: lengths are resolved against
/// that viewport and quantised against that cell metric, and passing two different metrics would
/// produce a consistent-looking tree measured with two rulers. Making that structural — one input
/// carrying engine and mapper together — is S2's job.
///
/// Pseudo-element boxes and list markers stay empty; generated content is S5.
pub(super) fn style_tree(
    dom: &StyleDom<'_>,
    engine: &StyloEngine,
    document: &crate::core::dom::Document,
    context: RenderContext,
) -> StyleTree {
    style_tree_measured(dom, engine, document, context).0
}

/// How much work one [`style_tree`] pass did.
///
/// `distinct` is the point of the memo: Stylo's sharing cache hands long runs of elements one
/// `ComputedValues` allocation, so a healthy page maps far fewer styles than it has elements. A
/// mapper that defeated that sharing would show up here and nowhere else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MapStats {
    pub(super) elements: usize,
    pub(super) distinct: usize,
}

pub(super) fn style_tree_measured(
    dom: &StyleDom<'_>,
    engine: &StyloEngine,
    document: &crate::core::dom::Document,
    context: RenderContext,
) -> (StyleTree, MapStats) {
    let mut mapper = Mapper::new(context);
    let mut tree = StyleTree::default();
    let mut elements = 0usize;

    // A pre-order walk, not `StyleDom::elements`, which is unordered: `super::policy` folds each
    // element's decorations into its descendants, so a parent must be mapped before its children.
    // Starting at the document node rather than `root_element` also covers a document with more
    // than one root.
    let mut stack = vec![(
        dom.document().as_node(),
        TextDecorationLine::empty(),
        LegacyAlign::None,
    )];
    while let Some((node, inherited_decorations, inherited_align)) = stack.pop() {
        let mut descend_decorations = inherited_decorations;
        let mut descend_align = inherited_align;
        if let Some(element) = node.as_element()
            && let Some(values) = element.primary_style()
        {
            let legacy_align = if element.has_author_text_align(&values) {
                LegacyAlign::None
            } else {
                element.legacy_align().unwrap_or(inherited_align)
            };
            let style = mapper.style(
                &values,
                ElementPolicy {
                    decorations: inherited_decorations,
                    control: element.control_kind(),
                    legacy_align,
                },
            );
            descend_decorations = inherited_decorations | text::decorations(&values);
            descend_align = legacy_align;
            elements += 1;
            if let Some(id) = element.dom_id() {
                tree.insert(id, style);
            }
        }
        // Reversed so the leftmost child is popped first; the order only has to be parent-first,
        // but document order keeps a failing test's output readable.
        let mut children = Vec::new();
        let mut child = node.first_child();
        while let Some(node) = child {
            children.push((node, descend_decorations, descend_align));
            child = node.next_sibling();
        }
        stack.extend(children.into_iter().rev());
    }

    let stats = MapStats {
        elements,
        distinct: mapper.memo.len(),
    };
    generated::populate(dom, engine, document, &mut mapper, &mut tree);
    tree.set_store(mapper.store);
    (tree, stats)
}

struct Mapper {
    context: RenderContext,
    lengths: Lengths,
    store: StyleStore,
    memo: HashMap<ByPtr, ComputedStyle>,
}

impl Mapper {
    fn new(context: RenderContext) -> Self {
        Self {
            context,
            lengths: Lengths::new(context.metrics.cell, context.viewport),
            store: StyleStore::default(),
            memo: HashMap::new(),
        }
    }

    fn with_store(context: RenderContext, store: StyleStore) -> Self {
        Self {
            context,
            lengths: Lengths::new(context.metrics.cell, context.viewport),
            store,
            memo: HashMap::new(),
        }
    }

    /// The mapped style, plus the bits that cannot be memoised.
    fn style(
        &mut self,
        values: &ServoArc<ComputedValues>,
        element: ElementPolicy,
    ) -> ComputedStyle {
        let mut style = match self.memo.get(&ByPtr(values.clone())) {
            Some(cached) => *cached,
            None => {
                let mapped = self.map(values);
                self.memo.insert(ByPtr(values.clone()), mapped);
                mapped
            }
        };
        policy::inherit_decorations(&mut style, element.decorations);
        policy::form_control(&mut style, element.control);
        policy::legacy_align(&mut style, element.legacy_align);
        style
    }

    fn map(&mut self, values: &ServoArc<ComputedValues>) -> ComputedStyle {
        let decorations = text::decorations(values);
        let font_size = text::font_size(values);
        let background_images = image::backgrounds(values, &self.lengths, &mut self.store);
        let masks = image::masks(values, &self.lengths, &mut self.store);
        let translation = transform::translation(values, &mut self.lengths, &mut self.store);
        let text_rendering = self.context.metrics.text;
        let lengths = &mut self.lengths;
        let store = &mut self.store;
        let mut style = ComputedStyle {
            display: display::display(values),
            float: sizing::float(values),
            clear: sizing::clear(values),
            width: sizing::size(
                &values.clone_width(),
                LengthAxis::Horizontal,
                lengths,
                store,
            ),
            height: sizing::size(&values.clone_height(), LengthAxis::Vertical, lengths, store),
            min_width: sizing::size(
                &values.clone_min_width(),
                LengthAxis::Horizontal,
                lengths,
                store,
            ),
            min_height: sizing::size(
                &values.clone_min_height(),
                LengthAxis::Vertical,
                lengths,
                store,
            ),
            max_width: sizing::max_size(
                &values.clone_max_width(),
                LengthAxis::Horizontal,
                lengths,
                store,
            ),
            max_height: sizing::max_size(
                &values.clone_max_height(),
                LengthAxis::Vertical,
                lengths,
                store,
            ),
            flex: flex::flex(values, lengths, store),
            alignment: alignment::alignment(values, lengths, store),
            grid: grid::grid(values, lengths, store),
            order: values.clone_order(),
            box_sizing: sizing::box_sizing(values),
            overflow: sizing::overflow(values),
            contain: sizing::contain(values),
            position: sizing::position(values),
            inset: edges::insets(values, lengths, store),
            translation,
            margin: edges::margins(values, lengths, store),
            padding: edges::paddings(values, lengths, store),
            border: edges::borders(values),
            white_space: text::white_space(values),
            cursor: text::cursor(values),
            visibility: text::visibility(values),
            table_layout: text::table_layout(values),
            border_collapse: text::border_collapse(values),
            border_spacing: text::border_spacing(values, lengths),
            caption_side: text::caption_side(values),
            direction: text::direction(values),
            empty_cells: text::empty_cells(values),
            text_align: text::text_align(values),
            vertical_align: text::vertical_align(values),
            list_style_type: text::list_style_type(values),
            list_style_position: text::list_style_position(values),
            color: color::foreground(&values.clone_color()),
            background: color::background(&values.resolve_color(&values.clone_background_color())),
            background_images,
            masks,
            bold: text::bold(values),
            underline: decorations.contains(TextDecorationLine::UNDERLINE),
            strike: decorations.contains(TextDecorationLine::LINE_THROUGH),
            font_size,
            text_presentation: font_size.presentation(text_rendering),
            ..ComputedStyle::default()
        };
        policy::hide_fully_transparent(&mut style, values.clone_opacity());
        style
    }
}

pub(super) fn restyle_tree_measured(
    dom: &StyleDom<'_>,
    engine: &StyloEngine,
    document: &crate::core::dom::Document,
    context: RenderContext,
    previous: &StyleTree,
    touched: &[crate::core::dom::NodeId],
) -> (StyleTree, MapStats) {
    if generated::requires_full_mapping(dom, previous, touched) {
        return style_tree_measured(dom, engine, document, context);
    }
    let mut tree = previous.clone();
    let mut mapper = Mapper::with_store(context, tree.cloned_store());
    let mut elements = 0usize;
    for id in touched {
        let Some(element) = dom.element(*id) else {
            continue;
        };
        let Some(values) = element.primary_style() else {
            continue;
        };
        let style = mapper.style(&values, policy_for(dom, document, *id, element));
        tree.insert(*id, style);
        elements += 1;
    }
    let stats = MapStats {
        elements,
        distinct: mapper.memo.len(),
    };
    tree.set_store(mapper.store);
    (tree, stats)
}

fn policy_for(
    dom: &StyleDom<'_>,
    document: &crate::core::dom::Document,
    id: crate::core::dom::NodeId,
    target: super::dom::StyloElement<'_>,
) -> ElementPolicy {
    let mut chain = Vec::new();
    let mut current = document.parent(id);
    while let Some(parent) = current {
        chain.push(parent);
        current = document.parent(parent);
    }
    let mut decorations = TextDecorationLine::empty();
    let mut legacy_align = LegacyAlign::None;
    for ancestor in chain.into_iter().rev() {
        let Some(element) = dom.element(ancestor) else {
            continue;
        };
        let Some(values) = element.primary_style() else {
            continue;
        };
        decorations |= text::decorations(&values);
        legacy_align = if element.has_author_text_align(&values) {
            LegacyAlign::None
        } else {
            element.legacy_align().unwrap_or(legacy_align)
        };
    }
    ElementPolicy {
        decorations,
        control: target.control_kind(),
        legacy_align: target.legacy_align().unwrap_or(legacy_align),
    }
}

/// A `ComputedValues` keyed by allocation identity.
///
/// Stylo's style sharing cache hands long runs of elements one allocation, so pointer identity is
/// what makes the mapper cost proportional to the number of *distinct* styles rather than to the
/// number of elements. The key owns the `Arc`, which is what keeps the address valid for as long as
/// the map holds it.
///
/// The address is the data pointer, not `ServoArc::heap_ptr`: that returns null for a statically
/// allocated `Arc`, which would collide every such style onto one key.
struct ByPtr(ServoArc<ComputedValues>);

impl ByPtr {
    fn address(&self) -> usize {
        std::ptr::from_ref::<ComputedValues>(&self.0) as usize
    }
}

impl PartialEq for ByPtr {
    fn eq(&self, other: &Self) -> bool {
        self.address() == other.address()
    }
}

impl Eq for ByPtr {}

impl Hash for ByPtr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.address().hash(state);
    }
}

use std::collections::HashMap;

use style::context::QuirksMode;
use stylo_dom::ElementState;

use crate::core::dom::{Document, DomQuirksMode, NodeId};
use crate::core::form::{FormState, control_kind};
use crate::core::style::{RenderContext, RenderMetrics, StyleTree};
use crate::css::{DynamicState, FocusSource, MediaContext};
use crate::pipeline::render::StyleInput;
use crate::pipeline::render::{StyleSessionId, StyleSource};

use super::dom::{StyleArena, StyleDom};
use super::engine::StyloEngine;

pub(crate) struct StyloRestyle {
    pub styles: StyleTree,
    pub visited: usize,
    pub touched: Vec<NodeId>,
    pub mapped: usize,
}

pub(crate) struct StyloSession<'a, 'dom> {
    engine: &'a StyloEngine,
    dom: &'a StyleDom<'dom>,
    document: &'a Document,
    dynamic: DynamicState,
    forms: FormState,
    snapshots: Vec<NodeId>,
}

pub(crate) fn with_session<R>(
    input: &StyleInput,
    document: &Document,
    forms: &FormState,
    media: MediaContext,
    f: impl for<'a, 'dom> FnOnce(&mut StyloSession<'a, 'dom>) -> R,
) -> R {
    let engine =
        StyloEngine::from_sources(media, quirks_mode(document.quirks_mode()), &input.roots);
    let arena = StyleArena::new();
    let dom = engine.mirror_with_form_state(&arena, document, forms);
    let mut session = StyloSession {
        engine: &engine,
        dom: &dom,
        document,
        dynamic: DynamicState::INERT,
        forms: forms.clone(),
        snapshots: Vec::new(),
    };
    f(&mut session)
}

pub(crate) fn cascade_once(
    document: &Document,
    forms: &FormState,
    media: MediaContext,
    author_css: &[&str],
) -> (StyleTree, usize) {
    let roots = author_css
        .iter()
        .map(|source| StyleSource {
            source: Some(std::sync::Arc::from(*source)),
            base_url: url::Url::parse("about:style").expect("the static style base parses"),
            media: std::sync::Arc::from(""),
            imports: std::sync::Arc::from([]),
        })
        .collect::<Vec<_>>()
        .into();
    let input = StyleInput {
        session: StyleSessionId {
            tab_id: 0,
            generation: 0,
            revision: 0,
        },
        roots,
    };
    with_session(&input, document, forms, media, |session| {
        (session.cascade(media), session.css_warnings())
    })
}

impl StyloSession<'_, '_> {
    pub(crate) fn css_warnings(&self) -> usize {
        self.engine.css_warnings()
    }

    pub(crate) fn cascade(&mut self, media: MediaContext) -> StyleTree {
        self.set_initial_dynamic(media.state);
        self.engine.cascade(self.dom);
        super::map::style_tree(self.dom, self.engine, self.document, render_context(media))
    }

    pub(crate) fn restyle(
        &mut self,
        media: MediaContext,
        previous: &StyleTree,
        forms: &FormState,
    ) -> StyloRestyle {
        let mut states = dynamic_states(self.document, self.dynamic, media.state);
        let dynamic = ElementState::HOVER
            | ElementState::ACTIVE
            | ElementState::FOCUS
            | ElementState::FOCUSRING
            | ElementState::FOCUS_WITHIN;
        let mut mask = dynamic;
        if &self.forms != forms {
            mask |= ElementState::CHECKED | ElementState::ENABLED | ElementState::DISABLED;
            for element in self.dom.elements() {
                let Some(id) = element.dom_id() else {
                    continue;
                };
                let desired = super::dom::static_state(self.document, forms, id);
                if let Some((_, state)) = states.iter_mut().find(|(other, _)| *other == id) {
                    *state |= desired;
                } else {
                    states.push((id, desired));
                }
            }
            self.forms = forms.clone();
        }
        let (change, changed) =
            super::invalidate::StateChange::replace(self.dom, &self.snapshots, &states, mask);
        let (visited, touched) = self.engine.restyle_with_nodes(self.dom, change.snapshots());
        self.snapshots = changed;
        self.dynamic = media.state;
        let (styles, mapped) = super::map::restyle_tree_measured(
            self.dom,
            self.engine,
            self.document,
            render_context(media),
            previous,
            &touched,
        );
        StyloRestyle {
            styles,
            visited,
            touched,
            mapped: mapped.elements,
        }
    }

    fn set_initial_dynamic(&mut self, state: DynamicState) {
        for (id, desired) in dynamic_states(self.document, DynamicState::INERT, state) {
            if let Some(element) = self.dom.element(id) {
                element.set_state(element.state() | desired);
            }
        }
        self.dynamic = state;
    }
}

fn dynamic_states(
    document: &Document,
    previous: DynamicState,
    next: DynamicState,
) -> Vec<(NodeId, ElementState)> {
    let mut states = HashMap::new();
    for target in [
        previous.hover,
        previous.active,
        previous.focus.map(|focus| focus.node),
    ]
    .into_iter()
    .flatten()
    {
        add_chain(document, target, ElementState::empty(), &mut states);
    }
    if let Some(target) = next.hover {
        add_chain(document, target, ElementState::HOVER, &mut states);
    }
    if let Some(target) = next.active {
        add_chain(document, target, ElementState::ACTIVE, &mut states);
    }
    if let Some(focus) = next.focus {
        add_chain(
            document,
            focus.node,
            ElementState::FOCUS_WITHIN,
            &mut states,
        );
        let entry = states.entry(focus.node).or_insert(ElementState::empty());
        *entry |= ElementState::FOCUS;
        if focus.source == FocusSource::Keyboard
            || control_kind(document, focus.node).is_some_and(|kind| kind.is_text_entry())
        {
            *entry |= ElementState::FOCUSRING;
        }
    }
    states.into_iter().collect()
}

fn add_chain(
    document: &Document,
    target: NodeId,
    state: ElementState,
    states: &mut HashMap<NodeId, ElementState>,
) {
    let mut current = Some(target);
    while let Some(id) = current {
        *states.entry(id).or_insert(ElementState::empty()) |= state;
        current = document.parent(id);
    }
}

fn render_context(media: MediaContext) -> RenderContext {
    RenderContext {
        viewport: media.viewport,
        metrics: RenderMetrics {
            cell: media.cell_metric,
            text: media.text_rendering,
        },
    }
}

fn quirks_mode(mode: DomQuirksMode) -> QuirksMode {
    match mode {
        DomQuirksMode::Quirks => QuirksMode::Quirks,
        DomQuirksMode::LimitedQuirks => QuirksMode::LimitedQuirks,
        DomQuirksMode::NoQuirks => QuirksMode::NoQuirks,
    }
}

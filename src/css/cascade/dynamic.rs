use std::collections::HashMap;
use std::sync::Arc;

use crate::core::dom::{Document, NodeId};
use crate::core::form::FormState;
use crate::core::style::{
    ComputedStyle, CssMaxSize, CssSize, Display, Marker, PseudoBox, PseudoElement, StyleStore,
    StyleTree,
};
use crate::css::effects::{DynamicEffect, property_effect};
use crate::css::parser::{StyleRule, parse_declarations};
use crate::css::presentational::presentational_hints;
use crate::css::selectors::{MatchTarget, matching_specificity_with_forms, uses_dynamic_state};
use crate::css::ua::{UaContext, inline_style, ua_style};
use crate::css::variables::{Environment, derive_environment, is_custom_name};
use crate::css::{StyleSheet, cascade::MediaContext};

use super::declaration::apply_declaration;
use super::document::{
    CascadeState, RuleIndex, compute_box_values, elements_in_document_order, is_replaced_element,
    pseudo_is_layout_item, resolve_declarations,
};
use super::media::active_style_rules;
use super::typography::apply_font_size;

pub(super) fn cascade_dynamic(
    sheets: &[StyleSheet],
    document: &Document,
    media: MediaContext,
    forms: &FormState,
    previous: &StyleTree,
    state: &CascadeState,
) -> Option<(StyleTree, CascadeState)> {
    let rules = active_style_rules(sheets, media);
    let index = RuleIndex::new(&rules, document);
    let previous_media = media.with_state(state.dynamic_state);
    let mut tree = previous.clone();
    let mut changed: HashMap<NodeId, bool> = HashMap::new();

    for (id, _) in elements_in_document_order(document) {
        let candidates = index.candidates(document, id);
        let parent_style = document.parent(id).map(|parent| tree.get(parent));
        let old_ua = ua_style(
            document,
            id,
            parent_style,
            UaContext {
                palette: previous_media.palette,
                state: previous_media.state,
            },
        );
        let new_ua = ua_style(
            document,
            id,
            parent_style,
            UaContext {
                palette: media.palette,
                state: media.state,
            },
        );
        let mut element_dirty = old_ua != new_ua;
        let mut pseudo_dirty = [false; 3];
        for rule_index in candidates.iter().copied() {
            let rule = rules[rule_index];
            if uses_dynamic_state(&rule.selectors) == Default::default() {
                continue;
            }
            for (target_index, target) in [
                MatchTarget::Element,
                MatchTarget::Pseudo(PseudoElement::Before),
                MatchTarget::Pseudo(PseudoElement::After),
                MatchTarget::Pseudo(PseudoElement::Marker),
            ]
            .into_iter()
            .enumerate()
            {
                let old = matching_specificity_with_forms(
                    &rule.selectors,
                    document,
                    forms,
                    id,
                    previous_media.state,
                    target,
                );
                let new = matching_specificity_with_forms(
                    &rule.selectors,
                    document,
                    forms,
                    id,
                    media.state,
                    target,
                );
                if old != new {
                    if rule.declarations.iter().any(|declaration| {
                        property_effect(&declaration.name) > DynamicEffect::Paint
                    }) {
                        return None;
                    }
                    if target_index == 0 {
                        element_dirty = true;
                    } else {
                        pseudo_dirty[target_index - 1] = true;
                    }
                }
            }
        }

        let parent_changed = document
            .parent(id)
            .and_then(|parent| changed.get(&parent))
            .copied()
            .unwrap_or(false);
        let old_style = tree.get(id);
        if element_dirty || parent_changed {
            let environment = state.environments.get(&id)?;
            let mut style = cascade_element(
                &rules,
                &candidates,
                document,
                forms,
                id,
                media,
                parent_style,
                state.root_font_size,
                environment,
                tree.store_mut(),
            );
            compute_box_values(document, &tree, id, &mut style);
            tree.insert(id, style);
        }
        let element_changed = old_style != tree.get(id);
        changed.insert(id, element_changed);

        let environment = state.environments.get(&id)?;
        let origin = tree.get(id);
        let flex_item = pseudo_is_layout_item(document, &tree, id, origin);
        for (index, which) in [PseudoElement::Before, PseudoElement::After]
            .into_iter()
            .enumerate()
        {
            if !element_changed && !pseudo_dirty[index] {
                continue;
            }
            let Some(old) = tree.pseudo(id, which).cloned() else {
                continue;
            };
            let style = cascade_pseudo_style(
                &rules,
                &candidates,
                document,
                forms,
                id,
                media,
                which,
                origin,
                flex_item,
                environment,
                tree.store_mut(),
            );
            tree.insert_pseudo(
                id,
                which,
                PseudoBox {
                    text: old.text,
                    style,
                },
            );
        }
        if (element_changed || pseudo_dirty[2])
            && let Some(old) = tree.marker(id).cloned()
        {
            let style = cascade_pseudo_style(
                &rules,
                &candidates,
                document,
                forms,
                id,
                media,
                PseudoElement::Marker,
                origin,
                false,
                environment,
                tree.store_mut(),
            );
            tree.insert_marker(
                id,
                Marker {
                    text: old.text,
                    reserve: old.reserve,
                    position: old.position,
                    style,
                },
            );
        }
    }
    let mut state = state.clone();
    state.dynamic_state = media.state;
    Some((tree, state))
}

#[allow(clippy::too_many_arguments)]
fn cascade_element(
    rules: &[&StyleRule],
    candidates: &[usize],
    document: &Document,
    forms: &FormState,
    id: NodeId,
    media: MediaContext,
    parent_style: Option<ComputedStyle>,
    root_font_size: crate::core::style::FontSize,
    environment: &Environment,
    store: &mut StyleStore,
) -> ComputedStyle {
    let mut style = ua_style(
        document,
        id,
        parent_style,
        UaContext {
            palette: media.palette,
            state: media.state,
        },
    );
    let ua_baseline = style;
    let mut declarations = Vec::new();
    let mut order = 0usize;
    for declaration in presentational_hints(document, id) {
        declarations.push((false, 0, order, declaration));
        order += 1;
    }
    for rule_index in candidates.iter().copied() {
        let rule = rules[rule_index];
        if let Some(specificity) = matching_specificity_with_forms(
            &rule.selectors,
            document,
            forms,
            id,
            media.state,
            MatchTarget::Element,
        ) {
            for declaration in &rule.declarations {
                declarations.push((
                    declaration.important,
                    specificity,
                    order,
                    declaration.clone(),
                ));
                order += 1;
            }
        }
    }
    if let Some(inline) = inline_style(document, id) {
        for declaration in parse_declarations(inline) {
            declarations.push((declaration.important, u32::MAX, order, declaration));
            order += 1;
        }
    }
    declarations
        .sort_by_key(|(important, specificity, order, _)| (*important, *specificity, *order));
    let font_root = if parent_style.is_none() {
        media.root_font_size
    } else {
        root_font_size
    };
    let declarations = resolve_declarations(
        declarations,
        environment,
        parent_style,
        ua_baseline,
        media.with_font_sizes(
            parent_style.map_or(font_root, |parent| parent.font_size),
            font_root,
        ),
    );
    for (_, _, _, declaration) in &declarations {
        apply_font_size(
            &mut style,
            parent_style,
            ua_baseline,
            declaration,
            media.with_font_sizes(
                parent_style.map_or(font_root, |parent| parent.font_size),
                font_root,
            ),
        );
    }
    let root_font_size = if parent_style.is_none() {
        style.font_size
    } else {
        root_font_size
    };
    style.text_presentation = style.font_size.presentation(media.text_rendering);
    let element_media = media.with_font_sizes(style.font_size, root_font_size);
    for (_, _, _, declaration) in declarations {
        apply_declaration(
            &mut style,
            parent_style,
            ua_baseline,
            &declaration,
            element_media,
            store,
        );
    }
    style.overflow = style.overflow.computed();
    if style.display.is_inline_flow() && !is_replaced_element(document, id)
        || matches!(
            style.display,
            Display::TABLE_HEADER_GROUP
                | Display::TABLE_ROW_GROUP
                | Display::TABLE_FOOTER_GROUP
                | Display::TABLE_ROW
        )
    {
        style.width = CssSize::Auto;
        style.height = CssSize::Auto;
        style.min_width = CssSize::Auto;
        style.min_height = CssSize::Auto;
        style.max_width = CssMaxSize::None;
        style.max_height = CssMaxSize::None;
    }
    style
}

#[allow(clippy::too_many_arguments)]
fn cascade_pseudo_style(
    rules: &[&StyleRule],
    candidates: &[usize],
    document: &Document,
    forms: &FormState,
    id: NodeId,
    media: MediaContext,
    which: PseudoElement,
    origin: ComputedStyle,
    flex_item: bool,
    origin_environment: &Arc<Environment>,
    store: &mut StyleStore,
) -> ComputedStyle {
    let mut declarations = Vec::new();
    let mut order = 0usize;
    for rule_index in candidates.iter().copied() {
        let rule = rules[rule_index];
        if let Some(specificity) = matching_specificity_with_forms(
            &rule.selectors,
            document,
            forms,
            id,
            media.state,
            MatchTarget::Pseudo(which),
        ) {
            for declaration in &rule.declarations {
                declarations.push((
                    declaration.important,
                    specificity,
                    order,
                    declaration.clone(),
                ));
                order += 1;
            }
        }
    }
    declarations
        .sort_by_key(|(important, specificity, order, _)| (*important, *specificity, *order));
    let inherited = ComputedStyle {
        display: Display::INLINE,
        white_space: origin.white_space,
        cursor: origin.cursor,
        visibility: origin.visibility,
        color: origin.color,
        background: (!origin.display.is_contents())
            .then_some(origin.background)
            .flatten(),
        bold: origin.bold,
        underline: origin.underline,
        strike: origin.strike,
        reverse: origin.reverse,
        font_size: origin.font_size,
        text_presentation: origin.text_presentation,
        list_style_type: origin.list_style_type,
        list_style_position: origin.list_style_position,
        border_collapse: origin.border_collapse,
        border_spacing: origin.border_spacing,
        caption_side: origin.caption_side,
        text_align: origin.text_align,
        legacy_align: origin.legacy_align,
        ..Default::default()
    };
    let ua_baseline = inherited;
    let custom_winners = declarations
        .iter()
        .filter(|(_, _, _, declaration)| is_custom_name(&declaration.name))
        .map(|(_, _, _, declaration)| (declaration.name.clone(), declaration.value.clone()))
        .collect();
    let environment = derive_environment(origin_environment.clone(), custom_winners);
    let declarations = resolve_declarations(
        declarations,
        &environment,
        Some(inherited),
        ua_baseline,
        media,
    );
    let mut style = inherited;
    for (_, _, _, declaration) in &declarations {
        apply_font_size(&mut style, Some(inherited), ua_baseline, declaration, media);
    }
    style.text_presentation = style.font_size.presentation(media.text_rendering);
    let pseudo_media = media.with_font_sizes(style.font_size, media.root_font_size);
    for (_, _, _, declaration) in declarations {
        apply_declaration(
            &mut style,
            Some(inherited),
            ua_baseline,
            &declaration,
            pseudo_media,
            store,
        );
    }
    style.overflow = style.overflow.computed();
    if style.position.is_absolute() || flex_item {
        style.float = crate::core::style::CssFloat::None;
        style.display = style.display.blockify();
    } else if style.float.is_floating() {
        style.display = style.display.blockify();
    }
    style
}

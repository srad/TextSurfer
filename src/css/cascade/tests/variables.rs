use super::*;

#[test]
fn custom_properties_inherit_with_case_sensitive_fallback_substitution() {
    let (_, styles, order) = cascade_source(
        "<style>
            main { --Tone: #ff0000; --space: 2ch }
            p { color: var(--Tone); width: var(--space); background: var(--tone, #0000ff) }
         </style><main><p>text</p></main>",
    );
    let p = *order.last().unwrap();
    assert_eq!(styles.get(p).color, Some(Rgba::new(255, 0, 0, 255)));
    assert_eq!(styles.get(p).background, Some(Rgb::new(0, 0, 255)));
    assert_eq!(styles.get(p).width, CssWidth::Cells(2));
}

#[test]
fn cycles_make_only_the_cycle_invalid_and_fallbacks_still_work() {
    let (_, styles, order) = cascade_source(
        "<style>
            p { --a: var(--b); --b: var(--a); --c: var(--a, green);
                color: var(--c); background: var(--a, blue) }
         </style><p>text</p>",
    );
    let p = *order.last().unwrap();
    assert_eq!(styles.get(p).color, Some(Rgba::new(0, 128, 0, 255)));
    assert_eq!(styles.get(p).background, Some(Rgb::new(0, 0, 255)));
}

#[test]
fn variable_failure_computes_to_unset_instead_of_reviving_a_lower_declaration() {
    let (_, styles, order) = cascade_source(
        "<style>
            main { color: red; width: 7ch }
            p { color: blue; color: var(--missing); width: 4ch; width: var(--missing) }
         </style><main><p>text</p></main>",
    );
    let p = *order.last().unwrap();
    assert_eq!(styles.get(p).color, Some(Rgba::new(255, 0, 0, 255)));
    assert_eq!(styles.get(p).width, CssWidth::Auto);
}

#[test]
fn invalid_computed_values_reset_longhands_and_shorthands_atomically() {
    let (_, styles, order) = cascade_source(
        "<style>
            main { color: red }
            p { --bad: nonsense; color: blue; color: var(--bad);
                margin: 1ch 2ch 3ch 4ch; margin: var(--bad) }
         </style><main><p>text</p></main>",
    );
    let p = *order.last().unwrap();
    assert_eq!(styles.get(p).color, Some(Rgba::new(255, 0, 0, 255)));
    assert_eq!(
        styles.get(p).margin,
        crate::core::style::MarginEdges::default()
    );
}

#[test]
fn direct_and_substituted_revert_restore_the_ua_baseline() {
    let (_, styles, order) = cascade_source(
        "<style>
            p { display: inline; display: var(--missing, revert); color: red; color: revert }
         </style><p>text</p>",
    );
    let p = *order.last().unwrap();
    assert_eq!(styles.get(p).display, Display::BLOCK);
    assert_ne!(styles.get(p).color, Some(Rgba::new(255, 0, 0, 255)));
}

#[test]
fn initial_masks_an_inherited_custom_property_while_unset_and_revert_inherit_it() {
    let (_, styles, order) = cascade_source(
        "<style>
            main { --tone: red }
            .initial { --tone: initial; color: var(--tone, blue) }
            .unset { --tone: unset; color: var(--tone) }
            .revert { --tone: revert; color: var(--tone) }
         </style><main><p class='initial'>a</p><p class='unset'>b</p><p class='revert'>c</p></main>",
    );
    let nodes = &order[order.len() - 3..];
    assert_eq!(styles.get(nodes[0]).color, Some(Rgba::new(0, 0, 255, 255)));
    assert_eq!(styles.get(nodes[1]).color, Some(Rgba::new(255, 0, 0, 255)));
    assert_eq!(styles.get(nodes[2]).color, Some(Rgba::new(255, 0, 0, 255)));
}

#[test]
fn custom_values_compute_before_inheritance_but_relative_units_resolve_at_use_site() {
    let (_, styles, order) = cascade_source(
        "<style>
            main { --base: 2em; --tone: green; --chosen: var(--tone); font-size: 10px }
            section { --tone: red; font-size: 20px }
            p { display: block; width: var(--base); color: var(--chosen) }
         </style><main><section><p>text</p></section></main>",
    );
    let p = *order.last().unwrap();
    assert_eq!(styles.get(p).width, CssWidth::Cells(4));
    assert_eq!(styles.get(p).color, Some(Rgba::new(0, 128, 0, 255)));
}

#[test]
fn pseudo_content_and_counters_consume_originating_custom_properties() {
    let source = "<style>
        ol { --step: list-item 2; --label: 'item ' }
        li { counter-increment: var(--step) }
        li::before { content: var(--label) counter(list-item) }
    </style><ol><li>a</li><li>b</li></ol>";
    assert_eq!(
        pseudo_texts(source, PseudoElement::Before),
        ["item 2", "item 4"]
    );
}

#[test]
fn custom_properties_flow_through_contents_and_none_elements() {
    let (_, styles, order) = cascade_source(
        "<style>
            .contents { display: contents; --tone: red }
            .none { display: none; --space: 3ch }
            span { display: block; color: var(--tone, blue); width: var(--space, 1ch) }
         </style><div class='contents'><div class='none'><span>text</span></div></div>",
    );
    let span = *order.last().unwrap();
    assert_eq!(styles.get(span).color, Some(Rgba::new(255, 0, 0, 255)));
    assert_eq!(styles.get(span).width, CssWidth::Cells(3));
}

#[test]
fn custom_property_winners_use_media_importance_specificity_and_inline_order() {
    let (_, styles, order) = cascade_source(
        "<style>
            p { --tone: red !important }
            @media screen { #target { --tone: blue !important } }
         </style><p id='target' style='--tone: green !important; color: var(--tone)'>text</p>",
    );
    assert_eq!(
        styles.get(*order.last().unwrap()).color,
        Some(Rgba::new(0, 128, 0, 255))
    );
}

#[test]
fn pseudo_local_custom_properties_override_the_originating_environment() {
    assert_eq!(
        pseudo_texts(
            "<style>p { --label: 'origin' } p::before { --label: 'pseudo'; content: var(--label) }</style><p>text</p>",
            PseudoElement::Before,
        ),
        ["pseudo"]
    );
}

#[test]
fn css_wide_keywords_produced_while_computing_custom_values_keep_custom_semantics() {
    let (_, styles, order) = cascade_source(
        "<style>
            main { --tone: red }
            p { --tone: var(--missing, initial); color: var(--tone, blue) }
         </style><main><p>text</p></main>",
    );
    assert_eq!(
        styles.get(*order.last().unwrap()).color,
        Some(Rgba::new(0, 0, 255, 255))
    );
}

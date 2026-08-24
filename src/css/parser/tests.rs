use super::diagnostics::MAX_RETAINED_DIAGNOSTICS;
use super::media::MAX_MEDIA_NESTING;
use super::*;
use crate::core::style::{CssLength, CssLengthUnit};
use crate::css::StateDeps;

#[test]
fn parses_selector_rules_and_declarations() {
    let sheet = CssparserParser
        .parse("article > p.note { display: block; margin: 1px 2px; } invalid { color }");
    assert_eq!(sheet.rules.len(), 1);
    let CssRule::Style(rule) = &sheet.rules[0] else {
        panic!("expected a style rule");
    };
    assert_eq!(rule.declarations.len(), 2);
    assert_eq!(rule.declarations[0].name, "display");
}

#[test]
fn stylesheet_records_dynamic_dependencies_once_across_media_rules() {
    let sheet = CssparserParser.parse(
        "main:is(.note, a:hover) span { color: red }
         @media screen { form:focus-within button:active { display: block } }
         a:focus-visible {}",
    );
    assert_eq!(
        sheet.state_deps,
        StateDeps {
            hover: true,
            focus: true,
            active: true,
        }
    );
}

fn css_parser_contract(parser: &dyn CssParser) {
    let sheet = parser.parse(
        "@media { a { display: block } }
         @media only SCREEN, not print, unknown, not unknown { b { display: block } }
         @media (width: 1px), screen and (color), screen { c { display: block } }
         @import url(theme.css);
         d { display: block }",
    );
    assert_eq!(sheet.rules.len(), 4);
    let CssRule::Media(empty) = &sheet.rules[0] else {
        panic!("expected empty media rule");
    };
    assert_eq!(empty.queries, MediaQueryList::Always);
    let CssRule::Media(types) = &sheet.rules[1] else {
        panic!("expected type media rule");
    };
    assert_eq!(
        types.queries,
        MediaQueryList::Any(vec![
            MediaQuery::Type {
                negated: false,
                media_type: "screen".to_string(),
            },
            MediaQuery::Type {
                negated: true,
                media_type: "print".to_string(),
            },
            MediaQuery::Type {
                negated: false,
                media_type: "unknown".to_string(),
            },
            MediaQuery::Type {
                negated: true,
                media_type: "unknown".to_string(),
            },
        ])
    );
    let CssRule::Media(partial) = &sheet.rules[2] else {
        panic!("expected partially unsupported media rule");
    };
    assert_eq!(
        partial.queries,
        MediaQueryList::Any(vec![
            MediaQuery::Condition {
                negated: false,
                media_type: None,
                features: vec![MediaFeature::Dimension {
                    axis: MediaAxis::Width,
                    condition: DimensionCondition::Compare {
                        comparison: MediaComparison::Equal,
                        value: CssLength::new(1.0, CssLengthUnit::Px).unwrap(),
                    },
                }],
            },
            MediaQuery::Never,
            MediaQuery::Type {
                negated: false,
                media_type: "screen".to_string(),
            },
        ])
    );
    assert!(matches!(sheet.rules[3], CssRule::Style(_)));
    assert_eq!(sheet.diagnostics.total(), 2);
    assert_eq!(
        sheet
            .diagnostics
            .entries()
            .iter()
            .map(|diagnostic| diagnostic.kind)
            .collect::<Vec<_>>(),
        vec![
            CssDiagnosticKind::UnsupportedMediaQuery,
            CssDiagnosticKind::InvalidImportForm,
        ]
    );
}

#[test]
fn cssparser_parser_passes_the_css_parser_contract() {
    css_parser_contract(&CssparserParser);
}

#[test]
fn invalid_at_rule_forms_recover_to_following_rules() {
    let sheet = CssparserParser.parse(
        "@media screen; p { display: block }
         @import url(theme.css) { ignored { display: none } }
         @unknown test { ignored { display: none } }
         div { display: block }",
    );
    assert_eq!(sheet.rules.len(), 2);
    assert!(
        sheet
            .diagnostics
            .entries()
            .iter()
            .any(|diagnostic| diagnostic.kind == CssDiagnosticKind::InvalidMediaRuleForm)
    );
    assert!(
        sheet
            .diagnostics
            .entries()
            .iter()
            .any(|diagnostic| diagnostic.kind == CssDiagnosticKind::InvalidImportForm)
    );
    assert_eq!(sheet.diagnostics.total(), 2);
}

#[test]
fn invalid_media_members_never_become_an_active_empty_list() {
    let sheet = CssparserParser.parse(
        "@media , { a { display: none } }
         @media scr\\65 en { b { display: block } }
         @media not print and (width) { c { display: none } }",
    );
    let CssRule::Media(invalid) = &sheet.rules[0] else {
        panic!("expected invalid media rule");
    };
    assert_eq!(
        invalid.queries,
        MediaQueryList::Any(vec![MediaQuery::Never, MediaQuery::Never])
    );
    let CssRule::Media(escaped) = &sheet.rules[1] else {
        panic!("expected escaped media rule");
    };
    assert_eq!(
        escaped.queries,
        MediaQueryList::Any(vec![MediaQuery::Type {
            negated: false,
            media_type: "screen".to_string(),
        }])
    );
    let CssRule::Media(supported_not) = &sheet.rules[2] else {
        panic!("expected supported media rule");
    };
    assert_eq!(
        supported_not.queries,
        MediaQueryList::Any(vec![MediaQuery::Condition {
            negated: true,
            media_type: Some("print".to_string()),
            features: vec![MediaFeature::Dimension {
                axis: MediaAxis::Width,
                condition: DimensionCondition::Boolean,
            }],
        }])
    );
}

#[test]
fn media_nesting_and_diagnostics_are_bounded() {
    let accepted = format!(
        "{}p {{ display: block }}{}",
        "@media screen {".repeat(MAX_MEDIA_NESTING),
        "}".repeat(MAX_MEDIA_NESTING)
    );
    assert_eq!(CssparserParser.parse(&accepted).diagnostics.total(), 0);

    let rejected = format!(
        "{}p {{ display: block }}{} q {{ display: block }}",
        "@media screen {".repeat(MAX_MEDIA_NESTING + 1),
        "}".repeat(MAX_MEDIA_NESTING + 1)
    );
    let rejected = CssparserParser.parse(&rejected);
    assert!(
        rejected
            .diagnostics
            .entries()
            .iter()
            .any(|diagnostic| diagnostic.kind == CssDiagnosticKind::NestingLimitExceeded)
    );

    let imports = format!(
        "p {{ display: block }}{}",
        "@import url(theme.css);".repeat(MAX_RETAINED_DIAGNOSTICS + 3)
    );
    let diagnostics = CssparserParser.parse(&imports).diagnostics;
    assert_eq!(diagnostics.total(), MAX_RETAINED_DIAGNOSTICS + 3);
    assert_eq!(diagnostics.entries().len(), MAX_RETAINED_DIAGNOSTICS);
    assert!(diagnostics.truncated());
}

#[test]
fn inline_declarations_preserve_values_and_important() {
    assert_eq!(
        parse_declarations("display: none !important; white-space: pre"),
        vec![
            Declaration {
                name: "display".to_string(),
                value: "none".to_string(),
                important: true,
            },
            Declaration {
                name: "white-space".to_string(),
                value: "pre".to_string(),
                important: false,
            },
        ]
    );
}

#[test]
fn leading_imports_are_retained_and_late_nested_and_qualified_forms_are_rejected() {
    let sheet = CssparserParser.parse(
        "@charset \"utf-8\";
         @import \"a.css\" screen and (min-width: 20px);
         @import url(b.css) layer(theme);
         p { display: block }
         @import 'late.css';
         @media screen { @import 'nested.css'; span { display: block } }",
    );
    let CssRule::Import(rule) = &sheet.rules[0] else {
        panic!("expected retained import");
    };
    assert_eq!(rule.url, "a.css");
    assert!(matches!(
        rule.queries,
        MediaQueryList::Any(ref queries) if matches!(queries[0], MediaQuery::Condition { .. })
    ));
    for invalid in [
        "@import url(b.css) layer(theme);",
        "p { display:block } @import 'late.css';",
        "@media screen { @import 'nested.css'; span { display:block } }",
    ] {
        assert!(
            CssparserParser
                .parse(invalid)
                .diagnostics
                .entries()
                .iter()
                .any(|diagnostic| diagnostic.kind == CssDiagnosticKind::InvalidImportForm),
            "{invalid}"
        );
    }
}

#[test]
fn media_features_parse_with_legacy_and_mq4_ranges() {
    let queries = parse_media_queries(
        "not (scripting: enabled), only screen and (prefers-color-scheme: dark) and \
         (min-width: 79.6px) and (max-height: 30ch), (width > 10px)",
    );
    let MediaQueryList::Any(queries) = queries else {
        panic!("expected media members");
    };
    assert!(matches!(
        queries[0],
        MediaQuery::Condition { negated: true, .. }
    ));
    assert!(matches!(
        queries[1],
        MediaQuery::Condition { negated: false, .. }
    ));
    assert!(matches!(
        queries[2],
        MediaQuery::Condition {
            features: ref values,
            ..
        } if matches!(
            values.as_slice(),
            [MediaFeature::Dimension {
                condition: DimensionCondition::Compare {
                    comparison: MediaComparison::Greater,
                    ..
                },
                ..
            }]
        )
    ));
}

#[test]
fn mq4_dimension_ranges_parse_in_both_directions_and_chain() {
    let queries = parse_media_queries(
        "(width >= 640px), (640px <= width), (400px < width <= 80ch), (width = 40em)",
    );
    let MediaQueryList::Any(queries) = queries else {
        panic!("expected media members");
    };
    assert_eq!(queries.len(), 4);
    assert!(
        queries
            .iter()
            .all(|query| !matches!(query, MediaQuery::Never))
    );
    assert_eq!(
        parse_media_queries("(device-width > 1px), (min-width > 1px)"),
        MediaQueryList::Any(vec![MediaQuery::Never, MediaQuery::Never])
    );
}

#[test]
fn important_uses_css_token_rules_instead_of_string_suffixes() {
    let declarations =
        parse_declarations("display: none ! /**/ IMPORTANT; color: var(--looks-like-!important)");
    assert!(declarations[0].important);
    assert_eq!(declarations[0].value, "none");
    assert!(!declarations[1].important);
    assert_eq!(declarations[1].value, "var(--looks-like-!important)");
}

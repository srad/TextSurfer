use std::collections::HashMap;

use super::*;

#[test]
fn substitution_handles_nested_fallbacks_and_token_boundaries() {
    let environment = derive_environment(
        Environment::root(),
        HashMap::from([
            ("--n".to_string(), "12".to_string()),
            ("--color".to_string(), "rgb(1, 2, 3)".to_string()),
        ]),
    );
    assert_eq!(environment.substitute("var(--n)px").unwrap(), "12/**/px");
    assert!(
        environment
            .substitute("linear(var(--missing, var(--color)), var(--missing,))")
            .is_ok()
    );
    assert_eq!(environment.substitute("'var(--n)'").unwrap(), "'var(--n)'");
}

#[test]
fn fallback_references_participate_in_cycle_detection() {
    let environment = derive_environment(
        Environment::root(),
        HashMap::from([
            ("--a".to_string(), "var(--missing, var(--b))".to_string()),
            ("--b".to_string(), "var(--a)".to_string()),
        ]),
    );
    assert!(environment.substitute("var(--a)").is_err());
    assert_eq!(environment.substitute("var(--a, green)").unwrap(), " green");
}

#[test]
fn long_dependency_chains_resolve_without_using_the_call_stack() {
    let mut values = HashMap::new();
    values.insert("--v0".to_string(), "green".to_string());
    for index in 1..2_048 {
        values.insert(format!("--v{index}"), format!("var(--v{})", index - 1));
    }
    let environment = derive_environment(Environment::root(), values);
    assert_eq!(environment.substitute("var(--v2047)").unwrap(), "green");
}

#[test]
fn substitution_budget_and_component_depth_are_enforced() {
    let large = Rc::<str>::from("x".repeat(64));
    assert!(syntax::substitute("var(--x)", |_| Lookup::Value(large.clone()), 32).is_err());

    let nested = format!("{}red{}", "fn(".repeat(66), ")".repeat(66));
    assert!(!validate_declaration_value(&nested, true));
}

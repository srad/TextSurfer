use super::*;

fn tab() -> Tab {
    Tab::new(
        0,
        "https://start".to_string(),
        0,
        DisplayList::default(),
        "Ready".to_string(),
    )
}

#[test]
fn back_and_forward_walk_the_stack_and_stop_at_the_ends() {
    let mut t = tab();
    t.push_history("https://a");
    t.push_history("https://b");
    assert_eq!(t.current(), Some("https://b"));
    assert!(t.back());
    assert_eq!(t.current(), Some("https://a"));
    assert!(t.back());
    assert_eq!(t.current(), Some("https://start"));
    assert!(!t.back());
    assert!(t.forward());
    assert_eq!(t.current(), Some("https://a"));
    assert!(t.forward());
    assert_eq!(t.current(), Some("https://b"));
    assert!(!t.forward());
}

#[test]
fn a_new_navigation_truncates_the_forward_stack() {
    let mut t = tab();
    t.push_history("https://a");
    t.push_history("https://b");
    t.back();
    t.push_history("https://c");
    assert_eq!(t.history, vec!["https://start", "https://a", "https://c"]);
    assert_eq!(t.current(), Some("https://c"));
    assert!(!t.forward());
}

#[test]
fn navigating_back_then_forward_restores_the_forward_stack() {
    let mut t = tab();
    t.push_history("https://a");
    t.back();
    assert!(t.forward());
    assert_eq!(t.current(), Some("https://a"));
    assert_eq!(t.history, vec!["https://start", "https://a"]);
}

#[test]
fn a_blank_tab_has_no_fake_history_entry() {
    let mut tab = Tab::new(
        0,
        String::new(),
        0,
        DisplayList::default(),
        "Ready".to_string(),
    );
    assert!(tab.history.is_empty());
    assert_eq!(tab.current(), None);
    tab.push_history("https://example.com");
    assert_eq!(tab.current(), Some("https://example.com"));
}

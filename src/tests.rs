use super::*;

fn terminal_key(kind: KeyEventKind) -> event::KeyEvent {
    event::KeyEvent::new_with_kind(KeyCode::Char('g'), KeyModifiers::NONE, kind)
}

#[test]
fn release_events_are_dropped() {
    assert_eq!(
        from_terminal_key(terminal_key(KeyEventKind::Release)),
        None,
        "a key-up must never reach the app"
    );
}

#[test]
fn repeat_events_are_mapped_for_normal_key_repeat() {
    assert!(from_terminal_key(terminal_key(KeyEventKind::Repeat)).is_some());
}

#[test]
fn press_events_are_mapped_with_modifiers() {
    let key = event::KeyEvent::new_with_kind(
        KeyCode::Char('x'),
        KeyModifiers::SHIFT | KeyModifiers::CONTROL,
        KeyEventKind::Press,
    );
    let mapped = from_terminal_key(key).unwrap();
    assert_eq!(mapped.code, Key::Char('x'));
    assert!(mapped.modifiers.shift && mapped.modifiers.ctrl && !mapped.modifiers.alt);
}

#[test]
fn cli_parses_the_start_url_and_rejects_unknown_flags() {
    let cli = Cli::try_parse_from(["textsurfer", "--url", "https://example.com"]).unwrap();
    assert_eq!(cli.url.as_deref(), Some("https://example.com"));
    assert_eq!(cli.js, JsMode::Auto);
    let cli =
        Cli::try_parse_from(["textsurfer", "--user-agent", "test-agent", "--js", "off"]).unwrap();
    assert_eq!(cli.user_agent.as_deref(), Some("test-agent"));
    assert_eq!(cli.js, JsMode::Off);
    let cli = Cli::try_parse_from(["textsurfer", "--dump", "--rows", "31"]).unwrap();
    assert_eq!(cli.rows, 31);
    assert!(Cli::try_parse_from(["textsurfer", "--unknown"]).is_err());
}

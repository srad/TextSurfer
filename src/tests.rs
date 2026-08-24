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

fn terminal_mouse(kind: event::MouseEventKind) -> event::MouseEvent {
    event::MouseEvent {
        kind,
        column: 7,
        row: 9,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn mouse_reports_map_onto_domain_events() {
    let down = from_terminal_mouse(terminal_mouse(event::MouseEventKind::Down(
        event::MouseButton::Middle,
    )))
    .unwrap();
    assert_eq!(down.kind, MouseKind::Press(MouseButton::Middle));
    assert_eq!(down.at, Point { col: 7, row: 9 });
    assert_eq!(
        from_terminal_mouse(terminal_mouse(event::MouseEventKind::Up(
            event::MouseButton::Left
        )))
        .unwrap()
        .kind,
        MouseKind::Release(MouseButton::Left)
    );
    assert_eq!(
        from_terminal_mouse(terminal_mouse(event::MouseEventKind::ScrollUp))
            .unwrap()
            .kind,
        MouseKind::Wheel(WheelDirection::Up)
    );
}

#[test]
fn a_drag_is_plain_motion_and_a_sideways_wheel_is_dropped() {
    // winit reports motion with a button held as `CursorMoved`, and the app has no drag
    // semantics to distinguish the two with.
    for kind in [
        event::MouseEventKind::Moved,
        event::MouseEventKind::Drag(event::MouseButton::Left),
    ] {
        assert_eq!(
            from_terminal_mouse(terminal_mouse(kind)).unwrap().kind,
            MouseKind::Move
        );
    }
    for kind in [
        event::MouseEventKind::ScrollLeft,
        event::MouseEventKind::ScrollRight,
    ] {
        assert_eq!(from_terminal_mouse(terminal_mouse(kind)), None);
    }
}

#[cfg(feature = "vga")]
#[test]
fn both_frontends_report_the_same_press() {
    // The adapters take different library types; what reaches the app must not differ.
    use textsurfer::vga::input::from_window_button;
    use winit::event::{ElementState, MouseButton as WinitMouseButton};

    let at = Point { col: 7, row: 9 };
    let terminal = from_terminal_mouse(terminal_mouse(event::MouseEventKind::Down(
        event::MouseButton::Right,
    )))
    .unwrap();
    let window = from_window_button(WinitMouseButton::Right, ElementState::Pressed, at).unwrap();
    assert_eq!(terminal, window);
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

#[test]
fn frontend_selection_honors_overrides_and_rejects_conflicts() {
    let terminal = Cli::try_parse_from(["textsurfer", "--terminal"]).unwrap();
    assert_eq!(
        frontend_choice(&terminal).unwrap(),
        FrontendChoice::Terminal
    );
    let vga = Cli::try_parse_from(["textsurfer", "--vga"]).unwrap();
    assert_eq!(frontend_choice(&vga).unwrap(), FrontendChoice::Vga);
    let scaled = Cli::try_parse_from(["textsurfer", "--vga-scale", "2"]).unwrap();
    assert_eq!(frontend_choice(&scaled).unwrap(), FrontendChoice::Vga);
    let conflict = Cli::try_parse_from(["textsurfer", "--terminal", "--vga"]).unwrap();
    assert!(frontend_choice(&conflict).is_err());
    let scale = Cli::try_parse_from(["textsurfer", "--terminal", "--vga-scale", "2"]).unwrap();
    assert!(frontend_choice(&scale).is_err());
}

#[test]
fn compiled_frontend_is_the_interactive_default() {
    let cli = Cli::try_parse_from(["textsurfer"]).unwrap();
    #[cfg(feature = "vga")]
    assert_eq!(frontend_choice(&cli).unwrap(), FrontendChoice::Vga);
    #[cfg(not(feature = "vga"))]
    assert_eq!(frontend_choice(&cli).unwrap(), FrontendChoice::Terminal);
}

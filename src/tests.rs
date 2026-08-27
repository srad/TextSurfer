use super::*;
use std::collections::VecDeque;

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
        MouseKind::Wheel { rows: -WHEEL_ROWS }
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
    assert!(cli.log_file.is_none());
    assert_eq!(cli.js, JsMode::Auto);
    let cli =
        Cli::try_parse_from(["textsurfer", "--user-agent", "test-agent", "--js", "off"]).unwrap();
    assert_eq!(cli.user_agent.as_deref(), Some("test-agent"));
    assert_eq!(cli.js, JsMode::Off);
    let cli = Cli::try_parse_from(["textsurfer", "--log-file", "trace.log"]).unwrap();
    assert_eq!(
        cli.log_file.as_deref(),
        Some(std::path::Path::new("trace.log"))
    );
    let cli = Cli::try_parse_from(["textsurfer", "--dump", "--rows", "31"]).unwrap();
    assert_eq!(cli.rows, 31);
    assert!(Cli::try_parse_from(["textsurfer", "--unknown"]).is_err());
}

#[test]
fn default_user_agent_is_versioned_identifiable_and_contactable() {
    assert_eq!(
        default_user_agent(),
        concat!(
            "TextSurfer/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/srad/TextSurfer)"
        )
    );
}

#[test]
fn log_filter_errors_are_reported_before_startup() {
    let error = parse_log_filter("[").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().starts_with("invalid RUST_LOG:"));
}

#[test]
fn log_files_append_instead_of_erasing_an_earlier_run() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("textsurfer.log");
    std::fs::write(&path, b"earlier\n").unwrap();
    let mut file = open_log_file(&path).unwrap();
    writeln!(file, "later").unwrap();
    drop(file);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "earlier\nlater\n");
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

enum ScriptItem {
    Idle,
    Event(Event),
    PollError,
    ReadError,
}

struct ScriptedEvents {
    items: VecDeque<ScriptItem>,
    ready: Option<io::Result<Event>>,
    polls: Vec<Duration>,
}

impl ScriptedEvents {
    fn new(items: impl IntoIterator<Item = ScriptItem>) -> Self {
        Self {
            items: items.into_iter().collect(),
            ready: None,
            polls: Vec::new(),
        }
    }
}

impl TerminalEvents for ScriptedEvents {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        self.polls.push(timeout);
        match self.items.pop_front() {
            Some(ScriptItem::Idle) => Ok(false),
            Some(ScriptItem::Event(event)) => {
                self.ready = Some(Ok(event));
                Ok(true)
            }
            Some(ScriptItem::PollError) => Err(io::Error::other("poll failed")),
            Some(ScriptItem::ReadError) => {
                self.ready = Some(Err(io::Error::other("read failed")));
                Ok(true)
            }
            None => Err(io::Error::other("script exhausted before quit")),
        }
    }

    fn read(&mut self) -> io::Result<Event> {
        self.ready
            .take()
            .unwrap_or_else(|| Err(io::Error::other("read without ready event")))
    }
}

fn terminal_event(code: KeyCode) -> Event {
    Event::Key(event::KeyEvent::new(code, KeyModifiers::NONE))
}

fn quit_script() -> [ScriptItem; 3] {
    [
        ScriptItem::Event(terminal_event(KeyCode::Esc)),
        ScriptItem::Event(terminal_event(KeyCode::Char('q'))),
        ScriptItem::Idle,
    ]
}

#[test]
fn terminal_idle_and_quit_cycles_poll_step_and_draw_only_dirty_states() {
    let mut script = Vec::from([ScriptItem::Idle]);
    script.extend(quit_script());
    let mut events = ScriptedEvents::new(script);
    let mut app = App::new();
    let mut steps = 0_u64;
    let mut draws = 0;
    run_with(
        &mut events,
        &mut app,
        || {
            steps += 1;
            Duration::from_millis(steps * 50)
        },
        None,
        |_, _| {
            draws += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        events.polls,
        vec![IDLE_POLL, IDLE_POLL, Duration::ZERO, Duration::ZERO]
    );
    assert!(steps >= 2);
    assert_eq!(draws, 2);
}

#[test]
fn terminal_sustained_mouse_queue_is_one_coalesced_frame_transaction() {
    let moved = Event::Mouse(terminal_mouse(event::MouseEventKind::Moved));
    let mut script = Vec::from([ScriptItem::Idle]);
    script.extend(
        (0..EVENTS_PER_FRAME - 2)
            .map(|_| ScriptItem::Event(moved.clone()))
            .collect::<Vec<_>>(),
    );
    script.extend(quit_script());
    let mut events = ScriptedEvents::new(script);
    let mut app = App::new();
    let mut draws = 0;
    run_with(&mut events, &mut app, Duration::default, None, |_, _| {
        draws += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(events.polls.len(), EVENTS_PER_FRAME + 1);
    assert_eq!(draws, 2);
}

#[test]
fn terminal_duplicate_resize_queue_does_not_feed_back_into_draws() {
    let mut script = Vec::from([ScriptItem::Idle]);
    script.extend(
        (0..EVENTS_PER_FRAME - 2)
            .map(|_| ScriptItem::Event(Event::Resize(80, 24)))
            .collect::<Vec<_>>(),
    );
    script.extend(quit_script());
    let mut events = ScriptedEvents::new(script);
    let mut app = App::new();
    let mut draws = 0;
    run_with(&mut events, &mut app, Duration::default, None, |_, _| {
        draws += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(draws, 2);
}

#[test]
fn terminal_poll_read_and_draw_errors_exit_the_loop() {
    let mut poll = ScriptedEvents::new([ScriptItem::PollError]);
    let mut app = App::new();
    assert!(run_with(&mut poll, &mut app, Duration::default, None, |_, _| Ok(())).is_err());

    let mut read = ScriptedEvents::new([ScriptItem::ReadError]);
    let mut app = App::new();
    assert!(run_with(&mut read, &mut app, Duration::default, None, |_, _| Ok(())).is_err());

    let mut draw = ScriptedEvents::new([ScriptItem::Idle]);
    let mut app = App::new();
    assert!(
        run_with(&mut draw, &mut app, Duration::default, None, |_, _| Err(
            io::Error::other("draw failed")
        ))
        .is_err()
    );
}

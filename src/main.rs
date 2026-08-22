use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use textsurfer::app::App;
use textsurfer::app::net::{Navigate, PoolNet};
use textsurfer::core::event::{Key, KeyEvent, KeyModifiers as AppKeyModifiers};
use textsurfer::core::geom::Size;
use textsurfer::net::{FetchPool, FileFetch, SchemeFetch, UreqFetch};
use textsurfer::ui::chrome;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    user_agent: Option<String>,
    #[arg(long, value_enum, default_value_t = JsMode::Auto)]
    js: JsMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum JsMode {
    #[default]
    Auto,
    On,
    Off,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    if cli.js == JsMode::On {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "JavaScript execution is not available before milestone M5",
        ));
    }
    ratatui::run(|terminal| {
        let fetch: Arc<dyn textsurfer::net::Fetch> = Arc::new(SchemeFetch {
            http: Arc::new(match cli.user_agent {
                Some(user_agent) => UreqFetch::with_user_agent(user_agent),
                None => UreqFetch::new(),
            }),
            file: Arc::new(FileFetch),
        });
        let net: Arc<dyn Navigate> = Arc::new(PoolNet::new(Arc::new(FetchPool::spawn(fetch, 4))));
        let mut app = App::with_net(net);
        let area = terminal.size()?;
        app.on_resize(Size {
            cols: area.width,
            rows: area.height,
        });
        if let Some(url) = cli.url {
            app.submit_url(&url);
        }
        run(terminal, &mut app)
    })
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    let started = Instant::now();
    loop {
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    if let Some(event) = from_terminal_key(key) {
                        app.handle_key(event);
                    }
                }
                Event::Resize(cols, rows) => app.on_resize(Size { cols, rows }),
                _ => {}
            }
        }
        app.step(started.elapsed());
        if app.take_dirty() {
            terminal.draw(|frame| {
                let view = app.chrome_view();
                chrome::draw(frame, &view);
            })?;
        }
        if app.should_quit() {
            return Ok(());
        }
    }
}

fn from_terminal_key(key: event::KeyEvent) -> Option<KeyEvent> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let code = match key.code {
        KeyCode::Char(ch) => Key::Char(ch),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::F(n) => Key::F(n),
        other => Key::Other(format!("{other:?}")),
    };
    let modifiers = key.modifiers;
    Some(KeyEvent {
        code,
        modifiers: AppKeyModifiers {
            shift: modifiers.contains(KeyModifiers::SHIFT),
            ctrl: modifiers.contains(KeyModifiers::CONTROL),
            alt: modifiers.contains(KeyModifiers::ALT),
        },
    })
}

#[cfg(test)]
mod tests {
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
        let cli = Cli::try_parse_from(["textsurfer", "--user-agent", "test-agent", "--js", "off"])
            .unwrap();
        assert_eq!(cli.user_agent.as_deref(), Some("test-agent"));
        assert_eq!(cli.js, JsMode::Off);
        assert!(Cli::try_parse_from(["textsurfer", "--unknown"]).is_err());
    }
}

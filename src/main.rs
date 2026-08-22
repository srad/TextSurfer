use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use url::Url;

use textsurfer::app::App;
use textsurfer::app::net::{Navigate, PoolNet};
use textsurfer::app::page_load::{PageLoad, PageLoadOptions, STYLESHEET_DEADLINE};
use textsurfer::app::render::{ResponseKind, response_kind};
use textsurfer::core::event::{Key, KeyEvent, KeyModifiers as AppKeyModifiers};
use textsurfer::core::geom::Size;
use textsurfer::core::url::url_fix;
use textsurfer::css::ColorScheme;
use textsurfer::net::{
    FetchPayload, FetchPool, FetchRequest, FileFetch, ResourceId, SchemeFetch, UreqFetch,
    charset_from_content_type, decode, decode_text,
};
use textsurfer::ui::chrome;
use textsurfer::ui::theme::NORTON;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    user_agent: Option<String>,
    #[arg(long, value_enum, default_value_t = JsMode::Auto)]
    js: JsMode,
    /// Render the page to stdout and exit instead of opening the terminal UI.
    #[arg(long)]
    dump: bool,
    /// Column budget used by --dump.
    #[arg(long, default_value_t = 80)]
    cols: u16,
    /// Row budget used for CSS media queries by --dump.
    #[arg(long, default_value_t = 24)]
    rows: u16,
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
    let fetch: Arc<dyn textsurfer::net::Fetch> = Arc::new(SchemeFetch {
        http: Arc::new(match cli.user_agent.clone() {
            Some(user_agent) => UreqFetch::with_user_agent(user_agent),
            None => UreqFetch::new(),
        }),
        file: Arc::new(FileFetch),
    });
    if cli.dump {
        let Some(url) = cli.url.as_deref() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--dump needs a --url",
            ));
        };
        return dump(fetch, url, cli.cols, cli.rows);
    }
    ratatui::run(|terminal| {
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

fn dump(fetch: Arc<dyn textsurfer::net::Fetch>, url: &str, cols: u16, rows: u16) -> io::Result<()> {
    let lines = dump_lines(fetch, url, cols, rows)?;
    let mut out = io::stdout().lock();
    for line in lines {
        writeln!(out, "{}", line.trim_end())?;
    }
    Ok(())
}

fn dump_lines(
    fetch: Arc<dyn textsurfer::net::Fetch>,
    url: &str,
    cols: u16,
    rows: u16,
) -> io::Result<Vec<String>> {
    let fixed = url_fix(url);
    let parsed = Url::parse(&fixed)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    let pool = FetchPool::spawn(fetch, 4);
    pool.submit(0, 0, ResourceId::DOCUMENT, FetchRequest { url: parsed });
    let response = loop {
        if let Some(payload) = pool.try_recv() {
            break payload
                .result
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let charset = response
        .content_type
        .as_deref()
        .and_then(charset_from_content_type);
    let viewport = Size {
        cols: cols.max(1),
        rows: rows.max(1),
    };
    let mut detach_pool = false;
    let lines = match response_kind(response.content_type.as_deref()) {
        ResponseKind::Html => {
            let decoded = decode(&response.body, charset.as_deref());
            let mut load = PageLoad::new(
                &decoded.text,
                response.final_url,
                decoded.encoding,
                PageLoadOptions {
                    viewport,
                    palette: NORTON.palette(),
                    scripting: false,
                    color_scheme: ColorScheme::Dark,
                    started: Duration::ZERO,
                },
            );
            let started = Instant::now();
            loop {
                for command in load.take_commands() {
                    pool.submit(0, 0, command.resource_id, FetchRequest { url: command.url });
                }
                if load.take_cancel_requested() {
                    pool.cancel(0, 0);
                }
                if load.applicable_is_settled() || started.elapsed() >= STYLESHEET_DEADLINE {
                    detach_pool = !load.is_settled();
                    pool.cancel(0, 0);
                    break;
                }
                if let Some(FetchPayload {
                    resource_id,
                    result,
                    ..
                }) = pool.try_recv()
                {
                    let _ = load.deliver(resource_id, result);
                } else {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            load.force_render().painted.text_lines()
        }
        ResponseKind::PlainText => decode_text(&response.body, charset.as_deref())
            .text
            .lines()
            .map(str::to_string)
            .collect(),
        ResponseKind::Unsupported(kind) => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("unsupported content type: {kind}"),
            ));
        }
    };
    if detach_pool {
        pool.shutdown_without_waiting();
    }
    Ok(lines)
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
    use textsurfer::net::{Fetch, FetchError, FetchResponse};

    struct ExternalDumpFetch;

    impl Fetch for ExternalDumpFetch {
        fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
            let (body, content_type) = if request.url.path().ends_with("site.css") {
                (b"#navigation { display:none }".to_vec(), "text/css")
            } else {
                (
                    b"<!doctype html><link rel=stylesheet href='/site.css'>\
                      <nav id=navigation>sidebar</nav><main>article</main>"
                        .to_vec(),
                    "text/html; charset=utf-8",
                )
            };
            Ok(FetchResponse {
                final_url: request.url.clone(),
                body,
                content_type: Some(content_type.to_string()),
            })
        }
    }

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
        let cli = Cli::try_parse_from(["textsurfer", "--dump", "--rows", "31"]).unwrap();
        assert_eq!(cli.rows, 31);
        assert!(Cli::try_parse_from(["textsurfer", "--unknown"]).is_err());
    }

    #[test]
    fn dump_uses_the_external_stylesheet_load_driver() {
        let lines = dump_lines(
            Arc::new(ExternalDumpFetch),
            "https://example.com/article",
            80,
            24,
        )
        .unwrap();
        assert!(lines.iter().any(|line| line.contains("article")));
        assert!(!lines.iter().any(|line| line.contains("sidebar")));
    }
}

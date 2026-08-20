use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::{DefaultTerminal, Terminal};

use textsurf::app::App;
use textsurf::app::net::{Navigate, PoolNet};
use textsurf::core::event::{Key, KeyEvent, KeyModifiers as AppKeyModifiers};
use textsurf::core::geom::Size;
use textsurf::net::{FetchPool, FileFetch, SchemeFetch, UreqFetch};
use textsurf::ui::chrome;

fn main() -> io::Result<()> {
    let start_url = parse_start_url();
    let mut terminal = try_init()?;
    let fetch: Arc<dyn textsurf::net::Fetch> = Arc::new(SchemeFetch {
        http: Arc::new(UreqFetch::new()),
        file: Arc::new(FileFetch),
    });
    let net: Arc<dyn Navigate> = Arc::new(PoolNet::new(Arc::new(FetchPool::spawn(fetch, 4))));
    let mut app = App::with_net(net);
    if let Some(url) = start_url {
        app.submit_url(&url);
    }
    let result = run(&mut terminal, &mut app);
    try_restore(&mut terminal)?;
    result
}

fn parse_start_url() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--url" {
            return args.next();
        }
    }
    None
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => app.handle_key(from_terminal_key(key)),
                Event::Resize(cols, rows) => app.on_resize(Size { cols, rows }),
                _ => {}
            }
        }
        app.step(Instant::now());
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

fn try_init() -> io::Result<DefaultTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn try_restore(terminal: &mut DefaultTerminal) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn from_terminal_key(key: event::KeyEvent) -> KeyEvent {
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
    KeyEvent {
        code,
        modifiers: AppKeyModifiers {
            shift: modifiers.contains(KeyModifiers::SHIFT),
            ctrl: modifiers.contains(KeyModifiers::CONTROL),
            alt: modifiers.contains(KeyModifiers::ALT),
        },
    }
}

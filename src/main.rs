use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use ratatui::DefaultTerminal;
use ratatui::crossterm::ExecutableCommand;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};

use textsurfer::app::App;
use textsurfer::app::net::{Navigate, PoolNet};
use textsurfer::core::event::{
    Key, KeyEvent, KeyModifiers as AppKeyModifiers, MouseButton, MouseEvent, MouseKind,
    WheelDirection,
};
use textsurfer::core::geom::{Point, Size};
use textsurfer::net::{FetchPool, FileFetch, SchemeFetch, UreqFetch};
use textsurfer::pipeline::dump::dump_lines;
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
    /// Render the page to stdout and exit instead of opening an interactive frontend.
    #[arg(long)]
    dump: bool,
    /// Explicitly select the window with its CP437/Unifont 8x16 faces.
    /// Requires the `vga` feature.
    #[arg(long)]
    vga: bool,
    #[arg(long)]
    terminal: bool,
    /// Integer pixel multiplier for VGA, so the 8x16 cell stays legible on a
    /// high-density display.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=8))]
    vga_scale: u8,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrontendChoice {
    Terminal,
    Vga,
}

fn frontend_choice(cli: &Cli) -> io::Result<FrontendChoice> {
    if cli.terminal && (cli.vga || cli.vga_scale != 1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--terminal conflicts with --vga and a non-default --vga-scale",
        ));
    }
    if cli.terminal {
        return Ok(FrontendChoice::Terminal);
    }
    if cli.vga || cli.vga_scale != 1 {
        return Ok(FrontendChoice::Vga);
    }
    #[cfg(feature = "vga")]
    {
        Ok(FrontendChoice::Vga)
    }
    #[cfg(not(feature = "vga"))]
    {
        Ok(FrontendChoice::Terminal)
    }
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
    if frontend_choice(&cli)? == FrontendChoice::Vga {
        return run_vga(fetch, &cli);
    }
    // `ratatui::restore` leaves raw mode and the alternate screen, but knows nothing
    // about mouse capture: without this a panic would leave the shell reporting mouse
    // escape codes at the user.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = io::stdout().execute(DisableMouseCapture);
        previous_hook(info);
    }));
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
        io::stdout().execute(EnableMouseCapture)?;
        let outcome = run(terminal, &mut app);
        io::stdout().execute(DisableMouseCapture)?;
        outcome
    })
}

/// Start the framebuffer frontend, or explain that it was not built in.
///
/// The flag exists in both builds so the failure is a clear message rather than an
/// unrecognised argument, matching how `--js on` reports an unavailable engine.
#[cfg(feature = "vga")]
fn run_vga(fetch: Arc<dyn textsurfer::net::Fetch>, cli: &Cli) -> io::Result<()> {
    let net: Arc<dyn Navigate> = Arc::new(PoolNet::new(Arc::new(FetchPool::spawn(fetch, 4))));
    let options = textsurfer::vga::VgaOptions {
        scale: usize::from(cli.vga_scale),
        ..Default::default()
    };
    textsurfer::vga::run(net, options, cli.url.clone())
}

#[cfg(not(feature = "vga"))]
fn run_vga(_fetch: Arc<dyn textsurfer::net::Fetch>, _cli: &Cli) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this build has no framebuffer frontend; rebuild with --features vga",
    ))
}

fn dump(fetch: Arc<dyn textsurfer::net::Fetch>, url: &str, cols: u16, rows: u16) -> io::Result<()> {
    let lines = dump_lines(fetch, url, Size { cols, rows }, NORTON.palette())?;
    let mut out = io::stdout().lock();
    for line in lines {
        writeln!(out, "{}", line.trim_end())?;
    }
    Ok(())
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
                Event::Mouse(mouse) => {
                    if let Some(event) = from_terminal_mouse(mouse) {
                        app.handle_mouse(event);
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

/// Translate a terminal mouse report, keeping the window adapter's behaviour.
///
/// A drag is reported as plain motion: winit sends `CursorMoved` while a button is held
/// too, and the app has no drag semantics to tell them apart with. Horizontal wheels have
/// nowhere to scroll and are dropped.
fn from_terminal_mouse(mouse: event::MouseEvent) -> Option<MouseEvent> {
    let button = |button| match button {
        event::MouseButton::Left => MouseButton::Left,
        event::MouseButton::Right => MouseButton::Right,
        event::MouseButton::Middle => MouseButton::Middle,
    };
    let kind = match mouse.kind {
        event::MouseEventKind::Down(pressed) => MouseKind::Press(button(pressed)),
        event::MouseEventKind::Up(released) => MouseKind::Release(button(released)),
        event::MouseEventKind::Moved | event::MouseEventKind::Drag(_) => MouseKind::Move,
        event::MouseEventKind::ScrollUp => MouseKind::Wheel(WheelDirection::Up),
        event::MouseEventKind::ScrollDown => MouseKind::Wheel(WheelDirection::Down),
        event::MouseEventKind::ScrollLeft | event::MouseEventKind::ScrollRight => return None,
    };
    Some(MouseEvent {
        kind,
        at: Point {
            col: mouse.column,
            row: mouse.row,
        },
    })
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
mod tests;

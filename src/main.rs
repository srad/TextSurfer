use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use ratatui::DefaultTerminal;
use ratatui::crossterm::ExecutableCommand;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEventKind, KeyModifiers,
};

use textsurfer::app::App;
use textsurfer::app::net::{Navigate, PoolNet};
use textsurfer::core::event::{
    InputBatch, InputEvent, Key, KeyEvent, KeyModifiers as AppKeyModifiers, MouseButton,
    MouseEvent, MouseKind, ResizePhase,
};
use textsurfer::core::frame::{EVENTS_PER_FRAME, FrameDamage, FrameScheduler};
use textsurfer::core::geom::{Point, Size};
use textsurfer::net::{FetchPool, FileFetch, SchemeFetch, UreqFetch, default_user_agent};
use textsurfer::pipeline::dump::dump_lines_with_scripts;
use textsurfer::script::JsEngineFactory;
use textsurfer::ui::frame::FrameComposer;
use textsurfer::ui::mouse::WHEEL_ROWS;
use textsurfer::ui::theme::DEFAULT;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

const DIAGNOSTIC_FILTER: &str = "textsurfer=debug,textsurfer::perf=trace";

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    user_agent: Option<String>,
    #[arg(long)]
    log_file: Option<PathBuf>,
    #[arg(long, conflicts_with = "log_file")]
    diagnostics: Option<PathBuf>,
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
    let _diagnostics_guard = init_logging(cli.log_file.as_deref(), cli.diagnostics.as_deref())?;
    let script_factory = script_factory(cli.js)?;
    let fetch: Arc<dyn textsurfer::net::Fetch> = Arc::new(SchemeFetch {
        http: Arc::new(match cli.user_agent.clone() {
            Some(user_agent) => UreqFetch::with_user_agent(user_agent),
            None => UreqFetch::with_user_agent(default_user_agent()),
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
        return dump(fetch, url, cli.cols, cli.rows, script_factory);
    }
    if frontend_choice(&cli)? == FrontendChoice::Vga {
        return run_vga(fetch, &cli, script_factory);
    }
    // `ratatui::restore` leaves raw mode and the alternate screen, but knows nothing
    // about mouse capture: without this a panic would leave the shell reporting mouse
    // escape codes at the user.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = io::stdout().execute(DisableMouseCapture);
        let _ = io::stdout().execute(DisableBracketedPaste);
        previous_hook(info);
    }));
    ratatui::run(|terminal| {
        let picker = ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks());
        let net: Arc<dyn Navigate> = Arc::new(PoolNet::new(Arc::new(FetchPool::spawn(fetch, 4))));
        let mut app = App::with_net(net);
        app.set_script_factory(script_factory);
        let area = terminal.size()?;
        app.on_resize(Size {
            cols: area.width,
            rows: area.height,
        });
        if let Some(url) = cli.url {
            app.submit_url(&url);
        }
        io::stdout().execute(EnableMouseCapture)?;
        io::stdout().execute(EnableBracketedPaste)?;
        let outcome = run(terminal, &mut app, picker);
        io::stdout().execute(DisableBracketedPaste)?;
        io::stdout().execute(DisableMouseCapture)?;
        outcome
    })
}

fn script_factory(mode: JsMode) -> io::Result<Option<Arc<dyn JsEngineFactory>>> {
    if mode == JsMode::Off {
        return Ok(None);
    }
    #[cfg(feature = "js")]
    {
        Ok(Some(Arc::new(textsurfer::script::BoaEngineFactory)))
    }
    #[cfg(not(feature = "js"))]
    {
        if mode == JsMode::On {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "JavaScript execution is unavailable; rebuild with --features js",
            ))
        } else {
            Ok(None)
        }
    }
}

fn init_logging(
    log_path: Option<&Path>,
    diagnostic_prefix: Option<&Path>,
) -> io::Result<Option<tracing_chrome::FlushGuard>> {
    if let Some(prefix) = diagnostic_prefix {
        return init_diagnostics(prefix).map(Some);
    }
    let Some(path) = log_path else {
        return Ok(None);
    };
    let filter = match std::env::var("RUST_LOG") {
        Ok(value) => parse_log_filter(&value)?,
        Err(std::env::VarError::NotPresent) => parse_log_filter("textsurfer=debug")?,
        Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidInput, error)),
    };
    let file = open_log_file(path)?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .try_init()
        .map_err(|error| io::Error::other(format!("cannot initialize logging: {error}")))?;
    Ok(None)
}

fn parse_log_filter(value: &str) -> io::Result<EnvFilter> {
    EnvFilter::try_new(value).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid RUST_LOG: {error}"),
        )
    })
}

fn open_log_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

#[derive(Debug, PartialEq, Eq)]
struct DiagnosticPaths {
    log: PathBuf,
    trace: PathBuf,
}

fn diagnostic_paths(prefix: &Path) -> DiagnosticPaths {
    let mut log = prefix.as_os_str().to_os_string();
    log.push(".log");
    let mut trace = prefix.as_os_str().to_os_string();
    trace.push(".trace.json");
    DiagnosticPaths {
        log: log.into(),
        trace: trace.into(),
    }
}

fn create_diagnostic_files(paths: &DiagnosticPaths) -> io::Result<(File, File)> {
    if let Some(parent) = paths.log.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.log)
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "cannot create diagnostics log {}: {error}",
                    paths.log.display()
                ),
            )
        })?;
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&paths.trace)
    {
        Ok(trace) => Ok((log, trace)),
        Err(error) => {
            drop(log);
            let _ = fs::remove_file(&paths.log);
            Err(io::Error::new(
                error.kind(),
                format!(
                    "cannot create diagnostics trace {}: {error}",
                    paths.trace.display()
                ),
            ))
        }
    }
}

fn init_diagnostics(prefix: &Path) -> io::Result<tracing_chrome::FlushGuard> {
    let paths = diagnostic_paths(prefix);
    let (log, trace) = create_diagnostic_files(&paths)?;
    let filter = parse_log_filter(DIAGNOSTIC_FILTER)?;
    let (chrome, guard) = tracing_chrome::ChromeLayerBuilder::new()
        .writer(BufWriter::new(trace))
        .include_args(true)
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(Mutex::new(log))
                .with_ansi(false)
                .with_filter(filter),
        )
        .with(chrome.with_filter(filter_fn(|metadata| {
            metadata.target() == "textsurfer::perf"
        })));
    if let Err(error) = subscriber.try_init() {
        drop(guard);
        let _ = fs::remove_file(&paths.log);
        let _ = fs::remove_file(&paths.trace);
        return Err(io::Error::other(format!(
            "cannot initialize diagnostics: {error}"
        )));
    }
    Ok(guard)
}

/// Start the framebuffer frontend, or explain that it was not built in.
///
/// The flag exists in both builds so the failure is a clear message rather than an
/// unrecognised argument, matching how `--js on` reports an unavailable engine.
#[cfg(feature = "vga")]
fn run_vga(
    fetch: Arc<dyn textsurfer::net::Fetch>,
    cli: &Cli,
    script_factory: Option<Arc<dyn JsEngineFactory>>,
) -> io::Result<()> {
    let net: Arc<dyn Navigate> = Arc::new(PoolNet::new(Arc::new(FetchPool::spawn(fetch, 4))));
    let options = textsurfer::vga::VgaOptions {
        scale: usize::from(cli.vga_scale),
        ..Default::default()
    };
    textsurfer::vga::run_with_scripts(net, options, cli.url.clone(), script_factory)
}

#[cfg(not(feature = "vga"))]
fn run_vga(
    _fetch: Arc<dyn textsurfer::net::Fetch>,
    _cli: &Cli,
    _script_factory: Option<Arc<dyn JsEngineFactory>>,
) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this build has no framebuffer frontend; rebuild with --features vga",
    ))
}

fn dump(
    fetch: Arc<dyn textsurfer::net::Fetch>,
    url: &str,
    cols: u16,
    rows: u16,
    script_factory: Option<Arc<dyn JsEngineFactory>>,
) -> io::Result<()> {
    let lines = dump_lines_with_scripts(
        fetch,
        url,
        Size { cols, rows },
        DEFAULT.palette(),
        script_factory,
    )?;
    let mut out = io::stdout().lock();
    for line in lines {
        writeln!(out, "{}", line.trim_end())?;
    }
    Ok(())
}

fn run(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    picker: ratatui_image::picker::Picker,
) -> io::Result<()> {
    let started = Instant::now();
    let mut events = CrosstermEvents;
    let area = terminal.size()?;
    let mut composer = FrameComposer::with_image_picker(area.into(), picker);
    let image_work = composer.image_work_signal();
    run_with(
        &mut events,
        app,
        || started.elapsed(),
        image_work,
        |app, damage| {
            let view = app.chrome_view();
            let span = tracing::trace_span!(
                target: "textsurfer::perf",
                "terminal_present",
                full = damage.content.full,
                scroll_rows = damage.content.scroll_rows,
                repaint = ?damage.content.repaint
            );
            let _guard = span.enter();
            composer.present(terminal.backend_mut(), &view, damage)
        },
    )
}

const IDLE_POLL: Duration = Duration::from_secs(60 * 60);

trait TerminalEvents {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool>;
    fn read(&mut self) -> io::Result<Event>;
}

struct CrosstermEvents;

impl TerminalEvents for CrosstermEvents {
    fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        event::poll(timeout)
    }

    fn read(&mut self) -> io::Result<Event> {
        event::read()
    }
}

fn run_with<E, N, D>(
    events: &mut E,
    app: &mut App,
    mut now: N,
    image_work: Option<std::sync::Arc<textsurfer::ui::frame::ImageWorkSignal>>,
    mut draw: D,
) -> io::Result<()>
where
    E: TerminalEvents,
    N: FnMut() -> Duration,
    D: FnMut(&App, &FrameDamage) -> io::Result<()>,
{
    let mut scheduler = FrameScheduler::default();
    let mut clipboard = arboard::Clipboard::new().ok();
    loop {
        let current = now();
        if let Some(batch) = scheduler.take_due(current) {
            app.advance(&batch, current);
        } else if app.next_wake().is_some_and(|deadline| deadline <= current) {
            app.advance(&InputBatch::new(), current);
        }
        if app.take_screenshot_request() {
            save_screenshot(app);
        }
        sync_clipboard(app, clipboard.as_mut());
        let mut damage = app.take_damage();
        if damage.is_empty() && image_work.as_ref().is_some_and(|work| work.ready()) {
            damage = FrameDamage::full();
        }
        if !damage.is_empty() {
            draw(app, &damage)?;
        }
        if app.should_quit() {
            // Before the pool's `Drop` can join a worker parked in the fetch timeout.
            app.shutdown_net();
            return Ok(());
        }
        let current = now();
        let mut timeout = scheduler
            .next_deadline(app.next_wake())
            .map_or(IDLE_POLL, |deadline| deadline.saturating_sub(current));
        if image_work.as_ref().is_some_and(|work| work.pending()) {
            timeout = timeout.min(Duration::from_millis(16));
        }
        if events.poll(timeout)? {
            push_terminal_event(&mut scheduler, events.read()?, now());
            for _ in 1..EVENTS_PER_FRAME {
                if !events.poll(Duration::ZERO)? {
                    break;
                }
                push_terminal_event(&mut scheduler, events.read()?, now());
            }
        }
    }
}

/// Write the whole page to `screenshots/`, naming the file after this frontend.
#[cfg(feature = "vga")]
fn save_screenshot(app: &mut App) {
    textsurfer::vga::capture::save_active_page(
        app,
        textsurfer::core::frontend::Frontend::Terminal,
        std::path::Path::new(textsurfer::vga::capture::SCREENSHOT_DIR),
    );
}

/// The glyph table that rasterises a capture is the window frontend's, so a build
/// without it can only say so.
#[cfg(not(feature = "vga"))]
fn save_screenshot(app: &mut App) {
    app.flash("screenshots need a build with --features vga".to_string());
}

fn push_terminal_event(scheduler: &mut FrameScheduler, event: Event, now: Duration) {
    let event = match event {
        Event::Key(key) => from_terminal_key(key).map(InputEvent::Key),
        Event::Mouse(mouse) => from_terminal_mouse(mouse).map(InputEvent::Mouse),
        Event::Resize(cols, rows) => Some(InputEvent::Resize {
            size: Size { cols, rows },
            phase: ResizePhase::Preview,
        }),
        Event::FocusLost => Some(InputEvent::PointerLeft),
        Event::Paste(text) => Some(InputEvent::Paste(text)),
        _ => None,
    };
    if let Some(event) = event {
        scheduler.push(event, now);
    }
}

fn sync_clipboard(app: &mut App, clipboard: Option<&mut arboard::Clipboard>) {
    use textsurfer::ui::widgets::text_field::ClipboardAction;

    let Some(request) = app.take_clipboard_request() else {
        return;
    };
    let Some(clipboard) = clipboard else {
        app.flash("system clipboard is unavailable".to_string());
        return;
    };
    match request {
        ClipboardAction::Write(text) => {
            if let Err(error) = clipboard.set_text(text) {
                app.flash(format!("cannot write clipboard: {error}"));
            }
        }
        ClipboardAction::Read => match clipboard.get_text() {
            Ok(text) => app.deliver_clipboard_text(&text),
            Err(error) => app.flash(format!("cannot read clipboard: {error}")),
        },
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
        event::MouseEventKind::ScrollUp => MouseKind::Wheel { rows: -WHEEL_ROWS },
        event::MouseEventKind::ScrollDown => MouseKind::Wheel { rows: WHEEL_ROWS },
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

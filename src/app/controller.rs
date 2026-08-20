use std::sync::Arc;
use std::time::Instant;

use crate::core::event::{Key, KeyEvent};
use crate::core::focus::Focus;
use crate::core::geom::Size;
use crate::core::url::url_fix;
use crate::html::{Html5everParser, HtmlParser, tree_dump};
use crate::net::{FetchPayload, charset_from_content_type, decode};
use crate::ui::chrome::ChromeView;
use crate::ui::editing::EditBuffer;
use crate::ui::keymap::{Action, DefaultKeymap, Keymap};
use crate::ui::mouse::ChromeGeometry;
use crate::ui::widgets::content::ContentLines;
use crate::ui::widgets::status::StatusView;
use crate::ui::widgets::tabs::TabChip;

use super::net::{Navigate, NoopNet, Route, route};
use super::startpage::{content_for, start_page};
use super::tabs::TabManager;

const DEFAULT_SIZE: Size = Size { cols: 80, rows: 24 };

pub struct App {
    focus: Focus,
    tabs: TabManager,
    address: EditBuffer,
    message: String,
    dirty: bool,
    quit: bool,
    generation: u64,
    geometry: ChromeGeometry,
    net: Arc<dyn Navigate>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self::with_net(Arc::new(NoopNet))
    }

    pub fn with_net(net: Arc<dyn Navigate>) -> Self {
        Self {
            focus: Focus::Content,
            tabs: TabManager::new(start_page()),
            address: EditBuffer::new(),
            message: String::new(),
            dirty: true,
            quit: false,
            generation: 0,
            geometry: ChromeGeometry::for_size(DEFAULT_SIZE),
            net,
        }
    }

    pub fn handle_key(&mut self, event: KeyEvent) {
        match DefaultKeymap.resolve(&event, self.focus) {
            Some(action) => self.apply(action),
            None => self.edit(event),
        }
    }

    pub fn submit_url(&mut self, url: &str) {
        let fixed = url_fix(url);
        if !fixed.is_empty() {
            self.open_url(&fixed);
        }
    }

    pub fn on_resize(&mut self, size: Size) {
        self.geometry = ChromeGeometry::for_size(size);
        self.touch();
    }

    pub fn step(&mut self, _now: Instant) {
        while let Some(payload) = self.net.poll_result() {
            let _ = self.deliver_fetch(payload);
        }
    }

    pub fn deliver_fetch(&mut self, payload: FetchPayload) -> bool {
        let accepted = payload.generation == self.tabs.active().generation;
        if !accepted {
            self.message = format!("dropped stale fetch for generation {}", payload.generation);
            self.touch();
            return false;
        }
        let tab = self.tabs.active_mut();
        match payload.result {
            Ok(response) => {
                let charset = response
                    .content_type
                    .as_deref()
                    .and_then(charset_from_content_type);
                let decoded = decode(&response.body, charset.as_deref());
                let outcome = Html5everParser::new(false).parse_document(&decoded.text);
                let dump = tree_dump(&outcome.document.borrow(), false);
                tab.content = dump.lines().map(str::to_string).collect();
                tab.title = response.final_url.clone().into();
                tab.url = response.final_url.clone().into();
                tab.push_history(response.final_url.as_str());
                self.message = format!(
                    "accepted gen {} - {} ({} parse errors)",
                    payload.generation, response.final_url, outcome.parse_errors
                );
            }
            Err(error) => {
                tab.content = vec![
                    format!("failed to load {}", tab.url),
                    String::new(),
                    format!("  {error}"),
                ];
                self.message =
                    format!("accepted gen {} - load failed: {error}", payload.generation);
            }
        }
        self.touch();
        true
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn active_url(&self) -> &str {
        &self.tabs.active().url
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn chrome_view(&self) -> ChromeView {
        let active = self.tabs.active();
        let tabs = self
            .tabs
            .tabs()
            .iter()
            .map(|tab| TabChip {
                title: tab.title.clone(),
                url: tab.url.clone(),
            })
            .collect();
        let address = if self.focus == Focus::Address {
            self.address.text()
        } else {
            active.url.clone()
        };
        ChromeView {
            geometry: self.geometry,
            tabs,
            active_tab: self.tabs.active_index(),
            address,
            address_cursor: self.address.cursor(),
            address_focused: self.focus == Focus::Address,
            content: ContentLines {
                lines: active.content.clone(),
                scroll: active.scroll,
            },
            status: StatusView {
                url: active.url.clone(),
                message: self.message.clone(),
            },
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Quit => self.quit = true,
            Action::NewTab => self.new_tab(),
            Action::CloseTab => self.close_tab(),
            Action::NextTab => {
                self.tabs.next();
                self.touch();
            }
            Action::PrevTab => {
                self.tabs.prev();
                self.touch();
            }
            Action::FocusAddress => {
                self.address.set_text(&self.tabs.active().url);
                self.focus = Focus::Address;
                self.touch();
            }
            Action::FocusContent => {
                self.focus = Focus::Content;
                self.touch();
            }
            Action::SubmitAddress => self.submit_address(),
            Action::ScrollDown => self.scroll(1),
            Action::ScrollUp => self.scroll(-1),
            Action::ScrollTop => self.set_scroll(0),
            Action::ScrollBottom => self.set_scroll(u16::MAX),
            Action::ActivateLink => self.pending("link activation arrives in M2"),
            Action::NextLink => self.pending("tab-cycle link navigation arrives in M2"),
            Action::PrevLink => self.pending("shift-tab link navigation arrives in M2"),
            Action::Help => self.pending("the help overlay arrives in M2"),
        }
    }

    fn edit(&mut self, event: KeyEvent) {
        if self.focus != Focus::Address {
            return;
        }
        match event.code {
            Key::Char(ch) => self.address.insert(ch),
            Key::Backspace => self.address.backspace(),
            Key::Delete => self.address.delete(),
            Key::Left => self.address.left(),
            Key::Right => self.address.right(),
            Key::Home => self.address.home(),
            Key::End => self.address.end(),
            _ => return,
        }
        self.touch();
    }

    fn submit_address(&mut self) {
        let fixed = url_fix(&self.address.text());
        if fixed.is_empty() {
            self.message = "type a URL or search terms first".to_string();
            self.touch();
            return;
        }
        self.open_url(&fixed);
    }

    fn open_url(&mut self, url: &str) {
        let parsed = match url::Url::parse(url) {
            Ok(parsed) => parsed,
            Err(_) => {
                self.message = format!("cannot parse url: {url}");
                self.touch();
                return;
            }
        };
        match route(&parsed) {
            Route::StartPage => {
                self.generation = self.generation.wrapping_add(1);
                let generation = self.generation;
                self.tabs
                    .open_tab(url.to_string(), generation, start_page());
            }
            Route::Fetch => {
                self.generation = self.generation.wrapping_add(1);
                let generation = self.generation;
                self.tabs
                    .open_tab(url.to_string(), generation, content_for(url));
                self.net.submit(generation, parsed);
            }
            Route::Reject => {
                self.message = format!("unsupported scheme: {}", parsed.scheme());
                self.touch();
                return;
            }
        }
        self.message = format!("loading {url}");
        self.address.set_text(url);
        self.focus = Focus::Content;
        self.touch();
    }

    fn new_tab(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.tabs.open_tab(String::new(), generation, start_page());
        self.focus = Focus::Content;
        self.touch();
    }

    fn close_tab(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.tabs.close_active(generation, start_page());
        self.touch();
    }

    fn pending(&mut self, notice: &str) {
        self.message = notice.to_string();
        self.touch();
    }

    fn max_scroll(&self) -> u16 {
        let rows = self.geometry.content_rows();
        self.tabs.active().content.len().saturating_sub(rows) as u16
    }

    fn scroll(&mut self, delta: i32) {
        let max = self.max_scroll();
        let next = i32::from(self.tabs.active().scroll) + delta;
        let clamped = next.clamp(0, i32::from(max));
        self.tabs.active_mut().scroll = clamped as u16;
        self.touch();
    }

    fn set_scroll(&mut self, requested: u16) {
        let max = self.max_scroll();
        self.tabs.active_mut().scroll = requested.min(max);
        self.touch();
    }

    fn touch(&mut self) {
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::event::KeyModifiers;
    use crate::net::{FetchError, FetchResponse};
    use std::sync::Mutex;
    use url::Url;

    fn press(code: Key) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::default(),
        }
    }

    #[derive(Default)]
    struct FakeNet {
        pending: Mutex<Vec<FetchPayload>>,
        submitted: Mutex<Vec<(u64, Url)>>,
    }

    impl Navigate for FakeNet {
        fn submit(&self, generation: u64, url: Url) {
            self.submitted
                .lock()
                .unwrap()
                .push((generation, url.clone()));
            self.pending.lock().unwrap().push(FetchPayload {
                generation,
                result: Ok(FetchResponse {
                    final_url: url,
                    body: b"<p>hi there</p>".to_vec(),
                    content_type: None,
                }),
            });
        }

        fn poll_result(&self) -> Option<FetchPayload> {
            self.pending.lock().unwrap().pop()
        }
    }

    #[test]
    fn fresh_app_shows_the_start_page_in_content_focus() {
        let mut app = App::new();
        assert_eq!(app.focus(), Focus::Content);
        assert_eq!(app.tab_count(), 1);
        assert!(app.active_url().is_empty());
        assert!(app.take_dirty());
        assert!(!app.take_dirty());
        let view = app.chrome_view();
        assert!(!view.address_focused);
        assert_eq!(
            view.content.lines.first().unwrap(),
            "TextSurf - a text-mode browser"
        );
    }

    #[test]
    fn q_quits_but_never_while_typing_in_the_address() {
        let mut app = App::new();
        app.handle_key(press(Key::Char('q')));
        assert!(app.should_quit());
        let mut app = App::new();
        app.handle_key(press(Key::Char('/')));
        app.handle_key(press(Key::Char('q')));
        assert!(!app.should_quit());
        assert_eq!(app.active_url(), "");
        assert_eq!(app.focus(), Focus::Address);
    }

    #[test]
    fn submit_address_loads_a_url_fixed_tab() {
        let mut app = App::new();
        app.handle_key(press(Key::Char('/')));
        for ch in "example.com".chars() {
            app.handle_key(press(Key::Char(ch)));
        }
        app.handle_key(press(Key::Enter));
        assert_eq!(app.tab_count(), 2);
        assert_eq!(app.active_url(), "https://example.com");
        assert_eq!(app.focus(), Focus::Content);
        assert!(app.message().contains("loading https://example.com"));
    }

    #[test]
    fn empty_submission_shows_a_hint_and_stays_put() {
        let mut app = App::new();
        app.handle_key(press(Key::Char('/')));
        app.handle_key(press(Key::Enter));
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.message(), "type a URL or search terms first");
    }

    #[test]
    fn scroll_clamps_to_the_content_window() {
        let mut app = App::new();
        app.on_resize(Size { cols: 80, rows: 8 });
        assert_eq!(app.geometry.content_rows(), 5);
        let max = app.tabs.active().content.len().saturating_sub(5) as u16;
        for _ in 0..10 {
            app.handle_key(press(Key::Char('j')));
        }
        assert_eq!(app.tabs.active().scroll, max);
        app.handle_key(press(Key::Char('k')));
        assert_eq!(app.tabs.active().scroll, max - 1);
        app.handle_key(press(Key::End));
        assert_eq!(app.tabs.active().scroll, max);
        app.handle_key(press(Key::Home));
        assert_eq!(app.tabs.active().scroll, 0);
    }

    #[test]
    fn resize_remaps_the_geometry_and_clamps_scroll() {
        let mut app = App::new();
        app.handle_key(press(Key::End));
        app.on_resize(Size { cols: 40, rows: 10 });
        let max = app.tabs.active().content.len().saturating_sub(5) as u16;
        assert!(app.tabs.active().scroll <= max);
    }

    #[test]
    fn stale_fetch_results_are_dropped_fresh_ones_accepted() {
        let mut app = App::new();
        app.submit_url("https://example.com");
        let generation = app.tabs.active().generation;
        assert!(!app.deliver_fetch(FetchPayload {
            generation: generation - 1,
            result: Ok(FetchResponse {
                final_url: url::Url::parse("https://example.com/").unwrap(),
                body: vec![],
                content_type: None,
            }),
        }));
        assert!(app.message().contains("stale"));
        assert!(app.deliver_fetch(FetchPayload {
            generation,
            result: Ok(FetchResponse {
                final_url: url::Url::parse("https://example.com/").unwrap(),
                body: vec![],
                content_type: None,
            }),
        }));
        assert!(app.message().contains("accepted"));
    }

    #[test]
    fn fake_fetch_loads_the_parsed_tree_into_the_tab() {
        let fake: Arc<dyn Navigate> = Arc::new(FakeNet::default());
        let mut app = App::with_net(fake);
        app.submit_url("example.com");
        assert_eq!(app.tab_count(), 2);
        assert_eq!(app.active_url(), "https://example.com");
        assert!(
            app.tabs
                .active()
                .content
                .iter()
                .any(|l| l.starts_with("  fetching"))
        );
        app.step(Instant::now());
        let tab = app.tabs.active();
        assert!(tab.content.first().unwrap().starts_with("#document"));
        assert!(tab.content.iter().any(|line| line.contains("hi there")));
        assert_eq!(tab.title, "https://example.com/");
        assert!(
            app.message()
                .contains("accepted gen 1 - https://example.com/")
        );
    }

    #[test]
    fn fetch_errors_land_in_the_tab_content() {
        struct ErrorNet {
            pending: Mutex<Vec<FetchPayload>>,
        }
        impl Navigate for ErrorNet {
            fn submit(&self, generation: u64, _url: Url) {
                self.pending.lock().unwrap().push(FetchPayload {
                    generation,
                    result: Err(FetchError::HttpStatus(404)),
                });
            }
            fn poll_result(&self) -> Option<FetchPayload> {
                self.pending.lock().unwrap().pop()
            }
        }
        let fake: Arc<dyn Navigate> = Arc::new(ErrorNet {
            pending: Mutex::new(Vec::new()),
        });
        let mut app = App::with_net(fake);
        app.submit_url("https://example.com");
        app.step(Instant::now());
        assert!(
            app.tabs
                .active()
                .content
                .iter()
                .any(|l| l.contains("failed to load"))
        );
        assert!(app.message().contains("http status 404"));
    }

    #[test]
    fn unknown_schemes_never_open_a_tab() {
        let mut app = App::new();
        app.submit_url("about:config");
        assert_eq!(app.tab_count(), 1);
        assert!(app.message().contains("unsupported scheme: about"));
    }

    #[test]
    fn about_blank_opens_the_start_page_without_fetching() {
        let mut app = App::new();
        app.submit_url("about:blank");
        assert_eq!(app.tab_count(), 2);
        assert_eq!(app.active_url(), "about:blank");
        assert_eq!(app.tabs.active().content, start_page());
    }

    #[test]
    fn unparseable_inputs_show_a_status_error() {
        let mut app = App::new();
        app.open_url("not a url at all with spaces");
        assert_eq!(app.tab_count(), 1);
        assert!(app.message().contains("cannot parse url"));
    }

    #[test]
    fn tab_cycling_keeps_per_tab_state() {
        let mut app = App::new();
        app.submit_url("https://a.example");
        app.submit_url("https://b.example");
        assert_eq!(app.tab_count(), 3);
        app.handle_key(ctrl(press(Key::Char('p'))));
        assert_eq!(app.active_url(), "https://a.example");
        app.handle_key(ctrl(press(Key::Char('n'))));
        assert_eq!(app.active_url(), "https://b.example");
        app.handle_key(ctrl(press(Key::Char('w'))));
        assert_eq!(app.tab_count(), 2);
        app.handle_key(ctrl(press(Key::Char('w'))));
        app.handle_key(ctrl(press(Key::Char('w'))));
        assert_eq!(app.tab_count(), 1);
        assert!(app.active_url().is_empty());
    }

    fn ctrl(mut event: KeyEvent) -> KeyEvent {
        event.modifiers.ctrl = true;
        event
    }
}

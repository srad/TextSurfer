use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use crate::core::event::{Key, KeyEvent};
use crate::core::focus::Focus;
use crate::core::geom::Size;
use crate::core::url::url_fix;
use crate::css::ColorScheme;
use crate::net::{FetchPayload, ResourceId, charset_from_content_type, decode, decode_text};
use crate::paint::DisplayList;
use crate::ui::chrome::ChromeView;
use crate::ui::editing::EditBuffer;
use crate::ui::keymap::{Action, DefaultKeymap, Keymap};
use crate::ui::mouse::ChromeGeometry;
use crate::ui::widgets::content::ContentLines;
use crate::ui::widgets::status::StatusView;
use crate::ui::widgets::tabs::TabChip;

use super::net::{Navigate, NoopNet, Route, route};
use super::page_load::{PageLoad, PageLoadOptions};
use super::render::{RenderedPage, ResponseKind, paint_document, response_kind};
use super::startpage::{content_for, start_page_for};
use super::tabs::TabManager;
use crate::ui::theme::NORTON;
use crate::ui::widgets::menu::MENUS;

const DEFAULT_SIZE: Size = Size { cols: 80, rows: 24 };
const STARTUP_HINT: &str = "type a URL and press Enter";

pub struct App {
    focus: Focus,
    tabs: TabManager,
    address: EditBuffer,
    dirty: bool,
    quit: bool,
    generation: u64,
    geometry: ChromeGeometry,
    net: Arc<dyn Navigate>,
    menu_open: bool,
    menu_active: usize,
    menu_item: usize,
    focus_before_menu: Focus,
    now: Duration,
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
        let geometry = ChromeGeometry::for_size(DEFAULT_SIZE);
        Self {
            focus: Focus::Address,
            tabs: TabManager::new(
                start_page_for(content_viewport(geometry)),
                STARTUP_HINT.to_string(),
            ),
            address: EditBuffer::new(),
            dirty: true,
            quit: false,
            generation: 0,
            geometry,
            net,
            menu_open: false,
            menu_active: 0,
            menu_item: 0,
            focus_before_menu: Focus::Address,
            now: Duration::ZERO,
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
        let width = self.geometry.content_cols();
        let rows = self.geometry.content_rows();
        let viewport = Size {
            cols: width.min(usize::from(u16::MAX)) as u16,
            rows: rows.min(usize::from(u16::MAX)) as u16,
        };
        let active = self.tabs.active_index();
        for (index, tab) in self.tabs.tabs_mut().iter_mut().enumerate() {
            if tab.url.is_empty() || tab.url == "about:blank" {
                tab.painted = start_page_for(viewport);
            } else if let Some(load) = tab.load.as_mut() {
                if index == active {
                    if let Some(page) = load.resize(viewport) {
                        apply_rendered_page(tab, page, width);
                    }
                } else {
                    load.set_viewport(viewport);
                    tab.render_dirty = true;
                }
            } else if tab.layout_width != width
                && let (Some(document), Some(styles)) = (&tab.document, &tab.styles)
            {
                tab.painted =
                    paint_document(&document.borrow(), styles, viewport, NORTON.palette());
            }
            tab.layout_width = width;
            tab.scroll = tab.scroll.min(tab.painted.len().saturating_sub(rows));
        }
        self.touch();
    }

    pub fn step(&mut self, now: Duration) {
        self.now = now;
        while let Some(payload) = self.net.poll_result() {
            let _ = self.deliver_fetch(payload);
        }
        let width = self.geometry.content_cols();
        let active = self.tabs.active_mut();
        let page = active
            .load
            .as_mut()
            .and_then(|load| load.render_if_ready(now));
        if let Some(page) = page {
            let css_warnings = page.css_warnings;
            let parse_errors = page.parse_errors;
            apply_rendered_page(active, page, width);
            update_load_message(active, parse_errors, css_warnings);
            self.touch();
        }
    }

    pub fn deliver_fetch(&mut self, payload: FetchPayload) -> bool {
        let tab_id = payload.tab_id;
        let generation = payload.generation;
        let resource_id = payload.resource_id;
        let active_index = self.tabs.active_index();
        let viewport = Size {
            cols: self.geometry.content_cols().min(usize::from(u16::MAX)) as u16,
            rows: self.geometry.content_rows().min(usize::from(u16::MAX)) as u16,
        };
        let width = self.geometry.content_cols();
        let mut commands = Vec::new();
        let mut cancel = false;
        let mut visible_change = resource_id == ResourceId::DOCUMENT;
        let accepted = {
            let Some((index, tab)) = self.tabs.find_load_mut(tab_id, generation) else {
                return false;
            };
            if resource_id != ResourceId::DOCUMENT {
                let Some(load) = tab.load.as_mut() else {
                    return false;
                };
                if !load.deliver(resource_id, payload.result) {
                    return false;
                }
                commands = load.take_commands();
                cancel = load.take_cancel_requested();
                let page = (index == active_index)
                    .then(|| load.render_if_ready(self.now))
                    .flatten();
                if let Some(page) = page {
                    let css_warnings = page.css_warnings;
                    let parse_errors = page.parse_errors;
                    apply_rendered_page(tab, page, width);
                    update_load_message(tab, parse_errors, css_warnings);
                    visible_change = true;
                } else if index != active_index {
                    tab.render_dirty = true;
                }
                true
            } else {
                match payload.result {
                    Ok(response) => {
                        let kind = response_kind(response.content_type.as_deref());
                        let charset = response
                            .content_type
                            .as_deref()
                            .and_then(charset_from_content_type);
                        tab.title = response.final_url.clone().into();
                        tab.url = response.final_url.clone().into();
                        match kind {
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
                                        started: self.now,
                                    },
                                );
                                commands = load.take_commands();
                                cancel = load.take_cancel_requested();
                                let parse_errors = load.parse_errors();
                                let page = (index == active_index)
                                    .then(|| load.render_if_ready(self.now))
                                    .flatten();
                                tab.load = Some(load);
                                if let Some(page) = page {
                                    let css_warnings = page.css_warnings;
                                    apply_rendered_page(tab, page, width);
                                    update_load_message(tab, parse_errors, css_warnings);
                                } else {
                                    tab.render_dirty = index != active_index;
                                    let count =
                                        tab.load.as_ref().map_or(0, PageLoad::external_occurrences);
                                    tab.message =
                                        format!("loading {} ({count} stylesheets)", tab.url);
                                }
                            }
                            ResponseKind::PlainText => {
                                let decoded = decode_text(&response.body, charset.as_deref());
                                tab.load = None;
                                tab.document = None;
                                tab.styles = None;
                                tab.painted = DisplayList::from_lines(
                                    &decoded.text.lines().map(str::to_string).collect::<Vec<_>>(),
                                );
                                tab.message = format!(
                                    "accepted gen {} - {} (plain text)",
                                    generation, tab.url
                                );
                            }
                            ResponseKind::Unsupported(content_type) => {
                                tab.load = None;
                                tab.document = None;
                                tab.styles = None;
                                tab.painted = DisplayList::from_lines(&[
                                    format!("cannot display {}", tab.url),
                                    String::new(),
                                    format!("  unsupported content type: {content_type}"),
                                ]);
                                tab.message = format!("unsupported content type: {content_type}");
                            }
                        }
                    }
                    Err(error) => {
                        tab.load = None;
                        tab.document = None;
                        tab.styles = None;
                        tab.painted = DisplayList::from_lines(&[
                            format!("failed to load {}", tab.url),
                            String::new(),
                            format!("  {error}"),
                        ]);
                        tab.message = format!("accepted gen {generation} - load failed: {error}");
                    }
                }
                true
            }
        };
        for command in commands {
            self.net
                .submit(tab_id, generation, command.resource_id, command.url);
        }
        if cancel {
            self.net.cancel(tab_id, generation);
        }
        if visible_change {
            self.touch();
        }
        accepted
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
        &self.tabs.active().message
    }

    pub fn active_url(&self) -> &str {
        &self.tabs.active().url
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn chrome_view(&self) -> ChromeView<'_> {
        let active = self.tabs.active();
        let address = if self.focus == Focus::Address {
            Cow::Borrowed(self.address.text())
        } else {
            Cow::Borrowed(active.url.as_str())
        };
        ChromeView {
            geometry: self.geometry,
            theme: NORTON,
            can_back: active.history_pos > 0,
            can_forward: active.history_pos + 1 < active.history.len(),
            address,
            address_cursor: self.address.cursor(),
            address_focused: self.focus == Focus::Address,
            menu_open: self.menu_open,
            menu_active: self.menu_active,
            menu_item: self.menu_item,
            tabs: self
                .tabs
                .tabs()
                .iter()
                .map(|tab| TabChip {
                    title: Cow::Borrowed(tab.title.as_str()),
                    url: Cow::Borrowed(tab.url.as_str()),
                })
                .collect(),
            active_tab: self.tabs.active_index(),
            content: ContentLines {
                painted: &active.painted,
                scroll: active.scroll,
            },
            status: StatusView {
                url: Cow::Borrowed(active.url.as_str()),
                message: Cow::Borrowed(active.message.as_str()),
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
                self.activate_current();
                self.touch();
            }
            Action::PrevTab => {
                self.tabs.prev();
                self.activate_current();
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
            Action::ScrollPageDown => self.scroll(self.page_step()),
            Action::ScrollPageUp => self.scroll(-self.page_step()),
            Action::ScrollTop => self.set_scroll(0),
            Action::ScrollBottom => self.set_scroll(usize::MAX),
            Action::ActivateLink => self.pending("link activation arrives in M2"),
            Action::NextLink => self.pending("tab-cycle link navigation arrives in M2"),
            Action::PrevLink => self.pending("shift-tab link navigation arrives in M2"),
            Action::Help => self.pending("the help overlay arrives in M2"),
            Action::Back => self.go_back(),
            Action::Forward => self.go_forward(),
            Action::Reload => self.reload(),
            Action::Home => self.home(),
            Action::FocusMenu => self.focus_menu(),
            Action::MenuOpen(menu) => self.open_menu(menu),
            Action::MenuClose => self.close_menu(),
            Action::MenuNext => {
                let len = MENUS[self.menu_active].len();
                if len > 0 {
                    self.menu_item = (self.menu_item + 1).min(len - 1);
                }
                self.touch();
            }
            Action::MenuPrev => {
                self.menu_item = self.menu_item.saturating_sub(1);
                self.touch();
            }
            Action::MenuLeft => {
                self.menu_active = if self.menu_active == 0 {
                    MENUS.len() - 1
                } else {
                    self.menu_active - 1
                };
                self.menu_item = 0;
                self.touch();
            }
            Action::MenuRight => {
                self.menu_active = (self.menu_active + 1) % MENUS.len();
                self.menu_item = 0;
                self.touch();
            }
            Action::MenuSelect => self.menu_select(),
            Action::ThemeInfo => {
                self.tabs.active_mut().message = "theme: Norton".to_string();
                self.touch();
            }
        }
    }

    fn focus_menu(&mut self) {
        if self.menu_open && self.focus == Focus::Menu {
            self.close_menu();
        } else {
            self.focus_before_menu = self.focus;
            self.menu_active = 0;
            self.menu_item = 0;
            self.menu_open = true;
            self.focus = Focus::Menu;
            self.touch();
        }
    }

    fn open_menu(&mut self, menu: usize) {
        if self.focus != Focus::Menu {
            self.focus_before_menu = self.focus;
        }
        self.focus = Focus::Menu;
        self.menu_open = true;
        self.menu_active = menu.min(MENUS.len() - 1);
        self.menu_item = 0;
        self.touch();
    }

    fn close_menu(&mut self) {
        self.menu_open = false;
        self.focus = self.focus_before_menu;
        self.touch();
    }

    fn menu_select(&mut self) {
        let action = menu_item_action(self.menu_active, self.menu_item);
        self.menu_open = false;
        self.focus = self.focus_before_menu;
        self.touch();
        if let Some(action) = action {
            self.apply(action);
        }
    }

    fn edit(&mut self, event: KeyEvent) {
        if self.focus != Focus::Address || event.modifiers.ctrl || event.modifiers.alt {
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
        let fixed = url_fix(self.address.text());
        if fixed.is_empty() {
            self.tabs.active_mut().message = "type a URL or search terms first".to_string();
            self.touch();
            return;
        }
        self.navigate(&fixed, true);
        self.address.set_text(&fixed);
    }

    fn open_url(&mut self, url: &str) {
        self.navigate(url, true);
    }

    fn navigate(&mut self, raw: &str, record: bool) {
        let fixed = url_fix(raw);
        if fixed.is_empty() {
            return;
        }
        let parsed = match url::Url::parse(&fixed) {
            Ok(parsed) => parsed,
            Err(_) => {
                self.tabs.active_mut().message = format!("cannot parse url: {fixed}");
                self.touch();
                return;
            }
        };
        match route(&parsed) {
            Route::StartPage => {
                self.net
                    .cancel(self.tabs.active().id, self.tabs.active().generation);
                self.generation = self.generation.wrapping_add(1);
                let generation = self.generation;
                let page = start_page_for(content_viewport(self.geometry));
                self.repoint_active(&fixed, generation, page);
                if record {
                    self.tabs.active_mut().push_history(&fixed);
                }
                self.tabs.active_mut().message = "Ready".to_string();
            }
            Route::Fetch => {
                self.net
                    .cancel(self.tabs.active().id, self.tabs.active().generation);
                self.generation = self.generation.wrapping_add(1);
                let generation = self.generation;
                self.repoint_active(&fixed, generation, content_for(&fixed));
                if record {
                    self.tabs.active_mut().push_history(&fixed);
                }
                self.tabs.active_mut().message = format!("loading {fixed}");
                self.net.submit(
                    self.tabs.active().id,
                    generation,
                    ResourceId::DOCUMENT,
                    parsed,
                );
            }
            Route::Reject => {
                self.tabs.active_mut().message = format!("unsupported scheme: {}", parsed.scheme());
                self.touch();
                return;
            }
        }
        self.focus = Focus::Content;
        self.touch();
    }

    fn repoint_active(&mut self, url: &str, generation: u64, painted: DisplayList) {
        let tab = self.tabs.active_mut();
        tab.url = url.to_string();
        tab.title = url.to_string();
        tab.painted = painted;
        tab.scroll = 0;
        tab.layout_width = self.geometry.content_cols();
        tab.generation = generation;
        tab.document = None;
        tab.styles = None;
        tab.load = None;
        tab.render_dirty = false;
    }

    fn go_back(&mut self) {
        if self.tabs.active_mut().back() {
            let url = self
                .tabs
                .active()
                .current()
                .expect("back moved to a history entry")
                .to_string();
            self.navigate(&url, false);
        } else {
            self.tabs.active_mut().message = "already at the first page".to_string();
            self.touch();
        }
    }

    fn go_forward(&mut self) {
        if self.tabs.active_mut().forward() {
            let url = self
                .tabs
                .active()
                .current()
                .expect("forward moved to a history entry")
                .to_string();
            self.navigate(&url, false);
        } else {
            self.tabs.active_mut().message = "already at the last page".to_string();
            self.touch();
        }
    }

    fn reload(&mut self) {
        let url = self.tabs.active().url.clone();
        if url.is_empty() {
            self.tabs.active_mut().message = "nothing to reload".to_string();
            self.touch();
            return;
        }
        self.navigate(&url, false);
    }

    fn home(&mut self) {
        self.navigate("about:blank", true);
    }

    fn new_tab(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.tabs.open_tab(
            String::new(),
            generation,
            start_page_for(content_viewport(self.geometry)),
            STARTUP_HINT.to_string(),
        );
        self.address.set_text("");
        self.focus = Focus::Address;
        self.touch();
    }

    fn close_tab(&mut self) {
        self.net
            .cancel(self.tabs.active().id, self.tabs.active().generation);
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.tabs.close_active(
            generation,
            start_page_for(content_viewport(self.geometry)),
            STARTUP_HINT.to_string(),
        );
        if self.focus == Focus::Address {
            self.address.set_text(&self.tabs.active().url);
        }
        self.touch();
    }

    fn activate_current(&mut self) {
        let width = self.geometry.content_cols();
        let now = self.now;
        let tab = self.tabs.active_mut();
        let page = match tab.load.as_mut() {
            Some(load) if tab.render_dirty && load.has_painted() => Some(load.force_render()),
            Some(load) => load.render_if_ready(now),
            None => None,
        };
        if let Some(page) = page {
            let css_warnings = page.css_warnings;
            let parse_errors = page.parse_errors;
            apply_rendered_page(tab, page, width);
            update_load_message(tab, parse_errors, css_warnings);
        }
        tab.render_dirty = false;
    }

    fn pending(&mut self, notice: &str) {
        self.tabs.active_mut().message = notice.to_string();
        self.touch();
    }

    fn max_scroll(&self) -> usize {
        let rows = self.geometry.content_rows();
        self.tabs.active().painted.len().saturating_sub(rows)
    }

    fn page_step(&self) -> i32 {
        let rows = self.geometry.content_rows().saturating_sub(1).max(1);
        i32::try_from(rows).unwrap_or(i32::MAX)
    }

    fn scroll(&mut self, delta: i32) {
        let max = self.max_scroll();
        let current = self.tabs.active().scroll;
        self.tabs.active_mut().scroll = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            current.saturating_add(delta as usize).min(max)
        };
        self.touch();
    }

    fn set_scroll(&mut self, requested: usize) {
        let max = self.max_scroll();
        self.tabs.active_mut().scroll = requested.min(max);
        self.touch();
    }

    fn touch(&mut self) {
        self.dirty = true;
    }
}

fn content_viewport(geometry: ChromeGeometry) -> Size {
    Size {
        cols: geometry.content_cols().min(usize::from(u16::MAX)) as u16,
        rows: geometry.content_rows().min(usize::from(u16::MAX)) as u16,
    }
}

fn apply_rendered_page(tab: &mut super::tab::Tab, page: RenderedPage, width: usize) {
    tab.painted = page.painted;
    tab.layout_width = width;
    tab.document = Some(page.document);
    tab.styles = Some(page.styles);
    tab.render_dirty = false;
}

fn update_load_message(tab: &mut super::tab::Tab, parse_errors: usize, css_warnings: usize) {
    let Some(load) = tab.load.as_ref() else {
        return;
    };
    let occurrences = load.external_occurrences();
    let failures = load.failed_resources();
    if occurrences == 0 && failures == 0 && !load.external_disabled() {
        tab.message = if css_warnings == 0 {
            format!(
                "accepted gen {} - {} ({} parse errors)",
                tab.generation, tab.url, parse_errors
            )
        } else {
            format!(
                "accepted gen {} - {} ({} parse errors, {} CSS warnings)",
                tab.generation, tab.url, parse_errors, css_warnings
            )
        };
        return;
    }
    let disabled = if load.external_disabled() {
        ", external CSS disabled"
    } else {
        ""
    };
    tab.message = format!(
        "accepted gen {} - {} ({} parse errors, {} CSS warnings, {} stylesheets, {} failed{})",
        tab.generation, tab.url, parse_errors, css_warnings, occurrences, failures, disabled
    );
}

fn menu_item_action(menu: usize, item: usize) -> Option<Action> {
    match (menu, item) {
        (0, 0) => Some(Action::NewTab),
        (0, 1) => Some(Action::CloseTab),
        (0, 2) => Some(Action::Reload),
        (0, 3) => Some(Action::Quit),
        (1, 0) => Some(Action::Back),
        (1, 1) => Some(Action::Forward),
        (1, 2) => Some(Action::Home),
        (2, 0) => Some(Action::ThemeInfo),
        (3, 0) => Some(Action::Help),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::startpage::start_page;
    use crate::core::event::KeyModifiers;
    use crate::net::{FetchError, FetchResponse};
    use proptest::prelude::*;
    use std::sync::Mutex;
    use url::Url;

    proptest! {
        #[test]
        fn scroll_clamping_is_a_fixed_point_under_any_key_sequence(
            document_rows in 0usize..400,
            terminal_rows in 7u16..40,
            steps in prop::collection::vec(
                prop::sample::select(vec![
                    Key::Char('j'),
                    Key::Char('k'),
                    Key::Char(' '),
                    Key::Char('b'),
                    Key::Home,
                    Key::End,
                ]),
                0..24,
            ),
        ) {
            let mut app = App::new();
            app.handle_key(press(Key::Esc));
            app.on_resize(Size { cols: 80, rows: terminal_rows });
            app.tabs.active_mut().url = "https://example.com".to_string();
            app.tabs.active_mut().painted =
                DisplayList::from_lines(&vec![String::new(); document_rows]);
            for step in steps {
                app.handle_key(press(step));
                let max = app
                    .tabs
                    .active()
                    .painted
                    .len()
                    .saturating_sub(app.geometry.content_rows());
                prop_assert!(app.tabs.active().scroll <= max);
            }
            let settled = app.tabs.active().scroll;
            app.on_resize(Size { cols: 80, rows: terminal_rows });
            prop_assert_eq!(
                app.tabs.active().scroll,
                settled,
                "re-clamping an already clamped scroll must not move it"
            );
        }
    }

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
        fn submit(&self, tab_id: u64, generation: u64, resource_id: ResourceId, url: Url) {
            self.submitted
                .lock()
                .unwrap()
                .push((generation, url.clone()));
            self.pending.lock().unwrap().push(FetchPayload {
                tab_id,
                generation,
                resource_id,
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
    fn fresh_app_starts_focused_on_the_address_bar() {
        let mut app = App::new();
        assert_eq!(app.focus(), Focus::Address);
        assert_eq!(app.tab_count(), 1);
        assert!(app.active_url().is_empty());
        assert_eq!(app.message(), STARTUP_HINT);
        assert!(app.take_dirty());
        assert!(!app.take_dirty());
        let view = app.chrome_view();
        assert!(view.address_focused);
        assert_eq!(view.content.painted, &start_page());
    }

    #[test]
    fn q_never_quits_while_typing_but_does_from_content() {
        let mut app = App::new();
        app.handle_key(press(Key::Char('q')));
        assert!(!app.should_quit(), "fresh app types q into the address");
        assert_eq!(app.focus(), Focus::Address);
        app.handle_key(press(Key::Char('/')));
        assert!(!app.should_quit(), "typing more text never quits");
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.handle_key(press(Key::Char('q')));
        assert!(app.should_quit(), "q from content focus must quit");
    }

    #[test]
    fn control_and_alt_chords_do_not_insert_characters_in_the_address() {
        let mut app = App::new();
        app.handle_key(ctrl(press(Key::Char('t'))));
        app.handle_key(alt(press(Key::Char('x'))));
        assert!(app.chrome_view().address.is_empty());
    }

    #[test]
    fn submit_address_loads_the_current_tab() {
        let mut app = App::new();
        for ch in "example.com".chars() {
            app.handle_key(press(Key::Char(ch)));
        }
        app.handle_key(press(Key::Enter));
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.active_url(), "https://example.com");
        assert_eq!(app.focus(), Focus::Content);
        assert!(app.message().contains("loading https://example.com"));
    }

    #[test]
    fn empty_submission_shows_a_hint_and_stays_put() {
        let mut app = App::new();
        app.handle_key(press(Key::Enter));
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.message(), "type a URL or search terms first");
    }

    #[test]
    fn scroll_clamps_to_the_content_window() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.on_resize(Size { cols: 80, rows: 9 });
        assert_eq!(app.geometry.content_rows(), 3);
        app.tabs.active_mut().painted = DisplayList::from_lines(&vec![String::new(); 10]);
        let max = app.tabs.active().painted.len().saturating_sub(3);
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
    fn paging_keys_move_a_screen_at_a_time_not_a_line() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.on_resize(Size { cols: 80, rows: 26 });
        app.tabs.active_mut().painted = DisplayList::from_lines(&vec![String::new(); 200]);
        let page = app.geometry.content_rows() - 1;
        app.handle_key(press(Key::PageDown));
        assert_eq!(app.tabs.active().scroll, page);
        app.handle_key(press(Key::Char(' ')));
        assert_eq!(app.tabs.active().scroll, page * 2);
        app.handle_key(press(Key::PageUp));
        assert_eq!(app.tabs.active().scroll, page);
        app.handle_key(press(Key::Char('b')));
        assert_eq!(app.tabs.active().scroll, 0);
    }

    #[test]
    fn resize_remaps_the_geometry_and_clamps_scroll() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.handle_key(press(Key::End));
        app.on_resize(Size { cols: 40, rows: 15 });
        let max = app.tabs.active().painted.len().saturating_sub(7);
        assert!(app.tabs.active().scroll <= max);
        assert_eq!(app.tabs.active().layout_width, 38);
    }

    #[test]
    fn resize_refits_the_start_page_to_the_content_viewport() {
        let mut app = App::new();
        app.on_resize(Size {
            cols: 158,
            rows: 42,
        });

        assert_eq!(app.tabs.active().painted.rows.len(), 36);
        assert!(
            app.tabs
                .active()
                .painted
                .text_lines()
                .iter()
                .all(|row| unicode_width::UnicodeWidthStr::width(row.as_str()) == 156)
        );
    }

    #[test]
    fn long_documents_scroll_past_u16_max_without_wrapping() {
        let mut app = App::new();
        app.tabs.active_mut().painted = DisplayList::from_lines(&vec![String::new(); 70_000]);
        app.handle_key(press(Key::Esc));
        app.handle_key(press(Key::End));
        assert_eq!(app.tabs.active().scroll, 69_982);
    }

    #[test]
    fn stale_fetch_results_are_dropped_fresh_ones_accepted() {
        let mut app = App::new();
        app.submit_url("https://example.com");
        let generation = app.tabs.active().generation;
        let tab_id = app.tabs.active().id;
        let message = app.message().to_string();
        assert!(!app.deliver_fetch(FetchPayload {
            tab_id,
            generation: generation - 1,
            resource_id: ResourceId::DOCUMENT,
            result: Ok(FetchResponse {
                final_url: url::Url::parse("https://example.com/").unwrap(),
                body: vec![],
                content_type: None,
            }),
        }));
        assert_eq!(app.message(), message);
        assert!(app.deliver_fetch(FetchPayload {
            tab_id,
            generation,
            resource_id: ResourceId::DOCUMENT,
            result: Ok(FetchResponse {
                final_url: url::Url::parse("https://example.com/").unwrap(),
                body: vec![],
                content_type: None,
            }),
        }));
        assert!(app.message().contains("accepted"));
    }

    #[test]
    fn a_background_tabs_current_fetch_is_delivered_to_that_tab() {
        let mut app = App::new();
        app.submit_url("https://a.example");
        let background_tab_id = app.tabs.active().id;
        let background_generation = app.tabs.active().generation;
        app.new_tab();
        assert!(app.deliver_fetch(FetchPayload {
            tab_id: background_tab_id,
            generation: background_generation,
            resource_id: ResourceId::DOCUMENT,
            result: Ok(FetchResponse {
                final_url: url::Url::parse("https://a.example/final").unwrap(),
                body: b"<p>background complete</p>".to_vec(),
                content_type: Some("text/html; charset=\"utf-8\"".to_string()),
            }),
        }));
        assert_eq!(app.tabs.active_index(), 1);
        assert_eq!(app.tabs.tabs()[0].url, "https://a.example/final");
        assert!(
            !app.tabs.tabs()[0]
                .painted
                .text_lines()
                .iter()
                .any(|line| line.contains("background complete"))
        );
        assert_eq!(app.message(), STARTUP_HINT);
        app.apply(Action::PrevTab);
        assert!(
            app.tabs
                .active()
                .painted
                .text_lines()
                .iter()
                .any(|line| line.contains("background complete"))
        );
        assert!(app.message().contains("accepted gen"));
    }

    #[test]
    fn fake_fetch_loads_the_rendered_document_into_the_tab() {
        let fake: Arc<dyn Navigate> = Arc::new(FakeNet::default());
        let mut app = App::with_net(fake);
        app.submit_url("example.com");
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.active_url(), "https://example.com");
        assert!(
            app.tabs
                .active()
                .painted
                .text_lines()
                .iter()
                .any(|l| l.starts_with("  fetching"))
        );
        app.step(Duration::ZERO);
        let tab = app.tabs.active();
        assert_eq!(
            tab.painted.text_lines().first().map(String::as_str),
            Some("hi there")
        );
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
            fn submit(&self, tab_id: u64, generation: u64, resource_id: ResourceId, _url: Url) {
                self.pending.lock().unwrap().push(FetchPayload {
                    tab_id,
                    generation,
                    resource_id,
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
        app.step(Duration::ZERO);
        assert!(
            app.tabs
                .active()
                .painted
                .text_lines()
                .iter()
                .any(|l| l.contains("failed to load"))
        );
        assert!(app.message().contains("http status 404"));
    }

    #[test]
    fn plain_text_is_rendered_without_html_parsing() {
        let mut app = App::new();
        app.submit_url("https://example.com/plain");
        let generation = app.tabs.active().generation;
        let tab_id = app.tabs.active().id;
        assert!(app.deliver_fetch(FetchPayload {
            tab_id,
            generation,
            resource_id: ResourceId::DOCUMENT,
            result: Ok(FetchResponse {
                final_url: Url::parse("https://example.com/plain").unwrap(),
                body: b"one\ntwo".to_vec(),
                content_type: Some("text/plain; charset=\"utf-8\"".to_string()),
            }),
        }));
        assert_eq!(app.tabs.active().painted.text_lines(), vec!["one", "two"]);
        assert!(app.message().contains("plain text"));
    }

    #[test]
    fn unsupported_valid_media_types_are_not_parsed_as_html() {
        let mut app = App::new();
        app.submit_url("https://example.com/image");
        let generation = app.tabs.active().generation;
        let tab_id = app.tabs.active().id;
        assert!(app.deliver_fetch(FetchPayload {
            tab_id,
            generation,
            resource_id: ResourceId::DOCUMENT,
            result: Ok(FetchResponse {
                final_url: Url::parse("https://example.com/image").unwrap(),
                body: b"not really a png".to_vec(),
                content_type: Some("image/png".to_string()),
            }),
        }));
        assert!(app.tabs.active().painted.text_lines()[0].starts_with("cannot display"));
        assert_eq!(app.message(), "unsupported content type: image/png");
    }

    #[test]
    fn embedded_author_styles_participate_in_rendering() {
        let mut app = App::new();
        app.submit_url("https://example.com/styled");
        let generation = app.tabs.active().generation;
        let tab_id = app.tabs.active().id;
        assert!(app.deliver_fetch(FetchPayload {
            tab_id,
            generation,
            resource_id: ResourceId::DOCUMENT,
            result: Ok(FetchResponse {
                final_url: Url::parse("https://example.com/styled").unwrap(),
                body: b"<style>p.secret { display: none }</style><p class=secret>hidden</p><div>shown</div>".to_vec(),
                content_type: Some("text/html".to_string()),
            }),
        }));
        let lines = app.tabs.active().painted.text_lines();
        assert!(lines.iter().any(|line| line == "shown"));
        assert!(!lines.iter().any(|line| line.contains("hidden")));
    }

    #[test]
    fn valid_imports_are_scheduled_and_late_imports_warn() {
        let fake = Arc::new(FakeNet::default());
        let mut app = App::with_net(fake.clone());
        app.submit_url("https://example.com");
        let pending = fake.pending.lock().unwrap().pop().unwrap();
        assert!(
            app.deliver_fetch(FetchPayload {
                tab_id: pending.tab_id,
                generation: pending.generation,
                resource_id: ResourceId::DOCUMENT,
                result: Ok(FetchResponse {
                    final_url: Url::parse("https://example.com/").unwrap(),
                    body: br#"<!doctype html><html><head>
                    <style>@import url(one.css); p { display: block }</style>
                    <style>@media (width: 1px) { p { display: none } } @import url(two.css);</style>
                    </head><body><p>shown</p></body></html>"#
                        .to_vec(),
                    content_type: Some("text/html; charset=utf-8".to_string()),
                }),
            })
        );
        assert_eq!(fake.submitted.lock().unwrap().len(), 2);
        app.step(Duration::ZERO);
        assert_eq!(
            app.message(),
            "accepted gen 1 - https://example.com/ (0 parse errors, 1 CSS warnings, 1 stylesheets, 0 failed)"
        );
        assert!(
            app.tabs
                .active()
                .painted
                .text_lines()
                .iter()
                .any(|line| line == "shown")
        );
    }

    #[test]
    fn zero_css_warnings_preserve_the_existing_acceptance_message() {
        let mut app = App::new();
        app.submit_url("https://example.com");
        let generation = app.tabs.active().generation;
        let tab_id = app.tabs.active().id;
        assert!(app.deliver_fetch(FetchPayload {
            tab_id,
            generation,
            resource_id: ResourceId::DOCUMENT,
            result: Ok(FetchResponse {
                final_url: Url::parse("https://example.com/").unwrap(),
                body: b"<!doctype html><html><body><p>shown</p></body></html>".to_vec(),
                content_type: Some("text/html; charset=utf-8".to_string()),
            }),
        }));
        assert_eq!(
            app.message(),
            "accepted gen 1 - https://example.com/ (0 parse errors)"
        );
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
        assert_eq!(app.tab_count(), 1);
        assert_eq!(app.active_url(), "about:blank");
        assert_eq!(app.tabs.active().painted, start_page());
    }

    #[test]
    fn unparseable_inputs_show_a_status_error() {
        let mut app = App::new();
        app.open_url("http://");
        assert_eq!(app.tab_count(), 1);
        assert!(app.message().contains("cannot parse url"));
    }

    #[test]
    fn tab_cycling_keeps_per_tab_state() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.submit_url("https://a.example");
        app.new_tab();
        app.submit_url("https://b.example");
        assert_eq!(app.tab_count(), 2);
        app.handle_key(ctrl(press(Key::Char('p'))));
        assert_eq!(app.active_url(), "https://a.example");
        app.handle_key(ctrl(press(Key::Char('n'))));
        assert_eq!(app.active_url(), "https://b.example");
        app.handle_key(ctrl(press(Key::Char('w'))));
        assert_eq!(app.tab_count(), 1);
        app.handle_key(ctrl(press(Key::Char('w'))));
        assert_eq!(app.tab_count(), 1);
        assert!(app.active_url().is_empty());
    }

    fn ctrl(mut event: KeyEvent) -> KeyEvent {
        event.modifiers.ctrl = true;
        event
    }

    fn alt(mut event: KeyEvent) -> KeyEvent {
        event.modifiers.alt = true;
        event
    }

    #[test]
    fn back_and_forward_navigate_in_place_and_refetch() {
        let fake: Arc<dyn Navigate> = Arc::new(FakeNet::default());
        let mut app = App::with_net(fake);
        app.handle_key(press(Key::Esc));
        app.submit_url("https://a.example");
        app.step(Duration::ZERO);
        app.handle_key(alt(press(Key::Home)));
        assert_eq!(app.active_url(), "about:blank");
        app.handle_key(alt(press(Key::Left)));
        app.step(Duration::ZERO);
        assert_eq!(app.tab_count(), 1, "back must not open a new tab");
        assert!(app.active_url().starts_with("https://a.example"));
        assert!(app.message().contains("accepted gen 3"));
        assert!(app.chrome_view().can_forward);
        app.handle_key(alt(press(Key::Right)));
        assert_eq!(app.active_url(), "about:blank");
        assert!(!app.chrome_view().can_forward);
    }

    #[test]
    fn reload_refetches_the_current_url_in_place() {
        let fake: Arc<dyn Navigate> = Arc::new(FakeNet::default());
        let mut app = App::with_net(fake);
        app.handle_key(press(Key::Esc));
        app.submit_url("https://a.example");
        app.step(Duration::ZERO);
        assert!(app.message().contains("accepted gen 1"));
        app.handle_key(press(Key::Char('r')));
        app.step(Duration::ZERO);
        assert_eq!(app.tab_count(), 1, "reload must not open a new tab");
        assert!(app.message().contains("accepted gen 2"));
        assert!(app.active_url().starts_with("https://a.example"));
    }

    #[test]
    fn home_navigates_the_current_tab_to_the_start_page() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.submit_url("https://a.example");
        assert_eq!(app.tab_count(), 1);
        app.handle_key(alt(press(Key::Home)));
        assert_eq!(app.tab_count(), 1, "home must not open a new tab");
        assert_eq!(app.active_url(), "about:blank");
        assert_eq!(app.tabs.active().painted, start_page());
        assert_eq!(app.tabs.active().scroll, 0);
    }

    #[test]
    fn nav_button_enabled_flags_track_the_history_cursor() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.submit_url("https://a.example");
        app.handle_key(alt(press(Key::Home)));
        assert!(app.chrome_view().can_back);
        assert!(!app.chrome_view().can_forward);
        app.handle_key(alt(press(Key::Left)));
        assert!(!app.chrome_view().can_back);
        assert!(app.chrome_view().can_forward);
        app.handle_key(alt(press(Key::Left)));
        assert_eq!(app.message(), "already at the first page");
        assert_eq!(app.active_url(), "https://a.example");
    }

    #[test]
    fn scroll_is_reset_by_an_in_place_navigation() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.on_resize(Size { cols: 80, rows: 14 });
        app.submit_url("https://a.example");
        let max = app.tabs.active().painted.len().saturating_sub(6);
        app.handle_key(press(Key::End));
        assert_eq!(app.tabs.active().scroll, max);
        app.handle_key(alt(press(Key::Home)));
        assert_eq!(app.tabs.active().scroll, 0);
        assert_eq!(app.tabs.active().layout_width, app.geometry.content_cols());
    }

    #[test]
    fn f10_opens_the_file_menu_and_arrows_walk_it() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.handle_key(press(Key::F(10)));
        let view = app.chrome_view();
        assert!(view.menu_open);
        assert_eq!(view.menu_active, 0);
        assert_eq!(app.focus(), Focus::Menu);
        app.handle_key(press(Key::Down));
        app.handle_key(press(Key::Down));
        assert_eq!(app.chrome_view().menu_item, 2);
        app.handle_key(press(Key::Left));
        assert_eq!(app.chrome_view().menu_active, 3);
        app.handle_key(press(Key::Right));
        assert_eq!(app.chrome_view().menu_active, 0);
    }

    #[test]
    fn selecting_file_quit_exits_the_app() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.handle_key(alt(press(Key::Char('f'))));
        for _ in 0..3 {
            app.handle_key(press(Key::Down));
        }
        app.handle_key(press(Key::Enter));
        assert!(app.should_quit());
    }

    #[test]
    fn selecting_file_new_tab_opens_one_and_closes_the_menu() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.handle_key(press(Key::F(10)));
        app.handle_key(press(Key::Enter));
        assert_eq!(app.tab_count(), 2);
        assert_eq!(app.focus(), Focus::Address);
        assert!(!app.chrome_view().menu_open);
    }

    #[test]
    fn alt_letter_opens_the_matching_menu() {
        let mut app = App::new();
        app.handle_key(press(Key::Esc));
        app.handle_key(alt(press(Key::Char('v'))));
        assert_eq!(app.chrome_view().menu_active, 2);
        app.handle_key(press(Key::Enter));
        assert_eq!(app.message(), "theme: Norton");
    }

    #[test]
    fn menu_keystrokes_never_reach_the_address_buffer() {
        let mut app = App::new();
        app.handle_key(press(Key::F(10)));
        app.handle_key(press(Key::Char('x')));
        assert!(!app.should_quit());
        assert_eq!(app.focus(), Focus::Menu);
        app.handle_key(press(Key::Esc));
        assert_eq!(app.focus(), Focus::Address);
        assert!(!app.chrome_view().menu_open);
    }

    #[test]
    fn esc_closes_the_menu_and_f10_reopens_it() {
        let mut app = App::new();
        app.handle_key(press(Key::F(10)));
        app.handle_key(press(Key::F(10)));
        assert!(!app.chrome_view().menu_open);
        assert_eq!(app.focus(), Focus::Address);
        app.handle_key(press(Key::F(10)));
        assert!(app.chrome_view().menu_open);
    }

    #[test]
    fn closing_a_menu_restores_the_previous_focus() {
        let mut app = App::new();
        app.handle_key(press(Key::F(10)));
        app.handle_key(press(Key::Esc));
        assert_eq!(app.focus(), Focus::Address);
    }

    #[test]
    fn closing_a_tab_from_the_menu_syncs_the_focused_address() {
        let mut app = App::new();
        app.tabs.active_mut().url = "https://first.example/".to_string();
        app.new_tab();
        app.tabs.active_mut().url = "https://second.example/".to_string();
        app.address.set_text("unfinished draft");
        app.close_tab();
        assert_eq!(app.focus(), Focus::Address);
        assert_eq!(app.chrome_view().address, "https://first.example/");
    }
}

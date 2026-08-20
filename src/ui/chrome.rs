use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position};
use unicode_width::UnicodeWidthChar;

use crate::ui::mouse::ChromeGeometry;
use crate::ui::widgets::content::{Content, ContentLines};
use crate::ui::widgets::status::{StatusBar, StatusView};
use crate::ui::widgets::tabs::{TabBar, TabChip};
use crate::ui::widgets::toolbar::{NAV_BUTTONS_WIDTH, Toolbar};

pub struct ChromeView {
    pub geometry: ChromeGeometry,
    pub tabs: Vec<TabChip>,
    pub active_tab: usize,
    pub address: String,
    pub address_cursor: usize,
    pub address_focused: bool,
    pub content: ContentLines,
    pub status: StatusView,
}

pub fn draw(frame: &mut Frame<'_>, view: &ChromeView) {
    let rows = Layout::vertical([
        Constraint::Length(view.geometry.tabs_rows),
        Constraint::Length(view.geometry.toolbar_rows),
        Constraint::Min(0),
        Constraint::Length(view.geometry.status_rows),
    ])
    .split(frame.area());

    frame.render_widget(
        TabBar {
            tabs: &view.tabs,
            active: view.active_tab,
        },
        rows[0],
    );
    frame.render_widget(
        Toolbar {
            address: &view.address,
            focused: view.address_focused,
        },
        rows[1],
    );
    frame.render_widget(
        Content {
            lines: &view.content,
        },
        rows[2],
    );
    frame.render_widget(StatusBar { view: &view.status }, rows[3]);

    if view.address_focused {
        frame.set_cursor_position(Position::new(address_cursor_cell(view, rows[1]), rows[1].y));
    }
}

fn address_cursor_cell(view: &ChromeView, toolbar: ratatui::layout::Rect) -> u16 {
    let budget = usize::from(toolbar.width.saturating_sub(NAV_BUTTONS_WIDTH));
    let mut width = 0usize;
    for ch in view.address.chars().take(view.address_cursor) {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > budget {
            break;
        }
        width += ch_width;
    }
    NAV_BUTTONS_WIDTH + width as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geom::Size;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn draft() -> ChromeView {
        ChromeView {
            geometry: ChromeGeometry::for_size(Size { cols: 60, rows: 10 }),
            tabs: vec![TabChip {
                title: "example.com".to_string(),
                url: String::new(),
            }],
            active_tab: 0,
            address: "https://example.com".to_string(),
            address_cursor: 0,
            address_focused: false,
            content: ContentLines {
                lines: vec!["hello".to_string()],
                scroll: 0,
            },
            status: StatusView {
                url: "https://example.com".to_string(),
                message: "Ready".to_string(),
            },
        }
    }

    #[test]
    fn full_chrome_snapshot() {
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let view = draft();
        terminal.draw(|frame| draw(frame, &view)).unwrap();
        insta::assert_snapshot!(crate::ui::test_util::buffer_string(
            terminal.backend().buffer()
        ));
    }

    #[test]
    fn focused_address_places_the_cursor() {
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut view = draft();
        view.address_focused = true;
        view.address_cursor = view.address.chars().count();
        terminal.draw(|frame| draw(frame, &view)).unwrap();
        assert_eq!(terminal.backend().cursor_position(), Position::new(28, 1));
    }
}

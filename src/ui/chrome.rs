use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::ui::mouse::ChromeGeometry;
use crate::ui::theme::Theme;
use crate::ui::widgets::content::{Content, ContentLines};
use crate::ui::widgets::menu::{MenuBar, MenuPopup};
use crate::ui::widgets::status::{StatusBar, StatusView};
use crate::ui::widgets::tabs::{TabBar, TabChip, active_span};
use crate::ui::widgets::toolbar::{FIELD_TEXT, Toolbar};

pub struct ChromeView<'a> {
    pub geometry: ChromeGeometry,
    pub theme: Theme,
    pub can_back: bool,
    pub can_forward: bool,
    pub address: Cow<'a, str>,
    pub address_cursor: usize,
    pub address_focused: bool,
    pub menu_open: bool,
    pub menu_active: usize,
    pub menu_item: usize,
    pub tabs: Vec<TabChip<'a>>,
    pub active_tab: usize,
    pub content: ContentLines<'a>,
    pub status: StatusView<'a>,
}

pub fn draw(frame: &mut Frame<'_>, view: &ChromeView<'_>) {
    let area = frame.area();
    let layout = view.geometry.layout(area);
    let frame_style = Style::default().fg(view.theme.frame);
    frame
        .buffer_mut()
        .set_style(area, Style::default().bg(view.theme.bg));

    let divider = |frame: &mut Frame<'_>, rect: Rect| {
        let width = rect.width;
        buf_set_string(frame, rect, 0, "├", frame_style);
        buf_set_string(frame, rect, width - 1, "┤", frame_style);
        buf_set_string(
            frame,
            rect,
            1,
            &"─".repeat(usize::from(width.saturating_sub(2))),
            frame_style,
        );
    };

    let divider_with_opening = |frame: &mut Frame<'_>, rect: Rect, opening: Option<(u16, u16)>| {
        let width = rect.width;
        let left_rail = if opening.is_some_and(|(left, _)| left == 1) {
            "│"
        } else {
            "├"
        };
        buf_set_string(frame, rect, 0, left_rail, frame_style);
        buf_set_string(frame, rect, width - 1, "┤", frame_style);
        for col in 1..width.saturating_sub(1) {
            let glyph = match opening {
                Some((left, _)) if col == left => "┘",
                Some((_, right)) if col == right => "└",
                Some((left, right)) if col > left && col < right => continue,
                _ => "─",
            };
            buf_set_string(frame, rect, col, glyph, frame_style);
        }
    };

    if let Some(rect) = layout.menu {
        frame.render_widget(
            MenuBar {
                active: view.menu_active,
                open: view.menu_open,
                theme: &view.theme,
            },
            rect,
        );
    }
    if let Some(rect) = layout.tabs {
        frame.render_widget(
            TabBar {
                tabs: &view.tabs,
                active: view.active_tab,
                theme: &view.theme,
            },
            rect,
        );
    }
    let opening = if let (Some(tabs), Some(divider)) = (layout.tabs, layout.tab_divider)
        && tabs.width >= 2
        && divider.width >= 2
    {
        active_span(&view.tabs, view.active_tab, tabs.width - 2)
            .map(|(left, right)| (left + 1, right + 1))
    } else {
        None
    };
    if let Some(rect) = layout.tab_divider {
        divider_with_opening(frame, rect, opening);
    }
    if let Some(rect) = layout.toolbar {
        frame.render_widget(
            Toolbar {
                address: &view.address,
                focused: view.address_focused,
                back_enabled: view.can_back,
                forward_enabled: view.can_forward,
                theme: &view.theme,
            },
            rect,
        );
    }
    if let Some(rect) = layout.toolbar_divider {
        divider(frame, rect);
    }
    if let Some(rect) = layout.content.filter(|rect| rect.width >= 2) {
        frame.render_widget(
            Content {
                lines: &view.content,
                theme: &view.theme,
            },
            rect,
        );
    }
    if let Some(rect) = layout.status {
        frame.render_widget(
            StatusBar {
                view: &view.status,
                theme: &view.theme,
            },
            rect,
        );
    }

    if view.menu_open {
        frame.render_widget(
            MenuPopup {
                menu: view.menu_active,
                selected: view.menu_item,
                theme: &view.theme,
            },
            area,
        );
    }

    if view.address_focused
        && let Some(toolbar) = layout.toolbar
    {
        frame.set_cursor_position(Position::new(address_cursor_cell(view, toolbar), toolbar.y));
    }
}

fn buf_set_string(frame: &mut Frame<'_>, rect: Rect, col: u16, text: &str, style: Style) {
    frame
        .buffer_mut()
        .set_string(rect.x + col, rect.y, text, style);
}

fn address_cursor_cell(view: &ChromeView<'_>, toolbar: Rect) -> u16 {
    let budget = usize::from(toolbar.width.saturating_sub(FIELD_TEXT + 1));
    let mut width = 0usize;
    for grapheme in view.address.graphemes(true).take(view.address_cursor) {
        let ch_width = UnicodeWidthStr::width(grapheme);
        if width + ch_width > budget {
            break;
        }
        width += ch_width;
    }
    toolbar.x + FIELD_TEXT + width as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geom::Size;
    use crate::ui::mouse::ChromeGeometry;
    use crate::ui::theme::NORTON;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    static DRAFT_CONTENT: std::sync::LazyLock<crate::paint::DisplayList> =
        std::sync::LazyLock::new(|| crate::paint::DisplayList::from_lines(&["hello".to_string()]));

    fn draft() -> ChromeView<'static> {
        ChromeView {
            geometry: ChromeGeometry::for_size(Size { cols: 60, rows: 10 }),
            theme: NORTON,
            can_back: false,
            can_forward: false,
            address: "https://example.com".to_string().into(),
            address_cursor: 0,
            address_focused: false,
            menu_open: false,
            menu_active: 0,
            menu_item: 0,
            tabs: Vec::new(),
            active_tab: 0,
            content: ContentLines {
                painted: &DRAFT_CONTENT,
                scroll: 0,
            },
            status: StatusView {
                url: "https://example.com".to_string().into(),
                message: "Ready".to_string().into(),
            },
        }
    }

    fn render(view: &ChromeView<'_>, size: Size) -> String {
        let backend = TestBackend::new(size.cols, size.rows);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, view)).unwrap();
        crate::ui::test_util::buffer_string(terminal.backend().buffer())
    }

    fn snapshot(size: Size) -> String {
        render(&draft(), size)
    }

    fn chip(title: &'static str) -> TabChip<'static> {
        TabChip {
            title: Cow::Borrowed(title),
            url: Cow::Borrowed("https://example.com"),
        }
    }

    #[test]
    fn full_chrome_snapshot() {
        insta::assert_snapshot!(snapshot(Size { cols: 60, rows: 10 }));
    }

    #[test]
    fn standard_sized_chrome_snapshot() {
        insta::assert_snapshot!(snapshot(Size { cols: 80, rows: 24 }));
    }

    #[test]
    fn tiny_window_still_draws_chrome() {
        insta::assert_snapshot!(snapshot(Size { cols: 40, rows: 4 }));
    }

    #[test]
    fn open_menu_overlays_a_dropdown() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut view = draft();
        view.menu_open = true;
        view.menu_active = 1;
        view.menu_item = 1;
        terminal.draw(|frame| draw(frame, &view)).unwrap();
        insta::assert_snapshot!(crate::ui::test_util::buffer_string(
            terminal.backend().buffer()
        ));
    }

    #[test]
    fn first_active_tab_joins_the_divider_without_a_left_junction() {
        let mut view = draft();
        view.tabs = vec![chip("first")];
        let rendered = render(&view, Size { cols: 40, rows: 8 });
        let rows: Vec<&str> = rendered.lines().collect();
        assert!(rows[1].starts_with("│┌ first ┐ ┌ + ┐"));
        assert!(rows[2].starts_with("│┘       └"));
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn the_frame_interior_has_the_theme_background() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let view = draft();
        terminal.draw(|frame| draw(frame, &view)).unwrap();
        let cells = terminal.backend().buffer().content();
        let blank = &cells[80 + 40];
        assert_eq!(blank.symbol(), " ");
        assert_eq!(
            blank.style().bg,
            Some(NORTON.bg),
            "blank chrome cells must carry the theme background"
        );
        let blank_content = &cells[8 * 80 + 20];
        assert_eq!(
            blank_content.style().bg,
            Some(NORTON.bg),
            "blank content cells must carry the theme background"
        );
        let bar = &cells[23 * 80 + 10];
        assert_eq!(
            bar.style().bg,
            Some(NORTON.bar_bg),
            "the bottom bar must carry the bar background"
        );
    }

    #[test]
    fn focused_address_places_the_cursor() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut view = draft();
        view.address_focused = true;
        view.address_cursor = view.address.chars().count();
        terminal.draw(|frame| draw(frame, &view)).unwrap();
        assert_eq!(terminal.backend().cursor_position(), Position::new(43, 3));
    }

    #[test]
    fn cursor_stays_inside_the_field_when_the_address_is_long() {
        let backend = TestBackend::new(40, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut view = draft();
        view.address = "x".repeat(200).into();
        view.address_focused = true;
        view.address_cursor = view.address.chars().count();
        terminal.draw(|frame| draw(frame, &view)).unwrap();
        let position = terminal.backend().cursor_position();
        assert!(position.x < 40, "cursor must stay inside the field");
    }
}

#[cfg(test)]
use std::borrow::Cow;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::ui::mouse::ChromeGeometry;
use crate::ui::theme::Theme;
use crate::ui::widgets::content::{Content, ContentLines};
use crate::ui::widgets::flash::{Flash, flash_rect};
use crate::ui::widgets::menu::{MenuBar, MenuPopup, popup_rect};
use crate::ui::widgets::scrollbar::{ScrollExtent, Scrollbar};
use crate::ui::widgets::status::{StatusBar, StatusView};
use crate::ui::widgets::tabs::{TabBar, TabChip, active_span};
use crate::ui::widgets::text_field::TextFieldView;
use crate::ui::widgets::text_field::{TextFieldMenu, menu_rect};
use crate::ui::widgets::toolbar::{FIELD_TEXT, Toolbar};

pub struct ChromeView<'a> {
    pub geometry: ChromeGeometry,
    pub theme: Theme,
    pub theme_index: usize,
    pub can_back: bool,
    pub can_forward: bool,
    pub address: TextFieldView<'a>,
    pub address_focused: bool,
    pub content_cursor: Option<(usize, usize)>,
    pub menu_open: bool,
    pub menu_active: usize,
    pub menu_item: usize,
    pub tabs: Vec<TabChip<'a>>,
    pub active_tab: usize,
    pub content: ContentLines<'a>,
    pub status: StatusView<'a>,
    /// A notice in front of the page, while one is live.
    pub flash: Option<&'a str>,
    pub text_field_menu: Option<TextFieldMenuView>,
}

#[derive(Clone, Copy)]
pub struct TextFieldMenuView {
    pub anchor: crate::core::geom::Point,
    pub can_copy: bool,
    pub can_cut: bool,
}

pub fn draw(frame: &mut Frame<'_>, view: &ChromeView<'_>) {
    let area = frame.area();
    if let Some(cursor) = compose(area, frame.buffer_mut(), view) {
        frame.set_cursor_position(cursor);
    }
}

pub fn compose(area: Rect, buffer: &mut Buffer, view: &ChromeView<'_>) -> Option<Position> {
    let layout = view.geometry.layout(area);
    let frame_style = Style::default().fg(view.theme.frame);
    buffer.set_style(area, Style::default().bg(view.theme.bg));

    let divider = |buffer: &mut Buffer, rect: Rect| {
        let width = rect.width;
        buf_set_string(buffer, rect, 0, "├", frame_style);
        buf_set_string(buffer, rect, width - 1, "┤", frame_style);
        buf_set_string(
            buffer,
            rect,
            1,
            &"─".repeat(usize::from(width.saturating_sub(2))),
            frame_style,
        );
    };

    let divider_with_opening = |buffer: &mut Buffer, rect: Rect, opening: Option<(u16, u16)>| {
        let width = rect.width;
        let left_rail = if opening.is_some_and(|(left, _)| left == 1) {
            "│"
        } else {
            "├"
        };
        buf_set_string(buffer, rect, 0, left_rail, frame_style);
        buf_set_string(buffer, rect, width - 1, "┤", frame_style);
        for col in 1..width.saturating_sub(1) {
            let glyph = match opening {
                Some((left, _)) if col == left => "┘",
                Some((_, right)) if col == right => "└",
                Some((left, right)) if col > left && col < right => continue,
                _ => "─",
            };
            buf_set_string(buffer, rect, col, glyph, frame_style);
        }
    };

    if let Some(rect) = layout.menu {
        MenuBar {
            active: view.menu_active,
            open: view.menu_open,
            theme: &view.theme,
        }
        .render(rect, buffer);
    }
    if let Some(rect) = layout.tabs {
        TabBar {
            tabs: &view.tabs,
            active: view.active_tab,
            theme: &view.theme,
        }
        .render(rect, buffer);
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
        divider_with_opening(buffer, rect, opening);
    }
    if let Some(rect) = layout.toolbar {
        Toolbar {
            address: view.address,
            focused: view.address_focused,
            back_enabled: view.can_back,
            forward_enabled: view.can_forward,
            theme: &view.theme,
        }
        .render(rect, buffer);
    }
    if let Some(rect) = layout.toolbar_divider {
        divider(buffer, rect);
    }
    if let Some(rect) = layout.content.filter(|rect| rect.width >= 2) {
        Content {
            lines: &view.content,
            theme: &view.theme,
        }
        .render(rect, buffer);
    }
    // After the content: the `Content` widget draws a rail in this column, so that any
    // partial row repaint still leaves a whole frame, and the bar takes it back here.
    draw_scrollbar(buffer, view, layout.scrollbar);
    if let Some(rect) = layout.status {
        StatusBar {
            view: &view.status,
            theme: &view.theme,
        }
        .render(rect, buffer);
    }

    compose_flash(buffer, view, area);

    if let Some(menu) = view.text_field_menu {
        TextFieldMenu {
            anchor: menu.anchor,
            can_copy: menu.can_copy,
            can_cut: menu.can_cut,
            theme: &view.theme,
        }
        .render(area, buffer);
    }

    if view.menu_open {
        MenuPopup {
            menu: view.menu_active,
            selected: view.menu_item,
            marked: (view.menu_active == crate::ui::widgets::menu::THEME_MENU)
                .then_some(view.theme_index),
            theme: &view.theme,
        }
        .render(area, buffer);
    }

    if view.address_focused
        && let Some(toolbar) = layout.toolbar
    {
        return Some(Position::new(address_cursor_cell(view, toolbar), toolbar.y));
    }
    page_cursor_position(view, area)
}

pub fn content_rect(view: &ChromeView<'_>, area: Rect) -> Option<Rect> {
    view.geometry.content_view_in(area).map(|content| Rect {
        x: content.origin.col,
        y: content.origin.row,
        width: content.cols,
        height: content.rows,
    })
}

pub fn compose_status(buffer: &mut Buffer, view: &ChromeView<'_>, area: Rect) -> Option<Rect> {
    let rect = view.geometry.layout(area).status?;
    clear_rect(buffer, rect, Style::default().bg(view.theme.bar_bg));
    StatusBar {
        view: &view.status,
        theme: &view.theme,
    }
    .render(rect, buffer);
    Some(rect)
}

/// Repaint the scrollbar column alone.
///
/// A retained scroll moves a full-width region, thumb included, so every frame that
/// touched the content owes the bar a redraw.
pub fn compose_scrollbar(buffer: &mut Buffer, view: &ChromeView<'_>, area: Rect) -> Option<Rect> {
    draw_scrollbar(buffer, view, view.geometry.layout(area).scrollbar)
}

fn draw_scrollbar(buffer: &mut Buffer, view: &ChromeView<'_>, rect: Option<Rect>) -> Option<Rect> {
    let rect = rect?;
    Scrollbar {
        extent: ScrollExtent {
            rows: rect.height,
            doc_rows: view.content.painted.len(),
            scroll: view.content.scroll,
        },
        theme: &view.theme,
    }
    .render(rect, buffer);
    Some(rect)
}

pub fn cursor_position(view: &ChromeView<'_>, area: Rect) -> Option<Position> {
    if view.address_focused
        && let Some(toolbar) = view.geometry.layout(area).toolbar
    {
        return Some(Position::new(address_cursor_cell(view, toolbar), toolbar.y));
    }
    page_cursor_position(view, area)
}

fn page_cursor_position(view: &ChromeView<'_>, area: Rect) -> Option<Position> {
    let (col, row) = view.content_cursor?;
    let content = view.geometry.layout(area).content?;
    let visible_row = row.checked_sub(view.content.scroll)?;
    if col >= usize::from(content.width.saturating_sub(2))
        || visible_row >= usize::from(content.height)
    {
        return None;
    }
    Some(Position::new(
        content
            .x
            .saturating_add(1)
            .saturating_add(u16::try_from(col).ok()?),
        content.y.saturating_add(u16::try_from(visible_row).ok()?),
    ))
}

pub fn compose_content_rows(
    buffer: &mut Buffer,
    view: &ChromeView<'_>,
    area: Rect,
    rows: std::ops::Range<u16>,
) -> Option<Rect> {
    let content = view.geometry.layout(area).content?;
    let start = rows.start.min(content.height);
    let end = rows.end.min(content.height);
    if start >= end {
        return None;
    }
    let rect = Rect::new(content.x, content.y + start, content.width, end - start);
    clear_rect(buffer, rect, Style::default().bg(view.theme.bg));
    let lines = ContentLines {
        painted: view.content.painted,
        scroll: view.content.scroll + usize::from(start),
        text_fields: view.content.text_fields.clone(),
    };
    Content {
        lines: &lines,
        theme: &view.theme,
    }
    .render(rect, buffer);
    Some(rect)
}

fn clear_rect(buffer: &mut Buffer, rect: Rect, style: Style) {
    for row in rect.y..rect.bottom() {
        for col in rect.x..rect.right() {
            buffer[(col, row)] = ratatui::buffer::Cell::default();
            buffer[(col, row)].set_style(style);
        }
    }
}

pub fn occlusion_rects(view: &ChromeView<'_>, area: Rect) -> Vec<Rect> {
    let mut rects = Vec::new();
    if view.menu_open {
        rects.push(popup_rect(area, area, view.menu_active));
    }
    if let Some(rect) = flash_position(view, area) {
        rects.push(rect);
    }
    if let Some(menu) = view.text_field_menu {
        rects.push(menu_rect(area, menu.anchor));
    }
    rects
}

/// Draw the flash notice over the page, if there is one.
///
/// Its own function because the rows it covers belong to the content widget: every path
/// that repaints those rows — a scroll, a row range, a whole page — paints over the box,
/// and owes it a redraw before the frame reaches the screen.
pub fn compose_flash(buffer: &mut Buffer, view: &ChromeView<'_>, area: Rect) -> Option<Rect> {
    let rect = flash_position(view, area)?;
    Flash {
        message: view.flash?,
        theme: &view.theme,
    }
    .render(rect, buffer);
    Some(rect)
}

fn flash_position(view: &ChromeView<'_>, area: Rect) -> Option<Rect> {
    flash_rect(content_rect(view, area)?, view.flash?)
}

fn buf_set_string(buffer: &mut Buffer, rect: Rect, col: u16, text: &str, style: Style) {
    buffer.set_string(rect.x + col, rect.y, text, style);
}

/// The grapheme index a click at `col` selects in an address field `toolbar_width` wide.
///
/// The inverse of [`address_cursor_cell`]: it walks the same widths and stops at the same
/// visible budget, so clicking a caret's own cell puts the caret back where it was.
pub fn address_index_at(address: TextFieldView<'_>, toolbar_width: u16, col: u16) -> usize {
    address.index_at(
        Rect::new(
            FIELD_TEXT,
            0,
            toolbar_width.saturating_sub(FIELD_TEXT + 1),
            1,
        ),
        col,
        0,
    )
}

fn address_cursor_cell(view: &ChromeView<'_>, toolbar: Rect) -> u16 {
    view.address
        .cursor_in(Rect::new(
            toolbar.x + FIELD_TEXT,
            toolbar.y,
            toolbar.width.saturating_sub(FIELD_TEXT + 1),
            1,
        ))
        .map_or(toolbar.x + FIELD_TEXT, |(col, _)| col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geom::Size;
    use crate::ui::test_util::draft_view as draft;
    use crate::ui::theme::DEFAULT;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

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
    fn a_flash_notice_sits_in_front_of_the_page() {
        let mut view = draft();
        view.flash = Some("saved screenshots/x.png");
        insta::assert_snapshot!(render(&view, Size { cols: 60, rows: 10 }));
    }

    #[test]
    fn a_flash_notice_is_an_occlusion_so_overlays_stay_off_it() {
        let mut view = draft();
        let area = Rect::new(0, 0, 60, 10);
        assert!(occlusion_rects(&view, area).is_empty());

        view.flash = Some("saved screenshots/x.png");
        let rects = occlusion_rects(&view, area);

        let content = content_rect(&view, area).expect("a content rect");
        assert_eq!(rects.len(), 1);
        assert!(content.contains(Position::new(rects[0].x, rects[0].y)));
    }

    #[test]
    fn tiny_window_still_draws_chrome() {
        insta::assert_snapshot!(snapshot(Size { cols: 40, rows: 4 }));
    }

    #[test]
    fn the_content_rect_starts_where_the_first_document_cell_is_painted() {
        let view = draft();
        let size = Size { cols: 60, rows: 10 };
        let backend = TestBackend::new(size.cols, size.rows);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &view)).unwrap();
        let area = Rect::new(0, 0, size.cols, size.rows);
        let rect = content_rect(&view, area).expect("a content rect");
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(rect.x, rect.y)].symbol(), "h");
        assert_eq!(buffer[(rect.x - 1, rect.y)].symbol(), "│");
        assert_eq!(
            buffer[(rect.right() - 1 + 1, rect.y)].symbol(),
            "▲",
            "the right rail is the scrollbar, and its first row is the up cap",
        );
    }

    #[test]
    fn the_content_rect_ends_on_the_last_row_the_widget_draws() {
        let view = draft();
        let size = Size { cols: 60, rows: 10 };
        let backend = TestBackend::new(size.cols, size.rows);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &view)).unwrap();
        let area = Rect::new(0, 0, size.cols, size.rows);
        let rect = content_rect(&view, area).expect("a content rect");
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(rect.x - 1, rect.bottom() - 1)].symbol(), "│");
        assert_ne!(
            buffer[(rect.x - 1, rect.bottom())].symbol(),
            "│",
            "the row below the content rect is the status bar, not content"
        );
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
        assert!(rows[1].starts_with("│┌ first [■]┐ ┌ + ┐"));
        assert!(rows[2].starts_with("│┘          └"));
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
            Some(DEFAULT.bg),
            "blank chrome cells must carry the theme background"
        );
        let blank_content = &cells[8 * 80 + 20];
        assert_eq!(
            blank_content.style().bg,
            Some(DEFAULT.bg),
            "blank content cells must carry the theme background"
        );
        let bar = &cells[23 * 80 + 10];
        assert_eq!(
            bar.style().bg,
            Some(DEFAULT.bar_bg),
            "the bottom bar must carry the bar background"
        );
    }

    #[test]
    fn focused_address_places_the_cursor() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut view = draft();
        let address =
            crate::ui::widgets::text_field::TextFieldState::with_text("https://example.com");
        view.address = TextFieldView::new(&address);
        view.address_focused = true;
        terminal.draw(|frame| draw(frame, &view)).unwrap();
        assert_eq!(terminal.backend().cursor_position(), Position::new(43, 3));
    }

    #[test]
    fn cursor_stays_inside_the_field_when_the_address_is_long() {
        let backend = TestBackend::new(40, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut view = draft();
        let long = "x".repeat(200);
        let address = crate::ui::widgets::text_field::TextFieldState::with_text(&long);
        view.address = TextFieldView::new(&address);
        view.address_focused = true;
        terminal.draw(|frame| draw(frame, &view)).unwrap();
        let position = terminal.backend().cursor_position();
        assert!(position.x < 40, "cursor must stay inside the field");
    }
}

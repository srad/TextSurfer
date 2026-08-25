use super::*;

use crate::core::event::{InputBatch, InputEvent, MouseButton, MouseEvent, MouseKind};
use crate::core::frame::{ChromeDamage, RowDamage};
use crate::core::geom::Point;
use crate::ui::mouse::WHEEL_ROWS;
use crate::ui::widgets::menu::popup_rect;
use crate::ui::widgets::toolbar::FIELD_TEXT;

/// The default geometry's content origin, where document cell `(0, scroll)` is painted.
const ORIGIN: Point = Point { col: 1, row: 5 };

/// The loaded fixture's URL, as the fetch's `final_url` normalises it.
const PAGE: &str = "https://a.example/";

fn at(col: u16, row: u16) -> Point {
    Point { col, row }
}

fn press(button: MouseButton, at: Point) -> MouseEvent {
    MouseEvent {
        kind: MouseKind::Press(button),
        at,
    }
}

fn release(button: MouseButton, at: Point) -> MouseEvent {
    MouseEvent {
        kind: MouseKind::Release(button),
        at,
    }
}

fn moved(at: Point) -> MouseEvent {
    MouseEvent {
        kind: MouseKind::Move,
        at,
    }
}

fn wheel(rows: i32, at: Point) -> MouseEvent {
    MouseEvent {
        kind: MouseKind::Wheel { rows },
        at,
    }
}

/// A document taller than the default 18-row viewport, so scrolling has somewhere to go.
fn tall_page() -> String {
    let mut html = String::from("<a href='/next'>go</a>");
    for line in 0..40 {
        html.push_str(&format!("<p>line {line}</p>"));
    }
    html
}

/// An app showing `html` at `https://a.example`, with the fetch already delivered.
fn loaded(html: &str) -> App {
    let net: Arc<dyn Navigate> = Arc::new(FakeNet::serving(html));
    let mut app = App::with_net(net);
    app.handle_key(super::press(Key::Esc));
    app.submit_url("https://a.example");
    app.step(Duration::ZERO);
    app.step(STYLESHEET_DEADLINE);
    app
}

/// The screen cell the first painted link occupies.
fn first_link_cell(app: &App) -> Point {
    let link = app
        .tabs
        .active()
        .painted
        .links
        .first()
        .expect("the fixture paints a link");
    let rect = link.rects.first().expect("a link has geometry");
    at(
        ORIGIN.col + rect.col as u16,
        ORIGIN.row + rect.row as u16 - app.tabs.active().scroll as u16,
    )
}

#[test]
fn a_press_in_the_content_moves_focus_there() {
    let mut app = App::new();
    assert_eq!(app.focus(), Focus::Address);
    app.handle_mouse(press(MouseButton::Left, at(ORIGIN.col + 4, ORIGIN.row + 2)));
    assert_eq!(app.focus(), Focus::Content);
}

#[test]
fn a_link_follows_on_release_over_the_pressed_node() {
    let mut app = loaded("<a href='/next'>go</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(press(MouseButton::Left, cell));
    assert_eq!(app.active_url(), PAGE);
    app.handle_mouse(release(MouseButton::Left, cell));
    assert_eq!(app.active_url(), "https://a.example/next");
}

#[test]
fn a_release_away_from_the_pressed_link_follows_nothing() {
    let mut app = loaded("<a href='/next'>go</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(press(MouseButton::Left, cell));
    app.handle_mouse(release(MouseButton::Left, at(cell.col, cell.row + 1)));
    assert_eq!(app.active_url(), PAGE);
}

#[test]
fn a_release_with_no_press_behind_it_follows_nothing() {
    let mut app = loaded("<a href='/next'>go</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(release(MouseButton::Left, cell));
    assert_eq!(app.active_url(), PAGE);
}

#[test]
fn a_base_href_resolves_the_link() {
    let mut app = loaded("<base href='https://cdn.example/x/'><a href='y'>go</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(press(MouseButton::Left, cell));
    app.handle_mouse(release(MouseButton::Left, cell));
    assert_eq!(app.active_url(), "https://cdn.example/x/y");
}

#[test]
fn a_same_document_fragment_defers_to_the_anchor_work() {
    let mut app = loaded("<a href='#here'>go</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(press(MouseButton::Left, cell));
    app.handle_mouse(release(MouseButton::Left, cell));
    assert_eq!(app.active_url(), PAGE);
    assert_eq!(app.message(), "anchor links arrive in M2");
}

#[test]
fn middle_click_opens_the_link_in_a_new_tab() {
    let mut app = loaded("<a href='/next'>go</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(press(MouseButton::Middle, cell));
    app.handle_mouse(release(MouseButton::Middle, cell));
    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.active_url(), "https://a.example/next");
    assert_eq!(app.focus(), Focus::Content);
}

#[test]
fn a_blank_target_opens_a_new_tab_on_a_plain_click() {
    let mut app = loaded("<a href='/next' target='_blank'>go</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(press(MouseButton::Left, cell));
    app.handle_mouse(release(MouseButton::Left, cell));
    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.active_url(), "https://a.example/next");
}

#[test]
fn the_wheel_scrolls_the_content_and_nothing_else() {
    let mut app = loaded(&tall_page());
    let content = at(ORIGIN.col, ORIGIN.row + 1);
    app.handle_mouse(wheel(WHEEL_ROWS, content));
    assert_eq!(app.tabs.active().scroll, 3);
    app.handle_mouse(wheel(-WHEEL_ROWS, content));
    assert_eq!(app.tabs.active().scroll, 0);
    app.handle_mouse(wheel(WHEEL_ROWS, at(10, 3)));
    assert_eq!(app.tabs.active().scroll, 0, "the toolbar does not scroll");
}

#[test]
fn a_wheel_batch_keeps_an_incremental_scroll_and_never_forces_a_full_repaint() {
    // The retained-scroll path depends on a wheel notch producing pure `scroll_rows`
    // damage. Dynamic state is deferred for the duration of the input batch, so nothing
    // upgrades the frame to a full content repaint that would zero the scroll delta.
    let mut app = loaded(&tall_page());
    let _ = app.take_damage(); // discard the load's full damage
    let content = at(ORIGIN.col, ORIGIN.row + 1);
    app.advance(
        &InputBatch::from(InputEvent::Mouse(wheel(WHEEL_ROWS, content))),
        Duration::ZERO,
    );
    let damage = app.take_damage();
    assert_eq!(app.tabs.active().scroll, WHEEL_ROWS as usize);
    assert_eq!(damage.content.scroll_rows, WHEEL_ROWS);
    assert!(
        !damage.content.full,
        "a wheel notch must stay an incremental scroll"
    );
    assert_eq!(damage.content.repaint, RowDamage::None);
    assert_eq!(damage.chrome, ChromeDamage::None);
}

#[test]
fn the_side_buttons_walk_history_from_any_zone() {
    let mut app = loaded("<p>hi</p>");
    app.submit_url("https://b.example");
    app.step(Duration::ZERO);
    app.handle_mouse(press(MouseButton::Back, at(10, 3)));
    app.step(Duration::ZERO);
    assert_eq!(app.active_url(), PAGE);
    app.handle_mouse(press(MouseButton::Forward, at(ORIGIN.col, ORIGIN.row)));
    app.step(Duration::ZERO);
    assert_eq!(app.active_url(), "https://b.example/");
}

#[test]
fn a_right_click_changes_nothing() {
    let mut app = loaded("<a href='/next'>go</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(press(MouseButton::Right, cell));
    app.handle_mouse(release(MouseButton::Right, cell));
    assert_eq!(app.active_url(), PAGE);
    assert_eq!(app.focus(), Focus::Content);
}

#[test]
fn tab_chips_activate_and_the_hint_box_opens_a_tab() {
    let mut app = App::new();
    app.handle_key(super::press(Key::Esc));
    app.submit_url("https://a.example");
    app.new_tab();
    app.submit_url("https://b.example");
    assert_eq!(app.tab_count(), 2);

    let chips = app.tab_chips();
    let boxes = crate::ui::widgets::tabs::layout_tabs(&chips, app.tabs.active_index(), 78);
    let first = boxes.first().expect("a chip for the first tab");
    app.handle_mouse(press(MouseButton::Left, at(1 + first.x, 1)));
    assert_eq!(app.active_url(), "https://a.example");
    assert_eq!(app.focus(), Focus::Content);

    let chips = app.tab_chips();
    let boxes = crate::ui::widgets::tabs::layout_tabs(&chips, app.tabs.active_index(), 78);
    let hint = boxes.last().expect("a new-tab hint");
    app.handle_mouse(press(MouseButton::Left, at(1 + hint.x, 1)));
    assert_eq!(app.tab_count(), 3);
}

#[test]
fn the_tab_divider_row_is_inert() {
    let mut app = App::new();
    app.handle_key(super::press(Key::Esc));
    app.submit_url("https://a.example");
    app.new_tab();
    app.submit_url("https://b.example");
    app.handle_mouse(press(MouseButton::Left, at(2, 2)));
    assert_eq!(app.active_url(), "https://b.example");
    assert_eq!(app.tab_count(), 2);
}

#[test]
fn the_toolbar_buttons_do_what_their_keys_do() {
    let mut app = loaded("<p>hi</p>");
    app.submit_url("https://b.example");
    app.step(Duration::ZERO);
    // `[‹]` occupies columns 1..=3 of the toolbar row.
    app.handle_mouse(press(MouseButton::Left, at(2, 3)));
    app.step(Duration::ZERO);
    assert_eq!(app.active_url(), PAGE);
    // `[⌂]` is the fourth button.
    app.handle_mouse(press(MouseButton::Left, at(14, 3)));
    assert_eq!(app.active_url(), "about:blank");
}

#[test]
fn a_dimmed_arrow_says_what_the_key_says() {
    let mut app = App::new();
    app.handle_mouse(press(MouseButton::Left, at(2, 3)));
    assert_eq!(app.message(), "already at the first page");
}

#[test]
fn clicking_the_address_field_focuses_it_and_places_the_caret() {
    let mut app = loaded("<p>hi</p>");
    app.handle_mouse(press(MouseButton::Left, at(ORIGIN.col, ORIGIN.row)));
    assert_eq!(app.focus(), Focus::Content);
    app.handle_mouse(press(MouseButton::Left, at(FIELD_TEXT + 5, 3)));
    assert_eq!(app.focus(), Focus::Address);
    assert_eq!(app.address.text(), PAGE);
    assert_eq!(app.address.cursor(), 5);
}

#[test]
fn clicking_the_field_while_typing_keeps_the_edit() {
    let mut app = App::new();
    for ch in "example.com".chars() {
        app.handle_key(super::press(Key::Char(ch)));
    }
    app.handle_mouse(press(MouseButton::Left, at(FIELD_TEXT + 3, 3)));
    assert_eq!(app.focus(), Focus::Address);
    assert_eq!(app.address.text(), "example.com");
    assert_eq!(app.address.cursor(), 3);
}

#[test]
fn menu_titles_open_and_toggle_and_items_dispatch() {
    let mut app = App::new();
    app.handle_mouse(press(MouseButton::Left, at(2, 0)));
    assert!(app.menu_open);
    assert_eq!(app.menu_active, 0);
    app.handle_mouse(press(MouseButton::Left, at(2, 0)));
    assert!(!app.menu_open, "the same title closes what it opened");

    app.handle_mouse(press(MouseButton::Left, at(2, 0)));
    let popup = popup_rect(
        ratatui::layout::Rect::new(0, 0, 80, 24),
        ratatui::layout::Rect::new(0, 0, 80, 24),
        0,
    );
    // File → New tab is the first row inside the popup border.
    app.handle_mouse(press(MouseButton::Left, at(popup.x + 2, popup.y + 1)));
    assert!(!app.menu_open);
    assert_eq!(app.tab_count(), 2);
}

#[test]
fn a_click_outside_an_open_menu_only_closes_it() {
    let mut app = loaded("<a href='/next'>go</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(press(MouseButton::Left, at(2, 0)));
    assert!(app.menu_open);
    app.handle_mouse(press(MouseButton::Left, cell));
    app.handle_mouse(release(MouseButton::Left, cell));
    assert!(!app.menu_open);
    assert_eq!(app.active_url(), PAGE);
}

#[test]
fn hover_previews_the_link_and_clears_when_it_leaves() {
    let mut app = loaded("<a href='/next'>go</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(moved(cell));
    assert!(app.hovers_link());
    assert_eq!(app.hovered_href(), Some("/next"));
    app.handle_mouse(moved(at(cell.col, cell.row + 2)));
    assert!(!app.hovers_link());
    app.handle_mouse(moved(cell));
    assert!(app.hovers_link());
    app.pointer_left();
    assert!(!app.hovers_link());
}

#[test]
fn hover_follows_the_content_when_it_scrolls_under_a_still_pointer() {
    let mut app = loaded(&tall_page());
    let cell = first_link_cell(&app);
    app.handle_mouse(moved(cell));
    assert!(app.hovers_link());
    app.handle_mouse(wheel(WHEEL_ROWS, cell));
    assert!(
        !app.hovers_link(),
        "the link scrolled out from under the pointer"
    );
}

#[test]
fn a_repeated_move_inside_one_link_does_not_repaint() {
    let mut app = loaded("<a href='/next'>going</a>");
    let cell = first_link_cell(&app);
    app.handle_mouse(moved(cell));
    let _ = app.take_dirty();
    app.handle_mouse(moved(at(cell.col + 1, cell.row)));
    assert!(!app.take_dirty(), "the target did not change");
}

#[test]
fn a_window_with_no_room_for_content_swallows_every_click() {
    let mut app = loaded("<a href='/next'>go</a>");
    app.on_resize(Size { cols: 40, rows: 4 });
    for row in 0..4 {
        app.handle_mouse(press(MouseButton::Left, at(2, row)));
        app.handle_mouse(release(MouseButton::Left, at(2, row)));
        app.handle_mouse(moved(at(2, row)));
        app.handle_mouse(wheel(WHEEL_ROWS, at(2, row)));
    }
    assert_eq!(app.active_url(), PAGE);
    assert!(!app.hovers_link());
}

#[test]
fn the_status_bar_row_is_inert() {
    let mut app = loaded("<p>hi</p>");
    app.handle_mouse(press(MouseButton::Left, at(ORIGIN.col, ORIGIN.row)));
    let before = app.focus();
    app.handle_mouse(press(MouseButton::Left, at(5, 23)));
    assert_eq!(app.focus(), before);
    assert_eq!(app.active_url(), PAGE);
}

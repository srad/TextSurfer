use super::*;

use crate::core::event::{MouseButton, MouseEvent, MouseKind};
use crate::core::geom::Point;
use crate::core::style::Rgba;
use crate::ui::theme::{DEFAULT_THEME_INDEX, PAPER_WHITE, THEMES};
use crate::ui::widgets::menu::{THEME_MENU, popup_rect, title_x};
use ratatui::layout::Rect;

fn loaded(html: &str) -> App {
    let net: Arc<dyn Navigate> = Arc::new(FakeNet::serving(html));
    let mut app = App::with_net(net);
    app.handle_key(press(Key::Esc));
    app.submit_url("https://a.example");
    app.step(Duration::ZERO);
    app.step(STYLESHEET_DEADLINE);
    app
}

#[test]
fn theme_action_changes_chrome_and_page_media_together() {
    let mut app = loaded(
        "<style>@media (prefers-color-scheme: dark) { #light { display: none } }
         @media (prefers-color-scheme: light) { #dark { display: none } }</style>
         <a href='/'>link</a><p id=dark>dark</p><p id=light>light</p>",
    );
    assert_eq!(app.theme_index(), DEFAULT_THEME_INDEX);
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line == "dark")
    );

    app.apply(Action::SetTheme(4));

    assert_eq!(app.theme(), &PAPER_WHITE);
    assert_eq!(app.message(), "theme: Paper White");
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line == "light")
    );
    assert!(
        !app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line == "dark")
    );
    let link = app
        .tabs
        .active()
        .painted
        .rows
        .iter()
        .flat_map(|row| &row.spans)
        .find(|span| span.text == "link")
        .unwrap();
    assert_eq!(
        link.style.fg,
        Some(Rgba::opaque(PAPER_WHITE.palette().link))
    );
}

#[test]
fn invalid_theme_is_inert_and_reselecting_reports_the_current_theme() {
    let mut app = App::new();
    let _ = app.take_damage();
    app.apply(Action::SetTheme(THEMES.len()));
    assert_eq!(app.theme_index(), DEFAULT_THEME_INDEX);
    assert!(app.take_damage().is_empty());

    app.apply(Action::SetTheme(DEFAULT_THEME_INDEX));
    assert_eq!(app.message(), "theme: Turbo Vision");
    assert!(!app.take_damage().is_empty());
}

#[test]
fn new_page_loads_use_the_selected_appearance() {
    let net: Arc<dyn Navigate> = Arc::new(FakeNet::serving(
        "<style>@media (prefers-color-scheme: dark) { p { display: none } }</style><p>light</p>",
    ));
    let mut app = App::with_net(net);
    app.apply(Action::SetTheme(4));
    app.submit_url("https://a.example");
    app.step(Duration::ZERO);
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line == "light")
    );
}

#[test]
fn keyboard_selection_and_reopening_view_follow_the_current_theme() {
    let mut app = App::new();
    for key in [
        Key::F(10),
        Key::Right,
        Key::Right,
        Key::Down,
        Key::Down,
        Key::Down,
        Key::Enter,
    ] {
        app.handle_key(press(key));
    }
    assert_eq!(app.theme_index(), 3);
    app.handle_key(alt(press(Key::Char('v'))));
    assert_eq!(app.menu_active, THEME_MENU);
    assert_eq!(app.menu_item, 3);
}

#[test]
fn mouse_selects_a_theme_from_the_view_popup() {
    let mut app = App::new();
    app.handle_mouse(MouseEvent {
        kind: MouseKind::Press(MouseButton::Left),
        at: Point {
            col: title_x(THEME_MENU),
            row: 0,
        },
    });
    let rect = popup_rect(Rect::new(0, 0, 80, 24), Rect::new(0, 0, 80, 24), THEME_MENU);
    app.handle_mouse(MouseEvent {
        kind: MouseKind::Press(MouseButton::Left),
        at: Point {
            col: rect.x + 1,
            row: rect.y + 3,
        },
    });
    assert_eq!(app.theme_index(), 2);
}

#[test]
fn background_pages_defer_repaint_until_activation() {
    let mut app = loaded(
        "<style>@media (prefers-color-scheme: dark) { #light { display: none } }
         @media (prefers-color-scheme: light) { #dark { display: none } }</style>
         <p id=dark>dark</p><p id=light>light</p>",
    );
    app.new_tab();
    app.apply(Action::SetTheme(4));
    assert!(app.tabs.tabs()[0].render_dirty);
    assert!(
        app.tabs.tabs()[0]
            .painted
            .text_lines()
            .iter()
            .any(|line| line == "dark")
    );

    assert!(app.tabs.activate(0));
    app.activate_current();
    assert!(!app.tabs.active().render_dirty);
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .iter()
            .any(|line| line == "light")
    );
}

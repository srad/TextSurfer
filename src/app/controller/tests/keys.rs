use proptest::prelude::*;

use super::*;
use crate::core::event::InputBatch;

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
        prop_assert_eq!(app.tabs.active().scroll, settled);
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
    assert!(!app.should_quit());
    assert_eq!(app.focus(), Focus::Address);
    app.handle_key(press(Key::Char('/')));
    assert!(!app.should_quit());
    let mut app = App::new();
    app.handle_key(press(Key::Esc));
    app.handle_key(press(Key::Char('q')));
    assert!(app.should_quit());
}

#[test]
fn control_and_alt_chords_do_not_insert_characters_in_the_address() {
    let mut app = App::new();
    app.handle_key(ctrl(press(Key::Char('t'))));
    app.handle_key(alt(press(Key::Char('x'))));
    assert!(app.chrome_view().address.is_empty());
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
fn a_screenshot_key_leaves_one_request_for_the_frontend_to_take() {
    let mut app = App::new();
    app.handle_key(press(Key::Esc));
    assert!(!app.take_screenshot_request());

    app.handle_key(press(Key::F(12)));

    assert!(app.take_screenshot_request());
    assert!(!app.take_screenshot_request());
}

#[test]
fn a_flash_shows_in_front_of_the_page_and_stays_on_the_status_bar() {
    let mut app = App::new();
    app.flash("saved screenshots/x.png".to_string());

    assert_eq!(app.flash_message(), Some("saved screenshots/x.png"));
    assert_eq!(app.tabs.active().message, "saved screenshots/x.png");
    assert_eq!(app.chrome_view().flash, Some("saved screenshots/x.png"));
}

#[test]
fn a_flash_comes_down_on_its_own_deadline_and_wakes_the_loop_to_do_it() {
    let mut app = App::new();
    app.flash("saved screenshots/x.png".to_string());
    let deadline = crate::app::controller::flash::FLASH;

    assert_eq!(app.next_wake(), Some(deadline));

    app.advance(&InputBatch::new(), deadline - Duration::from_millis(1));
    assert!(app.flash_message().is_some());

    app.advance(&InputBatch::new(), deadline);
    assert_eq!(app.flash_message(), None);
    assert_eq!(app.tabs.active().message, "saved screenshots/x.png");
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
    assert_eq!(app.message(), "theme: Turbo Vision");
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
    app.close_tab_at(app.tabs.active_index());
    assert_eq!(app.focus(), Focus::Address);
    assert_eq!(app.chrome_view().address, "https://first.example/");
}

use super::*;

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
    assert_eq!(app.tab_count(), 1);
    assert!(app.active_url().starts_with("https://a.example"));
    assert_eq!(app.tabs.active().generation, 3);
    assert!(app.message().starts_with("loaded https://a.example"));
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
    assert_eq!(app.tabs.active().generation, 1);
    assert!(app.message().starts_with("loaded https://a.example"));
    app.handle_key(press(Key::Char('r')));
    app.step(Duration::ZERO);
    assert_eq!(app.tab_count(), 1);
    assert_eq!(app.tabs.active().generation, 2);
    assert!(app.message().starts_with("loaded https://a.example"));
    assert!(app.active_url().starts_with("https://a.example"));
}

#[test]
fn home_navigates_the_current_tab_to_the_start_page() {
    let mut app = App::new();
    app.handle_key(press(Key::Esc));
    app.submit_url("https://a.example");
    assert_eq!(app.tab_count(), 1);
    app.handle_key(alt(press(Key::Home)));
    assert_eq!(app.tab_count(), 1);
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

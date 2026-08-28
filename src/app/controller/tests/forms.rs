use super::*;
use ratatui::widgets::Widget;

fn loaded(html: &str) -> (App, Arc<FakeNet>) {
    let net = Arc::new(FakeNet::serving(html));
    let mut app = App::with_net(net.clone());
    app.submit_url("https://example.com/form");
    app.step(Duration::ZERO);
    app.step(STYLESHEET_DEADLINE);
    (app, net)
}

#[test]
fn keyboard_focus_edits_toggles_and_submits_controls_in_dom_order() {
    let (mut app, net) = loaded(
        "<form action='/search'><input name=q size=5><input type=checkbox name=flag><button name=go value=yes>Go</button></form>",
    );
    app.handle_key(press(Key::Tab));
    app.take_damage();
    app.handle_key(press(Key::Char('h')));
    app.handle_key(press(Key::Char('i')));
    let field = app.tabs.active().dom_focus.unwrap().node;
    assert_eq!(app.tabs.active().text_fields[&field].text(), "hi");
    let damage = app.take_damage();
    assert!(!damage.content.full, "typing must not rebuild the page");
    assert!(matches!(
        damage.content.repaint,
        crate::core::frame::RowDamage::Ranges(_)
    ));
    assert!(app.chrome_view().content_cursor.is_some());
    let (cursor_col, cursor_row) = app.chrome_view().content_cursor.unwrap();
    assert!(app.tabs.active().painted.hits.iter().any(|hit| {
        hit.node == field
            && hit.kind == crate::paint::HitKind::Text
            && cursor_col >= hit.rect.col
            && cursor_col < hit.rect.col.saturating_add(hit.rect.width)
            && cursor_row >= hit.rect.row
            && cursor_row < hit.rect.row.saturating_add(hit.rect.height)
    }));

    app.handle_key(press(Key::Tab));
    app.handle_key(press(Key::Char(' ')));
    assert!(
        app.tabs
            .active()
            .painted
            .text_lines()
            .join("\n")
            .contains("[X]")
    );

    app.handle_key(press(Key::Tab));
    app.handle_key(press(Key::Enter));
    let submitted = net.submitted.lock().unwrap();
    assert_eq!(submitted.len(), 2);
    assert_eq!(
        submitted[1].1.as_str(),
        "https://example.com/search?q=hi&flag=on&go=yes"
    );
}

#[test]
fn form_selection_cut_and_paste_stay_on_the_row_only_path() {
    let (mut app, _) = loaded("<input name=q value=hello size=8>");
    app.handle_key(press(Key::Tab));
    app.take_damage();
    app.handle_key(ctrl(press(Key::Char('a'))));
    app.handle_key(ctrl(press(Key::Char('x'))));
    assert_eq!(
        app.take_clipboard_request(),
        Some(crate::ui::widgets::text_field::ClipboardAction::Write(
            "hello".to_string()
        ))
    );
    app.deliver_clipboard_text("world");
    let node = app.tabs.active().dom_focus.unwrap().node;
    assert_eq!(app.tabs.active().text_fields[&node].text(), "world");
    let damage = app.take_damage();
    assert!(!damage.content.full);
    assert!(matches!(
        damage.content.repaint,
        crate::core::frame::RowDamage::Ranges(_)
    ));
}

#[test]
fn retained_fields_follow_readonly_and_maxlength_rules() {
    let (mut app, _) = loaded("<input value=locked readonly><input value=abc maxlength=3>");
    app.handle_key(press(Key::Tab));
    app.handle_key(press(Key::Char('x')));
    let readonly = app.tabs.active().dom_focus.unwrap().node;
    assert_eq!(app.tabs.active().text_fields[&readonly].text(), "locked");

    app.handle_key(press(Key::Tab));
    app.handle_key(press(Key::Char('d')));
    let bounded = app.tabs.active().dom_focus.unwrap().node;
    assert_eq!(app.tabs.active().text_fields[&bounded].text(), "abc");
}

#[test]
fn retained_empty_field_redraws_its_placeholder() {
    let (mut app, _) = loaded("<input placeholder='Search Wikipedia' size=20>");
    app.handle_key(press(Key::Tab));
    let chrome = app.chrome_view();
    let field = chrome.content.text_fields[0].view;
    let backend = ratatui::backend::TestBackend::new(20, 1);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            crate::ui::widgets::text_field::TextField::new(field)
                .render(frame.area(), frame.buffer_mut());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "S");
    assert_eq!(buffer[(7, 0)].symbol(), "W");
}

#[test]
fn page_field_cursor_uses_the_content_interior_cell() {
    let (mut app, _) = loaded("<input size=5>");
    let area = ratatui::layout::Rect::new(0, 0, 80, 25);
    app.on_resize(Size { cols: 80, rows: 25 });
    app.handle_key(press(Key::Tab));
    app.handle_key(press(Key::Char('h')));
    app.handle_key(press(Key::Char('i')));
    let chrome = app.chrome_view();
    let content = chrome.geometry.layout(area).content.unwrap();
    let (col, _) = chrome.content_cursor.unwrap();
    let cursor = crate::ui::chrome::cursor_position(&chrome, area).unwrap();
    assert_eq!(cursor.x, content.x + 1 + col as u16);

    let backend = ratatui::backend::TestBackend::new(area.width, area.height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| crate::ui::chrome::draw(frame, &chrome))
        .unwrap();
    assert_eq!(terminal.backend().buffer()[cursor].symbol(), " ");
}

#[test]
fn textarea_enter_edits_instead_of_submitting() {
    let (mut app, net) = loaded("<form action=/send><textarea name=body></textarea></form>");
    app.handle_key(press(Key::Tab));
    app.handle_key(press(Key::Char('a')));
    app.handle_key(press(Key::Enter));
    app.handle_key(press(Key::Char('b')));
    app.handle_key(press(Key::Home));
    app.handle_key(press(Key::Char('X')));
    assert_eq!(net.submitted.lock().unwrap().len(), 1);
    let load = app.tabs.active().load.as_ref().unwrap();
    let document = app.tabs.active().document.as_ref().unwrap().borrow();
    let textarea = app.tabs.active().dom_focus.unwrap().node;
    assert_eq!(
        crate::core::form::text_value(&document, textarea, load.form_state()),
        "a\nXb"
    );
}

#[test]
fn post_submission_keeps_the_query_and_cannot_be_reloaded() {
    let (mut app, net) = loaded(
        "<form method=post action='/send?token=x'><input name=q value=hello><button>Send</button></form>",
    );
    app.handle_key(press(Key::Tab));
    app.handle_key(press(Key::Tab));
    app.handle_key(press(Key::Enter));
    assert_eq!(
        net.request_kinds.lock().unwrap().last(),
        Some(&FetchRequestKind::UrlEncodedPost(b"q=hello".to_vec()))
    );
    assert_eq!(app.active_url(), "https://example.com/send?token=x");
    app.reload();
    assert_eq!(app.message(), "cannot reload a submitted POST");
    assert_eq!(net.submitted.lock().unwrap().len(), 2);
}

#[test]
fn implicit_submit_requires_at_most_one_blocking_field() {
    let (mut app, net) = loaded("<form action=/send><input name=first><input name=second></form>");
    app.handle_key(press(Key::Tab));
    app.handle_key(press(Key::Enter));
    assert_eq!(net.submitted.lock().unwrap().len(), 1);
}

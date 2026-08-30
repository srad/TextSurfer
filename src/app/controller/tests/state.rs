use super::*;

use crate::core::event::{InputBatch, InputEvent, MouseButton, MouseEvent, MouseKind, ResizePhase};
use crate::core::frame::RowDamage;
use crate::core::geom::Point;
use crate::core::style::{Cursor, Rgb};

const ORIGIN: Point = Point { col: 1, row: 7 };

struct DeferredCssNet {
    html: Vec<u8>,
    pending: Mutex<Vec<FetchPayload>>,
    submitted: Mutex<Vec<(u64, u64, ResourceId, Url)>>,
}

impl DeferredCssNet {
    fn new(html: &str) -> Self {
        Self {
            html: html.as_bytes().to_vec(),
            pending: Mutex::default(),
            submitted: Mutex::default(),
        }
    }
}

impl Navigate for DeferredCssNet {
    fn submit(&self, tab_id: u64, generation: u64, resource_id: ResourceId, url: Url) -> Submitted {
        self.submitted
            .lock()
            .unwrap()
            .push((tab_id, generation, resource_id, url.clone()));
        if resource_id == ResourceId::DOCUMENT {
            self.pending.lock().unwrap().push(FetchPayload {
                tab_id,
                generation,
                resource_id,
                result: Ok(FetchResponse {
                    final_url: url,
                    status: 200,
                    body: self.html.clone(),
                    content_type: Some("text/html; charset=utf-8".to_string()),
                }),
            });
        }
        Submitted::Queued
    }

    fn poll_result(&self) -> FetchPoll {
        self.pending
            .lock()
            .unwrap()
            .pop()
            .map_or(FetchPoll::Empty, FetchPoll::Ready)
    }
}

fn loaded(html: &str) -> App {
    let net: Arc<dyn Navigate> = Arc::new(FakeNet::serving(html));
    let mut app = App::with_net(net);
    app.handle_key(press(Key::Esc));
    app.submit_url("https://a.example");
    app.step(Duration::ZERO);
    app.step(STYLESHEET_DEADLINE);
    app
}

fn event(kind: MouseKind, at: Point) -> MouseEvent {
    MouseEvent { kind, at }
}

fn link_cell(app: &App) -> Point {
    let rect = app.tabs.active().painted.links[0].rects[0];
    Point {
        col: ORIGIN.col + rect.col as u16,
        row: ORIGIN.row + rect.row as u16,
    }
}

#[test]
fn hover_active_and_pointer_focus_restyle_through_the_page_load() {
    let mut app = loaded(
        "<!doctype html><style>
         a:hover { color: red }
         a:active { background: green }
         a:focus { font-weight: bold }
         a:focus-visible { text-decoration: line-through }
         </style><a id=target href=#here>target</a>",
    );
    let link = app
        .tabs
        .active()
        .document
        .as_ref()
        .unwrap()
        .borrow()
        .element_by_id("target")
        .unwrap();
    let cell = link_cell(&app);
    app.handle_mouse(event(MouseKind::Move, cell));
    assert_eq!(
        app.tabs.active().styles.as_ref().unwrap().get(link).color,
        Some(Rgb::new(255, 0, 0).into())
    );

    app.handle_mouse(event(MouseKind::Press(MouseButton::Left), cell));
    let style = app.tabs.active().styles.as_ref().unwrap().get(link);
    assert_eq!(style.background, Some(Rgb::new(0, 128, 0)));
    assert!(style.bold);
    assert!(!style.strike);

    app.handle_mouse(event(
        MouseKind::Release(MouseButton::Left),
        Point {
            col: cell.col,
            row: cell.row + 2,
        },
    ));
    assert_eq!(
        app.tabs
            .active()
            .styles
            .as_ref()
            .unwrap()
            .get(link)
            .background,
        None
    );
    app.handle_mouse(event(
        MouseKind::Move,
        Point {
            col: cell.col,
            row: cell.row + 2,
        },
    ));
    assert_ne!(
        app.tabs.active().styles.as_ref().unwrap().get(link).color,
        Some(Rgb::new(255, 0, 0).into())
    );
}

#[test]
fn a_mismatched_release_keeps_the_original_active_press() {
    let mut app =
        loaded("<!doctype html><style>a:active { background: red }</style><a href=#x>target</a>");
    let cell = link_cell(&app);
    app.handle_mouse(event(MouseKind::Press(MouseButton::Left), cell));
    assert!(app.pressed.is_some());
    app.handle_mouse(event(MouseKind::Release(MouseButton::Middle), cell));
    assert!(app.pressed.is_some());
    app.handle_mouse(event(MouseKind::Release(MouseButton::Left), cell));
    assert!(app.pressed.is_none());
}

#[test]
fn an_inherited_css_cursor_changes_without_dirtying_the_canvas() {
    let mut app = loaded(
        "<!doctype html><style>p { cursor: text }</style><p><span id=target>target</span></p>",
    );
    let target = app
        .tabs
        .active()
        .document
        .as_ref()
        .unwrap()
        .borrow()
        .element_by_id("target")
        .unwrap();
    let text = app
        .tabs
        .active()
        .document
        .as_ref()
        .unwrap()
        .borrow()
        .first_child(target)
        .unwrap();
    let rect = app
        .tabs
        .active()
        .painted
        .hits
        .iter()
        .find(|hit| hit.node == text)
        .unwrap()
        .rect;
    let _ = app.take_dirty();
    app.handle_mouse(event(
        MouseKind::Move,
        Point {
            col: ORIGIN.col + rect.col as u16,
            row: ORIGIN.row + rect.row as u16,
        },
    ));
    assert_eq!(app.pointer_cursor(), Cursor::Text);
    assert!(!app.take_dirty());
}

#[test]
fn a_dynamic_cursor_restyle_does_not_dirty_the_canvas() {
    let mut app = loaded(
        "<!doctype html><style>#target:hover { cursor: text }</style><p id=target>target</p>",
    );
    let target = app
        .tabs
        .active()
        .document
        .as_ref()
        .unwrap()
        .borrow()
        .element_by_id("target")
        .unwrap();
    let rect = app
        .tabs
        .active()
        .painted
        .hits
        .iter()
        .find(|hit| hit.node == target)
        .unwrap()
        .rect;
    let _ = app.take_damage();
    app.handle_mouse(event(
        MouseKind::Move,
        Point {
            col: ORIGIN.col + rect.col as u16,
            row: ORIGIN.row + rect.row as u16,
        },
    ));
    assert_eq!(app.pointer_cursor(), Cursor::Text);
    let damage = app.take_damage();
    assert!(!damage.content.full);
    assert_eq!(damage.content.repaint, crate::core::frame::RowDamage::None);
}

#[test]
fn a_paint_only_hover_damages_document_rows_without_full_content() {
    let mut app = loaded(
        "<!doctype html><style>a:hover { color: red; background: blue }</style><a href=#x>target</a>",
    );
    let cell = link_cell(&app);
    let row = app.tabs.active().painted.links[0].rects[0].row;
    let _ = app.take_damage();
    app.handle_mouse(event(MouseKind::Move, cell));
    let damage = app.take_damage();
    assert!(!damage.content.full);
    assert_eq!(
        damage.content.repaint,
        crate::core::frame::RowDamage::Ranges(std::iter::once(row..row + 1).collect())
    );
}

#[test]
fn chrome_focus_hides_and_restores_retained_dom_focus() {
    let mut app = loaded(
        "<!doctype html><style>a:focus { font-weight: bold }</style><a id=target href=#x>target</a>",
    );
    let link = app
        .tabs
        .active()
        .document
        .as_ref()
        .unwrap()
        .borrow()
        .element_by_id("target")
        .unwrap();
    let cell = link_cell(&app);
    app.handle_mouse(event(MouseKind::Press(MouseButton::Left), cell));
    app.handle_mouse(event(
        MouseKind::Release(MouseButton::Left),
        Point {
            col: cell.col,
            row: cell.row + 2,
        },
    ));
    assert!(app.tabs.active().styles.as_ref().unwrap().get(link).bold);
    app.handle_mouse(event(
        MouseKind::Press(MouseButton::Left),
        Point { col: 30, row: 4 },
    ));
    assert!(!app.tabs.active().styles.as_ref().unwrap().get(link).bold);
    app.handle_key(press(Key::Esc));
    assert!(app.tabs.active().styles.as_ref().unwrap().get(link).bold);
}

#[test]
fn unfocusable_content_and_navigation_clear_dom_focus() {
    let mut app = loaded(
        "<!doctype html><style>a:focus { font-weight: bold }</style><a href=#x>link</a><p>plain</p>",
    );
    let cell = link_cell(&app);
    app.handle_mouse(event(MouseKind::Press(MouseButton::Left), cell));
    assert!(app.tabs.active().dom_focus.is_some());
    app.handle_mouse(event(
        MouseKind::Press(MouseButton::Left),
        Point {
            col: ORIGIN.col,
            row: ORIGIN.row + 1,
        },
    ));
    assert!(app.tabs.active().dom_focus.is_none());
    app.handle_mouse(event(MouseKind::Press(MouseButton::Left), cell));
    assert!(app.tabs.active().dom_focus.is_some());
    app.submit_url("https://b.example");
    assert!(app.tabs.active().dom_focus.is_none());
}

#[test]
fn state_restyles_preserve_messages_and_focus_is_per_tab() {
    let mut app = loaded(
        "<!doctype html><style>a:hover { color: red } a:focus { font-weight: bold }</style><a id=target href=#x>target</a>",
    );
    let cell = link_cell(&app);
    let message = app.message().to_string();
    app.handle_mouse(event(MouseKind::Move, cell));
    assert_eq!(app.message(), message);
    app.handle_mouse(event(MouseKind::Press(MouseButton::Left), cell));
    let focused = app.tabs.active().dom_focus;
    app.new_tab();
    assert_eq!(app.tabs.tabs_mut()[0].dom_focus, focused);
    assert!(app.tabs.activate(0));
    app.activate_current();
    app.focus = Focus::Content;
    app.refresh_hover();
    assert_eq!(app.tabs.active().dom_focus, focused);
}

#[test]
fn default_link_hover_and_inline_span_hover_reach_the_live_styles() {
    let mut link_app = loaded("<!doctype html><a id=target href=/>target</a>");
    let link = link_app
        .tabs
        .active()
        .document
        .as_ref()
        .unwrap()
        .borrow()
        .element_by_id("target")
        .unwrap();
    assert_eq!(
        link_app
            .tabs
            .active()
            .styles
            .as_ref()
            .unwrap()
            .get(link)
            .color,
        Some(crate::ui::theme::DEFAULT.palette().link.into())
    );
    let cell = link_cell(&link_app);
    link_app.handle_mouse(event(MouseKind::Move, cell));
    assert_eq!(
        link_app
            .tabs
            .active()
            .styles
            .as_ref()
            .unwrap()
            .get(link)
            .color,
        Some(crate::ui::theme::DEFAULT.palette().link_hover.into())
    );

    let mut span_app = loaded(
        "<!doctype html><style>span:hover { color: red }</style><p><span id=target>target</span></p>",
    );
    let document = span_app.tabs.active().document.as_ref().unwrap().borrow();
    let span = document.element_by_id("target").unwrap();
    let text = document.first_child(span).unwrap();
    drop(document);
    let rect = span_app
        .tabs
        .active()
        .painted
        .hits
        .iter()
        .find(|hit| hit.node == text)
        .unwrap()
        .rect;
    span_app.handle_mouse(event(
        MouseKind::Move,
        Point {
            col: ORIGIN.col + rect.col as u16,
            row: ORIGIN.row + rect.row as u16,
        },
    ));
    assert_eq!(
        span_app
            .tabs
            .active()
            .styles
            .as_ref()
            .unwrap()
            .get(span)
            .color,
        Some(Rgb::new(255, 0, 0).into())
    );
}

#[test]
fn keyboard_scroll_refreshes_hover_under_a_parked_pointer() {
    let mut html = String::from("<!doctype html><a href=/>target</a>");
    for index in 0..40 {
        html.push_str(&format!("<p>line {index}</p>"));
    }
    let mut app = loaded(&html);
    let cell = link_cell(&app);
    app.handle_mouse(event(MouseKind::Move, cell));
    assert!(app.hovers_link());
    app.handle_key(press(Key::Down));
    assert!(!app.hovers_link());
}

#[test]
fn scroll_damage_records_displacement_and_a_boundary_is_inert() {
    let mut html = String::from("<!doctype html>");
    for index in 0..40 {
        html.push_str(&format!("<p>line {index}</p>"));
    }
    let mut app = loaded(&html);
    let _ = app.take_damage();
    app.handle_key(press(Key::Down));
    let damage = app.take_damage();
    assert_eq!(damage.content.scroll_rows, 1);
    assert_eq!(damage.content.repaint, RowDamage::None);
    app.handle_key(press(Key::Home));
    let _ = app.take_damage();
    app.handle_key(press(Key::Up));
    assert!(app.take_damage().is_empty());
}

#[test]
fn resize_and_late_stylesheet_delivery_refresh_a_parked_pointer() {
    let mut resized = loaded("<!doctype html><a href=/>target</a>");
    let cell = link_cell(&resized);
    resized.handle_mouse(event(MouseKind::Move, cell));
    assert!(resized.hovers_link());
    resized.on_resize(Size { cols: 40, rows: 4 });
    assert!(!resized.hovers_link());

    let net = Arc::new(DeferredCssNet::new(
        "<!doctype html><link rel=stylesheet href=late.css><a href=/>target</a>",
    ));
    let mut app = App::with_net(net.clone());
    app.handle_key(press(Key::Esc));
    app.submit_url("https://a.example");
    app.step(Duration::ZERO);
    app.step(STYLESHEET_DEADLINE);
    let cell = link_cell(&app);
    app.handle_mouse(event(MouseKind::Move, cell));
    assert!(app.hovers_link());
    let (tab_id, generation, resource_id, url) = net
        .submitted
        .lock()
        .unwrap()
        .iter()
        .find(|(_, _, resource, _)| *resource != ResourceId::DOCUMENT)
        .cloned()
        .unwrap();
    assert!(app.deliver_fetch(FetchPayload {
        tab_id,
        generation,
        resource_id,
        result: Ok(FetchResponse {
            final_url: url,
            status: 200,
            body: b"a { display: none }".to_vec(),
            content_type: Some("text/css".to_string()),
        }),
    }));
    assert!(!app.hovers_link());
}

#[test]
fn layout_changing_hover_reaches_a_fixed_point_for_a_parked_pointer() {
    let mut app = loaded(
        "<!doctype html><style>span:hover { display: none }</style><p><span id=target>target</span></p>",
    );
    let document = app.tabs.active().document.as_ref().unwrap().borrow();
    let target = document.element_by_id("target").unwrap();
    let text = document.first_child(target).unwrap();
    drop(document);
    let rect = app
        .tabs
        .active()
        .painted
        .hits
        .iter()
        .find(|hit| hit.node == text)
        .unwrap()
        .rect;
    let at = Point {
        col: ORIGIN.col + rect.col as u16,
        row: ORIGIN.row + rect.row as u16,
    };
    app.handle_mouse(event(MouseKind::Move, at));
    let _ = app.take_dirty();
    for _ in 0..100_000 {
        app.handle_mouse(event(MouseKind::Move, at));
        assert!(!app.take_dirty());
    }
}

#[test]
fn resize_preview_retains_page_layout_until_the_settle_deadline() {
    let mut app = loaded("<!doctype html><p>one two three four five six seven eight</p>");
    let old_width = app.tabs.active().layout_width;
    let mut batch = InputBatch::new();
    batch.push(InputEvent::Resize {
        size: Size { cols: 40, rows: 12 },
        phase: ResizePhase::Preview,
    });
    app.advance(&batch, Duration::from_millis(1));
    assert_eq!(app.geometry.size, Size { cols: 40, rows: 12 });
    assert_eq!(app.tabs.active().layout_width, old_width);
    app.advance(&InputBatch::new(), Duration::from_millis(50));
    assert_eq!(app.tabs.active().layout_width, old_width);
    app.advance(&InputBatch::new(), Duration::from_millis(51));
    assert_eq!(app.tabs.active().layout_width, 38);
}

#[test]
fn continuous_hover_restyle_runs_once_after_input_becomes_quiet() {
    let mut app = loaded(
        "<!doctype html><style>a:hover { color: red }</style><a id=target href=/>target</a>",
    );
    let target = app
        .tabs
        .active()
        .document
        .as_ref()
        .unwrap()
        .borrow()
        .element_by_id("target")
        .unwrap();
    let mut batch = InputBatch::new();
    batch.push(InputEvent::Mouse(event(MouseKind::Move, link_cell(&app))));
    app.advance(&batch, Duration::from_millis(1));
    assert_ne!(
        app.tabs.active().styles.as_ref().unwrap().get(target).color,
        Some(Rgb::new(255, 0, 0).into())
    );
    app.advance(&batch, Duration::from_millis(10));
    assert_ne!(
        app.tabs.active().styles.as_ref().unwrap().get(target).color,
        Some(Rgb::new(255, 0, 0).into())
    );
    app.advance(&InputBatch::new(), Duration::from_millis(18));
    assert_eq!(
        app.tabs.active().styles.as_ref().unwrap().get(target).color,
        Some(Rgb::new(255, 0, 0).into())
    );
}

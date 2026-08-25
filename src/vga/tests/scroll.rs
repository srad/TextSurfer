//! End-to-end scroll correctness for the real `VgaBackend`/`Surface`.
//!
//! The existing composer scroll test drives ratatui's `TestBackend`, whose
//! `scroll_region_up` semantics differ from `Surface::scroll_rows`. This exercises the
//! actual pixel surface: an incrementally-scrolled surface must equal a fresh full
//! composition at the same scroll offset.

use ratatui::layout::Rect;

use crate::core::frame::FrameDamage;
use crate::core::geom::Size;
use crate::paint::DisplayList;
use crate::ui::ChromeGeometry;
use crate::ui::frame::FrameComposer;
use crate::ui::test_util::draft_view;
use crate::ui::theme::{DEFAULT, rgb_of};
use crate::vga::{SurfaceConfig, VgaBackend};

fn config(cols: u16, rows: u16) -> SurfaceConfig {
    SurfaceConfig {
        cols,
        rows,
        scale: 1,
        default_fg: rgb_of(DEFAULT.text),
        default_bg: rgb_of(DEFAULT.bg),
    }
}

#[test]
fn incremental_scroll_matches_a_fresh_full_compose_on_the_real_surface() {
    let cols = 60u16;
    let rows = 20u16;
    let area = Rect::new(0, 0, cols, rows);
    let lines = (0..80)
        .map(|row| format!("line {row:04} the quick brown fox"))
        .collect::<Vec<_>>();
    let painted = DisplayList::from_lines(&lines);

    let mut view = draft_view();
    view.geometry = ChromeGeometry::for_size(Size { cols, rows });
    view.content.painted = &painted;
    view.content.scroll = 0;

    let mut backend = VgaBackend::new(config(cols, rows));
    let mut composer = FrameComposer::new(area);
    composer
        .present(&mut backend, &view, &FrameDamage::full())
        .unwrap();

    for step in 1..=6usize {
        let k = 3i32;
        view.content.scroll += k as usize;
        let mut damage = FrameDamage::default();
        damage.scroll(k);
        composer.present(&mut backend, &view, &damage).unwrap();

        let mut fresh_backend = VgaBackend::new(config(cols, rows));
        let mut fresh_composer = FrameComposer::new(area);
        fresh_composer
            .present(&mut fresh_backend, &view, &FrameDamage::full())
            .unwrap();

        assert_eq!(
            backend.surface().pixels(),
            fresh_backend.surface().pixels(),
            "scroll step {step} (scroll={}) diverged from a fresh full compose",
            view.content.scroll
        );
    }
}

#[test]
fn scrolling_a_band_below_the_top_damages_that_band_in_pixel_coordinates() {
    // Regression: `scroll_rows` used to hand `mark_damage` a *flat buffer offset* as the y
    // coordinate, so any band not starting at row 0 (i.e. every real content band, which
    // sits below the chrome) reported no damage at all — `present()` then never re-copied
    // the scrolled bulk to the window, so the page appeared frozen while fragments piled up.
    // The old unit test only scrolled a row-0 band, where the flat offset happens to equal y.
    use crate::vga::Surface;

    let cols = 8u16;
    let rows = 40u16; // tall enough that a small band does not trip the full-damage collapse
    let cell_h = crate::vga::font::CELL_H;
    let mut surface = Surface::new(config(cols, rows));
    let _ = surface.take_damage_regions();

    // Scroll rows [4, 10) up by 2 — a band that starts well below the top.
    surface.scroll_rows(4..10, 2, true);

    let regions = surface.take_damage_regions();
    assert_eq!(regions.len(), 1, "one merged damage region: {regions:?}");
    let region = regions[0];
    let (pixel_width, _) = surface.pixel_size();
    assert_eq!(region.x, 0);
    assert_eq!(region.width, pixel_width);
    assert_eq!(
        region.y,
        4 * cell_h,
        "damage y must be the band top in pixels"
    );
    assert_eq!(
        region.height,
        (10 - 4) * cell_h,
        "damage must cover the whole scrolled band"
    );
}

#[test]
fn page_sized_scroll_does_not_panic() {
    let cols = 160u16;
    let rows = 50u16;
    let area = Rect::new(0, 0, cols, rows);
    let lines = (0..200)
        .map(|r| format!("line {r:04} content here"))
        .collect::<Vec<_>>();
    let painted = DisplayList::from_lines(&lines);
    let mut view = draft_view();
    view.geometry = ChromeGeometry::for_size(Size { cols, rows });
    view.content.painted = &painted;
    let mut backend = VgaBackend::new(config(cols, rows));
    let mut composer = FrameComposer::new(area);
    composer
        .present(&mut backend, &view, &FrameDamage::full())
        .unwrap();
    let content_rows = view.geometry.content_rows();
    let step = (content_rows - 1) as i32;
    for k in [step, step, step] {
        view.content.scroll += k as usize;
        let mut damage = FrameDamage::default();
        damage.scroll(k);
        composer.present(&mut backend, &view, &damage).unwrap();
    }
}

// A minimal fetcher that answers every request with one fixed HTML body.
struct FakeHtmlNet {
    body: Vec<u8>,
    pending: std::sync::Mutex<Vec<crate::net::FetchPayload>>,
}

impl crate::app::net::Navigate for FakeHtmlNet {
    fn submit(
        &self,
        tab_id: u64,
        generation: u64,
        resource_id: crate::net::ResourceId,
        url: url::Url,
    ) -> crate::net::Submitted {
        self.pending.lock().unwrap().push(crate::net::FetchPayload {
            tab_id,
            generation,
            resource_id,
            result: Ok(crate::net::FetchResponse {
                final_url: url,
                status: 200,
                body: self.body.clone(),
                content_type: Some("text/html".to_string()),
            }),
        });
        crate::net::Submitted::Queued
    }
    fn poll_result(&self) -> crate::net::FetchPoll {
        self.pending
            .lock()
            .unwrap()
            .pop()
            .map_or(crate::net::FetchPoll::Empty, crate::net::FetchPoll::Ready)
    }
}

#[test]
fn space_page_down_on_a_loaded_scaled_page_does_not_crash() {
    use crate::core::event::{Key, MouseButton, MouseEvent, MouseKind};
    use crate::core::geom::Point;
    use crate::vga::{VgaOptions, window::VgaApp};
    use std::sync::Arc;
    use std::time::Duration;

    let mut html = String::from("<h1>Great Big Heading</h1>");
    for i in 0..300 {
        html.push_str(&format!(
            "<h2>Section {i}</h2><p>paragraph body number {i} here</p>"
        ));
    }

    let net = Arc::new(FakeHtmlNet {
        body: html.into_bytes(),
        pending: std::sync::Mutex::new(Vec::new()),
    });
    let mut app = VgaApp::new(
        net,
        VgaOptions::default(),
        Some("https://x.example/".to_string()),
    )
    .expect("vga app");

    // Pump the fetch/render pipeline to steady state.
    for step in 0..40u64 {
        app.tick_at(Duration::from_millis(step * 50));
    }
    app.redraw().expect("initial redraw");

    // Focus the content: move the pointer into it and click.
    let at = Point { col: 5, row: 20 };
    app.move_pointer(at);
    app.inject_mouse(MouseEvent {
        kind: MouseKind::Press(MouseButton::Left),
        at,
    });
    app.inject_mouse(MouseEvent {
        kind: MouseKind::Release(MouseButton::Left),
        at,
    });
    let mut now = 2000u64;
    app.tick_at(Duration::from_millis(now));
    app.redraw().expect("redraw after focus");

    // Space = ScrollPageDown; do it many times through the real redraw path.
    for _ in 0..30 {
        app.inject_key(Key::Char(' '));
        now += 20;
        app.tick_at(Duration::from_millis(now));
        app.redraw().expect("redraw after page-down");
    }
    // Reached here without panicking.
    assert!(app.backend().surface().pixels().iter().any(|&p| p != 0));
}

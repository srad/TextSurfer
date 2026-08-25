use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use winit::dpi::PhysicalSize;
use winit::window::CursorIcon;

use crate::app::net::NoopNet;
use crate::core::event::Key;
use crate::core::geom::Point;
use crate::core::style::Cursor;
use crate::vga::VgaOptions;
use crate::vga::surface::PixelRect;
use crate::vga::window::{
    CursorPresentation, CursorUpdate, VgaApp, blit_pixels, cursor_presentation, cursor_update,
    union_damage,
};

fn vga(url: Option<&str>) -> VgaApp {
    VgaApp::new(
        Arc::new(NoopNet),
        VgaOptions::default(),
        url.map(str::to_string),
    )
    .expect("VGA app")
}

fn drain_initial_redraw(app: &mut VgaApp) {
    assert!(app.tick_at(Duration::ZERO).redraw);
    app.redraw().expect("initial redraw");
    assert!(!app.tick_at(Duration::ZERO).redraw);
}

#[test]
fn blank_launch_stays_quiescent_after_the_initial_redraw() {
    let mut app = vga(None);
    drain_initial_redraw(&mut app);
    for step in 1..=200 {
        assert!(!app.tick_at(Duration::from_millis(step * 50)).redraw);
    }
}

#[test]
fn pending_url_stays_quiescent_while_the_fetcher_has_no_result() {
    let mut app = vga(Some("https://example.com/"));
    drain_initial_redraw(&mut app);
    for step in 1..=99 {
        assert!(!app.tick_at(Duration::from_millis(step * 50)).redraw);
    }
}

#[test]
fn duplicate_physical_resize_does_not_dirty_an_unchanged_grid() {
    let mut app = vga(None);
    drain_initial_redraw(&mut app);
    app.resize(PhysicalSize::new(1280, 800));
    assert!(!app.tick_at(Duration::ZERO).redraw);
}

#[test]
fn a_headless_redraw_does_not_schedule_another_redraw() {
    let mut app = vga(None);
    drain_initial_redraw(&mut app);
    app.redraw().expect("headless redraw");
    for step in 1..=200 {
        assert!(!app.tick_at(Duration::from_millis(step * 50)).redraw);
    }
}

#[test]
fn selecting_a_theme_refreshes_the_headless_grid_and_requests_a_full_present() {
    let mut app = vga(None);
    drain_initial_redraw(&mut app);
    for key in [
        Key::F(10),
        Key::Right,
        Key::Right,
        Key::Down,
        Key::Down,
        Key::Down,
        Key::Down,
        Key::Enter,
    ] {
        app.inject_key(key);
    }
    assert!(app.tick_at(Duration::from_secs(1)).redraw);
    assert_eq!(app.theme_index(), 4);
    app.redraw().expect("theme redraw");
    assert!(app.force_full_present());
}

#[test]
fn full_blit_recolors_physical_margins() {
    let old = 0x0000AA;
    let fill = 0xE8E4D8;
    let pixels = vec![0x101010; 6];
    let mut target = vec![old; 15];
    blit_pixels(
        &mut target,
        (5, 3),
        &pixels,
        (3, 2),
        fill,
        true,
        &[PixelRect {
            x: 0,
            y: 0,
            width: 3,
            height: 2,
        }],
    );
    assert_eq!(&target[0..3], &[0x101010; 3]);
    assert_eq!(&target[5..8], &[0x101010; 3]);
    assert_eq!(&target[10..15], &[fill; 5]);
    assert_eq!(target[4], fill);
    assert_eq!(target[9], fill);
}

#[test]
fn repeated_pointer_position_on_the_start_page_never_dirties_the_ui() {
    let mut app = vga(None);
    drain_initial_redraw(&mut app);
    let at = Point { col: 40, row: 20 };
    assert!(app.move_pointer(at));
    assert!(!app.tick_at(Duration::ZERO).redraw);
    for _ in 0..100_000 {
        assert!(!app.move_pointer(at));
        assert!(!app.tick_at(Duration::ZERO).redraw);
    }
}

#[test]
fn cursor_updates_are_suppressed_until_the_presentation_changes() {
    let default = cursor_presentation(Cursor::Auto);
    assert_eq!(cursor_update(default, default), CursorUpdate::default());

    let pointer = cursor_presentation(Cursor::Pointer);
    assert_eq!(
        cursor_update(default, pointer),
        CursorUpdate {
            visible: None,
            icon: Some(CursorIcon::Pointer),
        }
    );
    assert_eq!(cursor_update(pointer, pointer), CursorUpdate::default());
}

#[test]
fn hidden_cursor_transition_is_bounded_and_reversible() {
    let default = cursor_presentation(Cursor::Default);
    let hidden = cursor_presentation(Cursor::None);
    assert_eq!(
        cursor_update(default, hidden),
        CursorUpdate {
            visible: Some(false),
            icon: None,
        }
    );
    assert_eq!(cursor_update(hidden, hidden), CursorUpdate::default());
    assert_eq!(
        cursor_update(hidden, default),
        CursorUpdate {
            visible: Some(true),
            icon: Some(CursorIcon::Default),
        }
    );
}

#[test]
fn every_supported_cursor_has_a_visible_native_presentation() {
    assert_eq!(
        cursor_presentation(Cursor::Text),
        CursorPresentation {
            visible: true,
            icon: CursorIcon::Text,
        }
    );
    assert!(cursor_presentation(Cursor::ZoomOut).visible);
}

#[test]
fn older_back_buffers_union_every_intervening_damage() {
    let left = softbuffer::Rect {
        x: 8,
        y: 16,
        width: NonZeroU32::new(8).unwrap(),
        height: NonZeroU32::new(16).unwrap(),
    };
    let right = softbuffer::Rect {
        x: 32,
        y: 48,
        width: NonZeroU32::new(16).unwrap(),
        height: NonZeroU32::new(32).unwrap(),
    };
    let union = union_damage(left, right);
    assert_eq!((union.x, union.y), (8, 16));
    assert_eq!((union.width.get(), union.height.get()), (40, 64));
}

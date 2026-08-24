use super::event::{
    InputBatch, InputEvent, Key, KeyEvent, KeyModifiers, MouseEvent, MouseKind, ResizePhase,
};
use super::frame::{ChromeDamage, FrameDamage, RowDamage};
use super::frame::{EVENTS_PER_FRAME, FRAME_INTERVAL, FrameScheduler};
use super::geom::{Point, Size};

fn point(col: u16, row: u16) -> Point {
    Point { col, row }
}

#[test]
fn continuous_input_coalesces_without_crossing_barriers() {
    let mut batch = InputBatch::new();
    batch.push(InputEvent::Mouse(MouseEvent {
        kind: MouseKind::Move,
        at: point(1, 1),
    }));
    batch.push(InputEvent::Mouse(MouseEvent {
        kind: MouseKind::Move,
        at: point(2, 2),
    }));
    batch.push(InputEvent::Mouse(MouseEvent {
        kind: MouseKind::Wheel { rows: 3 },
        at: point(2, 2),
    }));
    batch.push(InputEvent::Mouse(MouseEvent {
        kind: MouseKind::Wheel { rows: 6 },
        at: point(2, 2),
    }));
    batch.push(InputEvent::Key(KeyEvent {
        code: Key::Enter,
        modifiers: KeyModifiers::default(),
    }));
    batch.push(InputEvent::Resize {
        size: Size { cols: 80, rows: 24 },
        phase: ResizePhase::Preview,
    });
    batch.push(InputEvent::Resize {
        size: Size {
            cols: 100,
            rows: 30,
        },
        phase: ResizePhase::Preview,
    });

    assert_eq!(
        batch.as_slice(),
        &[
            InputEvent::Mouse(MouseEvent {
                kind: MouseKind::Move,
                at: point(2, 2),
            }),
            InputEvent::Mouse(MouseEvent {
                kind: MouseKind::Wheel { rows: 9 },
                at: point(2, 2),
            }),
            InputEvent::Key(KeyEvent {
                code: Key::Enter,
                modifiers: KeyModifiers::default(),
            }),
            InputEvent::Resize {
                size: Size {
                    cols: 100,
                    rows: 30
                },
                phase: ResizePhase::Preview,
            },
        ]
    );
}

#[test]
fn frame_damage_preserves_scroll_and_row_repaint() {
    let mut damage = FrameDamage::default();
    damage.scroll(4);
    damage.scroll(-1);
    damage.repaint_rows(8..10);
    damage.repaint_rows(9..12);
    damage.damage_chrome(ChromeDamage::Status);

    assert_eq!(damage.content.scroll_rows, 3);
    assert_eq!(
        damage.content.repaint,
        RowDamage::Ranges(std::iter::once(8..12).collect())
    );
    assert_eq!(damage.chrome, ChromeDamage::Status);
    assert!(!damage.is_empty());

    damage.repaint_content();
    assert!(damage.content.full);
    assert_eq!(damage.content.scroll_rows, 0);
    assert_eq!(damage.content.repaint, RowDamage::None);
}

#[test]
fn scheduler_coalesces_a_thousand_hertz_stream_to_sixty_hertz() {
    let mut scheduler = FrameScheduler::default();
    let mut frames = 0;
    for millis in 0..2_000 {
        let now = std::time::Duration::from_millis(millis);
        scheduler.push(
            InputEvent::Mouse(MouseEvent {
                kind: MouseKind::Move,
                at: point(millis as u16, 1),
            }),
            now,
        );
        if scheduler.take_due(now).is_some() {
            frames += 1;
        }
    }
    assert!(
        frames <= 121,
        "two seconds must not exceed the frame cadence"
    );
}

#[test]
fn scheduler_preserves_discrete_backlog_without_catch_up() {
    let mut scheduler = FrameScheduler::default();
    for index in 0..EVENTS_PER_FRAME + 7 {
        scheduler.push(
            InputEvent::Key(KeyEvent {
                code: Key::F(index as u8),
                modifiers: KeyModifiers::default(),
            }),
            std::time::Duration::ZERO,
        );
    }
    let first = scheduler.take_due(std::time::Duration::ZERO).unwrap();
    assert_eq!(first.len(), EVENTS_PER_FRAME);
    assert!(scheduler.has_pending_input());
    assert!(
        scheduler
            .take_due(FRAME_INTERVAL.saturating_sub(std::time::Duration::from_micros(1)))
            .is_none()
    );
    let second = scheduler
        .take_due(std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(second.len(), 7);
    assert!(!scheduler.has_pending_input());
}

#[test]
fn scheduler_waits_indefinitely_when_input_and_app_are_idle() {
    let scheduler = FrameScheduler::default();
    assert_eq!(scheduler.next_deadline(None), None);
    assert_eq!(
        scheduler.next_deadline(Some(std::time::Duration::from_millis(50))),
        Some(std::time::Duration::from_millis(50))
    );
}

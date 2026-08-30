use proptest::prelude::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::core::event::{Key, KeyEvent, KeyModifiers};

use super::*;

fn key(code: Key) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::default(),
    }
}

fn ctrl(code: Key) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers {
            ctrl: true,
            ..Default::default()
        },
    }
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Insert(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
}

fn any_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<char>().prop_map(Op::Insert),
        Just(Op::Backspace),
        Just(Op::Delete),
        Just(Op::Left),
        Just(Op::Right),
        Just(Op::Home),
        Just(Op::End),
    ]
}

proptest! {
    #[test]
    fn cursor_and_selection_never_leave_the_buffer(
        ops in prop::collection::vec(any_op(), 0..128),
        extend in any::<bool>(),
    ) {
        let mut field = TextFieldState::new();
        for op in ops {
            let code = match op {
                Op::Insert(character) => Key::Char(character),
                Op::Backspace => Key::Backspace,
                Op::Delete => Key::Delete,
                Op::Left => Key::Left,
                Op::Right => Key::Right,
                Op::Home => Key::Home,
                Op::End => Key::End,
            };
            field.handle_key(
                &KeyEvent {
                    code,
                    modifiers: KeyModifiers {
                        shift: extend,
                        ..Default::default()
                    },
                },
                true,
            );
            prop_assert!(field.cursor() <= field.len());
            if let Some(selection) = field.selection() {
                prop_assert!(selection.start <= selection.end);
                prop_assert!(selection.end <= field.len());
            }
        }
    }
}

#[test]
fn shifted_movement_selects_and_typing_replaces_the_selection() {
    let mut field = TextFieldState::with_text("alpha beta");
    field.set_cursor(5, false);
    field.handle_key(
        &KeyEvent {
            code: Key::Left,
            modifiers: KeyModifiers {
                shift: true,
                ..Default::default()
            },
        },
        false,
    );
    field.handle_key(&key(Key::Char('X')), false);
    assert_eq!(field.text(), "alphX beta");
    assert_eq!(field.cursor(), 5);
    assert_eq!(field.selection(), None);
}

#[test]
fn copy_cut_paste_and_select_all_are_grapheme_safe() {
    let mut field = TextFieldState::with_text("a\u{301}👩‍💻z");
    assert_eq!(
        field.handle_key(&ctrl(Key::Char('a')), false).clipboard,
        None
    );
    assert_eq!(field.selection(), Some(0..3));
    assert_eq!(
        field.handle_key(&ctrl(Key::Char('c')), false).clipboard,
        Some(ClipboardAction::Write("a\u{301}👩‍💻z".to_string()))
    );
    assert_eq!(
        field.handle_key(&ctrl(Key::Char('x')), false).clipboard,
        Some(ClipboardAction::Write("a\u{301}👩‍💻z".to_string()))
    );
    assert!(field.is_empty());
    assert_eq!(
        field.handle_key(&ctrl(Key::Char('v')), false).clipboard,
        Some(ClipboardAction::Read)
    );
    field.paste("é");
    assert_eq!(field.text(), "é");
    assert_eq!(field.cursor(), 1);
}

#[test]
fn pointer_index_and_cursor_share_the_rendered_window() {
    let field = TextFieldState::with_text("0123456789");
    let view = TextFieldView::new(&field).frame(TextFieldFrame::Brackets);
    assert_eq!(view.cursor_in(Rect::new(4, 2, 6, 1)), Some((8, 2)));
    assert_eq!(view.index_at(Rect::new(4, 2, 6, 1), 5, 2), 7);
}

#[test]
fn end_cursor_reserves_a_blank_cell_when_text_fills_the_view() {
    let field = TextFieldState::with_text("ab");
    let view = TextFieldView::new(&field);
    let backend = TestBackend::new(2, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            TextField::new(view).render(frame.area(), frame.buffer_mut());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let cursor = view.cursor_in(Rect::new(0, 0, 2, 1)).unwrap();
    assert_eq!(buffer[cursor].symbol(), " ");
    assert_eq!(buffer[(0, 0)].symbol(), "b");
}

#[test]
fn widget_renders_selection_and_masks_password_text() {
    let mut field = TextFieldState::with_text("secret");
    field.select_all();
    let backend = TestBackend::new(10, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            TextField::new(TextFieldView::new(&field).mask('*'))
                .style(Style::default().fg(Color::White))
                .selection_style(Style::default().bg(Color::Blue))
                .render(frame.area(), frame.buffer_mut());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "*");
    assert_eq!(buffer[(5, 0)].symbol(), "*");
    assert_eq!(buffer[(0, 0)].bg, Color::Blue);
    assert_eq!(buffer[(5, 0)].bg, Color::Blue);
}

#[test]
fn widget_clears_stale_placeholder_glyphs_before_drawing_a_value() {
    let field = TextFieldState::with_text("W");
    let backend = TestBackend::new(12, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.buffer_mut().set_string(
                0,
                0,
                "Search Wiki",
                Style::default().fg(Color::DarkGray),
            );
            TextField::new(TextFieldView::new(&field))
                .style(Style::default().fg(Color::White))
                .render(frame.area(), frame.buffer_mut());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), "W");
    for col in 1..12 {
        assert_eq!(buffer[(col, 0)].symbol(), " ");
    }
}

#[test]
fn context_menu_actions_use_the_same_selection_commands() {
    let mut field = TextFieldState::with_text("copy me");
    field.select_all();
    let edit = field.apply_menu(TextFieldMenuAction::Cut);
    assert!(edit.changed);
    assert_eq!(
        edit.clipboard,
        Some(ClipboardAction::Write("copy me".to_string()))
    );
    assert!(field.is_empty());
    assert_eq!(
        menu_action_at(
            Rect::new(0, 0, 40, 10),
            crate::core::geom::Point { col: 38, row: 9 },
            crate::core::geom::Point { col: 27, row: 5 },
        ),
        Some(TextFieldMenuAction::Cut)
    );
}

#[test]
fn context_menu_hover_selects_only_enabled_rows() {
    let backend = TestBackend::new(20, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    let anchor = crate::core::geom::Point { col: 3, row: 2 };
    terminal
        .draw(|frame| {
            TextFieldMenu {
                anchor,
                can_copy: false,
                can_cut: false,
                selected: Some(TextFieldMenuAction::Copy),
                theme: &crate::ui::theme::DEFAULT,
            }
            .render(frame.area(), frame.buffer_mut());
        })
        .unwrap();
    let rect = menu_rect(terminal.backend().buffer().area, anchor);
    let selected = crate::ui::theme::DEFAULT.selected();
    let copy = &terminal.backend().buffer()[(rect.x + 1, rect.y + 2)];
    assert_ne!(copy.bg, selected.bg.unwrap());
    assert!(copy.modifier.contains(Modifier::DIM));

    terminal
        .draw(|frame| {
            TextFieldMenu {
                anchor,
                can_copy: false,
                can_cut: false,
                selected: Some(TextFieldMenuAction::Paste),
                theme: &crate::ui::theme::DEFAULT,
            }
            .render(frame.area(), frame.buffer_mut());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    for col in rect.x + 1..rect.right() - 1 {
        assert_eq!(buffer[(col, rect.y + 3)].fg, selected.fg.unwrap());
        assert_eq!(buffer[(col, rect.y + 3)].bg, selected.bg.unwrap());
    }
}

#[test]
fn context_menu_is_an_opaque_chrome_themed_surface() {
    let backend = TestBackend::new(20, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            for row in 0..frame.area().height {
                frame.buffer_mut().set_string(
                    0,
                    row,
                    "XXXXXXXXXXXXXXXXXXXX",
                    Style::default().fg(Color::Magenta).bg(Color::Red),
                );
            }
            TextFieldMenu {
                anchor: crate::core::geom::Point { col: 3, row: 2 },
                can_copy: false,
                can_cut: false,
                selected: None,
                theme: &crate::ui::theme::DEFAULT,
            }
            .render(frame.area(), frame.buffer_mut());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rect = menu_rect(buffer.area, crate::core::geom::Point { col: 3, row: 2 });
    for row in rect.y..rect.bottom() {
        for col in rect.x..rect.right() {
            assert_ne!(buffer[(col, row)].symbol(), "X");
            assert_eq!(buffer[(col, row)].bg, crate::ui::theme::DEFAULT.bar_bg);
        }
    }
    assert_eq!(
        buffer[(rect.x + 1, rect.y + 3)].fg,
        crate::ui::theme::DEFAULT.bar_text
    );
}

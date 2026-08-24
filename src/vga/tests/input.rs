use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton as WinitMouseButton, MouseScrollDelta};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};

use crate::core::event::{Key, KeyModifiers, MouseButton, MouseKind, WheelDirection};
use crate::core::geom::Point;
use crate::vga::input::{WheelAccumulator, from_window_button, from_window_key};

fn press(key: WinitKey) -> Option<crate::core::event::KeyEvent> {
    from_window_key(&key, ElementState::Pressed, ModifiersState::empty())
}

fn press_with(key: WinitKey, modifiers: ModifiersState) -> Option<crate::core::event::KeyEvent> {
    from_window_key(&key, ElementState::Pressed, modifiers)
}

fn named(key: NamedKey) -> Option<crate::core::event::KeyEvent> {
    press(WinitKey::Named(key))
}

#[test]
fn characters_arrive_as_typed() {
    assert_eq!(
        press(WinitKey::Character("q".into())).unwrap().code,
        Key::Char('q')
    );
    assert_eq!(
        press(WinitKey::Character("/".into())).unwrap().code,
        Key::Char('/')
    );
}

#[test]
fn space_is_a_character_not_a_named_key() {
    // The keymap pages down on Space; it must not arrive as an unhandled named key.
    assert_eq!(named(NamedKey::Space).unwrap().code, Key::Char(' '));
}

#[test]
fn named_keys_map_onto_the_domain_keys() {
    for (named_key, expected) in [
        (NamedKey::Backspace, Key::Backspace),
        (NamedKey::Delete, Key::Delete),
        (NamedKey::Enter, Key::Enter),
        (NamedKey::Escape, Key::Esc),
        (NamedKey::Tab, Key::Tab),
        (NamedKey::Home, Key::Home),
        (NamedKey::End, Key::End),
        (NamedKey::PageUp, Key::PageUp),
        (NamedKey::PageDown, Key::PageDown),
        (NamedKey::ArrowUp, Key::Up),
        (NamedKey::ArrowDown, Key::Down),
        (NamedKey::ArrowLeft, Key::Left),
        (NamedKey::ArrowRight, Key::Right),
    ] {
        assert_eq!(named(named_key).unwrap().code, expected, "{named_key:?}");
    }
}

#[test]
fn function_keys_carry_their_number() {
    assert_eq!(named(NamedKey::F1).unwrap().code, Key::F(1));
    assert_eq!(named(NamedKey::F10).unwrap().code, Key::F(10));
    assert_eq!(named(NamedKey::F12).unwrap().code, Key::F(12));
}

#[test]
fn shift_tab_becomes_backtab_as_a_terminal_reports_it() {
    // The keymap is written against the terminal's BackTab; synthesising it here is
    // what keeps one keymap correct for both frontends.
    let event = press_with(WinitKey::Named(NamedKey::Tab), ModifiersState::SHIFT).unwrap();
    assert_eq!(event.code, Key::BackTab);
    assert!(
        event.modifiers.shift,
        "shift stays set, as crossterm reports it"
    );
}

#[test]
fn modifiers_are_carried_through() {
    let all = ModifiersState::SHIFT | ModifiersState::CONTROL | ModifiersState::ALT;
    let event = press_with(WinitKey::Character("t".into()), all).unwrap();
    assert_eq!(
        event.modifiers,
        KeyModifiers {
            shift: true,
            ctrl: true,
            alt: true,
        },
    );
}

#[test]
fn the_super_key_is_not_mistaken_for_a_modifier_we_track() {
    let event = press_with(WinitKey::Character("t".into()), ModifiersState::SUPER).unwrap();
    assert_eq!(event.modifiers, KeyModifiers::default());
}

#[test]
fn releases_are_dropped() {
    let released = from_window_key(
        &WinitKey::Named(NamedKey::Enter),
        ElementState::Released,
        ModifiersState::empty(),
    );
    assert!(released.is_none());
}

#[test]
fn bare_modifier_presses_are_not_keystrokes() {
    // They arrive as `modifiers` on the next real key instead.
    assert!(named(NamedKey::Shift).is_none());
    assert!(named(NamedKey::Control).is_none());
    assert!(named(NamedKey::Alt).is_none());
}

#[test]
fn dead_and_unidentified_keys_are_dropped() {
    assert!(press(WinitKey::Dead(Some('^'))).is_none());
    assert!(press(WinitKey::Dead(None)).is_none());
}

#[test]
fn an_empty_character_payload_is_dropped_rather_than_panicking() {
    assert!(press(WinitKey::Character("".into())).is_none());
}

#[test]
fn window_buttons_map_including_the_side_pair() {
    let at = Point { col: 3, row: 4 };
    let pressed = |button| from_window_button(button, ElementState::Pressed, at);
    assert_eq!(
        pressed(WinitMouseButton::Left).unwrap().kind,
        MouseKind::Press(MouseButton::Left)
    );
    assert_eq!(
        pressed(WinitMouseButton::Middle).unwrap().kind,
        MouseKind::Press(MouseButton::Middle)
    );
    assert_eq!(
        pressed(WinitMouseButton::Back).unwrap().kind,
        MouseKind::Press(MouseButton::Back)
    );
    assert_eq!(
        pressed(WinitMouseButton::Forward).unwrap().kind,
        MouseKind::Press(MouseButton::Forward)
    );
    assert_eq!(pressed(WinitMouseButton::Other(9)), None);
    assert_eq!(
        from_window_button(WinitMouseButton::Left, ElementState::Released, at)
            .unwrap()
            .kind,
        MouseKind::Release(MouseButton::Left)
    );
    assert_eq!(pressed(WinitMouseButton::Left).unwrap().at, at);
}

#[test]
fn a_wheel_line_is_one_notch_in_the_direction_it_points() {
    let mut wheel = WheelAccumulator::default();
    assert_eq!(
        wheel.push(MouseScrollDelta::LineDelta(0.0, 1.0), 48.0),
        Some(WheelDirection::Up)
    );
    assert_eq!(
        wheel.push(MouseScrollDelta::LineDelta(0.0, -1.0), 48.0),
        Some(WheelDirection::Down)
    );
    assert_eq!(
        wheel.push(MouseScrollDelta::LineDelta(0.0, 0.0), 48.0),
        None
    );
}

#[test]
fn trackpad_pixels_accumulate_into_whole_notches() {
    // A notch is three 16-pixel rows; four nudges of 12 pixels are one notch and a
    // remainder, not four notches and not nothing.
    let mut wheel = WheelAccumulator::default();
    let nudge = |wheel: &mut WheelAccumulator, pixels: f64| {
        wheel.push(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, pixels)),
            48.0,
        )
    };
    assert_eq!(nudge(&mut wheel, -12.0), None);
    assert_eq!(nudge(&mut wheel, -12.0), None);
    assert_eq!(nudge(&mut wheel, -12.0), None);
    assert_eq!(nudge(&mut wheel, -12.0), Some(WheelDirection::Down));
    assert_eq!(nudge(&mut wheel, -12.0), None);
}

#[test]
fn reversing_direction_drops_the_abandoned_remainder() {
    let mut wheel = WheelAccumulator::default();
    let nudge = |wheel: &mut WheelAccumulator, pixels: f64| {
        wheel.push(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, pixels)),
            48.0,
        )
    };
    assert_eq!(nudge(&mut wheel, 40.0), None);
    assert_eq!(nudge(&mut wheel, -40.0), None, "the upward part is gone");
    assert_eq!(nudge(&mut wheel, -40.0), Some(WheelDirection::Down));
}

//! Window key events translated into `core::event` types.
//!
//! The mirror of `main.rs`'s `from_terminal_key`, and it exists for the same reason:
//! `app` takes domain events, never a windowing or terminal library's own types. Keep
//! the two adapters behaviourally aligned — a key that reaches the app one way should
//! reach it the same way the other, or the same keymap will act differently per
//! frontend.

use winit::event::ElementState;
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};

use crate::core::event::{Key, KeyEvent, KeyModifiers};

/// Translate a window key event, or `None` if it carries nothing the app can use.
///
/// Releases and bare modifier presses are dropped, matching the terminal adapter,
/// which never sees them at all.
///
/// This takes the logical key and state rather than a `winit::event::KeyEvent`
/// because that struct has a private platform-specific field and so cannot be built
/// outside winit — which would leave this mapping untestable. Taking only what the
/// mapping reads keeps it a pure function over public types.
pub fn from_window_key(
    key: &WinitKey,
    state: ElementState,
    modifiers: ModifiersState,
) -> Option<KeyEvent> {
    if state != ElementState::Pressed {
        return None;
    }
    let shift = modifiers.shift_key();
    let code = match key {
        WinitKey::Named(named) => named_key(*named, shift)?,
        WinitKey::Character(text) => Key::Char(text.chars().next()?),
        // Dead keys await composition and unidentified keys carry no meaning here.
        WinitKey::Dead(_) | WinitKey::Unidentified(_) => return None,
    };
    Some(KeyEvent {
        code,
        modifiers: KeyModifiers {
            shift,
            ctrl: modifiers.control_key(),
            alt: modifiers.alt_key(),
        },
    })
}

/// Map a named key, folding Shift+Tab into `BackTab` the way a terminal reports it.
fn named_key(named: NamedKey, shift: bool) -> Option<Key> {
    let key = match named {
        NamedKey::Space => Key::Char(' '),
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::Enter => Key::Enter,
        NamedKey::Escape => Key::Esc,
        // A terminal sends BackTab for Shift+Tab, and the keymap is written against
        // that. Synthesising it here keeps one keymap correct for both frontends.
        NamedKey::Tab if shift => Key::BackTab,
        NamedKey::Tab => Key::Tab,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::ArrowUp => Key::Up,
        NamedKey::ArrowDown => Key::Down,
        NamedKey::ArrowLeft => Key::Left,
        NamedKey::ArrowRight => Key::Right,
        NamedKey::F1 => Key::F(1),
        NamedKey::F2 => Key::F(2),
        NamedKey::F3 => Key::F(3),
        NamedKey::F4 => Key::F(4),
        NamedKey::F5 => Key::F(5),
        NamedKey::F6 => Key::F(6),
        NamedKey::F7 => Key::F(7),
        NamedKey::F8 => Key::F(8),
        NamedKey::F9 => Key::F(9),
        NamedKey::F10 => Key::F(10),
        NamedKey::F11 => Key::F(11),
        NamedKey::F12 => Key::F(12),
        // Bare modifier presses are not keystrokes; they arrive as `modifiers` on the
        // next real key.
        _ => return None,
    };
    Some(key)
}

use crate::core::event::{Key, KeyEvent};
use crate::core::focus::Focus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    FocusAddress,
    FocusContent,
    SubmitAddress,
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    ScrollUp,
    ScrollDown,
    ScrollTop,
    ScrollBottom,
    ActivateLink,
    NextLink,
    PrevLink,
    Help,
}

pub trait Keymap {
    fn resolve(&self, event: &KeyEvent, focus: Focus) -> Option<Action>;
}

pub struct DefaultKeymap;

impl Keymap for DefaultKeymap {
    fn resolve(&self, event: &KeyEvent, focus: Focus) -> Option<Action> {
        let ctrl = event.modifiers.ctrl;
        match event.code {
            Key::Char(ch) => {
                if focus == Focus::Address {
                    return None;
                }
                let lower = ch.to_ascii_lowercase();
                match (lower, ctrl) {
                    ('q', false) => Some(Action::Quit),
                    ('?', false) => Some(Action::Help),
                    ('/', false) | ('a', false) => Some(Action::FocusAddress),
                    ('j', false) => Some(Action::ScrollDown),
                    ('k', false) => Some(Action::ScrollUp),
                    ('t', true) => Some(Action::NewTab),
                    ('w', true) => Some(Action::CloseTab),
                    ('n', true) => Some(Action::NextTab),
                    ('p', true) => Some(Action::PrevTab),
                    _ => None,
                }
            }
            Key::Enter => match focus {
                Focus::Address => Some(Action::SubmitAddress),
                Focus::Tabs => Some(Action::FocusContent),
                Focus::Content => Some(Action::ActivateLink),
            },
            Key::Esc => match focus {
                Focus::Address | Focus::Tabs => Some(Action::FocusContent),
                Focus::Content => None,
            },
            Key::Tab => match focus {
                Focus::Tabs => Some(Action::NextTab),
                _ => Some(Action::NextLink),
            },
            Key::BackTab => match focus {
                Focus::Tabs => Some(Action::PrevTab),
                _ => Some(Action::PrevLink),
            },
            Key::Down | Key::PageDown => Some(Action::ScrollDown),
            Key::Up | Key::PageUp => Some(Action::ScrollUp),
            Key::Home => Some(Action::ScrollTop),
            Key::End => Some(Action::ScrollBottom),
            Key::Left => match focus {
                Focus::Tabs => Some(Action::PrevTab),
                _ => None,
            },
            Key::Right => match focus {
                Focus::Tabs => Some(Action::NextTab),
                _ => None,
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::event::KeyModifiers;

    fn press(code: Key) -> KeyEvent {
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

    #[test]
    fn q_quits_in_content_but_never_while_typing() {
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('q')), Focus::Content),
            Some(Action::Quit)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('q')), Focus::Tabs),
            Some(Action::Quit)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('q')), Focus::Address),
            None
        );
    }

    #[test]
    fn printable_keys_belong_to_the_address_buffer() {
        for ch in ['a', 'Z', '1', '?', '/', 'j', 'k'] {
            assert_eq!(
                DefaultKeymap.resolve(&press(Key::Char(ch)), Focus::Address),
                None,
                "printable '{ch}' must never map to an action while typing"
            );
        }
    }

    #[test]
    fn ctrl_bindings_are_suspended_while_typing() {
        for code in [
            Key::Char('t'),
            Key::Char('w'),
            Key::Char('n'),
            Key::Char('p'),
        ] {
            assert_eq!(DefaultKeymap.resolve(&ctrl(code), Focus::Address), None);
        }
        assert_eq!(
            DefaultKeymap.resolve(&ctrl(Key::Char('t')), Focus::Content),
            Some(Action::NewTab)
        );
        assert_eq!(
            DefaultKeymap.resolve(&ctrl(Key::Char('w')), Focus::Content),
            Some(Action::CloseTab)
        );
        assert_eq!(
            DefaultKeymap.resolve(&ctrl(Key::Char('n')), Focus::Content),
            Some(Action::NextTab)
        );
        assert_eq!(
            DefaultKeymap.resolve(&ctrl(Key::Char('p')), Focus::Content),
            Some(Action::PrevTab)
        );
    }

    #[test]
    fn enter_submits_the_address_and_scrolls_content() {
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Enter), Focus::Address),
            Some(Action::SubmitAddress)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Enter), Focus::Tabs),
            Some(Action::FocusContent)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Enter), Focus::Content),
            Some(Action::ActivateLink)
        );
    }

    #[test]
    fn esc_returns_to_content_from_any_chrome_focus() {
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Esc), Focus::Address),
            Some(Action::FocusContent)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Esc), Focus::Tabs),
            Some(Action::FocusContent)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Esc), Focus::Content),
            None
        );
    }

    #[test]
    fn tab_navigates_links_and_cycles_tabs() {
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Tab), Focus::Content),
            Some(Action::NextLink)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::BackTab), Focus::Content),
            Some(Action::PrevLink)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Tab), Focus::Tabs),
            Some(Action::NextTab)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::BackTab), Focus::Tabs),
            Some(Action::PrevTab)
        );
    }

    #[test]
    fn scroll_and_jump_keys() {
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('j')), Focus::Content),
            Some(Action::ScrollDown)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('k')), Focus::Content),
            Some(Action::ScrollUp)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Down), Focus::Content),
            Some(Action::ScrollDown)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::PageUp), Focus::Content),
            Some(Action::ScrollUp)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Home), Focus::Content),
            Some(Action::ScrollTop)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::End), Focus::Content),
            Some(Action::ScrollBottom)
        );
    }

    #[test]
    fn focus_and_auxiliary_keys() {
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('/')), Focus::Content),
            Some(Action::FocusAddress)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('?')), Focus::Content),
            Some(Action::Help)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Other("F13".to_string())), Focus::Content),
            None
        );
    }
}

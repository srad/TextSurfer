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
    ScrollPageUp,
    ScrollPageDown,
    ScrollTop,
    ScrollBottom,
    ActivateLink,
    NextLink,
    PrevLink,
    Help,
    Back,
    Forward,
    Reload,
    Home,
    FocusMenu,
    MenuOpen(usize),
    MenuClose,
    MenuNext,
    MenuPrev,
    MenuLeft,
    MenuRight,
    MenuSelect,
    ThemeInfo,
}

pub trait Keymap {
    fn resolve(&self, event: &KeyEvent, focus: Focus) -> Option<Action>;
}

pub struct DefaultKeymap;

impl Keymap for DefaultKeymap {
    fn resolve(&self, event: &KeyEvent, focus: Focus) -> Option<Action> {
        let ctrl = event.modifiers.ctrl;
        let alt = event.modifiers.alt;
        if focus == Focus::Menu {
            return match event.code {
                Key::Esc => Some(Action::MenuClose),
                Key::Char(ch) => match ch.to_ascii_uppercase() {
                    'F' => Some(Action::MenuOpen(0)),
                    'N' => Some(Action::MenuOpen(1)),
                    'V' => Some(Action::MenuOpen(2)),
                    'H' => Some(Action::MenuOpen(3)),
                    _ => None,
                },
                Key::Down => Some(Action::MenuNext),
                Key::Up => Some(Action::MenuPrev),
                Key::Left => Some(Action::MenuLeft),
                Key::Right => Some(Action::MenuRight),
                Key::Enter => Some(Action::MenuSelect),
                Key::F(10) => Some(Action::MenuClose),
                _ => None,
            };
        }
        if focus == Focus::Address {
            return match event.code {
                Key::Enter => Some(Action::SubmitAddress),
                Key::Esc => Some(Action::FocusContent),
                Key::F(10) => Some(Action::FocusMenu),
                _ => None,
            };
        }
        let plain = !event.modifiers.shift && !ctrl && !alt;
        match event.code {
            Key::Char(ch) => {
                let lower = ch.to_ascii_lowercase();
                match (lower, ctrl, alt) {
                    ('q', false, false) => Some(Action::Quit),
                    ('?', false, false) => Some(Action::Help),
                    ('/', false, false) | ('a', false, false) => Some(Action::FocusAddress),
                    ('j', false, false) => Some(Action::ScrollDown),
                    ('k', false, false) => Some(Action::ScrollUp),
                    (' ', false, false) => Some(Action::ScrollPageDown),
                    ('b', false, false) => Some(Action::ScrollPageUp),
                    ('r', false, false) => Some(Action::Reload),
                    ('t', true, false) => Some(Action::NewTab),
                    ('w', true, false) => Some(Action::CloseTab),
                    ('n', true, false) => Some(Action::NextTab),
                    ('p', true, false) => Some(Action::PrevTab),
                    ('f', false, true) => Some(Action::MenuOpen(0)),
                    ('n', false, true) => Some(Action::MenuOpen(1)),
                    ('v', false, true) => Some(Action::MenuOpen(2)),
                    ('h', false, true) => Some(Action::MenuOpen(3)),
                    _ => None,
                }
            }
            Key::Enter => match focus {
                Focus::Address => Some(Action::SubmitAddress),
                Focus::Tabs => Some(Action::FocusContent),
                Focus::Content => Some(Action::ActivateLink),
                Focus::Menu => None,
            },
            Key::Esc => match focus {
                Focus::Address | Focus::Tabs => Some(Action::FocusContent),
                Focus::Content | Focus::Menu => None,
            },
            Key::Tab => match focus {
                Focus::Tabs => Some(Action::NextTab),
                _ => Some(Action::NextLink),
            },
            Key::BackTab => match focus {
                Focus::Tabs => Some(Action::PrevTab),
                _ => Some(Action::PrevLink),
            },
            Key::Down if plain => Some(Action::ScrollDown),
            Key::Up if plain => Some(Action::ScrollUp),
            Key::PageDown if plain => Some(Action::ScrollPageDown),
            Key::PageUp if plain => Some(Action::ScrollPageUp),
            Key::F(10) => Some(Action::FocusMenu),
            Key::Home => match (alt, plain, focus) {
                (true, false, Focus::Content)
                    if !event.modifiers.shift && !event.modifiers.ctrl =>
                {
                    Some(Action::Home)
                }
                (false, true, Focus::Content) => Some(Action::ScrollTop),
                _ => None,
            },
            Key::End if plain && focus == Focus::Content => Some(Action::ScrollBottom),
            Key::Left => match (alt, focus) {
                (true, Focus::Content) => Some(Action::Back),
                (false, Focus::Tabs) => Some(Action::PrevTab),
                _ => None,
            },
            Key::Right => match (alt, focus) {
                (true, Focus::Content) => Some(Action::Forward),
                (false, Focus::Tabs) => Some(Action::NextTab),
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

    fn alt(code: Key) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers {
                alt: true,
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
            Some(Action::ScrollPageUp)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::PageDown), Focus::Content),
            Some(Action::ScrollPageDown)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char(' ')), Focus::Content),
            Some(Action::ScrollPageDown),
            "space pages down like every pager"
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('b')), Focus::Content),
            Some(Action::ScrollPageUp)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char(' ')), Focus::Address),
            None,
            "typing a space in the address bar must never page"
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

    #[test]
    fn nav_buttons_are_alt_arrows_and_r() {
        assert_eq!(
            DefaultKeymap.resolve(&alt(Key::Left), Focus::Content),
            Some(Action::Back)
        );
        assert_eq!(
            DefaultKeymap.resolve(&alt(Key::Right), Focus::Content),
            Some(Action::Forward)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('r')), Focus::Content),
            Some(Action::Reload)
        );
        assert_eq!(
            DefaultKeymap.resolve(&alt(Key::Home), Focus::Content),
            Some(Action::Home)
        );
    }

    #[test]
    fn plain_home_keeps_scrolling_plain_arrows_stay_put() {
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Home), Focus::Content),
            Some(Action::ScrollTop)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Left), Focus::Content),
            None
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Right), Focus::Content),
            None
        );
    }

    #[test]
    fn nav_actions_are_suspended_while_typing() {
        assert_eq!(DefaultKeymap.resolve(&alt(Key::Left), Focus::Address), None);
        assert_eq!(
            DefaultKeymap.resolve(&alt(Key::Right), Focus::Address),
            None
        );
        assert_eq!(DefaultKeymap.resolve(&alt(Key::Home), Focus::Address), None);
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('r')), Focus::Address),
            None
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::End), Focus::Address),
            None
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::PageDown), Focus::Address),
            None
        );
    }

    #[test]
    fn modified_content_keys_do_not_trigger_plain_bindings() {
        assert_eq!(
            DefaultKeymap.resolve(&ctrl(Key::Char('j')), Focus::Content),
            None
        );
        assert_eq!(DefaultKeymap.resolve(&alt(Key::End), Focus::Content), None);
    }

    #[test]
    fn f10_opens_the_menu_from_any_focus() {
        for focus in [Focus::Content, Focus::Tabs, Focus::Address] {
            assert_eq!(
                DefaultKeymap.resolve(&press(Key::F(10)), focus),
                Some(Action::FocusMenu)
            );
        }
    }

    #[test]
    fn alt_letters_open_each_menu() {
        assert_eq!(
            DefaultKeymap.resolve(&alt(Key::Char('f')), Focus::Content),
            Some(Action::MenuOpen(0))
        );
        assert_eq!(
            DefaultKeymap.resolve(&alt(Key::Char('n')), Focus::Content),
            Some(Action::MenuOpen(1))
        );
        assert_eq!(
            DefaultKeymap.resolve(&alt(Key::Char('v')), Focus::Content),
            Some(Action::MenuOpen(2))
        );
        assert_eq!(
            DefaultKeymap.resolve(&alt(Key::Char('h')), Focus::Content),
            Some(Action::MenuOpen(3))
        );
    }

    #[test]
    fn menu_focus_navigates_and_never_leaks_printables() {
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Esc), Focus::Menu),
            Some(Action::MenuClose)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Down), Focus::Menu),
            Some(Action::MenuNext)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Up), Focus::Menu),
            Some(Action::MenuPrev)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Left), Focus::Menu),
            Some(Action::MenuLeft)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Right), Focus::Menu),
            Some(Action::MenuRight)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Enter), Focus::Menu),
            Some(Action::MenuSelect)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::F(10)), Focus::Menu),
            Some(Action::MenuClose)
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('f')), Focus::Menu),
            Some(Action::MenuOpen(0))
        );
        assert_eq!(
            DefaultKeymap.resolve(&press(Key::Char('q')), Focus::Menu),
            None,
            "printables must never leak an action while a menu is open"
        );
    }
}

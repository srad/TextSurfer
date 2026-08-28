#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Backspace,
    Delete,
    Enter,
    Esc,
    Tab,
    BackTab,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    F(u8),
    Other(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: Key,
    pub modifiers: KeyModifiers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Press(MouseButton),
    Release(MouseButton),
    Move,
    Wheel { rows: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub kind: MouseKind,
    pub at: crate::core::geom::Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizePhase {
    Preview,
    Settled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyEvent),
    Paste(String),
    Mouse(MouseEvent),
    Resize {
        size: crate::core::geom::Size,
        phase: ResizePhase,
    },
    PointerLeft,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputBatch {
    events: Vec<InputEvent>,
}

impl InputBatch {
    pub const fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn push(&mut self, event: InputEvent) {
        match (self.events.last_mut(), &event) {
            (
                Some(
                    previous @ InputEvent::Mouse(MouseEvent {
                        kind: MouseKind::Move,
                        ..
                    }),
                ),
                InputEvent::Mouse(MouseEvent {
                    kind: MouseKind::Move,
                    ..
                }),
            ) => *previous = event,
            (
                Some(InputEvent::Mouse(MouseEvent {
                    kind: MouseKind::Wheel { rows: previous },
                    at: previous_at,
                })),
                InputEvent::Mouse(MouseEvent {
                    kind: MouseKind::Wheel { rows },
                    at,
                }),
            ) if previous_at == at => *previous = previous.saturating_add(*rows),
            (
                Some(InputEvent::Resize {
                    size: previous,
                    phase: previous_phase,
                }),
                InputEvent::Resize { size, phase },
            ) if previous_phase == phase => *previous = *size,
            (Some(InputEvent::Paste(previous)), InputEvent::Paste(text)) => {
                previous.push_str(text);
            }
            _ => self.events.push(event),
        }
    }

    pub fn as_slice(&self) -> &[InputEvent] {
        &self.events
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn take_prefix(&mut self, limit: usize) -> Self {
        if self.events.len() <= limit {
            return std::mem::take(self);
        }
        let remaining = self.events.split_off(limit);
        Self {
            events: std::mem::replace(&mut self.events, remaining),
        }
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl From<InputEvent> for InputBatch {
    fn from(event: InputEvent) -> Self {
        let mut batch = Self::new();
        batch.push(event);
        batch
    }
}

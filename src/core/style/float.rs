#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssFloat {
    #[default]
    None,
    Left,
    Right,
}

impl CssFloat {
    pub const fn parse(keyword: &str) -> Option<Self> {
        Some(match keyword.as_bytes() {
            b"none" => Self::None,
            b"left" => Self::Left,
            b"right" => Self::Right,
            _ => return None,
        })
    }

    pub const fn is_floating(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Clear {
    #[default]
    None,
    Left,
    Right,
    Both,
}

impl Clear {
    pub const fn parse(keyword: &str) -> Option<Self> {
        Some(match keyword.as_bytes() {
            b"none" => Self::None,
            b"left" => Self::Left,
            b"right" => Self::Right,
            b"both" => Self::Both,
            _ => return None,
        })
    }
}

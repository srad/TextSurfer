#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListStyleType {
    None,
    #[default]
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

impl ListStyleType {
    pub const fn is_numeric(self) -> bool {
        !matches!(self, Self::None | Self::Disc | Self::Circle | Self::Square)
    }

    pub fn render(self, value: i64) -> String {
        match self {
            Self::None => String::new(),
            Self::Disc => "\u{2022}".to_string(),
            Self::Circle => "\u{25e6}".to_string(),
            Self::Square => "\u{25aa}".to_string(),
            Self::Decimal => value.to_string(),
            Self::DecimalLeadingZero => {
                if (0..10).contains(&value) {
                    format!("0{value}")
                } else {
                    value.to_string()
                }
            }
            Self::LowerAlpha => alphabetic(value, false),
            Self::UpperAlpha => alphabetic(value, true),
            Self::LowerRoman => roman(value, false),
            Self::UpperRoman => roman(value, true),
        }
    }
}

fn alphabetic(value: i64, upper: bool) -> String {
    if value < 1 {
        return value.to_string();
    }
    let base = if upper { b'A' } else { b'a' };
    let mut remaining = value;
    let mut letters = Vec::new();
    while remaining > 0 {
        let index = (remaining - 1) % 26;
        letters.push((base + index as u8) as char);
        remaining = (remaining - 1) / 26;
    }
    letters.iter().rev().collect()
}

fn roman(value: i64, upper: bool) -> String {
    const NUMERALS: [(i64, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    if !(1..4000).contains(&value) {
        return value.to_string();
    }
    let mut remaining = value;
    let mut out = String::new();
    for (amount, numeral) in NUMERALS {
        while remaining >= amount {
            out.push_str(numeral);
            remaining -= amount;
        }
    }
    if upper { out.to_uppercase() } else { out }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListStylePosition {
    #[default]
    Outside,
    Inside,
}

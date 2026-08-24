use crate::core::style::Rgb;
use crate::css::values::parse_color;

pub(super) fn non_negative_integer(source: &str) -> Option<usize> {
    let source = source.trim_start();
    let source = source.strip_prefix('+').unwrap_or(source);
    let digits: String = source.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    Some(digits.bytes().fold(0usize, |value, digit| {
        value
            .saturating_mul(10)
            .saturating_add((digit - b'0') as usize)
    }))
}

pub(super) fn dimension(source: &str, ignore_zero: bool) -> Option<String> {
    let source = source.trim_start();
    let mut end = 0;
    let mut decimal = false;
    for (index, ch) in source.char_indices() {
        if ch.is_ascii_digit() {
            end = index + ch.len_utf8();
        } else if ch == '.' && !decimal {
            decimal = true;
            end = index + 1;
        } else {
            break;
        }
    }
    let number = source.get(..end)?;
    let value = number.parse::<f64>().ok()?;
    if !value.is_finite() || value < 0.0 || ignore_zero && value == 0.0 {
        return None;
    }
    if source[end..].starts_with('%') {
        Some(format!("{value}%"))
    } else {
        Some(format!("{value}px"))
    }
}

pub(super) fn legacy_color(source: &str) -> Option<Rgb> {
    let source = source.trim();
    if source.is_empty() || source.eq_ignore_ascii_case("transparent") {
        return None;
    }
    if let Some(color) = parse_color(source)
        && color.alpha == 255
    {
        return Some(color.rgb);
    }
    let mut source: String = source.chars().take(128).collect();
    if source.starts_with('#') {
        source.remove(0);
    }
    let mut digits: String = source
        .chars()
        .flat_map(|ch| {
            if ch as u32 > 0xffff {
                ['0', '0'].into_iter().collect::<Vec<_>>()
            } else if ch.is_ascii_hexdigit() {
                vec![ch]
            } else {
                vec!['0']
            }
        })
        .collect();
    while !digits.len().is_multiple_of(3) {
        digits.push('0');
    }
    let component = digits.len() / 3;
    let mut channels = [0u8; 3];
    for (index, channel) in channels.iter_mut().enumerate() {
        let mut part = digits[index * component..(index + 1) * component].to_string();
        if part.len() > 8 {
            part.drain(..part.len() - 8);
        }
        while part.len() > 2 && part.starts_with('0') {
            part.remove(0);
        }
        if part.len() > 2 {
            part.truncate(2);
        }
        *channel = u8::from_str_radix(&part, 16).ok()?;
    }
    Some(Rgb::new(channels[0], channels[1], channels[2]))
}

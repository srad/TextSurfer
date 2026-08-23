use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

use super::decoder::post_process;

pub(super) const PRESCAN_WINDOW: usize = 1024;

pub(super) fn prescan_encoding(window: &[u8]) -> Option<&'static Encoding> {
    let end = window.len();
    let mut i = 0;
    while i < end {
        if window[i] != b'<' {
            i += 1;
            continue;
        }
        if is_meta_tag(window, i) {
            match scan_meta_attrs(window, i + 5) {
                (Some(enc), _) => return Some(post_process(enc)),
                (None, Some(next)) => {
                    i = next;
                    continue;
                }
                (None, None) => return xml_encoding(window),
            }
        }
        if window[i..].starts_with(b"<!--") {
            match find_sub(&window[i + 4..], b"-->") {
                Some(pos) => {
                    i += 4 + pos + 3;
                    continue;
                }
                None => return xml_encoding(window),
            }
        }
        match skip_tag(window, i + 1) {
            Some(next) => i = next,
            None => return xml_encoding(window),
        }
    }
    xml_encoding(window)
}

fn is_meta_tag(window: &[u8], i: usize) -> bool {
    if i + 5 > window.len() {
        return false;
    }
    window[i + 1..i + 5].eq_ignore_ascii_case(b"meta")
        && i + 5 < window.len()
        && (is_ws(window[i + 5]) || window[i + 5] == b'/')
}

fn scan_meta_attrs(window: &[u8], mut i: usize) -> (Option<&'static Encoding>, Option<usize>) {
    let end = window.len();
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut got_pragma = false;
    let mut need_pragma: Option<bool> = None;
    let mut charset: Option<Result<&'static Encoding, ()>> = None;
    loop {
        while i < end && (is_ws(window[i]) || window[i] == b'/') {
            i += 1;
        }
        if i >= end {
            return (None, None);
        }
        if window[i] == b'>' {
            break;
        }
        let name_start = i;
        while i < end && !is_ws(window[i]) && !matches!(window[i], b'=' | b'/' | b'>') {
            i += 1;
        }
        if i >= end {
            return (None, None);
        }
        let name = window[name_start..i].to_ascii_lowercase();
        let mut value: Option<&[u8]> = None;
        while i < end && is_ws(window[i]) {
            i += 1;
        }
        if i >= end {
            return (None, None);
        }
        if window[i] == b'=' {
            i += 1;
            while i < end && is_ws(window[i]) {
                i += 1;
            }
            if i >= end {
                return (None, None);
            }
            if window[i] == b'"' || window[i] == b'\'' {
                let quote = window[i];
                i += 1;
                let value_start = i;
                while i < end && window[i] != quote {
                    i += 1;
                }
                if i >= end {
                    return (None, None);
                }
                value = Some(&window[value_start..i]);
                i += 1;
            } else {
                let value_start = i;
                while i < end && !is_ws(window[i]) && window[i] != b'>' {
                    i += 1;
                }
                value = Some(&window[value_start..i]);
            }
        }
        if !seen.contains(&name) {
            seen.push(name.clone());
            if name == b"http-equiv"
                && value.is_some_and(|v| v.eq_ignore_ascii_case(b"content-type"))
            {
                got_pragma = true;
            } else if name == b"content" {
                let fresh = charset.is_none();
                if let Some(enc) = fresh
                    .then(|| value.and_then(encoding_from_meta_element))
                    .flatten()
                {
                    charset = Some(Ok(enc));
                    need_pragma = Some(true);
                }
            } else if name == b"charset" {
                charset = Some(match value {
                    Some(v) => Encoding::for_label(v).ok_or(()),
                    None => Err(()),
                });
                need_pragma = Some(false);
            }
        }
    }
    if need_pragma == Some(true) && !got_pragma {
        return (None, Some(i + 1));
    }
    match charset {
        Some(Ok(enc)) => (Some(enc), Some(i + 1)),
        _ => (None, Some(i + 1)),
    }
}

fn encoding_from_meta_element(s: &[u8]) -> Option<&'static Encoding> {
    let trimmed = trim_ws(s);
    let at = find_ci(trimmed, b"charset")?;
    let mut pos = at + 7;
    while pos < trimmed.len() && is_ws(trimmed[pos]) {
        pos += 1;
    }
    if pos >= trimmed.len() || trimmed[pos] != b'=' {
        return None;
    }
    pos += 1;
    while pos < trimmed.len() && is_ws(trimmed[pos]) {
        pos += 1;
    }
    if pos >= trimmed.len() {
        return None;
    }
    Encoding::for_label(&trimmed[pos..])
}

fn find_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
}

fn trim_ws(mut s: &[u8]) -> &[u8] {
    while s.first().is_some_and(|&b| is_ws(b)) {
        s = &s[1..];
    }
    while s.last().is_some_and(|&b| is_ws(b)) {
        s = &s[..s.len() - 1];
    }
    s
}

fn xml_encoding(window: &[u8]) -> Option<&'static Encoding> {
    if !window.starts_with(b"<?xml") {
        return None;
    }
    let decl_end = find_byte(window, b'>')?;
    let decl = &window[..decl_end];
    let mut pos = find_sub(decl, b"encoding")? + 8;
    while pos < decl.len() && decl[pos] <= 0x20 {
        pos += 1;
    }
    if pos >= decl.len() || decl[pos] != b'=' {
        return None;
    }
    pos += 1;
    while pos < decl.len() && decl[pos] <= 0x20 {
        pos += 1;
    }
    let quote = *decl.get(pos)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    pos += 1;
    let label_start = pos;
    while pos < decl.len() && decl[pos] != quote {
        pos += 1;
    }
    if pos >= decl.len() {
        return None;
    }
    let label = &decl[label_start..pos];
    if label.iter().any(|&b| b <= 0x20) {
        return None;
    }
    let enc = Encoding::for_label(label)?;
    if enc == UTF_16LE || enc == UTF_16BE {
        Some(UTF_8)
    } else {
        Some(enc)
    }
}

fn skip_tag(window: &[u8], mut i: usize) -> Option<usize> {
    let end = window.len();
    while i < end {
        match window[i] {
            b'"' => i = find_byte(&window[i + 1..], b'"').map(|pos| i + 1 + pos + 1)?,
            b'\'' => i = find_byte(&window[i + 1..], b'\'').map(|pos| i + 1 + pos + 1)?,
            b'>' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

fn find_sub(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn find_byte(haystack: &[u8], byte: u8) -> Option<usize> {
    haystack.iter().position(|&b| b == byte)
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0C | b'\r')
}

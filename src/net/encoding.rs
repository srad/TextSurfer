use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252, X_USER_DEFINED};
use mediatype::{MediaType, ReadParams, names::CHARSET};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub encoding: &'static Encoding,
}

const PRESCAN_WINDOW: usize = 1024;

pub fn decode(body: &[u8], header_charset: Option<&str>) -> Decoded {
    decode_inner(body, header_charset, true)
}

pub fn decode_text(body: &[u8], header_charset: Option<&str>) -> Decoded {
    decode_inner(body, header_charset, false)
}

fn decode_inner(body: &[u8], header_charset: Option<&str>, html_prescan: bool) -> Decoded {
    let (mut encoding, offset) = sniff_bom(body);
    if offset == 0 {
        let header = header_charset.and_then(|label| Encoding::for_label(label.trim().as_bytes()));
        encoding = header
            .or_else(|| {
                html_prescan
                    .then(|| prescan_encoding(&body[..body.len().min(PRESCAN_WINDOW)]))
                    .flatten()
            })
            .unwrap_or(UTF_8);
        encoding = post_process(encoding);
    }
    let (text, _) = encoding.decode_without_bom_handling(&body[offset..]);
    Decoded {
        text: text.into_owned(),
        encoding,
    }
}

pub fn charset_from_content_type(content_type: &str) -> Option<String> {
    MediaType::parse(content_type)
        .ok()?
        .get_param(CHARSET)
        .map(|value| value.unquoted_str().into_owned())
        .filter(|value| !value.is_empty())
}

fn post_process(encoding: &'static Encoding) -> &'static Encoding {
    if encoding == UTF_16LE || encoding == UTF_16BE {
        UTF_8
    } else if encoding == X_USER_DEFINED {
        WINDOWS_1252
    } else {
        encoding
    }
}

fn sniff_bom(body: &[u8]) -> (&'static Encoding, usize) {
    if body.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (UTF_8, 3)
    } else if body.starts_with(&[0xFF, 0xFE, 0x00, 0x00])
        || body.starts_with(&[0x00, 0x00, 0xFE, 0xFF])
    {
        (UTF_8, 0)
    } else if body.starts_with(&[0xFE, 0xFF]) {
        (UTF_16BE, 2)
    } else if body.starts_with(&[0xFF, 0xFE]) {
        (UTF_16LE, 2)
    } else {
        (UTF_8, 0)
    }
}

fn prescan_encoding(window: &[u8]) -> Option<&'static Encoding> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn latin1_cafe() -> Vec<u8> {
        b"caf\xe9".to_vec()
    }

    #[test]
    fn utf8_bom_is_stripped() {
        let d = decode(b"\xEF\xBB\xBFhello", None);
        assert_eq!(d.text, "hello");
        assert_eq!(d.encoding, UTF_8);
    }

    #[test]
    fn utf16le_bom() {
        let d = decode(b"\xFF\xFEh\0i\0", None);
        assert_eq!(d.text, "hi");
        assert_eq!(d.encoding, UTF_16LE);
    }

    #[test]
    fn utf16be_bom() {
        let d = decode(b"\xFE\xFF\0h\0i", None);
        assert_eq!(d.text, "hi");
        assert_eq!(d.encoding, UTF_16BE);
    }

    #[test]
    fn utf32_bom_falls_back_to_utf8() {
        let mut body = vec![0xFF, 0xFE, 0x00, 0x00];
        body.extend(b"caf\xe9");
        let d = decode(&body, None);
        assert_eq!(d.encoding, UTF_8);
        assert!(d.text.starts_with('\u{fffd}'));
    }

    #[test]
    fn header_charset_wins_over_meta() {
        let mut body = b"<head><meta charset=\"utf-8\"></head>".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, Some("iso-8859-1"));
        assert_eq!(d.text, "<head><meta charset=\"utf-8\"></head>caf\u{e9}");
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn header_label_is_trimmed_and_case_insensitive() {
        let mut body = b"<meta charset=windows-1252>".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, Some("  ISO-8859-1  "));
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn unknown_header_label_falls_through_to_meta() {
        let mut body = b"<meta charset=windows-1252>".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, Some("bogus"));
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
        assert_eq!(d.text, "<meta charset=windows-1252>caf\u{e9}");
    }

    #[test]
    fn unknown_header_label_and_no_meta_gives_utf8() {
        let d = decode(b"caf\xe9", Some("bogus"));
        assert_eq!(d.encoding, UTF_8);
        assert!(d.text.contains('\u{fffd}'));
    }

    #[test]
    fn utf16_label_without_bom_is_utf8() {
        let d = decode(b"hi", Some("utf-16le"));
        assert_eq!(d.encoding, UTF_8);
        assert_eq!(d.text, "hi");
    }

    #[test]
    fn no_encoding_hints_fall_back_to_utf8() {
        let d = decode(b"hi", None);
        assert_eq!(d.text, "hi");
        assert_eq!(d.encoding, UTF_8);
    }

    #[test]
    fn plain_text_does_not_treat_html_meta_as_an_encoding_hint() {
        let body = b"<meta charset=windows-1252>caf\xe9";
        let d = decode_text(body, None);
        assert_eq!(d.encoding, UTF_8);
        assert!(d.text.ends_with("caf\u{fffd}"));
    }

    #[test]
    fn meta_charset_attribute_is_sniffed() {
        let mut body = b"<html><meta charset=windows-1252>".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.text, "<html><meta charset=windows-1252>caf\u{e9}");
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn meta_uppercase_tag_and_name() {
        let mut body = b"<META CHARSET = \"windows-1252\">".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn meta_attribute_spacing_is_tolerated() {
        let mut body = b"<meta  charset = \"windows-1252\" >".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn meta_http_equiv_content_type() {
        let mut body: Vec<u8> =
            b"<head><meta http-equiv=\"Content-Type\" content=\"text/html; charset=shift_jis\"></head>"
                .to_vec();
        body.extend(b"\x82\xa0");
        let d = decode(&body, None);
        assert_eq!(d.encoding, encoding_rs::SHIFT_JIS);
        assert!(d.text.contains('\u{3042}'));
    }

    #[test]
    fn content_attr_found_in_either_order() {
        let mut body =
            b"<meta content=\"charset=windows-1252\" http-equiv=\"content-type\">".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
        assert!(d.text.contains('\u{e9}'));
    }

    #[test]
    fn content_without_http_equiv_is_ignored() {
        let mut body = b"<meta content=\"charset=windows-1252\">".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.encoding, UTF_8);
        assert!(d.text.contains('\u{fffd}'));
    }

    #[test]
    fn charset_failure_then_later_meta_is_sniffed() {
        let mut body = b"<meta charset=bogus><meta charset=windows-1252>".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn charset_attr_wins_over_content_attr() {
        let mut body = b"<meta http-equiv=\"content-type\" content=\"charset=utf-8\" charset=\"windows-1252\">".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn meta_after_other_tags_is_found() {
        let mut body = b"<html><head><title>x</title><meta charset=iso-8859-1></head>".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn meta_inside_comment_is_ignored() {
        let mut body = b"<!-- <meta charset=windows-1252> -->".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.encoding, UTF_8);
        assert!(d.text.contains('\u{fffd}'));
    }

    #[test]
    fn meta_beyond_1024_bytes_is_ignored() {
        let mut body = vec![b'a'; PRESCAN_WINDOW];
        body.extend(b"<meta charset=windows-1252>caf\xe9");
        let d = decode(&body, None);
        assert_eq!(d.encoding, UTF_8);
        assert!(d.text.contains('\u{fffd}'));
    }

    #[test]
    fn x_user_defined_is_mapped_to_windows_1252() {
        let mut body = b"<meta charset=x-user-defined>".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
    }

    #[test]
    fn meta_utf16_label_is_utf8() {
        let d = decode(b"<meta charset=utf-16le>hi", None);
        assert_eq!(d.encoding, UTF_8);
    }

    #[test]
    fn xml_declaration_encoding_is_the_fallback() {
        let mut body = b"<?xml version=\"1.0\" encoding=\"windows-1252\"?>".to_vec();
        body.extend(latin1_cafe());
        let d = decode(&body, None);
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
        assert!(d.text.contains('\u{e9}'));
    }

    #[test]
    fn charset_parameter_is_extracted_from_content_type() {
        assert_eq!(
            charset_from_content_type("text/html; charset=iso-8859-1"),
            Some("iso-8859-1".to_string())
        );
        assert_eq!(
            charset_from_content_type("text/html; charset=\"utf-8\""),
            Some("utf-8".to_string())
        );
        assert_eq!(
            charset_from_content_type("text/html; foo=bar; Charset=shift_jis"),
            Some("shift_jis".to_string())
        );
    }

    #[test]
    fn content_type_without_a_charset_parameter_yields_none() {
        assert_eq!(charset_from_content_type("text/html"), None);
        assert_eq!(charset_from_content_type("text/html; charset="), None);
        assert_eq!(charset_from_content_type(""), None);
    }

    #[test]
    fn header_charset_from_content_type_drives_decoding() {
        let body = b"<meta charset=windows-1252>caf\xe9".to_vec();
        let d = decode(
            &body,
            charset_from_content_type("text/html; charset=iso-8859-1").as_deref(),
        );
        assert_eq!(d.encoding, encoding_rs::WINDOWS_1252);
        assert!(d.text.ends_with('\u{e9}'));
    }
}

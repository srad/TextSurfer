use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252, X_USER_DEFINED};
use mediatype::{MediaType, ReadParams, names::CHARSET};

use super::Decoded;
use super::prescan::{PRESCAN_WINDOW, prescan_encoding};

pub fn decode(body: &[u8], header_charset: Option<&str>) -> Decoded {
    decode_inner(body, header_charset, true)
}

pub fn decode_text(body: &[u8], header_charset: Option<&str>) -> Decoded {
    decode_inner(body, header_charset, false)
}

pub fn html_encoding(body: &[u8], header_charset: Option<&str>) -> &'static Encoding {
    selected_encoding(body, header_charset, true).0
}

fn decode_inner(body: &[u8], header_charset: Option<&str>, html_prescan: bool) -> Decoded {
    let (encoding, offset) = selected_encoding(body, header_charset, html_prescan);
    let (text, _) = encoding.decode_without_bom_handling(&body[offset..]);
    Decoded {
        text: text.into_owned(),
        encoding,
    }
}

fn selected_encoding(
    body: &[u8],
    header_charset: Option<&str>,
    html_prescan: bool,
) -> (&'static Encoding, usize) {
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
    (encoding, offset)
}

pub fn charset_from_content_type(content_type: &str) -> Option<String> {
    MediaType::parse(content_type)
        .ok()?
        .get_param(CHARSET)
        .map(|value| value.unquoted_str().into_owned())
        .filter(|value| !value.is_empty())
}

pub(super) fn post_process(encoding: &'static Encoding) -> &'static Encoding {
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

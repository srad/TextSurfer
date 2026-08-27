use mediatype::MediaType;

use super::ResponseKind;

const RESOURCE_HEADER_LEN: usize = 1_445;
const HTML_SIGNATURES: &[&[u8]] = &[
    b"<!DOCTYPE HTML",
    b"<HTML",
    b"<HEAD",
    b"<SCRIPT",
    b"<IFRAME",
    b"<H1",
    b"<DIV",
    b"<FONT",
    b"<TABLE",
    b"<A",
    b"<STYLE",
    b"<TITLE",
    b"<B",
    b"<BODY",
    b"<BR",
    b"<P",
    b"<!--",
];

pub fn response_kind(content_type: Option<&str>, body: &[u8]) -> ResponseKind {
    if let Some(content_type) = content_type
        && let Ok(media_type) = MediaType::parse(content_type)
    {
        let essence = media_type.essence().to_string();
        return match essence.as_str() {
            "text/html" | "application/xhtml+xml" => ResponseKind::Html,
            "text/plain" => ResponseKind::PlainText,
            "unknown/unknown" | "application/unknown" | "*/*" => sniff_unknown(body),
            _ => ResponseKind::Unsupported(essence),
        };
    }
    sniff_unknown(body)
}

fn sniff_unknown(body: &[u8]) -> ResponseKind {
    let header = &body[..body.len().min(RESOURCE_HEADER_LEN)];
    let without_whitespace = header
        .iter()
        .position(|byte| !is_whitespace(*byte))
        .map_or(&[][..], |start| &header[start..]);

    if matches_html_signature(without_whitespace) {
        return ResponseKind::Html;
    }
    if without_whitespace.starts_with(b"<?xml") {
        return ResponseKind::Unsupported("text/xml".to_string());
    }
    if header.starts_with(b"%PDF-") {
        return ResponseKind::Unsupported("application/pdf".to_string());
    }
    if header.starts_with(b"%!PS-Adobe-") {
        return ResponseKind::Unsupported("application/postscript".to_string());
    }
    if header.starts_with(&[0xfe, 0xff])
        || header.starts_with(&[0xff, 0xfe])
        || header.starts_with(&[0xef, 0xbb, 0xbf])
    {
        return ResponseKind::PlainText;
    }
    if header.iter().copied().any(is_binary_data) {
        return ResponseKind::Unsupported("application/octet-stream".to_string());
    }
    ResponseKind::PlainText
}

fn matches_html_signature(input: &[u8]) -> bool {
    HTML_SIGNATURES.iter().any(|signature| {
        input.len() > signature.len()
            && input[..signature.len()].eq_ignore_ascii_case(signature)
            && matches!(input[signature.len()], b' ' | b'>')
    })
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

fn is_binary_data(byte: u8) -> bool {
    matches!(byte, 0x00..=0x08 | 0x0b | 0x0e..=0x1a | 0x1c..=0x1f)
}

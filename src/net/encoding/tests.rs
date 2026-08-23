use encoding_rs::{UTF_8, UTF_16BE, UTF_16LE};

use super::prescan::PRESCAN_WINDOW;
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
    let mut body = b"<meta content=\"charset=windows-1252\" http-equiv=\"content-type\">".to_vec();
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
    let mut body =
        b"<meta http-equiv=\"content-type\" content=\"charset=utf-8\" charset=\"windows-1252\">"
            .to_vec();
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

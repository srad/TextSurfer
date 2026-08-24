use super::legacy::{dimension, legacy_color, non_negative_integer};
use crate::core::style::Rgb;

#[test]
fn legacy_numbers_and_dimensions_follow_html_prefix_rules() {
    assert_eq!(non_negative_integer(" +12junk"), Some(12));
    assert_eq!(non_negative_integer("-1"), None);
    assert_eq!(dimension(" 12.5%junk", false).as_deref(), Some("12.5%"));
    assert_eq!(dimension("0", true), None);
}

#[test]
fn legacy_colours_accept_css_colours_and_repair_garbage() {
    assert_eq!(legacy_color("red"), Some(Rgb::new(255, 0, 0)));
    assert_eq!(legacy_color("#0f0"), Some(Rgb::new(0, 255, 0)));
    assert!(legacy_color("chucknorris").is_some());
    assert_eq!(legacy_color("#chucknorris"), legacy_color("chucknorris"));
    assert_eq!(
        legacy_color("123456789123456789123456789"),
        Some(Rgb::new(0x23, 0x23, 0x23))
    );
    assert_eq!(legacy_color("transparent"), None);
}

use super::{Attr, AttrNs};

pub fn attr_value<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|attr| attr.ns == AttrNs::None && attr.name.eq_ignore_ascii_case(name))
        .map(|attr| attr.value.as_str())
}

pub fn has_attr(attrs: &[Attr], name: &str) -> bool {
    attrs
        .iter()
        .any(|attr| attr.ns == AttrNs::None && attr.name.eq_ignore_ascii_case(name))
}

pub fn attr_number(attrs: &[Attr], name: &str) -> Option<i64> {
    attr_value(attrs, name)?.trim().parse::<i64>().ok()
}

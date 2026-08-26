use url::Url;

use crate::core::dom::{Attr, AttrNs, ElementNs, Node, SharedDocument};

const MAX_CONTENT_BYTES: usize = 4096;

pub(super) fn immediate_refresh(
    document: &SharedDocument,
    document_url: &Url,
    base: &Url,
) -> Option<Url> {
    let document = document.borrow();
    let mut stack: Vec<_> = document.roots().iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
            continue;
        };
        if *ns == ElementNs::Html && name == "meta" {
            let refresh = attr_value(attrs, "http-equiv")
                .is_some_and(|value| value.eq_ignore_ascii_case("refresh"));
            if refresh
                && let Some(content) =
                    attr_value(attrs, "content").filter(|value| !value.is_empty())
                && let Some((seconds, target)) = parse_content(content)
            {
                let url = if target.is_empty() {
                    Some(document_url.clone())
                } else {
                    base.join(target).ok()
                };
                if let Some(url) = url
                    && url.scheme() != "javascript"
                {
                    return (seconds == 0).then_some(url);
                }
            }
        }
        if !(*ns == ElementNs::Html && name == "template") {
            stack.extend(document.children(id).into_iter().rev());
        }
    }
    None
}

fn attr_value<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|attr| attr.ns == AttrNs::None && attr.name == name)
        .map(|attr| attr.value.as_str())
}

fn parse_content(input: &str) -> Option<(u64, &str)> {
    if input.len() > MAX_CONTENT_BYTES {
        return None;
    }
    let bytes = input.as_bytes();
    let mut position = skip_ascii_whitespace(bytes, 0);
    let time_start = position;
    while bytes.get(position).is_some_and(u8::is_ascii_digit) {
        position += 1;
    }
    let time = if position == time_start {
        if bytes.get(position) != Some(&b'.') {
            return None;
        }
        0
    } else {
        bytes[time_start..position]
            .iter()
            .fold(0_u64, |time, byte| {
                time.saturating_mul(10)
                    .saturating_add(u64::from(*byte - b'0'))
            })
    };
    while bytes
        .get(position)
        .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
    {
        position += 1;
    }
    if position < bytes.len() {
        let separator = bytes[position];
        if separator != b';' && separator != b',' && !separator.is_ascii_whitespace() {
            return None;
        }
        position = skip_ascii_whitespace(bytes, position);
        if bytes
            .get(position)
            .is_some_and(|byte| *byte == b';' || *byte == b',')
        {
            position += 1;
        }
        position = skip_ascii_whitespace(bytes, position);
    }
    if position == bytes.len() {
        return Some((time, ""));
    }
    let original = position;
    if bytes
        .get(position..position.saturating_add(3))
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"url"))
    {
        position += 3;
        position = skip_ascii_whitespace(bytes, position);
        if bytes.get(position) == Some(&b'=') {
            position += 1;
            position = skip_ascii_whitespace(bytes, position);
        } else {
            position = original;
        }
    }
    let quote = bytes
        .get(position)
        .copied()
        .filter(|byte| *byte == b'\'' || *byte == b'"');
    if quote.is_some() {
        position += 1;
    }
    let mut end = bytes.len();
    if let Some(quote) = quote
        && let Some(offset) = bytes[position..].iter().position(|byte| *byte == quote)
    {
        end = position + offset;
    }
    Some((time, &input[position..end]))
}

fn skip_ascii_whitespace(bytes: &[u8], mut position: usize) -> usize {
    while bytes.get(position).is_some_and(u8::is_ascii_whitespace) {
        position += 1;
    }
    position
}

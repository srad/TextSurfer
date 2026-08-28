use thiserror::Error;
use url::Url;

use super::{
    ControlKind, FormState, checkedness, control_kind, descendant_text, is_disabled, options,
    selected_index, text_value,
};
use crate::core::dom::{Document, ElementNs, Node, NodeId, attr_value};

pub const MAX_FORM_URL_BYTES: usize = 64 * 1024;
pub const MAX_FORM_BODY_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormSubmission {
    Get(Url),
    PostUrlEncoded { url: Url, body: Vec<u8> },
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FormError {
    #[error("the control has no form owner")]
    NoFormOwner,
    #[error("the form action is not a valid URL")]
    InvalidAction,
    #[error("form encoding is not supported: {0}")]
    UnsupportedEncoding(String),
    #[error("form URL exceeds {limit} bytes")]
    UrlTooLarge { limit: usize },
    #[error("form body exceeds {limit} bytes")]
    BodyTooLarge { limit: usize },
}

pub fn form_owner(document: &Document, control: NodeId) -> Option<NodeId> {
    let Some(Node::Element { attrs, .. }) = document.node(control) else {
        return None;
    };
    if let Some(id) = attr_value(attrs, "form") {
        let candidate = document.element_by_id(id)?;
        return is_html_element(document, candidate, "form").then_some(candidate);
    }
    let mut parent = document.parent(control);
    while let Some(node) = parent {
        if is_html_element(document, node, "form") {
            return Some(node);
        }
        parent = document.parent(node);
    }
    None
}

pub fn build_submission(
    document: &Document,
    forms: &FormState,
    submitter: Option<NodeId>,
    document_url: &Url,
    base_url: &Url,
) -> Result<FormSubmission, FormError> {
    let form = submitter
        .and_then(|node| {
            is_html_element(document, node, "form")
                .then_some(node)
                .or_else(|| form_owner(document, node))
        })
        .ok_or(FormError::NoFormOwner)?;
    build_submission_for_form(document, forms, form, submitter, document_url, base_url)
}

pub fn build_submission_for_form(
    document: &Document,
    forms: &FormState,
    form: NodeId,
    submitter: Option<NodeId>,
    document_url: &Url,
    base_url: &Url,
) -> Result<FormSubmission, FormError> {
    let attrs = match document.node(form) {
        Some(Node::Element { attrs, .. }) => attrs,
        _ => return Err(FormError::NoFormOwner),
    };
    let action = match attr_value(attrs, "action") {
        None | Some("") => document_url.clone(),
        Some(value) => base_url.join(value).map_err(|_| FormError::InvalidAction)?,
    };
    let entries = successful_entries(document, forms, form, submitter);
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in entries {
        serializer.append_pair(&normalize_newlines(&name), &normalize_newlines(&value));
    }
    let encoded = serializer.finish();
    let method = attr_value(attrs, "method").unwrap_or("get");
    if !method.eq_ignore_ascii_case("post") {
        let mut url = action;
        url.set_query(Some(&encoded));
        if url.as_str().len() > MAX_FORM_URL_BYTES {
            return Err(FormError::UrlTooLarge {
                limit: MAX_FORM_URL_BYTES,
            });
        }
        return Ok(FormSubmission::Get(url));
    }
    let enctype = attr_value(attrs, "enctype")
        .unwrap_or("application/x-www-form-urlencoded")
        .trim();
    if !enctype.eq_ignore_ascii_case("application/x-www-form-urlencoded") && !enctype.is_empty() {
        return Err(FormError::UnsupportedEncoding(enctype.to_string()));
    }
    let body = encoded.into_bytes();
    if body.len() > MAX_FORM_BODY_BYTES {
        return Err(FormError::BodyTooLarge {
            limit: MAX_FORM_BODY_BYTES,
        });
    }
    Ok(FormSubmission::PostUrlEncoded { url: action, body })
}

fn successful_entries(
    document: &Document,
    forms: &FormState,
    form: NodeId,
    submitter: Option<NodeId>,
) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for node in tree_order(document) {
        let Some(kind) = control_kind(document, node) else {
            continue;
        };
        if form_owner(document, node) != Some(form) || is_disabled(document, node) {
            continue;
        }
        let Some(Node::Element { attrs, .. }) = document.node(node) else {
            continue;
        };
        let Some(name) = attr_value(attrs, "name") else {
            continue;
        };
        if has_html_ancestor(document, node, "datalist")
            || kind == ControlKind::Select && crate::core::dom::has_attr(attrs, "multiple")
        {
            continue;
        }
        match kind {
            ControlKind::Text | ControlKind::Password | ControlKind::TextArea => {
                entries.push((name.to_string(), text_value(document, node, forms)));
            }
            ControlKind::Hidden => {
                let value = if name.eq_ignore_ascii_case("_charset_") {
                    "UTF-8".to_string()
                } else {
                    attr_value(attrs, "value").unwrap_or("").to_string()
                };
                entries.push((name.to_string(), value));
            }
            ControlKind::Checkbox | ControlKind::Radio if checkedness(document, node, forms) => {
                entries.push((
                    name.to_string(),
                    attr_value(attrs, "value").unwrap_or("on").to_string(),
                ));
            }
            ControlKind::Select => {
                if let Some(index) = selected_index(document, node, forms)
                    && let Some(option) = options(document, node).get(index).copied()
                    && !is_disabled(document, option)
                    && let Some(Node::Element { attrs, .. }) = document.node(option)
                {
                    let value = attr_value(attrs, "value")
                        .map(str::to_string)
                        .unwrap_or_else(|| descendant_text(document, option));
                    entries.push((name.to_string(), value));
                }
            }
            ControlKind::Submit if submitter == Some(node) => {
                entries.push((
                    name.to_string(),
                    attr_value(attrs, "value").unwrap_or("").to_string(),
                ));
            }
            _ => {}
        }
    }
    entries
}

fn tree_order(document: &Document) -> Vec<NodeId> {
    let mut result = Vec::new();
    let mut stack: Vec<NodeId> = document.roots().iter().rev().copied().collect();
    while let Some(node) = stack.pop() {
        result.push(node);
        stack.extend(document.children(node).into_iter().rev());
    }
    result
}

fn normalize_newlines(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                output.push_str("\r\n");
            }
            '\n' => output.push_str("\r\n"),
            other => output.push(other),
        }
    }
    output
}

fn is_html_element(document: &Document, id: NodeId, wanted: &str) -> bool {
    matches!(document.node(id), Some(Node::Element { name, ns, .. })
        if *ns == ElementNs::Html && name == wanted)
}

fn has_html_ancestor(document: &Document, id: NodeId, wanted: &str) -> bool {
    let mut parent = document.parent(id);
    while let Some(node) = parent {
        if is_html_element(document, node, wanted) {
            return true;
        }
        parent = document.parent(node);
    }
    false
}

//! The form-control model: what kind of control an element is, and what value it currently holds.
//!
//! `FormState` stores **only what the user changed**. Everything else is derived from the content
//! attributes on demand, so an empty state is exactly the document's initial state — which is what
//! `--dump`, a freshly loaded page and every test that does not simulate input all want. A reset is
//! `clear()`, and the per-load pivot needs no cooperation: a new document arrives with a new state.
//!
//! Rendering the control lives in `layout::replaced`; this module answers only *what* is in it.

use std::collections::HashMap;
use std::sync::LazyLock;

use super::dom::{Attr, Document, ElementNs, Node, NodeId, attr_value, has_attr};

mod interaction;
mod submission;

#[cfg(test)]
mod tests;

pub use interaction::FormMutationError;
pub use submission::{
    FormError, FormSubmission, build_submission, build_submission_for_form, form_owner,
};

/// How many cells a text entry control occupies when it declares no `size`.
pub const DEFAULT_INPUT_SIZE: usize = 20;
/// The `cols`/`rows` a `<textarea>` occupies when it declares neither.
pub const DEFAULT_TEXTAREA_COLS: usize = 20;
pub const DEFAULT_TEXTAREA_ROWS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    /// Every `<input>` state that renders as an editable one-line field.
    Text,
    /// `<input type=password>` — an editable field whose value is never painted in clear text.
    Password,
    Checkbox,
    Radio,
    /// `<input type=hidden>` — submitted, never rendered.
    Hidden,
    Submit,
    Reset,
    /// A button that does nothing on its own: `<input type=button>`, `<button type=button>`.
    Button,
    TextArea,
    Select,
    /// A control we can neither render nor submit faithfully: file, image, color, range, dates.
    /// It renders as a labelled stub and is never a successful control.
    Unsupported,
}

impl ControlKind {
    /// Whether the user edits this control by typing into it.
    pub fn is_text_entry(self) -> bool {
        matches!(self, Self::Text | Self::Password | Self::TextArea)
    }

    pub fn is_button(self) -> bool {
        matches!(self, Self::Submit | Self::Reset | Self::Button)
    }

    pub fn is_toggle(self) -> bool {
        matches!(self, Self::Checkbox | Self::Radio)
    }
}

/// What the user changed about one control. Absent from `FormState` means "still as authored".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlValue {
    Text(String),
    Checked(bool),
    /// Index into the control's `<option>` elements in tree order.
    Selected(usize),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormState {
    overrides: HashMap<NodeId, ControlValue>,
}

impl FormState {
    /// The shared "nothing has been typed into this page" state.
    ///
    /// Callers need a `&'static FormState` — the layout and cascade entry points take one by
    /// reference and most callers have no state of their own — and an empty map is immutable, so
    /// one shared instance serves every `--dump`, every test and every page before first input.
    pub fn empty() -> &'static Self {
        static EMPTY: LazyLock<FormState> = LazyLock::new(FormState::default);
        &EMPTY
    }

    pub fn get(&self, node: NodeId) -> Option<&ControlValue> {
        self.overrides.get(&node)
    }

    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }

    pub(crate) fn set(&mut self, node: NodeId, value: ControlValue) {
        self.overrides.insert(node, value);
    }

    /// Drop every override, so every control returns to the value its attributes declare.
    pub fn clear(&mut self) {
        self.overrides.clear();
    }
}

/// The control kind of `id`, or `None` if it is not a form control.
pub fn control_kind(document: &Document, id: NodeId) -> Option<ControlKind> {
    let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
        return None;
    };
    if *ns != ElementNs::Html {
        return None;
    }
    Some(match name.as_str() {
        "input" => input_kind(attr_value(attrs, "type")),
        "textarea" => ControlKind::TextArea,
        "select" => ControlKind::Select,
        "button" => match attr_value(attrs, "type") {
            Some(value) if value.eq_ignore_ascii_case("reset") => ControlKind::Reset,
            Some(value) if value.eq_ignore_ascii_case("button") => ControlKind::Button,
            // Missing and unknown `type` are both the submit state, per the HTML button state table.
            _ => ControlKind::Submit,
        },
        _ => return None,
    })
}

/// The `<input>` type state table. A missing or unrecognised `type` is the Text state, which is why
/// `<input>` with no attributes at all is an ordinary field.
fn input_kind(type_attr: Option<&str>) -> ControlKind {
    let Some(value) = type_attr else {
        return ControlKind::Text;
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "password" => ControlKind::Password,
        "checkbox" => ControlKind::Checkbox,
        "radio" => ControlKind::Radio,
        "hidden" => ControlKind::Hidden,
        "submit" => ControlKind::Submit,
        "reset" => ControlKind::Reset,
        "button" => ControlKind::Button,
        "text" | "search" | "url" | "tel" | "email" | "number" => ControlKind::Text,
        // file, image, color, range, date, time, month, week, datetime-local
        _ if !value.trim().is_empty() => ControlKind::Unsupported,
        _ => ControlKind::Text,
    }
}

/// The text a text-entry control currently holds.
pub fn text_value(document: &Document, id: NodeId, forms: &FormState) -> String {
    if let Some(ControlValue::Text(value)) = forms.get(id) {
        return value.clone();
    }
    match document.node(id) {
        // html5ever already drops the newline immediately after the start tag; the child text is
        // the default value verbatim otherwise.
        Some(Node::Element { name, .. }) if name == "textarea" => descendant_text(document, id),
        Some(Node::Element { attrs, .. }) => attr_value(attrs, "value").unwrap_or("").to_string(),
        _ => String::new(),
    }
}

/// What a text-entry control *shows*: its value, or its `placeholder` when the value is empty.
///
/// Deliberately separate from [`text_value`], which is what gets submitted. A placeholder is a
/// prompt, not a value, and must never reach the wire — keeping the two functions apart is what
/// makes that impossible rather than merely intended. The `bool` is "this is placeholder text", so
/// the caller can dim it.
pub fn display_text(document: &Document, id: NodeId, forms: &FormState) -> (String, bool) {
    let value = text_value(document, id, forms);
    if !value.is_empty() {
        return (value, false);
    }
    let placeholder = match document.node(id) {
        Some(Node::Element { attrs, .. }) => attr_value(attrs, "placeholder").unwrap_or(""),
        _ => "",
    };
    (placeholder.to_string(), !placeholder.is_empty())
}

/// Whether a checkbox, radio or option is currently checked.
pub fn checkedness(document: &Document, id: NodeId, forms: &FormState) -> bool {
    if let Some(ControlValue::Checked(value)) = forms.get(id) {
        return *value;
    }
    // An option's checkedness is its select's selection, not its own attribute: clicking a
    // different option must un-check this one without writing an override for it.
    if is_html_element(document, id, "option")
        && let Some(select) = owning_select(document, id)
        && let Some(index) = selected_index(document, select, forms)
    {
        return options(document, select).get(index).copied() == Some(id);
    }
    match document.node(id) {
        Some(Node::Element { attrs, .. }) => has_attr(attrs, "checked"),
        _ => false,
    }
}

fn is_html_element(document: &Document, id: NodeId, wanted: &str) -> bool {
    matches!(document.node(id), Some(Node::Element { name, ns, .. })
        if *ns == ElementNs::Html && name == wanted)
}

/// The `<select>` an `<option>` belongs to, through an `<optgroup>` if there is one.
pub fn owning_select(document: &Document, option: NodeId) -> Option<NodeId> {
    let parent = document.parent(option)?;
    if is_html_element(document, parent, "select") {
        return Some(parent);
    }
    if is_html_element(document, parent, "optgroup") {
        let grandparent = document.parent(parent)?;
        return is_html_element(document, grandparent, "select").then_some(grandparent);
    }
    None
}

/// The `<option>` elements of a `<select>`, in tree order, `<optgroup>`s flattened.
pub fn options(document: &Document, select: NodeId) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut stack: Vec<NodeId> = document.children(select).into_iter().rev().collect();
    while let Some(node) = stack.pop() {
        match document.node(node) {
            Some(Node::Element { name, ns, .. }) if *ns == ElementNs::Html && name == "option" => {
                found.push(node);
            }
            Some(Node::Element { name, ns, .. })
                if *ns == ElementNs::Html && name == "optgroup" =>
            {
                stack.extend(document.children(node).into_iter().rev());
            }
            _ => {}
        }
    }
    found
}

/// Which option a `<select>` shows: the override, else the last one declaring `selected`, else the
/// first. `None` when the element is not a select or has no options.
pub fn selected_index(document: &Document, select: NodeId, forms: &FormState) -> Option<usize> {
    let options = options(document, select);
    if options.is_empty() {
        return None;
    }
    if let Some(ControlValue::Selected(index)) = forms.get(select) {
        return Some((*index).min(options.len() - 1));
    }
    let authored = options.iter().rposition(|option| {
        matches!(document.node(*option), Some(Node::Element { attrs, .. }) if has_attr(attrs, "selected"))
    });
    Some(authored.unwrap_or(0))
}

/// The declared display width of a control, in cells.
pub fn field_width(attrs: &[Attr], kind: ControlKind) -> usize {
    let attribute = if kind == ControlKind::TextArea {
        "cols"
    } else {
        "size"
    };
    let default = if kind == ControlKind::TextArea {
        DEFAULT_TEXTAREA_COLS
    } else {
        DEFAULT_INPUT_SIZE
    };
    attr_value(attrs, attribute)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
        .min(u16::MAX as usize)
}

/// The declared row count of a `<textarea>`.
pub fn field_rows(attrs: &[Attr]) -> usize {
    attr_value(attrs, "rows")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TEXTAREA_ROWS)
        .min(u16::MAX as usize)
}

/// `maxlength`, in graphemes, if the control declares a usable one.
pub fn max_length(attrs: &[Attr]) -> Option<usize> {
    attr_value(attrs, "maxlength").and_then(|value| value.trim().parse::<usize>().ok())
}

/// Whether the control is disabled, honouring a disabled `<fieldset>` ancestor. A control inside
/// that fieldset's *first* `<legend>` escapes, which is the one carve-out the HTML spec grants.
pub fn is_disabled(document: &Document, id: NodeId) -> bool {
    if let Some(Node::Element { attrs, .. }) = document.node(id)
        && has_attr(attrs, "disabled")
    {
        return true;
    }
    let mut child = id;
    let mut parent = document.parent(id);
    while let Some(node) = parent {
        if let Some(Node::Element { name, ns, attrs }) = document.node(node)
            && *ns == ElementNs::Html
            && matches!(name.as_str(), "fieldset" | "optgroup")
            && has_attr(attrs, "disabled")
            && !(name == "fieldset" && in_first_legend(document, node, child))
        {
            return true;
        }
        child = node;
        parent = document.parent(node);
    }
    false
}

/// Whether `child` is the fieldset's first `<legend>` child.
fn in_first_legend(document: &Document, fieldset: NodeId, child: NodeId) -> bool {
    document
        .children(fieldset)
        .into_iter()
        .find(|node| {
            matches!(document.node(*node), Some(Node::Element { name, ns, .. })
                if *ns == ElementNs::Html && name == "legend")
        })
        .is_some_and(|legend| legend == child)
}

/// The text a `<button>` or `<option>` shows, gathered from its descendants.
pub fn descendant_text(document: &Document, id: NodeId) -> String {
    let mut text = String::new();
    let mut stack: Vec<NodeId> = document.children(id).into_iter().rev().collect();
    while let Some(node) = stack.pop() {
        match document.node(node) {
            Some(Node::Text { data }) => text.push_str(data),
            Some(Node::Element { .. }) => {
                stack.extend(document.children(node).into_iter().rev());
            }
            _ => {}
        }
    }
    text
}

//! Replaced elements the terminal cannot render, reduced to a generated text stand-in.
//!
//! One resolver serves every caller so the paths cannot drift apart. [`replaced_box`] answers
//! *what* the element shows and how big it wants to be; the caller decides how big it actually got
//! and asks for [`ReplacedBox::rows`] at that size. The element's own children are never laid out —
//! they are either absent (`<img>`, `<input>`) or already folded into the text (`<button>`,
//! `<select>`), which is why every result says so explicitly.
//!
//! Two callers want the *intrinsic* rendering rather than a box-sized one — the normal-flow and
//! table-cell inline paths — and they use [`replaced_content`], which is the same resolver asked
//! for its own natural width.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::core::dom::{Attr, AttrNs, Document, ElementNs, Node, NodeId, attr_value};
use crate::core::form::{
    ControlKind, FormState, checkedness, control_kind, descendant_text, display_text, field_rows,
    field_width, options, selected_index,
};
use crate::core::style::TextAlign;

/// How a stand-in occupies the width it is given.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fill {
    /// A text-entry field. Blank-padded to the full width, and trimmed from the left when the value
    /// is longer, so the end a caret would sit at stays on screen.
    Field,
    /// A label. Placed by `text-align` and never padded beyond its own width.
    Label,
}

/// What a replaced element shows, independent of how big its box turns out to be.
pub(super) struct ReplacedBox {
    /// The value or label, without brackets and without padding.
    text: String,
    fill: Fill,
    /// Whether `[` and `]` delimit the rendering. A control with a border of its own is already
    /// delimited by it; one without needs the brackets to be visible at all.
    bracketed: bool,
    /// The size the element asks for when CSS does not say, in cells.
    pub(super) intrinsic_cols: usize,
    pub(super) intrinsic_rows: usize,
    /// The text is a `placeholder`, not a value, so it should paint dimmed.
    pub(super) placeholder: bool,
    /// The stand-in is ordinary text that wraps inside its box, rather than generated content
    /// sized to it. Only an `<img>`'s `alt` sets this: it is the author's prose, and clipping it
    /// to the image's box would lose words a control's blank padding never carries.
    pub(super) wraps: bool,
    /// The blanks and newlines are structural — the control's own cells — so they must survive
    /// whatever white-space the surrounding flow uses. An `<img>`'s `alt` is ordinary text and
    /// does not set this.
    pub(super) preformatted: bool,
}

impl ReplacedBox {
    /// The rendering at exactly `width` × `height` cells. Rows past the element's own are blank:
    /// a box taller than the control stretches, the control does not.
    pub(super) fn rows(&self, width: usize, height: usize, align: TextAlign) -> Vec<String> {
        let inner = if self.bracketed {
            width.saturating_sub(2)
        } else {
            width
        };
        let mut lines: Vec<&str> = self.text.split('\n').collect();
        lines.resize(height, "");
        lines
            .into_iter()
            .take(height)
            .map(|line| {
                let body = match self.fill {
                    Fill::Field => field_window(line, inner),
                    Fill::Label => place(line, inner, align),
                };
                if self.bracketed {
                    format!("[{body}]")
                } else {
                    body
                }
            })
            .collect()
    }

    /// The rendering at its own natural size, for the inline path.
    fn intrinsic_text(&self) -> String {
        self.rows(self.intrinsic_cols, self.intrinsic_rows, TextAlign::Start)
            .join("\n")
    }
}

/// The text that stands in for a replaced element on the inline path.
pub(super) struct Replaced {
    pub(super) text: String,
    pub(super) preformatted: bool,
}

/// The stand-in for `id` at its intrinsic size, or `None` when the element is not replaced or
/// renders nothing. Inline boxes never paint a border, so this always brackets.
pub(super) fn replaced_content(
    document: &Document,
    id: NodeId,
    forms: &FormState,
) -> Option<Replaced> {
    let box_ = replaced_box(document, id, forms, false)?;
    Some(Replaced {
        text: box_.intrinsic_text(),
        preformatted: box_.preformatted,
    })
}

/// What `id` shows, or `None` when it is not a replaced element or renders nothing.
///
/// `bordered` is whether the box will paint a border of its own; it decides the brackets.
pub(super) fn replaced_box(
    document: &Document,
    id: NodeId,
    forms: &FormState,
    bordered: bool,
) -> Option<ReplacedBox> {
    let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
        return None;
    };
    if *ns != ElementNs::Html {
        return None;
    }
    if name == "img" {
        // `[alt]` carries its own brackets by convention and is ordinary text, not a field.
        return image_fallback(attrs).map(|text| ReplacedBox {
            intrinsic_cols: UnicodeWidthStr::width(text.as_str()),
            intrinsic_rows: 1,
            text,
            fill: Fill::Label,
            bracketed: false,
            placeholder: false,
            preformatted: false,
            wraps: true,
        });
    }
    let kind = control_kind(document, id)?;
    control_box(document, id, attrs, kind, forms, bordered)
}

fn image_fallback(attrs: &[Attr]) -> Option<String> {
    let alt = attrs
        .iter()
        .find(|attr| attr.ns == AttrNs::None && attr.name == "alt");
    match alt {
        Some(attr) => {
            let value = attr.value.trim();
            (!value.is_empty()).then(|| format!("[{value}]"))
        }
        None => Some("[img]".to_string()),
    }
}

fn control_box(
    document: &Document,
    id: NodeId,
    attrs: &[Attr],
    kind: ControlKind,
    forms: &FormState,
    bordered: bool,
) -> Option<ReplacedBox> {
    /// A one-line label whose glyphs are the whole rendering: it never takes brackets and never
    /// pads. `[X]`, `(*)` and the like are drawn shapes, not delimited fields.
    fn glyph(text: &str) -> Option<ReplacedBox> {
        Some(ReplacedBox {
            intrinsic_cols: UnicodeWidthStr::width(text),
            intrinsic_rows: 1,
            text: text.to_string(),
            fill: Fill::Label,
            bracketed: false,
            placeholder: false,
            preformatted: true,
            wraps: false,
        })
    }
    fn label(text: String, bordered: bool) -> Option<ReplacedBox> {
        let width = UnicodeWidthStr::width(text.as_str());
        Some(ReplacedBox {
            intrinsic_cols: width + if bordered { 0 } else { 2 },
            intrinsic_rows: 1,
            text,
            fill: Fill::Label,
            bracketed: !bordered,
            placeholder: false,
            preformatted: true,
            wraps: false,
        })
    }
    match kind {
        // Submitted, never seen. The UA sheet also gives it `display: none`; this is the second
        // lock, so an author's `input[type=hidden] { display: block }` cannot reveal it.
        ControlKind::Hidden => None,
        ControlKind::Checkbox => glyph(if checkedness(document, id, forms) {
            "[X]"
        } else {
            "[ ]"
        }),
        ControlKind::Radio => glyph(if checkedness(document, id, forms) {
            "(*)"
        } else {
            "( )"
        }),
        ControlKind::Submit | ControlKind::Reset | ControlKind::Button => {
            label(button_label(document, id, attrs, kind), bordered)
        }
        ControlKind::Select => {
            let selected = selected_index(document, id, forms)
                .and_then(|index| options(document, id).get(index).copied())
                .map(|option| descendant_text(document, option))
                .unwrap_or_default();
            label(format!("{} \u{25be}", collapse(&selected)), bordered)
        }
        // A control whose shape we can draw but whose behaviour we cannot supply. The trailing `?`
        // says "this is a control, and not one of the ones that work" without spending prose on it.
        ControlKind::Unsupported => {
            let name = attr_value(attrs, "type")
                .unwrap_or("")
                .trim()
                .to_lowercase();
            label(format!("{name}?"), bordered)
        }
        ControlKind::TextArea => {
            let (text, placeholder) = display_text(document, id, forms);
            Some(ReplacedBox {
                text,
                fill: Fill::Field,
                // A textarea's extent is its multi-row field; brackets would only clutter it.
                bracketed: false,
                intrinsic_cols: field_width(attrs, kind),
                intrinsic_rows: field_rows(attrs),
                placeholder,
                preformatted: true,
                wraps: false,
            })
        }
        ControlKind::Text | ControlKind::Password => {
            let (value, placeholder) = display_text(document, id, forms);
            let text = if kind == ControlKind::Password && !placeholder {
                "*".repeat(value.graphemes(true).count())
            } else {
                collapse(&value)
            };
            Some(ReplacedBox {
                text,
                fill: Fill::Field,
                bracketed: !bordered,
                intrinsic_cols: field_width(attrs, kind) + if bordered { 0 } else { 2 },
                intrinsic_rows: 1,
                placeholder,
                preformatted: true,
                wraps: false,
            })
        }
    }
}

/// A button's label: its `value` attribute, else its own text, else the state's default. An empty
/// `value=""` is deliberate and stays empty; only an absent one falls back.
fn button_label(document: &Document, id: NodeId, attrs: &[Attr], kind: ControlKind) -> String {
    if let Some(value) = attr_value(attrs, "value") {
        return collapse(value);
    }
    let own = collapse(&descendant_text(document, id));
    if !own.is_empty() {
        return own;
    }
    match kind {
        ControlKind::Submit => "Submit".to_string(),
        ControlKind::Reset => "Reset".to_string(),
        _ => String::new(),
    }
}

/// Exactly `width` cells of `value`, padded with spaces.
///
/// The padding is blank rather than a filler glyph on purpose: the UA sheet paints the field in
/// reverse video, so its extent is carried by colour. A filler glyph would double that cue and,
/// worse, make `[hi____]` indistinguishable from a field whose value really is `hi____`.
///
/// Wider values are trimmed at a grapheme boundary from the left, so the end a caret would sit at
/// stays on screen; the caret-following window arrives with editing.
fn field_window(value: &str, width: usize) -> String {
    let value_width = UnicodeWidthStr::width(value);
    if value_width <= width {
        return format!("{value}{}", " ".repeat(width - value_width));
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut used = 0;
    for grapheme in value.graphemes(true).rev() {
        let cells = UnicodeWidthStr::width(grapheme);
        if used + cells > width {
            break;
        }
        used += cells;
        kept.push(grapheme);
    }
    kept.reverse();
    let mut text: String = kept.concat();
    // A trimmed wide grapheme can leave the window one cell short of `width`.
    text.push_str(&" ".repeat(width.saturating_sub(used)));
    text
}

/// `value` placed within `width` cells according to `align`, clipped on grapheme boundaries when it
/// does not fit. Unlike a field this pads only as far as the alignment requires.
fn place(value: &str, width: usize, align: TextAlign) -> String {
    let clipped = clip(value, width);
    let used = UnicodeWidthStr::width(clipped.as_str());
    let slack = width.saturating_sub(used);
    match align {
        TextAlign::Center => format!(
            "{}{clipped}{}",
            " ".repeat(slack / 2),
            " ".repeat(slack.div_ceil(2))
        ),
        TextAlign::Right => format!("{}{clipped}", " ".repeat(slack)),
        _ => format!("{clipped}{}", " ".repeat(slack)),
    }
}

fn clip(value: &str, width: usize) -> String {
    if UnicodeWidthStr::width(value) <= width {
        return value.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for grapheme in value.graphemes(true) {
        let cells = UnicodeWidthStr::width(grapheme);
        if used + cells > width {
            break;
        }
        used += cells;
        out.push_str(grapheme);
    }
    out
}

/// Squash the newlines and runs of spaces an attribute or element text may carry, so a control's
/// one-line label cannot break its own box.
fn collapse(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

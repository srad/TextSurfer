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
use crate::core::image::DecodedImage;
use crate::core::style::{
    BorderSide, BoxSizing, CellMetric, ComputedStyle, CssMaxSize, CssPadding, CssSize, TextAlign,
};

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
    pub(super) control: bool,
    pub(super) image: bool,
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
    pub(super) image: bool,
    pub(super) intrinsic_cols: usize,
    pub(super) intrinsic_rows: usize,
}

#[derive(Clone, Copy)]
pub(super) struct ReplacedInput<'a> {
    pub(super) forms: &'a FormState,
    pub(super) image: Option<&'a DecodedImage>,
    pub(super) style: ComputedStyle,
    pub(super) metric: CellMetric,
    pub(super) width_basis: Option<usize>,
}

/// The stand-in for `id` at its intrinsic size, or `None` when the element is not replaced or
/// renders nothing. Inline boxes never paint a border, so this always brackets.
pub(super) fn replaced_content(
    document: &Document,
    id: NodeId,
    input: ReplacedInput<'_>,
) -> Option<Replaced> {
    let box_ = replaced_box(document, id, false, input)?;
    Some(Replaced {
        text: box_.intrinsic_text(),
        preformatted: box_.preformatted,
        image: box_.image,
        intrinsic_cols: box_.intrinsic_cols,
        intrinsic_rows: box_.intrinsic_rows,
    })
}

/// What `id` shows, or `None` when it is not a replaced element or renders nothing.
///
/// `bordered` is whether the box will paint a border of its own; it decides the brackets.
pub(super) fn replaced_box(
    document: &Document,
    id: NodeId,
    bordered: bool,
    input: ReplacedInput<'_>,
) -> Option<ReplacedBox> {
    let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
        return None;
    };
    if *ns != ElementNs::Html {
        return None;
    }
    if name == "img" {
        if let Some(image) = input.image {
            let (cols, rows) = image_cells(image, input.style, input.metric, input.width_basis);
            return Some(ReplacedBox {
                intrinsic_cols: cols,
                intrinsic_rows: rows,
                text: std::iter::repeat_n("\u{fffc}".repeat(cols), rows)
                    .collect::<Vec<_>>()
                    .join("\n"),
                fill: Fill::Label,
                bracketed: false,
                placeholder: false,
                preformatted: true,
                wraps: false,
                control: false,
                image: true,
            });
        }
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
            control: false,
            image: false,
        });
    }
    let kind = control_kind(document, id)?;
    control_box(document, id, attrs, kind, input.forms, bordered)
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
            control: true,
            image: false,
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
            control: true,
            image: false,
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
                control: true,
                image: false,
            })
        }
        ControlKind::Text | ControlKind::Password => {
            let (value, placeholder) = display_text(document, id, forms);
            let text = if kind == ControlKind::Password && !placeholder {
                "*".repeat(value.graphemes(true).count())
            } else {
                one_line(&value)
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
                control: true,
                image: false,
            })
        }
    }
}

/// The content box a decoded image occupies, in cells.
///
/// This is CSS 2.1 §10.4's constraint table for replaced elements: `min-*` and `max-*` never
/// distort the picture, they resize it along both axes at once. Clamping the two axes
/// independently — what this used to do — turned an over-wide image into a stretched one.
///
/// Two details are ours rather than the specification's. The arithmetic runs in CSS pixels and
/// quantises once at the end: a cell is 8×16, so a square picture is not a square box, and
/// comparing a cell ratio against the picture's own would lose a row. And rounding matches
/// [`CellMetric::resolve_cells`] rather than rounding up, so a 75px-wide picture and an authored
/// `width: 75px` land on the same nine columns instead of differing by one.
pub(super) fn image_cells(
    image: &DecodedImage,
    style: ComputedStyle,
    metric: CellMetric,
    width_basis: Option<usize>,
) -> (usize, usize) {
    let column_px = u64::from(metric.column_px());
    let row_px = u64::from(metric.row_px());
    let image_width = u64::from(image.width).max(1);
    let image_height = u64::from(image.height).max(1);
    let rows_for_cols = |cols: usize| -> usize {
        round_div(cols as u64 * column_px * image_height, image_width * row_px).max(1)
    };
    let cols_for_rows = |rows: usize| -> usize {
        round_div(rows as u64 * row_px * image_width, image_height * column_px).max(1)
    };

    // `box-sizing: border-box` measures every one of these against the border box, so the
    // element's own chrome comes off before the picture is fitted to what is left.
    let (chrome_cols, chrome_rows) = match style.box_sizing {
        BoxSizing::ContentBox => (0, 0),
        BoxSizing::BorderBox => (
            edge_cells(style.padding.left, style.border.left)
                .saturating_add(edge_cells(style.padding.right, style.border.right)),
            edge_cells(style.padding.top, style.border.top)
                .saturating_add(edge_cells(style.padding.bottom, style.border.bottom)),
        ),
    };
    let content_cols = |value: usize| value.saturating_sub(chrome_cols);
    let content_rows = |value: usize| value.saturating_sub(chrome_rows);

    let min_cols = content_cols(minimum(style.min_width, width_basis));
    let min_rows = content_rows(minimum(style.min_height, None));
    // A minimum overrides a maximum it contradicts, so folding it in here leaves the table below
    // with only the violations it actually describes.
    let max_cols = maximum(style.max_width, width_basis)
        .saturating_sub(chrome_cols)
        .max(min_cols);
    let max_rows = maximum(style.max_height, None)
        .saturating_sub(chrome_rows)
        .max(min_rows);

    let width = definite_width(style.width, width_basis).map(content_cols);
    let height = definite_height(style.height).map(content_rows);
    let (cols, rows) = match (width, height) {
        (Some(cols), Some(rows)) => (cols.max(1), rows.max(1)),
        (Some(cols), None) => (cols.max(1), rows_for_cols(cols)),
        (None, Some(rows)) => (cols_for_rows(rows), rows.max(1)),
        (None, None) => (
            round_div(image_width, column_px).max(1),
            round_div(image_height, row_px).max(1),
        ),
    };

    let (cols, rows) = match (
        cols > max_cols,
        cols < min_cols,
        rows > max_rows,
        rows < min_rows,
    ) {
        // Both maxima are violated: the axis that has to shrink further decides the scale.
        (true, _, true, _) => {
            if max_cols.saturating_mul(rows) <= max_rows.saturating_mul(cols) {
                (max_cols, rows_for_cols(max_cols).max(min_rows))
            } else {
                (cols_for_rows(max_rows).max(min_cols), max_rows)
            }
        }
        // Both minima are violated: the axis that has to grow further decides the scale.
        (_, true, _, true) => {
            if min_cols.saturating_mul(rows) <= min_rows.saturating_mul(cols) {
                (cols_for_rows(min_rows).max(min_cols), min_rows)
            } else {
                (min_cols, rows_for_cols(min_cols).max(min_rows))
            }
        }
        // One axis is pinned each way, so the ratio cannot be kept and both bounds win.
        (_, true, true, _) => (min_cols, max_rows),
        (true, _, _, true) => (max_cols, min_rows),
        (true, ..) => (max_cols, rows_for_cols(max_cols).max(min_rows)),
        (_, true, ..) => (min_cols, rows_for_cols(min_cols).min(max_rows)),
        (_, _, true, _) => (cols_for_rows(max_rows).max(min_cols), max_rows),
        (_, _, _, true) => (cols_for_rows(min_rows).min(max_cols), min_rows),
        _ => (cols, rows),
    };
    // Two of the table's rows scale one axis to satisfy a minimum on the other and can overshoot
    // that axis' own maximum — `min-width` with a smaller `max-height` is the case the WPT
    // replaced-sizing tests pin. The picture is distorted at that point whichever way we go, so
    // the declared bounds win, which is also what browsers show.
    (
        cols.clamp(min_cols, max_cols).max(1),
        rows.clamp(min_rows, max_rows).max(1),
    )
}

/// One edge's contribution to the chrome `box-sizing: border-box` measures. A padding that only a
/// containing block could resolve counts as nothing, which is what the rest of the layout does
/// with it too.
fn edge_cells(padding: CssPadding, border: BorderSide) -> usize {
    padding
        .cells()
        .unwrap_or(0)
        .saturating_add(border.layout_width())
}

/// `numerator / denominator`, rounded to the nearest whole cell and away from zero at a half, so
/// that it agrees with [`CellMetric::resolve_cells`].
fn round_div(numerator: u64, denominator: u64) -> usize {
    let denominator = denominator.max(1);
    ((numerator + denominator / 2) / denominator) as usize
}

fn definite_width(size: CssSize, basis: Option<usize>) -> Option<usize> {
    match size {
        CssSize::Auto | CssSize::Calc(_) => None,
        CssSize::Cells(value) => Some(value),
        CssSize::Percent(value) => basis.map(|basis| value.resolve(basis)),
    }
}

fn definite_height(size: CssSize) -> Option<usize> {
    match size {
        CssSize::Cells(value) => Some(value),
        CssSize::Auto | CssSize::Percent(_) | CssSize::Calc(_) => None,
    }
}

fn minimum(size: CssSize, basis: Option<usize>) -> usize {
    definite_width(size, basis).unwrap_or(0)
}

fn maximum(size: CssMaxSize, basis: Option<usize>) -> usize {
    match size {
        CssMaxSize::None | CssMaxSize::Calc(_) => usize::MAX,
        CssMaxSize::Cells(value) => value,
        CssMaxSize::Percent(value) => basis.map_or(usize::MAX, |basis| value.resolve(basis)),
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

fn one_line(value: &str) -> String {
    value
        .chars()
        .filter(|character| !matches!(character, '\r' | '\n'))
        .collect()
}

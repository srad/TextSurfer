use style::values::computed::text::TextDecorationLine;

use crate::core::form::ControlKind;
use crate::core::style::{ComputedStyle, LegacyAlign, Visibility};

/// The two things one element contributes that its `ComputedValues` cannot.
///
/// Bundled rather than passed as a pair of arguments so a call site cannot silently swap them, and
/// so the next per-element rule has somewhere to go.
#[derive(Clone, Copy, Default)]
pub(super) struct ElementPolicy {
    pub(super) decorations: TextDecorationLine,
    pub(super) control: Option<ControlKind>,
    pub(super) legacy_align: LegacyAlign,
}

/// A form control renders as a bracketed stand-in, and `reverse` is what tells it from body text.
///
/// `css::ua::apply_form_control` documents why the emphasis is a style bit rather than a colour, and
/// is the other half of this rule: everything else it does — `white-space: pre`, the cursor, the
/// centred label, `display: none` for a hidden input — is expressible in CSS and belongs to the
/// user-agent sheet at S4c. `reverse` is not, and never will be.
///
/// Outside the memo, because it is keyed on the element rather than on its computed values: two
/// `<input>`s differing only in `type` share one `ComputedValues` — no sheet mentions the attribute,
/// so Stylo's sharing cache hands them one allocation — while differing here.
pub(super) fn form_control(style: &mut ComputedStyle, control: Option<ControlKind>) {
    // A hidden input is submitted and never rendered, so there is no stand-in to distinguish.
    style.reverse = control.is_some_and(|kind| kind != ControlKind::Hidden);
}

pub(super) fn legacy_align(style: &mut ComputedStyle, align: LegacyAlign) {
    style.legacy_align = align;
}

/// `opacity: 0` computes to `visibility: hidden`.
///
/// The locked terminal rule: a terminal has no partial transparency, but `opacity: 0` over a styled
/// box is how the web builds a custom control, so the one value that has an exact equivalent is
/// honoured. A pure function of the computed values, so it stays inside the mapper's memo.
pub(super) fn hide_fully_transparent(style: &mut ComputedStyle, opacity: f32) {
    if opacity == 0.0 {
        style.visibility = Visibility::Hidden;
    }
}

/// Fold the decorations inherited from ancestors into this element's own.
///
/// `text-decoration-line` is a reset property in Stylo — its struct is `Text` (TextReset), which
/// `properties/data.py` declares `inherited=False` — while `ComputedStyle::underline` and `strike`
/// inherit, which is what makes every descendant of a link underlined. Browsers get the same effect
/// by propagating decorations at paint time; the terminal has no separate decoration layer, so the
/// mapper folds them in as it walks.
///
/// This is why the walk must be pre-order, and why these two bits sit outside the memo: two
/// elements can share one `ComputedValues` and still differ here.
pub(super) fn inherit_decorations(style: &mut ComputedStyle, inherited: TextDecorationLine) {
    style.underline |= inherited.contains(TextDecorationLine::UNDERLINE);
    style.strike |= inherited.contains(TextDecorationLine::LINE_THROUGH);
}

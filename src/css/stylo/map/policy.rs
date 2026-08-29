use style::values::computed::text::TextDecorationLine;

use crate::core::style::{ComputedStyle, Visibility};

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

use style::properties::ComputedValues;
use style::values::specified::box_::{DisplayInside, DisplayOutside};

use crate::core::style::{Display, DisplayInternal, DisplayOutside as Outside};

/// Stylo packs `display` as an outside value, an inside value and a list-item bit; ours splits the
/// same information across three enums.
///
/// The `inside()` value is read **first**, because `none` and `contents` are both
/// `DisplayOutside::None` and are told apart only by their inside value.
pub(super) fn display(values: &ComputedValues) -> Display {
    let value = values.clone_display();
    match value.inside() {
        DisplayInside::None => return Display::NONE,
        DisplayInside::Contents => return Display::CONTENTS,
        _ => {}
    }
    // `table-caption` is an *outside* value in Stylo and an internal one for us, so it is matched
    // before the internal-table family rather than alongside it.
    if value.outside() == DisplayOutside::TableCaption {
        return Display::Internal(DisplayInternal::TableCaption);
    }
    if let Some(internal) = table_internal(value.inside()) {
        return Display::Internal(internal);
    }
    let outside = match value.outside() {
        DisplayOutside::Block | DisplayOutside::TableCaption | DisplayOutside::InternalTable => {
            Outside::Block
        }
        DisplayOutside::Inline | DisplayOutside::None => Outside::Inline,
    };
    // `list-item` is restricted to flow and flow-root by `DisplayInside::is_valid_for_list_item`,
    // which is why only `Display::flow` carries the bit.
    match value.inside() {
        DisplayInside::Flow => Display::flow(outside, false, value.is_list_item()),
        DisplayInside::FlowRoot => Display::flow(outside, true, value.is_list_item()),
        DisplayInside::Table => Display::table(outside),
        DisplayInside::Flex => Display::flex(outside),
        DisplayInside::Grid => Display::grid(outside),
        // Already handled above; a `_` arm rather than an `unreachable!` because page-controlled
        // input never justifies a panic.
        DisplayInside::None | DisplayInside::Contents => Display::NONE,
        _ => Display::flow(outside, false, false),
    }
}

fn table_internal(inside: DisplayInside) -> Option<DisplayInternal> {
    Some(match inside {
        DisplayInside::TableRowGroup => DisplayInternal::TableRowGroup,
        DisplayInside::TableHeaderGroup => DisplayInternal::TableHeaderGroup,
        DisplayInside::TableFooterGroup => DisplayInternal::TableFooterGroup,
        DisplayInside::TableRow => DisplayInternal::TableRow,
        DisplayInside::TableCell => DisplayInternal::TableCell,
        DisplayInside::TableColumn => DisplayInternal::TableColumn,
        DisplayInside::TableColumnGroup => DisplayInternal::TableColumnGroup,
        _ => return None,
    })
}

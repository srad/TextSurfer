use crate::layout::table::TableOutput;

use super::{BoxTree, LayoutRect, TextFragment};

pub(super) fn append_table_output(
    tree: &mut BoxTree,
    output: TableOutput,
    col: usize,
    row: usize,
    depth: usize,
    merge_base: usize,
) {
    tree.height = tree.height.max(row.saturating_add(output.height));
    for mut layout_box in output.boxes {
        offset_rect(&mut layout_box.border_rect, col, row);
        offset_rect(&mut layout_box.content_rect, col, row);
        layout_box.depth += depth;
        tree.boxes.push(layout_box);
    }
    for mut fill in output.fills {
        offset_rect(&mut fill.rect, col, row);
        fill.depth += depth;
        tree.fills.push(fill);
    }
    for mut stroke in output.strokes {
        offset_rect(&mut stroke.rect, col, row);
        stroke.depth += depth;
        stroke.merge_group = merge_base
            .saturating_mul(1_000_000)
            .saturating_add(stroke.merge_group);
        tree.strokes.push(stroke);
    }
    for fragment in output.fragments {
        tree.fragments.push(TextFragment {
            node: fragment.node,
            col: col.saturating_add(fragment.col),
            row: row.saturating_add(fragment.row),
            text: fragment.text,
            depth: depth.saturating_add(fragment.depth),
            style: fragment.style,
        });
    }
}

fn offset_rect(rect: &mut LayoutRect, col: usize, row: usize) {
    rect.col = rect.col.saturating_add(col);
    rect.row = rect.row.saturating_add(row);
}

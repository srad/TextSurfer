use crate::layout::clip::ClipRegion;
use crate::layout::table::TableOutput;

use super::{BoxTree, TextFragment};

pub(super) fn append_table_output(
    tree: &mut BoxTree,
    output: TableOutput,
    col: isize,
    row: isize,
    depth: usize,
    merge_base: usize,
) {
    if row >= 0 {
        tree.height = tree
            .height
            .max((row as usize).saturating_add(output.height));
    }
    for mut layout_box in output.boxes {
        let Some(border_rect) = ClipRegion::translate_rect(layout_box.border_rect, col, row) else {
            continue;
        };
        layout_box.border_rect = border_rect;
        layout_box.content_rect =
            ClipRegion::translate_rect(layout_box.content_rect, col, row).unwrap_or_default();
        layout_box.depth += depth;
        tree.boxes.push(layout_box);
    }
    for mut fill in output.fills {
        let Some(rect) = ClipRegion::translate_rect(fill.rect, col, row) else {
            continue;
        };
        fill.rect = rect;
        fill.depth += depth;
        tree.fills.push(fill);
    }
    for mut stroke in output.strokes {
        let Some(translated) = ClipRegion::translate_stroke(stroke, col, row) else {
            continue;
        };
        stroke = translated;
        stroke.depth += depth;
        stroke.merge_group = merge_base
            .saturating_mul(1_000_000)
            .saturating_add(stroke.merge_group);
        tree.strokes.push(stroke);
    }
    for mut image in output.images {
        let Some(rect) = ClipRegion::translate_rect(image.rect, col, row) else {
            continue;
        };
        let Some(clip) = ClipRegion::translate_rect(image.clip, col, row) else {
            continue;
        };
        image.rect = rect;
        image.clip = clip;
        image.depth += depth;
        tree.images.push(image);
    }
    for fragment in output.fragments {
        let fragment = TextFragment {
            node: fragment.node,
            col: fragment.col,
            row: fragment.row,
            text: fragment.text,
            depth: depth.saturating_add(fragment.depth),
            style: fragment.style,
        };
        if let Some(fragment) = ClipRegion::translate_fragment(&fragment, col, row) {
            tree.fragments.push(fragment);
        }
    }
}

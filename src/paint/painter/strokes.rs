use std::collections::BTreeMap;

use crate::core::style::{BorderColor, BorderSide, CellStyle, Rgba};
use crate::layout::BorderStroke;

use super::row::RowBuffer;

#[derive(Clone, Copy)]
struct StrokeCell {
    mask: u8,
    group: usize,
    depth: usize,
    rank: u8,
    style: CellStyle,
}

pub(super) fn draw_strokes(
    rows: &mut BTreeMap<usize, RowBuffer>,
    viewport_width: usize,
    document_height: usize,
    strokes: &[BorderStroke],
) {
    let mut cells = BTreeMap::<(usize, usize), StrokeCell>::new();
    for stroke in strokes {
        add_stroke(&mut cells, *stroke, viewport_width, document_height);
    }
    for ((row, col), cell) in cells {
        let buffer = rows
            .entry(row)
            .or_insert_with(|| RowBuffer::new(viewport_width));
        buffer.write(col, border_glyph(cell.mask), cell.style);
    }
}

fn add_stroke(
    cells: &mut BTreeMap<(usize, usize), StrokeCell>,
    stroke: BorderStroke,
    viewport_width: usize,
    document_height: usize,
) {
    let rect = stroke.rect;
    if rect.width == 0
        || rect.height == 0
        || rect.col >= viewport_width
        || rect.row >= document_height
    {
        return;
    }
    let right = rect
        .col
        .saturating_add(rect.width - 1)
        .min(viewport_width - 1);
    let bottom = rect
        .row
        .saturating_add(rect.height - 1)
        .min(document_height - 1);
    if stroke.edges.top.is_visible() {
        add_horizontal(cells, rect.row, rect.col, right, stroke, stroke.edges.top);
    }
    if stroke.edges.bottom.is_visible() {
        add_horizontal(cells, bottom, rect.col, right, stroke, stroke.edges.bottom);
    }
    if stroke.edges.left.is_visible() {
        add_vertical(cells, rect.col, rect.row, bottom, stroke, stroke.edges.left);
    }
    if stroke.edges.right.is_visible() {
        add_vertical(cells, right, rect.row, bottom, stroke, stroke.edges.right);
    }
}

fn add_horizontal(
    cells: &mut BTreeMap<(usize, usize), StrokeCell>,
    row: usize,
    left: usize,
    right: usize,
    stroke: BorderStroke,
    side: BorderSide,
) {
    for col in left..=right {
        let mut mask = 0;
        if col > left {
            mask |= 8;
        }
        if col < right {
            mask |= 2;
        }
        if left == right {
            mask = 10;
        }
        place_stroke(cells, row, col, mask, stroke, side);
    }
}

fn add_vertical(
    cells: &mut BTreeMap<(usize, usize), StrokeCell>,
    col: usize,
    top: usize,
    bottom: usize,
    stroke: BorderStroke,
    side: BorderSide,
) {
    for row in top..=bottom {
        let mut mask = 0;
        if row > top {
            mask |= 1;
        }
        if row < bottom {
            mask |= 4;
        }
        if top == bottom {
            mask = 5;
        }
        place_stroke(cells, row, col, mask, stroke, side);
    }
}

fn place_stroke(
    cells: &mut BTreeMap<(usize, usize), StrokeCell>,
    row: usize,
    col: usize,
    mask: u8,
    stroke: BorderStroke,
    side: BorderSide,
) {
    let rank = side.style as u8;
    let mut style = stroke.style;
    style.fg = match side.color {
        BorderColor::CurrentColor => stroke.style.fg,
        BorderColor::Transparent => None,
        BorderColor::Rgb(color) => Some(Rgba::opaque(color)),
    };
    match cells.get_mut(&(row, col)) {
        Some(cell) if cell.group == stroke.merge_group => {
            cell.mask |= mask;
            if rank >= cell.rank {
                cell.rank = rank;
                cell.style = style;
            }
        }
        Some(cell) if stroke.depth >= cell.depth => {
            *cell = StrokeCell {
                mask,
                group: stroke.merge_group,
                depth: stroke.depth,
                rank,
                style,
            };
        }
        Some(_) => {}
        None => {
            cells.insert(
                (row, col),
                StrokeCell {
                    mask,
                    group: stroke.merge_group,
                    depth: stroke.depth,
                    rank,
                    style,
                },
            );
        }
    }
}

fn border_glyph(mask: u8) -> &'static str {
    match mask {
        1 | 4 | 5 => "│",
        2 | 8 | 10 => "─",
        3 => "└",
        6 => "┌",
        7 => "├",
        9 => "┘",
        11 => "┴",
        12 => "┐",
        13 => "┤",
        14 => "┬",
        15 => "┼",
        _ => "─",
    }
}

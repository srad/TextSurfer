use std::collections::BTreeMap;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::core::dom::NodeId;
use crate::layout::{BoxTree, LayoutRect};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayList {
    pub lines: Vec<String>,
    pub hits: Vec<HitRegion>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HitRegion {
    pub node: NodeId,
    pub rect: LayoutRect,
    pub depth: usize,
}

impl DisplayList {
    pub fn hit_test(&self, col: usize, row: usize) -> Option<NodeId> {
        self.hits
            .iter()
            .filter(|hit| {
                col >= hit.rect.col
                    && col < hit.rect.col.saturating_add(hit.rect.width)
                    && row >= hit.rect.row
                    && row < hit.rect.row.saturating_add(hit.rect.height)
            })
            .max_by_key(|hit| hit.depth)
            .map(|hit| hit.node)
    }
}

pub trait Painter: Send + Sync {
    fn paint(&self, box_tree: &BoxTree) -> DisplayList;
}

#[derive(Default)]
pub struct BasicPainter;

impl Painter for BasicPainter {
    fn paint(&self, box_tree: &BoxTree) -> DisplayList {
        let mut rows = BTreeMap::new();
        let mut boxes: Vec<_> = box_tree
            .boxes
            .iter()
            .filter(|layout_box| layout_box.border)
            .collect();
        boxes.sort_by_key(|layout_box| layout_box.depth);
        for layout_box in boxes {
            draw_border(
                &mut rows,
                box_tree.width,
                box_tree.height,
                layout_box.border_rect,
            );
        }
        let mut fragments: Vec<_> = box_tree.fragments.iter().collect();
        fragments.sort_by_key(|fragment| (fragment.depth, fragment.row, fragment.col));
        for fragment in fragments {
            if fragment.row >= box_tree.height || fragment.col >= box_tree.width {
                continue;
            }
            let row = rows
                .entry(fragment.row)
                .or_insert_with(|| RowBuffer::new(box_tree.width));
            write_line(
                &mut row.cells,
                &mut row.owners,
                fragment.col,
                &fragment.text,
            );
        }
        let mut lines = vec![String::new(); box_tree.height];
        for (row, buffer) in rows {
            lines[row] = buffer.cells.concat().trim_end().to_string();
        }
        DisplayList {
            lines,
            hits: box_tree
                .boxes
                .iter()
                .map(|layout_box| HitRegion {
                    node: layout_box.node,
                    rect: layout_box.border_rect,
                    depth: layout_box.depth,
                })
                .collect(),
        }
    }
}

struct RowBuffer {
    cells: Vec<String>,
    owners: Vec<Option<usize>>,
}

impl RowBuffer {
    fn new(width: usize) -> Self {
        Self {
            cells: vec![" ".to_string(); width],
            owners: vec![None; width],
        }
    }
}

fn draw_border(
    rows: &mut BTreeMap<usize, RowBuffer>,
    viewport_width: usize,
    document_height: usize,
    rect: LayoutRect,
) {
    if rect.width == 0 || rect.height == 0 || rect.row >= document_height {
        return;
    }
    let width = rect.width.min(viewport_width.saturating_sub(rect.col));
    if width == 0 {
        return;
    }
    if rect.height == 1 {
        write_border_row(
            rows,
            viewport_width,
            rect.row,
            rect.col,
            width,
            ['─', '─', '─'],
        );
        return;
    }
    write_border_row(
        rows,
        viewport_width,
        rect.row,
        rect.col,
        width,
        ['┌', '─', '┐'],
    );
    let bottom = rect.row.saturating_add(rect.height - 1);
    if bottom < document_height {
        write_border_row(
            rows,
            viewport_width,
            bottom,
            rect.col,
            width,
            ['└', '─', '┘'],
        );
    }
    for row in rect.row.saturating_add(1)..bottom.min(document_height) {
        let buffer = rows
            .entry(row)
            .or_insert_with(|| RowBuffer::new(viewport_width));
        write_line(&mut buffer.cells, &mut buffer.owners, rect.col, "│");
        if width > 1 {
            write_line(
                &mut buffer.cells,
                &mut buffer.owners,
                rect.col + width - 1,
                "│",
            );
        }
    }
}

fn write_border_row(
    rows: &mut BTreeMap<usize, RowBuffer>,
    viewport_width: usize,
    row: usize,
    col: usize,
    width: usize,
    glyphs: [char; 3],
) {
    let [left, middle, right] = glyphs;
    let text = if width == 1 {
        left.to_string()
    } else {
        format!("{left}{}{right}", middle.to_string().repeat(width - 2))
    };
    let buffer = rows
        .entry(row)
        .or_insert_with(|| RowBuffer::new(viewport_width));
    write_line(&mut buffer.cells, &mut buffer.owners, col, &text);
}

fn write_line(cells: &mut [String], owners: &mut [Option<usize>], start: usize, text: &str) {
    let mut col = start;
    for grapheme in text.graphemes(true) {
        let width = UnicodeWidthStr::width(grapheme);
        if width == 0 {
            if let Some(owner) = col
                .checked_sub(1)
                .and_then(|index| owners.get(index))
                .copied()
                .flatten()
                && let Some(previous) = cells.get_mut(owner)
            {
                previous.push_str(grapheme);
            }
            continue;
        }
        if col.saturating_add(width) > cells.len() {
            break;
        }
        for target in col..col + width {
            clear_grapheme(cells, owners, target);
        }
        cells[col] = grapheme.to_string();
        owners[col] = Some(col);
        for offset in 1..width {
            cells[col + offset].clear();
            owners[col + offset] = Some(col);
        }
        col += width;
    }
}

fn clear_grapheme(cells: &mut [String], owners: &mut [Option<usize>], target: usize) {
    let Some(owner) = owners.get(target).copied().flatten() else {
        return;
    };
    let width = UnicodeWidthStr::width(cells[owner].as_str()).max(1);
    cells[owner] = " ".to_string();
    for index in owner..(owner + width).min(owners.len()) {
        owners[index] = None;
        if index != owner {
            cells[index] = " ".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dom::{Document, ElementNs};
    use crate::layout::{LayoutBox, TextFragment};

    #[test]
    fn paints_wide_graphemes_without_exceeding_the_cell_width() {
        let mut document = Document::new();
        let node = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let tree = BoxTree {
            width: 4,
            height: 1,
            fragments: vec![TextFragment {
                node,
                col: 1,
                row: 0,
                text: "界x".to_string(),
                depth: 0,
            }],
            ..Default::default()
        };
        assert_eq!(BasicPainter.paint(&tree).lines, vec![" 界x"]);
    }

    #[test]
    fn hit_testing_returns_the_deepest_box() {
        let mut document = Document::new();
        let parent = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let child = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
        let parent_rect = LayoutRect {
            col: 0,
            row: 0,
            width: 10,
            height: 3,
        };
        let child_rect = LayoutRect {
            col: 1,
            row: 1,
            width: 4,
            height: 1,
        };
        let tree = BoxTree {
            width: 10,
            height: 3,
            boxes: vec![
                LayoutBox {
                    node: parent,
                    border_rect: parent_rect,
                    content_rect: parent_rect,
                    depth: 0,
                    border: false,
                },
                LayoutBox {
                    node: child,
                    border_rect: child_rect,
                    content_rect: child_rect,
                    depth: 1,
                    border: false,
                },
            ],
            ..Default::default()
        };
        let display = BasicPainter.paint(&tree);
        assert_eq!(display.hit_test(2, 1), Some(child));
        assert_eq!(display.hit_test(8, 1), Some(parent));
    }

    #[test]
    fn overwriting_a_wide_grapheme_never_leaves_an_overwide_row() {
        let mut document = Document::new();
        let back = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let front = document.insert_element(None, "span", ElementNs::Html, vec![]);
        let tree = BoxTree {
            width: 2,
            height: 1,
            fragments: vec![
                TextFragment {
                    node: back,
                    col: 0,
                    row: 0,
                    text: "界".to_string(),
                    depth: 0,
                },
                TextFragment {
                    node: front,
                    col: 1,
                    row: 0,
                    text: "x".to_string(),
                    depth: 1,
                },
            ],
            ..Default::default()
        };
        let painted = BasicPainter.paint(&tree);
        assert_eq!(painted.lines, vec![" x"]);
        assert_eq!(UnicodeWidthStr::width(painted.lines[0].as_str()), 2);
    }

    #[test]
    fn borders_are_drawn_from_box_geometry() {
        let mut document = Document::new();
        let node = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let rect = LayoutRect {
            col: 1,
            row: 0,
            width: 4,
            height: 3,
        };
        let tree = BoxTree {
            width: 6,
            height: 3,
            boxes: vec![LayoutBox {
                node,
                border_rect: rect,
                content_rect: LayoutRect {
                    col: 2,
                    row: 1,
                    width: 2,
                    height: 1,
                },
                depth: 0,
                border: true,
            }],
            ..Default::default()
        };
        assert_eq!(
            BasicPainter.paint(&tree).lines,
            vec![" ┌──┐", " │  │", " └──┘"]
        );
    }

    #[test]
    fn untouched_document_rows_do_not_require_dense_cell_buffers() {
        let tree = BoxTree {
            width: 200,
            height: 20_000,
            ..Default::default()
        };
        let display = BasicPainter.paint(&tree);
        assert_eq!(display.lines.len(), 20_000);
        assert!(display.lines.iter().all(String::is_empty));
    }
}

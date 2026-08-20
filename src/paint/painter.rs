use crate::core::dom::NodeId;
use crate::layout::BoxTree;
use crate::layout::LayoutRect;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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
                    && col < hit.rect.col + hit.rect.width
                    && row >= hit.rect.row
                    && row < hit.rect.row + hit.rect.height
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
        let mut cells = vec![vec![" ".to_string(); box_tree.width]; box_tree.height];
        let mut owners = vec![vec![None; box_tree.width]; box_tree.height];
        let mut lines: Vec<_> = box_tree.lines.iter().collect();
        lines.sort_by_key(|line| (line.depth, line.row));
        for line in lines {
            if let (Some(row), Some(row_owners)) =
                (cells.get_mut(line.row), owners.get_mut(line.row))
            {
                write_line(row, row_owners, line.col, &line.text);
            }
        }
        DisplayList {
            lines: cells
                .into_iter()
                .map(|row| row.concat().trim_end().to_string())
                .collect(),
            hits: box_tree
                .boxes
                .iter()
                .map(|layout_box| HitRegion {
                    node: layout_box.node,
                    rect: layout_box.rect,
                    depth: layout_box.depth,
                })
                .collect(),
        }
    }
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
        if col + width > cells.len() {
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
    use crate::layout::{LayoutBox, LayoutLine};

    #[test]
    fn paints_wide_graphemes_without_exceeding_the_cell_width() {
        let mut document = Document::new();
        let node = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let tree = BoxTree {
            width: 4,
            height: 1,
            lines: vec![LayoutLine {
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
        let tree = BoxTree {
            width: 10,
            height: 3,
            boxes: vec![
                LayoutBox {
                    node: parent,
                    rect: LayoutRect {
                        col: 0,
                        row: 0,
                        width: 10,
                        height: 3,
                    },
                    depth: 0,
                },
                LayoutBox {
                    node: child,
                    rect: LayoutRect {
                        col: 1,
                        row: 1,
                        width: 4,
                        height: 1,
                    },
                    depth: 1,
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
            lines: vec![
                LayoutLine {
                    node: back,
                    col: 0,
                    row: 0,
                    text: "界".to_string(),
                    depth: 0,
                },
                LayoutLine {
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
}

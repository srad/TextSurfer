mod contrast;
mod row;
mod strokes;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use unicode_width::UnicodeWidthStr;

use crate::core::dom::NodeId;
use crate::core::style::{CellStyle, Palette};
use crate::layout::{BoxTree, LayoutRect};

use row::{RowBuffer, fill_background};
use strokes::draw_strokes;

pub use contrast::legible_foreground;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayList {
    pub rows: Vec<PaintedRow>,
    pub hits: Vec<HitRegion>,
    pub links: Vec<PaintedLink>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaintedRow {
    pub spans: Vec<PaintedSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaintedSpan {
    pub col: usize,
    pub text: String,
    pub style: CellStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaintedLink {
    pub node: NodeId,
    pub href: String,
    pub rects: Vec<LayoutRect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HitRegion {
    pub node: NodeId,
    pub rect: LayoutRect,
    pub depth: usize,
}

impl DisplayList {
    pub fn from_lines(lines: &[String]) -> Self {
        Self {
            rows: lines
                .iter()
                .map(|line| PaintedRow {
                    spans: if line.is_empty() {
                        Vec::new()
                    } else {
                        vec![PaintedSpan {
                            col: 0,
                            text: line.clone(),
                            style: CellStyle::default(),
                        }]
                    },
                })
                .collect(),
            ..Default::default()
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn row(&self, index: usize) -> Option<&PaintedRow> {
        self.rows.get(index)
    }

    pub fn text_lines(&self) -> Vec<String> {
        self.rows.iter().map(PaintedRow::text).collect()
    }

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

    pub fn link_at(&self, col: usize, row: usize) -> Option<&PaintedLink> {
        self.links.iter().find(|link| {
            link.rects.iter().any(|rect| {
                row == rect.row && col >= rect.col && col < rect.col.saturating_add(rect.width)
            })
        })
    }
}

impl PaintedRow {
    pub fn text(&self) -> String {
        let mut line = String::new();
        let mut col = 0usize;
        for span in &self.spans {
            while col < span.col {
                line.push(' ');
                col += 1;
            }
            line.push_str(&span.text);
            col += UnicodeWidthStr::width(span.text.as_str());
        }
        line
    }
}

pub trait Painter: Send + Sync {
    fn paint(&self, box_tree: &BoxTree, palette: Palette) -> DisplayList;
}

#[derive(Default)]
pub struct BasicPainter;

impl Painter for BasicPainter {
    fn paint(&self, box_tree: &BoxTree, palette: Palette) -> DisplayList {
        let mut rows = BTreeMap::new();
        let mut fills: Vec<_> = box_tree.fills.iter().collect();
        fills.sort_by_key(|fill| fill.depth);
        for fill in fills {
            fill_background(
                &mut rows,
                box_tree.width,
                box_tree.height,
                fill.rect,
                fill.color,
            );
        }
        draw_strokes(
            &mut rows,
            box_tree.width,
            box_tree.height,
            &box_tree.strokes,
        );
        let mut fragments: Vec<_> = box_tree.fragments.iter().collect();
        fragments.sort_by_key(|fragment| (fragment.depth, fragment.row, fragment.col));
        for fragment in fragments {
            if fragment.row >= box_tree.height || fragment.col >= box_tree.width {
                continue;
            }
            let row = rows
                .entry(fragment.row)
                .or_insert_with(|| RowBuffer::new(box_tree.width));
            row.write(fragment.col, &fragment.text, fragment.style);
        }
        let mut painted = vec![PaintedRow::default(); box_tree.height];
        for (row, buffer) in rows {
            painted[row] = buffer.into_row(palette);
        }
        DisplayList {
            rows: painted,
            hits: box_tree
                .boxes
                .iter()
                .map(|layout_box| HitRegion {
                    node: layout_box.node,
                    rect: layout_box.border_rect,
                    depth: layout_box.depth,
                })
                .collect(),
            links: box_tree
                .links
                .iter()
                .map(|link| PaintedLink {
                    node: link.node,
                    href: link.href.clone(),
                    rects: link.rects.clone(),
                })
                .collect(),
        }
    }
}

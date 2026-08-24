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

pub use contrast::{legible_foreground, resolve_cell_style};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayList {
    pub rows: Vec<PaintedRow>,
    pub hits: Vec<HitRegion>,
    pub links: Vec<PaintedLink>,
    pub scaled_text: Vec<ScaledTextRun>,
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
pub struct ScaledTextRun {
    pub node: NodeId,
    pub rect: LayoutRect,
    pub text: String,
    pub style: CellStyle,
    pub depth: usize,
    pub ink: bool,
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
    pub kind: HitKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HitKind {
    Box,
    Text,
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
            .max_by_key(|hit| (hit.depth, hit.kind))
            .map(|hit| hit.node)
    }

    pub fn link_at(&self, col: usize, row: usize) -> Option<&PaintedLink> {
        self.links.iter().find(|link| {
            link.rects.iter().any(|rect| {
                row >= rect.row
                    && row < rect.row.saturating_add(rect.height)
                    && col >= rect.col
                    && col < rect.col.saturating_add(rect.width)
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
        let mut scaled_text = Vec::new();
        for fragment in fragments {
            if fragment.row >= box_tree.height || fragment.col >= box_tree.width {
                continue;
            }
            let rect = fragment.rect();
            if fragment.style.scale > 1 {
                let mut reservation_style = fragment.style;
                reservation_style.underline = false;
                reservation_style.strike = false;
                for row_index in rect.row..rect.row.saturating_add(rect.height).min(box_tree.height)
                {
                    let row = rows
                        .entry(row_index)
                        .or_insert_with(|| RowBuffer::new(box_tree.width));
                    row.write(
                        rect.col,
                        &" ".repeat(rect.width.min(box_tree.width.saturating_sub(rect.col))),
                        reservation_style,
                    );
                }
                scaled_text.push(ScaledTextRun {
                    node: fragment.node,
                    rect,
                    text: fragment.text.clone(),
                    style: fragment.style,
                    depth: fragment.depth,
                    ink: !fragment
                        .style
                        .fg
                        .is_some_and(|foreground| foreground.alpha == 0),
                });
            } else if fragment.style.scale == 1 {
                let row = rows
                    .entry(fragment.row)
                    .or_insert_with(|| RowBuffer::new(box_tree.width));
                row.write(fragment.col, &fragment.text, fragment.style);
            }
        }
        for run in &mut scaled_text {
            let underline = run.style.underline;
            let strike = run.style.strike;
            if let Some(style) = rows
                .get(&run.rect.row)
                .and_then(|row| row.style_at(run.rect.col))
            {
                run.style = resolve_cell_style(style, palette);
                run.style.underline = underline;
                run.style.strike = strike;
            }
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
                    kind: HitKind::Box,
                })
                .chain(box_tree.fragments.iter().map(|fragment| HitRegion {
                    node: fragment.node,
                    rect: fragment.rect(),
                    depth: fragment.depth,
                    kind: HitKind::Text,
                }))
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
            scaled_text,
        }
    }
}

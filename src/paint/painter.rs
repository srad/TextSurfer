use std::collections::BTreeMap;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::core::dom::NodeId;
use crate::core::style::{CellStyle, Palette, Rgb};
use crate::layout::{BoxTree, LayoutRect};

const MIN_CONTRAST: f32 = 3.0;

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
        let mut boxes: Vec<_> = box_tree.boxes.iter().collect();
        boxes.sort_by_key(|layout_box| layout_box.depth);
        for layout_box in &boxes {
            if let Some(background) = layout_box.style.bg {
                fill_background(
                    &mut rows,
                    box_tree.width,
                    box_tree.height,
                    layout_box.border_rect,
                    background,
                );
            }
        }
        for layout_box in &boxes {
            if layout_box.border {
                draw_border(
                    &mut rows,
                    box_tree.width,
                    box_tree.height,
                    layout_box.border_rect,
                    layout_box.style,
                );
            }
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

pub fn legible_foreground(foreground: Rgb, background: Rgb, palette: Palette) -> Rgb {
    if foreground.contrast_ratio(background) >= MIN_CONTRAST {
        return foreground;
    }
    let target = if palette.text.contrast_ratio(background) >= MIN_CONTRAST {
        palette.text
    } else if background.relative_luminance() > 0.4 {
        Rgb::BLACK
    } else {
        Rgb::WHITE
    };
    for step in 1..=10u8 {
        let candidate = foreground.blend(target, f32::from(step) / 10.0);
        if candidate.contrast_ratio(background) >= MIN_CONTRAST {
            return candidate;
        }
    }
    target
}

struct RowBuffer {
    cells: Vec<String>,
    owners: Vec<Option<usize>>,
    styles: Vec<CellStyle>,
}

impl RowBuffer {
    fn new(width: usize) -> Self {
        Self {
            cells: vec![" ".to_string(); width],
            owners: vec![None; width],
            styles: vec![CellStyle::default(); width],
        }
    }

    fn write(&mut self, start: usize, text: &str, style: CellStyle) {
        write_line(
            &mut self.cells,
            &mut self.owners,
            &mut self.styles,
            start,
            text,
            style,
        );
    }

    fn fill_background(&mut self, from: usize, to: usize, background: Rgb) {
        for index in from..to.min(self.styles.len()) {
            self.styles[index].bg = Some(background);
        }
    }

    fn into_row(self, palette: Palette) -> PaintedRow {
        let mut spans: Vec<PaintedSpan> = Vec::new();
        let mut last_painted = 0usize;
        for (index, cell) in self.cells.iter().enumerate() {
            if cell.is_empty() {
                continue;
            }
            let mut style = self.styles[index];
            if let Some(foreground) = style.fg {
                style.fg = Some(legible_foreground(
                    foreground,
                    style.bg.unwrap_or(palette.background),
                    palette,
                ));
            }
            let blank = cell == " " && style == CellStyle::default();
            match spans.last_mut() {
                Some(span)
                    if span.style == style
                        && span.col + UnicodeWidthStr::width(span.text.as_str()) == index =>
                {
                    span.text.push_str(cell);
                }
                _ => spans.push(PaintedSpan {
                    col: index,
                    text: cell.clone(),
                    style,
                }),
            }
            if !blank {
                last_painted = spans.len();
            }
        }
        spans.truncate(last_painted);
        if let Some(span) = spans.last_mut()
            && span.style == CellStyle::default()
        {
            let trimmed = span.text.trim_end();
            if trimmed.len() != span.text.len() {
                span.text.truncate(trimmed.len());
            }
        }
        spans.retain(|span| !span.text.is_empty());
        PaintedRow { spans }
    }
}

fn fill_background(
    rows: &mut BTreeMap<usize, RowBuffer>,
    viewport_width: usize,
    document_height: usize,
    rect: LayoutRect,
    background: Rgb,
) {
    if rect.width == 0 || rect.height == 0 || rect.col >= viewport_width {
        return;
    }
    let last_row = rect
        .row
        .saturating_add(rect.height)
        .min(document_height)
        .min(rect.row.saturating_add(rect.height));
    for row in rect.row..last_row {
        let buffer = rows
            .entry(row)
            .or_insert_with(|| RowBuffer::new(viewport_width));
        buffer.fill_background(rect.col, rect.col.saturating_add(rect.width), background);
    }
}

fn draw_border(
    rows: &mut BTreeMap<usize, RowBuffer>,
    viewport_width: usize,
    document_height: usize,
    rect: LayoutRect,
    style: CellStyle,
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
            style,
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
        style,
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
            style,
        );
    }
    for row in rect.row.saturating_add(1)..bottom.min(document_height) {
        let buffer = rows
            .entry(row)
            .or_insert_with(|| RowBuffer::new(viewport_width));
        buffer.write(rect.col, "│", style);
        if width > 1 {
            buffer.write(rect.col + width - 1, "│", style);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn write_border_row(
    rows: &mut BTreeMap<usize, RowBuffer>,
    viewport_width: usize,
    row: usize,
    col: usize,
    width: usize,
    glyphs: [char; 3],
    style: CellStyle,
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
    buffer.write(col, &text, style);
}

fn write_line(
    cells: &mut [String],
    owners: &mut [Option<usize>],
    styles: &mut [CellStyle],
    start: usize,
    text: &str,
    style: CellStyle,
) {
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
        styles[col] = merge_style(styles[col], style);
        for offset in 1..width {
            cells[col + offset].clear();
            owners[col + offset] = Some(col);
            styles[col + offset] = styles[col];
        }
        col += width;
    }
}

fn merge_style(under: CellStyle, over: CellStyle) -> CellStyle {
    CellStyle {
        fg: over.fg,
        bg: over.bg.or(under.bg),
        ..over
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

    fn painted(tree: &BoxTree) -> DisplayList {
        BasicPainter.paint(tree, Palette::default())
    }

    fn plain_box(node: NodeId, rect: LayoutRect, depth: usize) -> LayoutBox {
        LayoutBox {
            node,
            border_rect: rect,
            content_rect: rect,
            depth,
            border: false,
            style: CellStyle::default(),
        }
    }

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
                style: CellStyle::default(),
            }],
            ..Default::default()
        };
        assert_eq!(painted(&tree).text_lines(), vec![" 界x"]);
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
                plain_box(parent, parent_rect, 0),
                plain_box(child, child_rect, 1),
            ],
            ..Default::default()
        };
        let display = painted(&tree);
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
                    style: CellStyle::default(),
                },
                TextFragment {
                    node: front,
                    col: 1,
                    row: 0,
                    text: "x".to_string(),
                    depth: 1,
                    style: CellStyle::default(),
                },
            ],
            ..Default::default()
        };
        let lines = painted(&tree).text_lines();
        assert_eq!(lines, vec![" x"]);
        assert_eq!(UnicodeWidthStr::width(lines[0].as_str()), 2);
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
                style: CellStyle::default(),
            }],
            ..Default::default()
        };
        assert_eq!(painted(&tree).text_lines(), vec![" ┌──┐", " │  │", " └──┘"]);
    }

    #[test]
    fn untouched_document_rows_do_not_require_dense_cell_buffers() {
        let tree = BoxTree {
            width: 200,
            height: 20_000,
            ..Default::default()
        };
        let display = painted(&tree);
        assert_eq!(display.rows.len(), 20_000);
        assert!(display.rows.iter().all(|row| row.spans.is_empty()));
    }

    #[test]
    fn backgrounds_paint_under_text_in_depth_order() {
        let mut document = Document::new();
        let outer = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let inner = document.insert_element(Some(outer), "span", ElementNs::Html, vec![]);
        let outer_rect = LayoutRect {
            col: 0,
            row: 0,
            width: 4,
            height: 1,
        };
        let inner_rect = LayoutRect {
            col: 2,
            row: 0,
            width: 2,
            height: 1,
        };
        let tree = BoxTree {
            width: 4,
            height: 1,
            boxes: vec![
                LayoutBox {
                    style: CellStyle {
                        bg: Some(Rgb::new(10, 10, 10)),
                        ..Default::default()
                    },
                    ..plain_box(outer, outer_rect, 0)
                },
                LayoutBox {
                    style: CellStyle {
                        bg: Some(Rgb::new(20, 20, 20)),
                        ..Default::default()
                    },
                    ..plain_box(inner, inner_rect, 1)
                },
            ],
            fragments: vec![TextFragment {
                node: inner,
                col: 2,
                row: 0,
                text: "hi".to_string(),
                depth: 1,
                style: CellStyle {
                    fg: Some(Rgb::WHITE),
                    ..Default::default()
                },
            }],
            ..Default::default()
        };
        let display = painted(&tree);
        let spans = &display.rows[0].spans;
        assert_eq!(spans[0].style.bg, Some(Rgb::new(10, 10, 10)));
        assert_eq!(spans[1].text, "hi");
        assert_eq!(
            spans[1].style.bg,
            Some(Rgb::new(20, 20, 20)),
            "the deeper background wins under the text"
        );
        assert_eq!(spans[1].style.fg, Some(Rgb::WHITE));
    }

    #[test]
    fn unreadable_author_colours_are_corrected_towards_the_theme_text() {
        let palette = Palette {
            text: Rgb::WHITE,
            background: Rgb::new(0, 0, 128),
            link: Rgb::new(255, 255, 0),
        };
        let corrected = legible_foreground(Rgb::BLACK, palette.background, palette);
        assert!(
            corrected.contrast_ratio(palette.background) >= MIN_CONTRAST,
            "black on navy must be pushed until it is readable"
        );
        let legible = Rgb::new(255, 255, 0);
        assert_eq!(
            legible_foreground(legible, palette.background, palette),
            legible,
            "a readable pair must be left untouched"
        );
    }

    #[test]
    fn links_carry_their_geometry_into_the_display_list() {
        let mut document = Document::new();
        let node = document.insert_element(None, "a", ElementNs::Html, vec![]);
        let rect = LayoutRect {
            col: 0,
            row: 0,
            width: 4,
            height: 1,
        };
        let tree = BoxTree {
            width: 8,
            height: 1,
            links: vec![crate::layout::LinkBox {
                node,
                href: "https://example.com/".to_string(),
                rects: vec![rect],
            }],
            ..Default::default()
        };
        let display = painted(&tree);
        assert_eq!(display.links.len(), 1);
        assert_eq!(
            display.link_at(2, 0).map(|link| link.href.as_str()),
            Some("https://example.com/")
        );
        assert!(display.link_at(6, 0).is_none());
    }

    #[test]
    fn plain_text_becomes_an_unstyled_display_list() {
        let lines = vec!["first".to_string(), String::new(), "third".to_string()];
        let display = DisplayList::from_lines(&lines);
        assert_eq!(display.text_lines(), lines);
        assert!(display.hits.is_empty() && display.links.is_empty());
    }
}

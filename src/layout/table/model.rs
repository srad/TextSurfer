use crate::core::dom::{AttrNs, ElementNs, Node, NodeId};
use crate::core::style::{ComputedStyle, Display};

use super::{TableFormatter, TableLimits};

#[derive(Clone, Default)]
pub(super) struct TableModel {
    pub(super) rows: Vec<TableRow>,
    pub(super) cells: Vec<TableCell>,
    pub(super) columns: usize,
    pub(super) captions: Vec<NodeId>,
    pub(super) column_nodes: Vec<ColumnTrack>,
}

#[derive(Clone)]
pub(super) struct TableRow {
    pub(super) node: Option<NodeId>,
    pub(super) group_node: Option<NodeId>,
}

#[derive(Clone)]
pub(super) struct TableCell {
    pub(super) owner: Option<NodeId>,
    pub(super) roots: Vec<NodeId>,
    pub(super) row: usize,
    pub(super) col: usize,
    pub(super) row_span: usize,
    pub(super) col_span: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SectionKind {
    Header,
    Body,
    Footer,
}

#[derive(Clone)]
struct RowSeed {
    node: Option<NodeId>,
    group: usize,
    group_node: Option<NodeId>,
    cells: Vec<CellSeed>,
}

#[derive(Clone)]
struct CellSeed {
    owner: Option<NodeId>,
    roots: Vec<NodeId>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct ColumnTrack {
    pub(super) group: Option<NodeId>,
    pub(super) column: Option<NodeId>,
}

impl TableFormatter<'_> {
    pub(super) fn build_model(&self, table: NodeId, limits: TableLimits) -> Option<TableModel> {
        let mut sections = Vec::new();
        let mut captions = Vec::new();
        let mut column_nodes = Vec::new();
        let mut anonymous_cells = Vec::new();
        let mut anonymous_rows = Vec::new();
        let mut group = 0usize;
        for child in self.document.children(table) {
            match self.styles.get(child).display {
                Display::TableCaption => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    captions.push(child);
                }
                Display::TableColumn => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    self.expand_column(child, None, &mut column_nodes);
                }
                Display::TableColumnGroup => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    let before = column_nodes.len();
                    for column in self.document.children(child) {
                        if self.styles.get(column).display == Display::TableColumn {
                            self.expand_column(column, Some(child), &mut column_nodes);
                        }
                    }
                    if column_nodes.len() == before {
                        self.expand_column(child, Some(child), &mut column_nodes);
                    }
                }
                Display::TableHeaderGroup => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    sections.push((SectionKind::Header, self.rows_in_group(child, group)));
                    group += 1;
                }
                Display::TableFooterGroup => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    sections.push((SectionKind::Footer, self.rows_in_group(child, group)));
                    group += 1;
                }
                Display::TableRowGroup => {
                    self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
                    sections.push((SectionKind::Body, self.rows_in_group(child, group)));
                    group += 1;
                }
                Display::TableRow => {
                    if !anonymous_cells.is_empty() {
                        anonymous_rows.push(RowSeed {
                            node: None,
                            group,
                            group_node: None,
                            cells: self.cell_seeds(std::mem::take(&mut anonymous_cells)),
                        });
                    }
                    anonymous_rows.push(self.row_seed(child, group, None));
                }
                Display::TableCell => anonymous_cells.push(child),
                Display::None => {}
                _ if self.is_ignorable(child) => {}
                _ => anonymous_cells.push(child),
            }
        }
        self.flush_anonymous(group, &mut anonymous_cells, &mut anonymous_rows);
        if !anonymous_rows.is_empty() {
            sections.push((SectionKind::Body, anonymous_rows));
        }

        let header = sections
            .iter()
            .position(|(kind, _)| *kind == SectionKind::Header);
        let footer = sections
            .iter()
            .position(|(kind, _)| *kind == SectionKind::Footer);
        let mut seeds = Vec::new();
        if let Some(index) = header {
            seeds.extend(sections[index].1.clone());
        }
        for (index, (_, rows)) in sections.iter().enumerate() {
            if Some(index) != header && Some(index) != footer {
                seeds.extend(rows.clone());
            }
        }
        if let Some(index) = footer {
            seeds.extend(sections[index].1.clone());
        }
        if seeds.len() > limits.max_rows {
            return None;
        }

        let mut occupied = vec![Vec::<bool>::new(); seeds.len()];
        let mut cells = Vec::new();
        let mut columns = 0usize;
        for (row, seed) in seeds.iter().enumerate() {
            for seed_cell in &seed.cells {
                if cells.len() >= limits.max_cells {
                    return None;
                }
                let col_span = seed_cell
                    .owner
                    .map(|node| self.span(node, "colspan", false))
                    .unwrap_or(1)
                    .clamp(1, 1_000);
                let raw_row_span = seed_cell
                    .owner
                    .map(|node| self.span(node, "rowspan", true))
                    .unwrap_or(1);
                let group_end = seeds
                    .iter()
                    .enumerate()
                    .skip(row + 1)
                    .find(|(_, candidate)| candidate.group != seed.group)
                    .map(|(index, _)| index)
                    .unwrap_or(seeds.len());
                let row_span = if raw_row_span == 0 {
                    group_end.saturating_sub(row).max(1)
                } else {
                    raw_row_span.clamp(1, seeds.len().saturating_sub(row).max(1))
                };
                let mut col = 0usize;
                while !range_free(&occupied, row, col, row_span, col_span) {
                    col += 1;
                    if col.saturating_add(col_span) > limits.max_columns {
                        return None;
                    }
                }
                if col.saturating_add(col_span) > limits.max_columns {
                    return None;
                }
                occupy(&mut occupied, row, col, row_span, col_span);
                columns = columns.max(col + col_span);
                cells.push(TableCell {
                    owner: seed_cell.owner,
                    roots: seed_cell.roots.clone(),
                    row,
                    col,
                    row_span,
                    col_span,
                });
            }
        }
        if columns > limits.max_columns {
            return None;
        }
        column_nodes.resize(columns, ColumnTrack::default());
        Some(TableModel {
            rows: seeds
                .into_iter()
                .map(|seed| TableRow {
                    node: seed.node,
                    group_node: seed.group_node,
                })
                .collect(),
            cells,
            columns,
            captions,
            column_nodes,
        })
    }

    fn rows_in_group(&self, group_node: NodeId, group: usize) -> Vec<RowSeed> {
        let mut rows = Vec::new();
        let mut cells = Vec::new();
        for child in self.document.children(group_node) {
            match self.styles.get(child).display {
                Display::TableRow => {
                    if !cells.is_empty() {
                        rows.push(RowSeed {
                            node: None,
                            group,
                            group_node: Some(group_node),
                            cells: self.cell_seeds(std::mem::take(&mut cells)),
                        });
                    }
                    rows.push(self.row_seed(child, group, Some(group_node)));
                }
                Display::TableCell => cells.push(child),
                Display::None => {}
                _ if self.is_ignorable(child) => {}
                _ => cells.push(child),
            }
        }
        if !cells.is_empty() {
            rows.push(RowSeed {
                node: None,
                group,
                group_node: Some(group_node),
                cells: self.cell_seeds(cells),
            });
        }
        rows
    }

    fn row_seed(&self, row: NodeId, group: usize, group_node: Option<NodeId>) -> RowSeed {
        let cells = self.cell_seeds(self.document.children(row));
        RowSeed {
            node: Some(row),
            group,
            group_node,
            cells,
        }
    }

    fn flush_anonymous(&self, group: usize, cells: &mut Vec<NodeId>, rows: &mut Vec<RowSeed>) {
        if !cells.is_empty() {
            rows.push(RowSeed {
                node: None,
                group,
                group_node: None,
                cells: self.cell_seeds(std::mem::take(cells)),
            });
        }
    }

    fn cell_seeds(&self, children: Vec<NodeId>) -> Vec<CellSeed> {
        let mut cells = Vec::new();
        let mut anonymous = Vec::new();
        for child in children {
            let display = self.styles.get(child).display;
            if display == Display::None || self.is_ignorable(child) {
                continue;
            }
            if display == Display::TableCell {
                if !anonymous.is_empty() {
                    cells.push(CellSeed {
                        owner: None,
                        roots: std::mem::take(&mut anonymous),
                    });
                }
                cells.push(CellSeed {
                    owner: Some(child),
                    roots: vec![child],
                });
            } else {
                anonymous.push(child);
            }
        }
        if !anonymous.is_empty() {
            cells.push(CellSeed {
                owner: None,
                roots: anonymous,
            });
        }
        cells
    }

    fn expand_column(&self, node: NodeId, group: Option<NodeId>, columns: &mut Vec<ColumnTrack>) {
        let span = self.span(node, "span", false).clamp(1, 1_000);
        columns.extend(std::iter::repeat_n(
            ColumnTrack {
                group,
                column: Some(node),
            },
            span,
        ));
    }

    fn span(&self, node: NodeId, name: &str, allow_zero: bool) -> usize {
        let Some(Node::Element {
            name: tag,
            ns: ElementNs::Html,
            attrs,
        }) = self.document.node(node)
        else {
            return 1;
        };
        let allowed = match name {
            "colspan" | "rowspan" => matches!(tag.as_str(), "td" | "th"),
            "span" => matches!(tag.as_str(), "col" | "colgroup"),
            _ => false,
        };
        if !allowed {
            return 1;
        }
        attrs
            .iter()
            .find(|attr| attr.ns == AttrNs::None && attr.name == name)
            .and_then(|attr| attr.value.trim().parse::<usize>().ok())
            .filter(|value| allow_zero || *value > 0)
            .unwrap_or(1)
    }

    fn is_ignorable(&self, node: NodeId) -> bool {
        matches!(
            self.document.node(node),
            Some(Node::Text { data }) if data.chars().all(char::is_whitespace)
        ) || matches!(
            self.document.node(node),
            Some(Node::Comment { .. } | Node::Pi { .. } | Node::Doctype { .. })
        )
    }

    pub(super) fn cell_style(&self, cell: &TableCell) -> ComputedStyle {
        cell.owner
            .map(|node| self.styles.get(node))
            .unwrap_or_default()
    }
}

fn range_free(
    occupied: &[Vec<bool>],
    row: usize,
    col: usize,
    row_span: usize,
    col_span: usize,
) -> bool {
    occupied[row..row + row_span]
        .iter()
        .all(|line| (col..col + col_span).all(|column| !line.get(column).copied().unwrap_or(false)))
}

fn occupy(occupied: &mut [Vec<bool>], row: usize, col: usize, row_span: usize, col_span: usize) {
    for line in &mut occupied[row..row + row_span] {
        line.resize(line.len().max(col + col_span), false);
        line[col..col + col_span].fill(true);
    }
}

mod borders;
mod captions;
mod content;
mod geometry;
mod layout;
mod model;
mod sizing;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::dom::{Document, NodeId};
use crate::core::style::{CellStyle, StyleTree};
use crate::layout::text_flow::Atom;
use crate::layout::{BackgroundFill, BorderStroke, LayoutBox};

use content::MetricAtom;
use model::TableModel;

#[derive(Clone, Copy)]
pub(super) struct TableLimits {
    pub max_rows: usize,
    pub max_cells: usize,
    pub max_columns: usize,
    pub max_width: usize,
    pub max_nesting: usize,
}

impl Default for TableLimits {
    fn default() -> Self {
        Self {
            max_rows: 65_535,
            max_cells: 65_535,
            max_columns: 4_096,
            max_width: 65_535,
            max_nesting: 64,
        }
    }
}

#[derive(Clone)]
pub(super) struct TableFragment {
    pub node: NodeId,
    pub col: usize,
    pub row: usize,
    pub text: String,
    pub depth: usize,
    pub style: CellStyle,
    pub clip_right: usize,
}

#[derive(Clone, Default)]
pub(super) struct TableOutput {
    pub width: usize,
    pub height: usize,
    pub boxes: Vec<LayoutBox>,
    pub fills: Vec<BackgroundFill>,
    pub strokes: Vec<BorderStroke>,
    pub fragments: Vec<TableFragment>,
    model: TableModel,
    #[cfg(test)]
    pub degraded: bool,
}

impl TableOutput {
    #[cfg(test)]
    pub fn plain_text(&self) -> String {
        let mut fragments = self.fragments.clone();
        fragments.sort_by_key(|fragment| (fragment.row, fragment.col));
        fragments
            .into_iter()
            .map(|fragment| fragment.text)
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Atom for TableOutput {
    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }
}

pub(super) struct TableFormatter<'a> {
    document: &'a Document,
    styles: &'a StyleTree,
    metric_cache: RefCell<HashMap<(NodeId, usize, usize, usize), MetricAtom>>,
}

impl<'a> TableFormatter<'a> {
    pub fn new(document: &'a Document, styles: &'a StyleTree) -> Self {
        Self {
            document,
            styles,
            metric_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn format(
        &self,
        table: NodeId,
        available_width: usize,
        limits: TableLimits,
        nesting: usize,
    ) -> TableOutput {
        let available_width = available_width.min(limits.max_width);
        if nesting >= limits.max_nesting {
            return self.degraded(table, available_width);
        }
        let Some(model) = self.build_model(table, limits) else {
            return self.degraded(table, available_width);
        };
        if model.rows.is_empty() || model.columns == 0 {
            return self.empty_table(table, model);
        }
        let output = self.layout_model(table, model, available_width, limits, nesting);
        if output.width > limits.max_width {
            self.degraded(table, available_width)
        } else {
            output
        }
    }
}

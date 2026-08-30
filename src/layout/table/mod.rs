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
use crate::core::form::FormState;
use crate::core::style::{CellStyle, StyleTree};
use crate::layout::LayoutInput;
use crate::layout::text_flow::Atom;
use crate::layout::{BackgroundFill, BorderStroke, LayoutBox};

use content::MetricAtom;
use model::TableModel;

#[derive(Clone, Copy)]
pub(super) struct TableRoot {
    pub(super) owner: Option<NodeId>,
    pub(super) style: crate::core::style::ComputedStyle,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
    minimum_width: usize,
    baseline: Option<usize>,
    model: TableModel,
    #[cfg(test)]
    pub degraded: bool,
}

impl TableOutput {
    pub(super) fn minimum_width(&self) -> usize {
        self.minimum_width.max(1).min(self.width.max(1))
    }

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

    fn minimum_width(&self) -> usize {
        self.minimum_width()
    }

    fn baseline(&self) -> usize {
        self.baseline
            .unwrap_or_else(|| self.height.saturating_sub(1))
    }
}

pub(super) struct TableFormatter<'a> {
    document: &'a Document,
    styles: &'a StyleTree,
    forms: &'a FormState,
    images: Option<&'a crate::core::image::ImageResources>,
    cell_metric: crate::core::style::CellMetric,
    metric_cache: RefCell<HashMap<(NodeId, usize, usize, usize), MetricAtom>>,
    output_cache: RefCell<HashMap<(NodeId, usize, TableLimits, usize), TableOutput>>,
}

impl<'a> TableFormatter<'a> {
    fn padding(
        &self,
        style: crate::core::style::ComputedStyle,
        _basis: usize,
    ) -> crate::core::style::EdgeSizes {
        self.styles.resolve_padding_edges(style.padding, 0)
    }

    pub fn new(input: LayoutInput<'a>) -> Self {
        Self {
            document: input.document,
            styles: input.styles,
            forms: input.forms,
            images: input.images,
            cell_metric: input.cell_metric,
            metric_cache: RefCell::new(HashMap::new()),
            output_cache: RefCell::new(HashMap::new()),
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
        let Some(model) = self.build_model(self.document.children(table), limits) else {
            return self.degraded(table, available_width);
        };
        let root = TableRoot {
            owner: Some(table),
            style: self.styles.get(table),
        };
        if model.rows.is_empty() || model.columns == 0 {
            return self.empty_table(root, model);
        }
        let output = self.layout_model(root, model, available_width, limits, nesting);
        if output.width > limits.max_width {
            self.degraded(table, available_width)
        } else {
            output
        }
    }

    pub fn format_atomic(
        &self,
        node: NodeId,
        available_width: usize,
        limits: TableLimits,
        nesting: usize,
    ) -> TableOutput {
        let available_width = available_width.min(limits.max_width);
        if nesting >= limits.max_nesting {
            return self.degraded(node, available_width);
        }
        let style = self.styles.get(node);
        let mut output = self.layout_model(
            TableRoot {
                owner: Some(node),
                style,
            },
            self.atomic_model(node),
            available_width,
            limits,
            nesting,
        );
        let bottom = self
            .padding(style, available_width)
            .bottom
            .saturating_add(style.border.bottom.layout_width());
        output.baseline = Some(output.height.saturating_sub(bottom + 1));
        output
    }

    pub(super) fn format_anonymous(
        &self,
        roots: Vec<NodeId>,
        parent_style: crate::core::style::ComputedStyle,
        inline: bool,
        available_width: usize,
        limits: TableLimits,
        nesting: usize,
    ) -> TableOutput {
        let available_width = available_width.min(limits.max_width);
        if nesting >= limits.max_nesting {
            return self.degraded_roots(&roots, available_width);
        }
        let display = if inline {
            crate::core::style::Display::INLINE_TABLE
        } else {
            crate::core::style::Display::TABLE
        };
        let root = TableRoot {
            owner: None,
            style: crate::core::style::ComputedStyle::anonymous_inheriting(parent_style, display),
        };
        let Some(model) = self.build_model(roots.clone(), limits) else {
            return self.degraded_roots(&roots, available_width);
        };
        if model.rows.is_empty() || model.columns == 0 {
            return self.empty_table(root, model);
        }
        let output = self.layout_model(root, model, available_width, limits, nesting);
        if output.width > limits.max_width {
            self.degraded_roots(&roots, available_width)
        } else {
            output
        }
    }

    pub(super) fn format_inline_atom(
        &self,
        node: NodeId,
        available_width: usize,
        limits: TableLimits,
        nesting: usize,
    ) -> TableOutput {
        let available_width = available_width.min(limits.max_width);
        let key = (node, available_width, limits, nesting);
        if let Some(output) = self.output_cache.borrow().get(&key).cloned() {
            return output;
        }
        let output = if self.styles.get(node).display.is_table() {
            self.format(node, available_width, limits, nesting)
        } else {
            self.format_atomic(node, available_width, limits, nesting)
        };
        self.output_cache.borrow_mut().insert(key, output.clone());
        output
    }

    #[cfg(test)]
    pub(super) fn cached_outputs(&self) -> usize {
        self.output_cache.borrow().len()
    }
}

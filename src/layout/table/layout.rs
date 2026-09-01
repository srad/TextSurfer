use super::captions::{layout_captions, minimum_width};
use super::content::definite_height;
use super::geometry::{TableGeometry, TablePlacement, place_table};
use super::model::TableModel;
use super::sizing::{grow_span, size_columns};
use super::{TableFormatter, TableLimits, TableOutput, TableRoot};

impl TableFormatter<'_> {
    pub(super) fn layout_model(
        &self,
        root: TableRoot,
        model: TableModel,
        available_width: usize,
        limits: TableLimits,
        nesting: usize,
    ) -> TableOutput {
        let table_style = root.style;
        let metrics = self.measure_cells(&model, limits, nesting);
        let geometry = TableGeometry::new(self, table_style, &model, available_width);
        let caption_minimum_width = minimum_width(self, &model, limits, nesting);
        let mut columns = size_columns(
            self,
            &model,
            &metrics,
            table_style,
            available_width,
            &geometry,
            caption_minimum_width,
        );
        let mut cells = self.layout_cells(
            &model,
            &metrics,
            &columns.widths,
            &geometry,
            limits,
            nesting,
        );
        let table_height = definite_height(self.styles, table_style.height)
            .into_iter()
            .chain(definite_height(self.styles, table_style.min_height))
            .max();
        if let Some(table_height) = table_height {
            let row_gap = if geometry.collapsed {
                geometry.grid
            } else {
                geometry.spacing_y
            };
            let overhead = geometry.table_edges.top
                + geometry.table_edges.bottom
                + geometry.table_padding.top
                + geometry.table_padding.bottom
                + row_gap.saturating_mul(model.rows.len() + 1);
            let required = table_height.saturating_sub(overhead);
            let row_count = cells.row_heights.len();
            grow_span(&mut cells.row_heights, 0, row_count, required);
        }
        let collapsed_columns = model
            .column_nodes
            .iter()
            .map(|track| {
                track.column.is_some_and(|node| {
                    self.styles.get(node).visibility == crate::core::style::Visibility::Collapse
                }) || track.group.is_some_and(|node| {
                    self.styles.get(node).visibility == crate::core::style::Visibility::Collapse
                })
            })
            .collect::<Vec<_>>();
        let collapsed_rows = model
            .rows
            .iter()
            .map(|row| {
                row.node.is_some_and(|node| {
                    self.styles.get(node).visibility == crate::core::style::Visibility::Collapse
                }) || row.group_node.is_some_and(|node| {
                    self.styles.get(node).visibility == crate::core::style::Visibility::Collapse
                })
            })
            .collect::<Vec<_>>();
        let column_gap = if geometry.collapsed {
            geometry.grid
        } else {
            geometry.spacing_x
        };
        let removed_width = columns
            .widths
            .iter()
            .zip(&collapsed_columns)
            .filter(|(_, collapsed)| **collapsed)
            .map(|(width, _)| width.saturating_add(column_gap))
            .sum::<usize>();
        for (width, collapsed) in columns.widths.iter_mut().zip(&collapsed_columns) {
            if *collapsed {
                *width = 0;
            }
        }
        columns.table_width = columns.table_width.saturating_sub(removed_width).max(1);
        columns.minimum_width = columns.minimum_width.saturating_sub(removed_width).max(1);
        for (height, collapsed) in cells.row_heights.iter_mut().zip(&collapsed_rows) {
            if *collapsed {
                *height = 0;
            }
        }
        let captions = layout_captions(self, &model, columns.table_width, limits, nesting);
        place_table(
            self,
            &geometry,
            TablePlacement {
                root,
                model,
                table_style,
                columns,
                cells,
                captions,
                collapsed_columns,
                collapsed_rows,
            },
        )
    }
}

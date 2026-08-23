use crate::core::dom::NodeId;

use super::captions::{layout_captions, natural_width};
use super::geometry::{TableGeometry, TablePlacement, place_table};
use super::model::TableModel;
use super::sizing::size_columns;
use super::{TableFormatter, TableLimits, TableOutput};

impl TableFormatter<'_> {
    pub(super) fn layout_model(
        &self,
        table: NodeId,
        model: TableModel,
        available_width: usize,
        limits: TableLimits,
        nesting: usize,
    ) -> TableOutput {
        let table_style = self.styles.get(table);
        let metrics = self.measure_cells(&model, limits, nesting);
        let geometry = TableGeometry::new(self, table_style, &model);
        let caption_natural = natural_width(self, &model, limits, nesting);
        let columns = size_columns(
            self,
            &model,
            &metrics,
            table_style,
            available_width,
            &geometry,
            caption_natural,
        );
        let cells = self.layout_cells(
            &model,
            &metrics,
            &columns.widths,
            &geometry,
            limits,
            nesting,
        );
        let captions = layout_captions(self, &model, columns.table_width, limits, nesting);
        place_table(
            self,
            &geometry,
            TablePlacement {
                table,
                model,
                table_style,
                columns,
                cells,
                captions,
            },
        )
    }
}

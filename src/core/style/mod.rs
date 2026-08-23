mod box_model;
mod color;
mod list;
mod table;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::core::dom::NodeId;

pub use box_model::{
    BorderColor, BorderEdges, BorderLineStyle, BorderSide, BoxSizing, CssPercentage, CssWidth,
    EdgeSizes,
};
pub use color::{CellStyle, Palette, Rgb, Rgba};
pub use list::{ListStylePosition, ListStyleType};
pub use table::{BorderCollapse, BorderSpacing, CaptionSide, TableLayoutMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    None,
    #[default]
    Inline,
    Block,
    ListItem,
    Table,
    InlineTable,
    TableHeaderGroup,
    TableRowGroup,
    TableFooterGroup,
    TableRow,
    TableCell,
    TableColumn,
    TableColumnGroup,
    TableCaption,
}

impl Display {
    pub const fn is_block_container(self) -> bool {
        matches!(self, Self::Block | Self::ListItem)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PseudoElement {
    Before,
    Marker,
    After,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WhiteSpace {
    #[default]
    Normal,
    NoWrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComputedStyle {
    pub display: Display,
    pub white_space: WhiteSpace,
    pub width: CssWidth,
    pub box_sizing: BoxSizing,
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,
    pub border: BorderEdges,
    pub table_layout: TableLayoutMode,
    pub border_collapse: BorderCollapse,
    pub border_spacing: BorderSpacing,
    pub caption_side: CaptionSide,
    pub list_style_type: ListStyleType,
    pub list_style_position: ListStylePosition,
    pub color: Option<Rgba>,
    pub background: Option<Rgb>,
    pub bold: bool,
    pub underline: bool,
    pub strike: bool,
    pub reverse: bool,
}

impl ComputedStyle {
    pub fn cell_style(&self) -> CellStyle {
        CellStyle {
            fg: self.color,
            bg: self.background,
            bold: self.bold,
            underline: self.underline,
            strike: self.strike,
            reverse: self.reverse,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PseudoBox {
    pub text: String,
    pub style: ComputedStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub text: String,
    pub reserve: usize,
    pub position: ListStylePosition,
    pub style: ComputedStyle,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleTree {
    styles: HashMap<NodeId, ComputedStyle>,
    pseudo: HashMap<(NodeId, PseudoElement), PseudoBox>,
    markers: HashMap<NodeId, Marker>,
}

impl StyleTree {
    pub fn insert(&mut self, node: NodeId, style: ComputedStyle) {
        self.styles.insert(node, style);
    }

    pub fn get(&self, node: NodeId) -> ComputedStyle {
        self.styles.get(&node).copied().unwrap_or_default()
    }

    pub fn insert_pseudo(&mut self, node: NodeId, which: PseudoElement, box_: PseudoBox) {
        self.pseudo.insert((node, which), box_);
    }

    pub fn pseudo(&self, node: NodeId, which: PseudoElement) -> Option<&PseudoBox> {
        self.pseudo.get(&(node, which))
    }

    pub fn insert_marker(&mut self, node: NodeId, marker: Marker) {
        self.markers.insert(node, marker);
    }

    pub fn marker(&self, node: NodeId) -> Option<&Marker> {
        self.markers.get(&node)
    }
}

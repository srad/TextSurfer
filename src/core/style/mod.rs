mod alignment;
mod box_model;
mod color;
mod display;
mod flex;
mod grid;
mod length;
mod list;
mod math;
mod render;
mod table;
mod typography;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::core::dom::NodeId;

pub use alignment::{
    Alignment, AlignmentSafety, AlignmentStyle, ContentAlignment, CssGap, ItemAlignment,
};
pub use box_model::{
    BorderColor, BorderEdges, BorderLineStyle, BorderSide, BoxSizing, CssInset, CssMargin,
    CssMaxSize, CssPercentage, CssSize, CssWidth, EdgeSizes, InsetEdges, MarginEdges, Overflow,
    OverflowAxes, Position,
};
pub use color::{CellStyle, Palette, Rgb, Rgba};
pub use display::{
    Display, DisplayBox, DisplayInside, DisplayInternal, DisplayMode, DisplayOutside, Visibility,
};
pub use flex::{AxisCellLength, FlexBasis, FlexDirection, FlexStyle, FlexWrap};
pub(crate) use grid::GridStore;
pub use grid::{
    GridArea, GridAreas, GridAreasData, GridAutoFlow, GridIdent, GridLength, GridLines,
    GridPlacement, GridRepeat, GridStyle, GridTemplate, GridTemplateComponent, GridTemplateData,
    GridTracks, RepeatCount, TrackBreadthMax, TrackBreadthMin, TrackSize,
};
pub use length::{CellMetric, CssLength, CssLengthUnit, CssNumber, LengthAxis};
pub use list::{ListStylePosition, ListStyleType};
pub use math::CssCalc;
pub(crate) use math::{CssCalcExpr, CssCalcStore};
pub use render::{RenderContext, RenderMetrics};
pub use table::{BorderCollapse, BorderSpacing, CaptionSide, TableLayoutMode};
pub use typography::{FontSize, TextPresentation, TextRendering};

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
pub enum Cursor {
    #[default]
    Auto,
    Default,
    None,
    ContextMenu,
    Help,
    Pointer,
    Progress,
    Wait,
    Cell,
    Crosshair,
    Text,
    VerticalText,
    Alias,
    Copy,
    Move,
    NoDrop,
    NotAllowed,
    Grab,
    Grabbing,
    EResize,
    NResize,
    NeResize,
    NwResize,
    SResize,
    SeResize,
    SwResize,
    WResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ColResize,
    RowResize,
    AllScroll,
    ZoomIn,
    ZoomOut,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    Left,
    Right,
    Center,
    Justify,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LegacyAlign {
    #[default]
    None,
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComputedStyle {
    pub display: Display,
    pub white_space: WhiteSpace,
    pub cursor: Cursor,
    pub width: CssSize,
    pub height: CssSize,
    pub min_width: CssSize,
    pub min_height: CssSize,
    pub max_width: CssMaxSize,
    pub max_height: CssMaxSize,
    pub flex: FlexStyle,
    pub alignment: AlignmentStyle,
    pub grid: GridStyle,
    pub order: i32,
    pub box_sizing: BoxSizing,
    pub overflow: OverflowAxes,
    pub visibility: Visibility,
    pub position: Position,
    pub inset: InsetEdges,
    pub margin: MarginEdges,
    pub padding: EdgeSizes,
    pub border: BorderEdges,
    pub table_layout: TableLayoutMode,
    pub border_collapse: BorderCollapse,
    pub border_spacing: BorderSpacing,
    pub caption_side: CaptionSide,
    pub text_align: TextAlign,
    pub vertical_align: VerticalAlign,
    pub(crate) legacy_align: LegacyAlign,
    pub list_style_type: ListStyleType,
    pub list_style_position: ListStylePosition,
    pub color: Option<Rgba>,
    pub background: Option<Rgb>,
    pub bold: bool,
    pub underline: bool,
    pub strike: bool,
    pub reverse: bool,
    pub font_size: FontSize,
    pub text_presentation: TextPresentation,
}

impl ComputedStyle {
    pub fn anonymous_inheriting(parent: Self, display: Display) -> Self {
        Self {
            display,
            white_space: parent.white_space,
            cursor: parent.cursor,
            visibility: parent.visibility,
            list_style_type: parent.list_style_type,
            list_style_position: parent.list_style_position,
            border_collapse: parent.border_collapse,
            border_spacing: parent.border_spacing,
            caption_side: parent.caption_side,
            text_align: parent.text_align,
            legacy_align: parent.legacy_align,
            color: parent.color,
            bold: parent.bold,
            underline: parent.underline,
            strike: parent.strike,
            reverse: parent.reverse,
            font_size: parent.font_size,
            text_presentation: parent.text_presentation,
            ..Default::default()
        }
    }

    pub fn cell_style(&self) -> CellStyle {
        CellStyle {
            fg: self.color,
            bg: self.background,
            bold: self.bold,
            underline: self.underline,
            strike: self.strike,
            reverse: self.reverse,
            dim: self.text_presentation.dim,
            scale: self.text_presentation.scale,
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

/// The variable-length payloads that [`ComputedStyle`] refers to by handle, so it can stay `Copy`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StyleStore {
    pub(crate) calculations: CssCalcStore,
    pub(crate) grid: GridStore,
}

#[derive(Clone, Copy)]
pub(crate) struct StyleStoreCheckpoint {
    calculations: math::CssCalcCheckpoint,
    grid: grid::GridStoreCheckpoint,
}

impl StyleStore {
    pub(crate) fn checkpoint(&self) -> StyleStoreCheckpoint {
        StyleStoreCheckpoint {
            calculations: self.calculations.checkpoint(),
            grid: self.grid.checkpoint(),
        }
    }

    pub(crate) fn rollback(&mut self, checkpoint: StyleStoreCheckpoint) {
        self.calculations.rollback(checkpoint.calculations);
        self.grid.rollback(checkpoint.grid);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleTree {
    styles: HashMap<NodeId, ComputedStyle>,
    pseudo: HashMap<(NodeId, PseudoElement), PseudoBox>,
    markers: HashMap<NodeId, Marker>,
    store: StyleStore,
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

    pub(crate) fn set_store(&mut self, store: StyleStore) {
        self.store = store;
    }

    pub fn resolve_calc(&self, value: CssCalc, basis: f32) -> Option<f32> {
        self.store.calculations.resolve(value, basis)
    }

    pub(crate) fn grid(&self) -> &GridStore {
        &self.store.grid
    }
}

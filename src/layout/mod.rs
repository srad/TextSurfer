mod clip;
pub mod engine;
mod replaced;
mod restyle;
mod table;
mod text_flow;

pub use engine::{
    BackgroundFill, BorderEdge, BorderStroke, BoxTree, ImagePlacement, LayoutBox, LayoutEngine,
    LayoutLimits, LayoutRect, LinkBox, TaffyLayoutEngine, TextFragment,
};
pub(crate) use restyle::{RestyleFailure, restyle, restyle_nodes};

use crate::core::dom::Document;
use crate::core::form::FormState;
use crate::core::style::StyleTree;

/// The three things every layout pass reads and none of it writes: the tree, its computed styles,
/// and the user's form edits.
///
/// They are one parameter rather than three because they are always passed together and never
/// independently — and because three adjacent shared references invite a silent swap at a call
/// site that still compiles. Naming them at the construction site makes that unrepresentable.
#[derive(Clone, Copy)]
pub(crate) struct LayoutInput<'a> {
    pub(crate) document: &'a Document,
    pub(crate) styles: &'a StyleTree,
    pub(crate) forms: &'a FormState,
    pub(crate) images: Option<&'a crate::core::image::ImageResources>,
    pub(crate) cell_metric: crate::core::style::CellMetric,
}

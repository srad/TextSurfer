use crate::layout::BoxTree;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayList;

pub trait Painter: Send + Sync {
    fn paint(&self, box_tree: &BoxTree) -> DisplayList;
}

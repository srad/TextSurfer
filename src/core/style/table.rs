#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableLayoutMode {
    #[default]
    Auto,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderCollapse {
    #[default]
    Separate,
    Collapse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BorderSpacing {
    pub horizontal: usize,
    pub vertical: usize,
}

impl BorderSpacing {
    pub const fn new(horizontal: usize, vertical: usize) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptionSide {
    #[default]
    Top,
    Bottom,
}

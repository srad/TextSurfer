#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub col: u16,
    pub row: u16,
}

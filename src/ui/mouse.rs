use crate::core::geom::{Point, Size};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseZone {
    Tabs,
    Address,
    Content,
    Outside,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChromeGeometry {
    pub size: Size,
    pub tabs_rows: u16,
    pub toolbar_rows: u16,
    pub status_rows: u16,
}

impl ChromeGeometry {
    pub fn for_size(size: Size) -> Self {
        Self {
            size,
            tabs_rows: 1,
            toolbar_rows: 1,
            status_rows: 1,
        }
    }

    pub fn content_rows(&self) -> usize {
        self.size
            .rows
            .saturating_sub(self.tabs_rows + self.toolbar_rows + self.status_rows) as usize
    }

    pub fn zone_at(&self, at: Point) -> MouseZone {
        if at.row >= self.size.rows || at.col >= self.size.cols {
            return MouseZone::Outside;
        }
        if at.row < self.tabs_rows {
            MouseZone::Tabs
        } else if at.row < self.tabs_rows + self.toolbar_rows {
            MouseZone::Address
        } else {
            MouseZone::Content
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> ChromeGeometry {
        ChromeGeometry::for_size(Size { cols: 80, rows: 24 })
    }

    #[test]
    fn row_zones_split_tabs_toolbar_and_content() {
        let g = geometry();
        assert_eq!(g.zone_at(Point { col: 5, row: 0 }), MouseZone::Tabs);
        assert_eq!(g.zone_at(Point { col: 5, row: 1 }), MouseZone::Address);
        assert_eq!(g.zone_at(Point { col: 5, row: 2 }), MouseZone::Content);
        assert_eq!(g.zone_at(Point { col: 5, row: 23 }), MouseZone::Content);
    }

    #[test]
    fn points_outside_the_terminal_are_outside() {
        let g = geometry();
        assert_eq!(g.zone_at(Point { col: 80, row: 0 }), MouseZone::Outside);
        assert_eq!(g.zone_at(Point { col: 0, row: 24 }), MouseZone::Outside);
    }

    #[test]
    fn content_rows_track_the_chrome_height() {
        let g = geometry();
        assert_eq!(g.content_rows(), 21);
        let small = ChromeGeometry::for_size(Size { cols: 40, rows: 4 });
        assert_eq!(small.content_rows(), 1);
        let tiny = ChromeGeometry::for_size(Size { cols: 40, rows: 2 });
        assert_eq!(tiny.content_rows(), 0);
    }
}

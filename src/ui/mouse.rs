use crate::core::geom::{Point, Size};
use ratatui::layout::{Position, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseZone {
    Menu,
    Tabs,
    Address,
    Content,
    Outside,
}

pub const CHROME_ROWS: u16 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChromeLayout {
    pub menu: Option<Rect>,
    pub tabs: Option<Rect>,
    pub tab_divider: Option<Rect>,
    pub toolbar: Option<Rect>,
    pub toolbar_divider: Option<Rect>,
    pub content: Option<Rect>,
    pub status: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChromeGeometry {
    pub size: Size,
    pub menu_rows: u16,
    pub tabs_rows: u16,
    pub toolbar_rows: u16,
    pub status_rows: u16,
}

impl ChromeGeometry {
    pub fn for_size(size: Size) -> Self {
        Self {
            size,
            menu_rows: 1,
            tabs_rows: 2,
            toolbar_rows: 2,
            status_rows: 1,
        }
    }

    pub fn content_rows(&self) -> usize {
        self.size.rows.saturating_sub(CHROME_ROWS) as usize
    }

    pub fn content_cols(&self) -> usize {
        self.size.cols.saturating_sub(2) as usize
    }

    pub fn zone_at(&self, at: Point) -> MouseZone {
        if at.row >= self.size.rows || at.col >= self.size.cols {
            return MouseZone::Outside;
        }
        let position = Position::new(at.col, at.row);
        let layout = self.layout(Rect::new(0, 0, self.size.cols, self.size.rows));
        if layout.menu.is_some_and(|rect| rect.contains(position)) {
            MouseZone::Menu
        } else if layout
            .tabs
            .into_iter()
            .chain(layout.tab_divider)
            .any(|rect| rect.contains(position))
        {
            MouseZone::Tabs
        } else if layout
            .toolbar
            .into_iter()
            .chain(layout.toolbar_divider)
            .any(|rect| rect.contains(position))
        {
            MouseZone::Address
        } else if layout.content.is_some_and(|rect| rect.contains(position)) {
            MouseZone::Content
        } else {
            MouseZone::Outside
        }
    }

    pub fn layout(&self, area: Rect) -> ChromeLayout {
        let band =
            |row: u16| (row < area.height).then(|| Rect::new(area.x, area.y + row, area.width, 1));
        let content_rows = area.height.saturating_sub(CHROME_ROWS);
        ChromeLayout {
            menu: band(0),
            tabs: band(1),
            tab_divider: band(2),
            toolbar: band(3),
            toolbar_divider: band(4),
            content: (content_rows > 0)
                .then(|| Rect::new(area.x, area.y + 5, area.width, content_rows)),
            status: (area.height >= CHROME_ROWS)
                .then(|| Rect::new(area.x, area.bottom() - 1, area.width, 1)),
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
    fn row_zones_split_menu_tabs_toolbar_and_content() {
        let g = geometry();
        assert_eq!(g.zone_at(Point { col: 5, row: 0 }), MouseZone::Menu);
        assert_eq!(g.zone_at(Point { col: 5, row: 1 }), MouseZone::Tabs);
        assert_eq!(g.zone_at(Point { col: 5, row: 2 }), MouseZone::Tabs);
        assert_eq!(g.zone_at(Point { col: 5, row: 3 }), MouseZone::Address);
        assert_eq!(g.zone_at(Point { col: 5, row: 4 }), MouseZone::Address);
        assert_eq!(g.zone_at(Point { col: 5, row: 5 }), MouseZone::Content);
        assert_eq!(g.zone_at(Point { col: 5, row: 6 }), MouseZone::Content);
        assert_eq!(g.zone_at(Point { col: 5, row: 23 }), MouseZone::Outside);
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
        assert_eq!(g.content_rows(), 18);
        let small = ChromeGeometry::for_size(Size { cols: 40, rows: 8 });
        assert_eq!(small.content_rows(), 2);
        let tiny = ChromeGeometry::for_size(Size { cols: 40, rows: 2 });
        assert_eq!(tiny.content_rows(), 0);
    }

    #[test]
    fn content_cols_are_the_frame_interior() {
        let g = geometry();
        assert_eq!(g.content_cols(), 78);
        let narrow = ChromeGeometry::for_size(Size { cols: 2, rows: 24 });
        assert_eq!(narrow.content_cols(), 0);
    }
}

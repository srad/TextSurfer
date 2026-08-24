use crate::core::geom::{Point, Size};
use crate::ui::widgets::menu::{popup_item_at, title_at};
use crate::ui::widgets::tabs::{TabChip, TabSlot, tab_at};
use crate::ui::widgets::toolbar::{BUTTON_COUNT, BUTTON_PITCH};
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

/// Document rows one wheel notch moves. Frontends size a pixel-delta notch by it, so a
/// trackpad and a wheel travel the same distance.
pub const WHEEL_ROWS: i32 = 3;

/// Which `[‹][›][↻][⌂]` button covers a toolbar column, if any. The glyph and its two
/// brackets are all live, matching what the eye reads as one button.
fn button_at(col: u16) -> Option<usize> {
    let offset = col.checked_sub(1)?;
    let index = offset / BUTTON_PITCH;
    (index < BUTTON_COUNT && offset % BUTTON_PITCH < 3).then_some(usize::from(index))
}

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

/// What sits under the pointer, in terms the controller can act on without knowing any
/// widget geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromeTarget {
    MenuTitle(usize),
    MenuItem(usize),
    Tab(usize),
    NewTab,
    ToolbarButton(usize),
    /// A cell of the address field, as a column offset inside the toolbar row.
    Address {
        col: u16,
    },
    /// A cell of the viewport, as offsets inside the content view.
    Content {
        col: u16,
        row: u16,
    },
    Inert,
}

/// The chrome state a hit test needs but the geometry does not own.
pub struct ChromeState<'a> {
    pub tabs: &'a [TabChip<'a>],
    pub active_tab: usize,
    pub open_menu: Option<usize>,
}

/// Where the document is painted on screen, and how much of it is visible.
///
/// `origin` is the cell that holds document cell `(0, scroll)`; the frame rails sit
/// outside it. Every screen-to-document conversion goes through this, so the painter
/// and the pointer cannot disagree about which row a cell belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentView {
    pub origin: Point,
    pub cols: u16,
    pub rows: u16,
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

    pub fn content_view(&self) -> Option<ContentView> {
        self.content_view_in(Rect::new(0, 0, self.size.cols, self.size.rows))
    }

    /// The same view for an area that is not the geometry's own size, which the chrome
    /// widgets are rendered into by several tests.
    pub fn content_view_in(&self, area: Rect) -> Option<ContentView> {
        let rect = self.layout(area).content?;
        let cols = rect.width.checked_sub(2)?;
        if cols == 0 || rect.height == 0 {
            return None;
        }
        Some(ContentView {
            origin: Point {
                col: rect.x + 1,
                row: rect.y,
            },
            cols,
            rows: rect.height,
        })
    }

    /// Resolve a pointer cell to the thing the user is pointing at.
    ///
    /// An open dropdown is tested first: it overlays the zones below it, so a point
    /// inside it belongs to the menu however `zone_at` classifies the cell. Divider rows
    /// are inert — only the rows a widget actually writes are live.
    pub fn target_at(&self, at: Point, chrome: ChromeState<'_>) -> ChromeTarget {
        let area = Rect::new(0, 0, self.size.cols, self.size.rows);
        let position = Position::new(at.col, at.row);
        if let Some(menu) = chrome.open_menu
            && let Some(item) = popup_item_at(area, menu, position)
        {
            return ChromeTarget::MenuItem(item);
        }
        let layout = self.layout(area);
        match self.zone_at(at) {
            MouseZone::Menu => title_at(at.col, self.size.cols)
                .map_or(ChromeTarget::Inert, ChromeTarget::MenuTitle),
            MouseZone::Tabs => layout
                .tabs
                .filter(|rect| rect.contains(position) && rect.width >= 2)
                .and_then(|rect| {
                    tab_at(
                        chrome.tabs,
                        chrome.active_tab,
                        rect.width - 2,
                        at.col.checked_sub(rect.x + 1)?,
                    )
                })
                .map_or(ChromeTarget::Inert, |slot| match slot {
                    TabSlot::Tab(index) => ChromeTarget::Tab(index),
                    TabSlot::NewTab => ChromeTarget::NewTab,
                }),
            MouseZone::Address => layout
                .toolbar
                .filter(|rect| rect.contains(position))
                .map_or(ChromeTarget::Inert, |rect| {
                    let col = at.col - rect.x;
                    match button_at(col) {
                        Some(button) => ChromeTarget::ToolbarButton(button),
                        None => ChromeTarget::Address { col },
                    }
                }),
            MouseZone::Content => self
                .content_view()
                .filter(|view| {
                    at.col >= view.origin.col
                        && at.col < view.origin.col + view.cols
                        && at.row >= view.origin.row
                        && at.row < view.origin.row + view.rows
                })
                .map_or(ChromeTarget::Inert, |view| ChromeTarget::Content {
                    col: at.col - view.origin.col,
                    row: at.row - view.origin.row,
                }),
            MouseZone::Outside => ChromeTarget::Inert,
        }
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

    fn chips() -> Vec<TabChip<'static>> {
        vec![
            TabChip {
                title: "one".into(),
                url: "https://one.example".into(),
            },
            TabChip {
                title: "two".into(),
                url: "https://two.example".into(),
            },
        ]
    }

    fn target(at: Point, open_menu: Option<usize>) -> ChromeTarget {
        let chips = chips();
        geometry().target_at(
            at,
            ChromeState {
                tabs: &chips,
                active_tab: 0,
                open_menu,
            },
        )
    }

    #[test]
    fn the_toolbar_row_splits_into_buttons_and_the_field() {
        for (col, button) in [(1, 0), (3, 0), (5, 1), (13, 3)] {
            assert_eq!(
                target(Point { col, row: 3 }, None),
                ChromeTarget::ToolbarButton(button),
                "column {col}",
            );
        }
        // The gap between two brackets belongs to neither button.
        assert_eq!(
            target(Point { col: 4, row: 3 }, None),
            ChromeTarget::Address { col: 4 }
        );
        assert_eq!(
            target(Point { col: 30, row: 3 }, None),
            ChromeTarget::Address { col: 30 }
        );
    }

    #[test]
    fn divider_rows_carry_no_target() {
        assert_eq!(target(Point { col: 5, row: 2 }, None), ChromeTarget::Inert);
        assert_eq!(target(Point { col: 5, row: 4 }, None), ChromeTarget::Inert);
    }

    #[test]
    fn content_cells_are_reported_relative_to_the_view() {
        assert_eq!(
            target(Point { col: 1, row: 5 }, None),
            ChromeTarget::Content { col: 0, row: 0 }
        );
        assert_eq!(
            target(Point { col: 78, row: 22 }, None),
            ChromeTarget::Content { col: 77, row: 17 }
        );
        // The frame rails are not the document.
        assert_eq!(target(Point { col: 0, row: 5 }, None), ChromeTarget::Inert);
        assert_eq!(target(Point { col: 79, row: 5 }, None), ChromeTarget::Inert);
    }

    #[test]
    fn tab_chips_and_the_new_tab_hint_resolve_to_their_slots() {
        assert_eq!(target(Point { col: 2, row: 1 }, None), ChromeTarget::Tab(0));
        let strip = crate::ui::widgets::tabs::layout_tabs(&chips(), 0, 78);
        let hint = strip.last().expect("a new-tab hint");
        assert_eq!(
            target(
                Point {
                    col: 1 + hint.x,
                    row: 1
                },
                None
            ),
            ChromeTarget::NewTab
        );
    }

    #[test]
    fn an_open_dropdown_wins_over_the_rows_it_covers() {
        // Row 5 is content until the File menu is open over it, and the popup's own
        // bottom border on row 6 is not an item.
        assert_eq!(
            target(Point { col: 3, row: 5 }, None),
            ChromeTarget::Content { col: 2, row: 0 }
        );
        assert_eq!(
            target(Point { col: 3, row: 5 }, Some(0)),
            ChromeTarget::MenuItem(3)
        );
        assert_eq!(
            target(Point { col: 3, row: 6 }, Some(0)),
            ChromeTarget::Content { col: 2, row: 1 }
        );
        assert_eq!(
            target(Point { col: 3, row: 0 }, Some(0)),
            ChromeTarget::MenuTitle(0)
        );
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
    fn the_content_view_is_the_cells_the_content_widget_writes() {
        let view = geometry().content_view().expect("a content view");
        assert_eq!(view.origin, Point { col: 1, row: 5 });
        assert_eq!(view.cols, 78);
        assert_eq!(view.rows, 18);
        assert_eq!(usize::from(view.cols), geometry().content_cols());
        assert_eq!(usize::from(view.rows), geometry().content_rows());
    }

    #[test]
    fn a_window_without_room_for_content_has_no_view() {
        let short = ChromeGeometry::for_size(Size { cols: 40, rows: 6 });
        assert_eq!(short.content_view(), None);
        let narrow = ChromeGeometry::for_size(Size { cols: 2, rows: 24 });
        assert_eq!(narrow.content_view(), None);
    }

    #[test]
    fn one_content_row_is_enough_for_a_view() {
        let geometry = ChromeGeometry::for_size(Size { cols: 40, rows: 7 });
        let view = geometry.content_view().expect("a single row is usable");
        assert_eq!(view.rows, 1);
        assert_eq!(view.origin, Point { col: 1, row: 5 });
    }

    #[test]
    fn content_cols_are_the_frame_interior() {
        let g = geometry();
        assert_eq!(g.content_cols(), 78);
        let narrow = ChromeGeometry::for_size(Size { cols: 2, rows: 24 });
        assert_eq!(narrow.content_cols(), 0);
    }
}

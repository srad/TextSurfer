use crate::core::geom::{Point, Size};
use crate::ui::widgets::menu::{popup_item_at, title_at};
use crate::ui::widgets::tabs::{TabChip, TabSlot, tab_at};
use crate::ui::widgets::toolbar::{
    EXPANDED_MIN_ROWS, EXPANDED_MIN_WIDTH, ToolbarTarget, layout_toolbar,
};
use ratatui::layout::{Position, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseZone {
    Menu,
    Tabs,
    Address,
    Content,
    Outside,
}

pub const COMPACT_CHROME_ROWS: u16 = 6;
pub const EXPANDED_CHROME_ROWS: u16 = 8;

/// Document rows one wheel notch moves. Frontends size a pixel-delta notch by it, so a
/// trackpad and a wheel travel the same distance.
pub const WHEEL_ROWS: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChromeLayout {
    pub menu: Option<Rect>,
    pub tabs: Option<Rect>,
    pub tab_divider: Option<Rect>,
    pub toolbar: Option<Rect>,
    pub toolbar_divider: Option<Rect>,
    pub content: Option<Rect>,
    /// The content band's right-hand column, which the page scrollbar takes over from
    /// the frame rail.
    pub scrollbar: Option<Rect>,
    pub status: Option<Rect>,
}

/// What sits under the pointer, in terms the controller can act on without knowing any
/// widget geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromeTarget {
    MenuTitle(usize),
    MenuItem(usize),
    Tab(usize),
    /// A tab's close box, which is inside the chip but is not the chip.
    TabClose(usize),
    NewTab,
    ToolbarButton(usize),
    /// A cell of the page scrollbar, as a row offset inside the content band.
    Scrollbar {
        row: u16,
    },
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
}

impl ChromeGeometry {
    pub fn for_size(size: Size) -> Self {
        Self { size }
    }

    pub fn content_rows(&self) -> usize {
        self.layout(Rect::new(0, 0, self.size.cols, self.size.rows))
            .content
            .map_or(0, |rect| usize::from(rect.height))
    }

    pub fn content_cols(&self) -> usize {
        self.size.cols.saturating_sub(2) as usize
    }

    pub fn content_view(&self) -> Option<ContentView> {
        self.content_view_in(Rect::new(0, 0, self.size.cols, self.size.rows))
    }

    /// The column the page scrollbar owns, for its own size.
    ///
    /// The painter and the pointer both measure the track from this, so they cannot
    /// disagree about how tall it is.
    pub fn scrollbar_rect(&self) -> Option<Rect> {
        self.layout(Rect::new(0, 0, self.size.cols, self.size.rows))
            .scrollbar
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
    /// are inert — only the rows a widget actually writes are live. The scrollbar is a
    /// target inside the content zone rather than a zone of its own, the way toolbar
    /// buttons live inside the address zone.
    pub fn target_at(&self, at: Point, chrome: ChromeState<'_>) -> ChromeTarget {
        let area = Rect::new(0, 0, self.size.cols, self.size.rows);
        let position = Position::new(at.col, at.row);
        if let Some(menu) = chrome.open_menu
            && let Some(item) = popup_item_at(area, menu, position)
        {
            return ChromeTarget::MenuItem(item);
        }
        let layout = self.layout(area);
        let zone = self.zone_at(at);
        if zone == MouseZone::Content
            && let Some(scrollbar) = layout.scrollbar
            && scrollbar.contains(position)
        {
            return ChromeTarget::Scrollbar {
                row: at.row - scrollbar.y,
            };
        }
        match zone {
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
                    TabSlot::Close(index) => ChromeTarget::TabClose(index),
                    TabSlot::NewTab => ChromeTarget::NewTab,
                }),
            MouseZone::Address => layout
                .toolbar
                .filter(|rect| rect.contains(position))
                .map_or(ChromeTarget::Inert, |rect| {
                    match layout_toolbar(rect).target_at(position) {
                        Some(ToolbarTarget::Button(button)) => ChromeTarget::ToolbarButton(button),
                        Some(ToolbarTarget::Address { col }) => ChromeTarget::Address { col },
                        None => ChromeTarget::Inert,
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
        let expanded = area.width >= EXPANDED_MIN_WIDTH && area.height >= EXPANDED_MIN_ROWS;
        let toolbar_height = if expanded { 3 } else { 1 };
        let divider_row = 3 + toolbar_height;
        let content_row = divider_row + 1;
        let chrome_rows = if expanded {
            EXPANDED_CHROME_ROWS
        } else {
            COMPACT_CHROME_ROWS
        };
        let content_rows = area.height.saturating_sub(chrome_rows);
        let content = (content_rows > 0)
            .then(|| Rect::new(area.x, area.y + content_row, area.width, content_rows));
        ChromeLayout {
            menu: band(0),
            tabs: band(1),
            tab_divider: band(2),
            toolbar: (3 < area.height).then(|| {
                Rect::new(
                    area.x,
                    area.y + 3,
                    area.width,
                    toolbar_height.min(area.height - 3),
                )
            }),
            toolbar_divider: band(divider_row),
            content,
            scrollbar: content
                .filter(|rect| rect.width >= 2)
                .map(|rect| Rect::new(rect.right() - 1, rect.y, 1, rect.height)),
            status: (area.height >= chrome_rows)
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
        assert_eq!(g.zone_at(Point { col: 5, row: 5 }), MouseZone::Address);
        assert_eq!(g.zone_at(Point { col: 5, row: 6 }), MouseZone::Address);
        assert_eq!(g.zone_at(Point { col: 5, row: 7 }), MouseZone::Content);
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
    fn the_toolbar_rows_split_into_buttons_and_the_field() {
        for (col, button) in [(2, 0), (6, 0), (8, 1), (24, 3)] {
            for row in 3..=5 {
                assert_eq!(
                    target(Point { col, row }, None),
                    ChromeTarget::ToolbarButton(button),
                    "cell {col},{row}",
                );
            }
        }
        assert_eq!(target(Point { col: 7, row: 3 }, None), ChromeTarget::Inert);
        assert_eq!(target(Point { col: 1, row: 4 }, None), ChromeTarget::Inert);
        assert_eq!(target(Point { col: 78, row: 4 }, None), ChromeTarget::Inert);
        assert_eq!(
            target(Point { col: 30, row: 3 }, None),
            ChromeTarget::Address { col: 30 }
        );
    }

    #[test]
    fn divider_rows_carry_no_target() {
        assert_eq!(target(Point { col: 5, row: 2 }, None), ChromeTarget::Inert);
        assert_eq!(target(Point { col: 5, row: 6 }, None), ChromeTarget::Inert);
    }

    #[test]
    fn content_cells_are_reported_relative_to_the_view() {
        assert_eq!(
            target(Point { col: 1, row: 7 }, None),
            ChromeTarget::Content { col: 0, row: 0 }
        );
        assert_eq!(
            target(Point { col: 78, row: 22 }, None),
            ChromeTarget::Content { col: 77, row: 15 }
        );
        // The left rail is not the document, and the right one is the scrollbar.
        assert_eq!(target(Point { col: 0, row: 7 }, None), ChromeTarget::Inert);
        assert_eq!(
            target(Point { col: 79, row: 7 }, None),
            ChromeTarget::Scrollbar { row: 0 }
        );
    }

    #[test]
    fn the_scrollbar_owns_the_right_hand_column_of_the_content_band() {
        for (row, expected) in [(7u16, 0u16), (22, 15)] {
            assert_eq!(
                target(Point { col: 79, row }, None),
                ChromeTarget::Scrollbar { row: expected },
                "screen row {row}",
            );
        }
        // Above and below the band it is chrome, not bar.
        assert_ne!(
            target(Point { col: 79, row: 6 }, None),
            ChromeTarget::Scrollbar { row: 0 }
        );
        assert_eq!(
            target(Point { col: 79, row: 23 }, None),
            ChromeTarget::Inert
        );
    }

    #[test]
    fn an_open_dropdown_does_not_swallow_the_scrollbar_beside_it() {
        // The popup is anchored under its title and never reaches the right edge, so
        // the bar stays live while a menu is open.
        assert_eq!(
            target(Point { col: 79, row: 7 }, Some(0)),
            ChromeTarget::Scrollbar { row: 0 }
        );
    }

    #[test]
    fn a_window_too_narrow_for_a_frame_has_no_scrollbar() {
        assert_eq!(
            ChromeGeometry::for_size(Size { cols: 1, rows: 24 }).scrollbar_rect(),
            None
        );
        let bandless = ChromeGeometry::for_size(Size { cols: 40, rows: 6 });
        assert_eq!(bandless.scrollbar_rect(), None);
    }

    #[test]
    fn the_scrollbar_column_is_the_bands_last_and_as_tall_as_the_view() {
        let g = geometry();
        let rect = g.scrollbar_rect().expect("a scrollbar column");
        let view = g.content_view().expect("a content view");
        assert_eq!(rect.x, view.origin.col + view.cols);
        assert_eq!(rect.width, 1);
        assert_eq!(rect.y, view.origin.row);
        assert_eq!(rect.height, view.rows);
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
        // Row 5 is toolbar until the File menu opens over it; its bottom border is inert.
        assert_eq!(
            target(Point { col: 3, row: 5 }, None),
            ChromeTarget::ToolbarButton(0)
        );
        assert_eq!(
            target(Point { col: 3, row: 5 }, Some(0)),
            ChromeTarget::MenuItem(3)
        );
        assert_eq!(
            target(Point { col: 3, row: 6 }, Some(0)),
            ChromeTarget::Inert
        );
        assert_eq!(
            target(Point { col: 3, row: 0 }, Some(0)),
            ChromeTarget::MenuTitle(0)
        );
    }

    #[test]
    fn content_rows_track_the_chrome_height() {
        let g = geometry();
        assert_eq!(g.content_rows(), 16);
        let small = ChromeGeometry::for_size(Size { cols: 40, rows: 8 });
        assert_eq!(small.content_rows(), 2);
        let tiny = ChromeGeometry::for_size(Size { cols: 40, rows: 2 });
        assert_eq!(tiny.content_rows(), 0);
    }

    #[test]
    fn expanded_toolbar_requires_both_width_and_height() {
        let narrow = ChromeGeometry::for_size(Size { cols: 46, rows: 9 });
        let short = ChromeGeometry::for_size(Size { cols: 47, rows: 8 });
        let expanded = ChromeGeometry::for_size(Size { cols: 47, rows: 9 });
        let narrow_area = Rect::new(0, 0, narrow.size.cols, narrow.size.rows);
        let short_area = Rect::new(0, 0, short.size.cols, short.size.rows);
        let expanded_area = Rect::new(0, 0, expanded.size.cols, expanded.size.rows);
        assert_eq!(narrow.layout(narrow_area).toolbar.unwrap().height, 1);
        assert_eq!(short.layout(short_area).toolbar.unwrap().height, 1);
        assert_eq!(expanded.layout(expanded_area).toolbar.unwrap().height, 3);
    }

    #[test]
    fn the_content_view_is_the_cells_the_content_widget_writes() {
        let view = geometry().content_view().expect("a content view");
        assert_eq!(view.origin, Point { col: 1, row: 7 });
        assert_eq!(view.cols, 78);
        assert_eq!(view.rows, 16);
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

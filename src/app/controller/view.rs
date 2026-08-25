use std::borrow::Cow;

use crate::core::focus::Focus;
use crate::ui::chrome::ChromeView;
use crate::ui::widgets::content::ContentLines;
use crate::ui::widgets::status::StatusView;
use crate::ui::widgets::tabs::TabChip;

use super::App;

impl App {
    /// The tab strip's chips, shared by the renderer and the pointer hit test so both
    /// measure the same boxes.
    pub(super) fn tab_chips(&self) -> Vec<TabChip<'_>> {
        self.tabs
            .tabs()
            .iter()
            .map(|tab| TabChip {
                title: Cow::Borrowed(tab.title.as_str()),
                url: Cow::Borrowed(tab.url.as_str()),
            })
            .collect()
    }

    pub fn chrome_view(&self) -> ChromeView<'_> {
        let active = self.tabs.active();
        let address = if self.focus == Focus::Address {
            Cow::Borrowed(self.address.text())
        } else {
            Cow::Borrowed(active.url.as_str())
        };
        ChromeView {
            geometry: self.geometry,
            theme: *self.theme(),
            theme_index: self.theme_index(),
            can_back: active.history_pos > 0,
            can_forward: active.history_pos + 1 < active.history.len(),
            address,
            address_cursor: self.address.cursor(),
            address_focused: self.focus == Focus::Address,
            menu_open: self.menu_open,
            menu_active: self.menu_active,
            menu_item: self.menu_item,
            tabs: self.tab_chips(),
            active_tab: self.tabs.active_index(),
            content: ContentLines {
                painted: &active.painted,
                scroll: active.scroll,
            },
            status: StatusView {
                url: Cow::Borrowed(active.url.as_str()),
                message: Cow::Borrowed(active.message.as_str()),
                hover: self.hovered_href().map(Cow::Borrowed),
            },
        }
    }
}

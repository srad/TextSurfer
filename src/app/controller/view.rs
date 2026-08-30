use std::borrow::Cow;

use crate::core::focus::Focus;
use crate::pipeline::render::RenderStage;
use crate::ui::chrome::ChromeView;
use crate::ui::chrome::TextFieldMenuView;
use crate::ui::widgets::content::ContentLines;
use crate::ui::widgets::status::{LoadProgress, LoadProgressAmount, StatusView};
use crate::ui::widgets::tabs::TabChip;
use crate::ui::widgets::text_field::TextFieldView;

use super::App;
use super::pointer::TextFieldTarget;

impl App {
    pub(super) fn load_progress(&self) -> Option<LoadProgress> {
        let active = self.tabs.active();
        let pulse = (self.now.as_millis() / 100 % 10) as u8;
        if active.document_pending {
            return Some(LoadProgress {
                phase: "fetch",
                amount: LoadProgressAmount::Indeterminate {
                    pulse,
                    elapsed: None,
                },
            });
        }
        if let Some(pending) = active.pending_load.as_ref() {
            let (completed, total) = pending.progress();
            return Some(LoadProgress {
                phase: "parse",
                amount: LoadProgressAmount::Determinate { completed, total },
            });
        }
        if self
            .render_inflight
            .is_some_and(|key| key.tab_id == active.id && key.generation == active.generation)
        {
            let activity = self.renders.activity().filter(|activity| {
                activity.key.tab_id == active.id && activity.key.generation == active.generation
            });
            return Some(LoadProgress {
                phase: activity.map_or("render", |activity| match activity.stage {
                    RenderStage::Cascade => "styles",
                    RenderStage::Restyle => "restyle",
                    RenderStage::Layout => "layout",
                    RenderStage::Paint => "paint",
                }),
                amount: LoadProgressAmount::Indeterminate {
                    pulse,
                    elapsed: activity.map(|activity| activity.elapsed),
                },
            });
        }
        let load = active.load.as_ref()?;
        if load.has_render_work() {
            return Some(LoadProgress {
                phase: "queued",
                amount: LoadProgressAmount::Indeterminate {
                    pulse,
                    elapsed: None,
                },
            });
        }
        if !load.is_settled() {
            let (completed, total) = load.resource_progress();
            return Some(LoadProgress {
                phase: "resources",
                amount: LoadProgressAmount::Items { completed, total },
            });
        }
        None
    }

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
            TextFieldView::new(&self.address)
        } else {
            TextFieldView::display(active.url.as_str())
        };
        ChromeView {
            geometry: self.geometry,
            theme: *self.theme(),
            theme_index: self.theme_index(),
            can_back: active.history_pos > 0,
            can_forward: active.history_pos + 1 < active.history.len(),
            hovered_button: self.hovered_toolbar_button(),
            address,
            address_focused: self.focus == Focus::Address,
            content_cursor: self.content_cursor(),
            menu_open: self.menu_open,
            menu_active: self.menu_active,
            menu_item: self.menu_item,
            tabs: self.tab_chips(),
            active_tab: self.tabs.active_index(),
            content: ContentLines {
                painted: &active.painted,
                scroll: active.scroll,
                text_fields: self.content_text_fields(),
            },
            status: StatusView {
                url: Cow::Borrowed(active.url.as_str()),
                message: Cow::Borrowed(active.message.as_str()),
                hover: self.hovered_href().map(Cow::Borrowed),
                progress: self.load_progress(),
            },
            flash: self.flash_message(),
            text_field_menu: self.text_context.map(|context| {
                let has_selection = match context.target {
                    TextFieldTarget::Address => self.address.selection().is_some(),
                    TextFieldTarget::Form(node) => active
                        .text_fields
                        .get(&node)
                        .is_some_and(|field| field.selection().is_some()),
                };
                TextFieldMenuView {
                    anchor: context.anchor,
                    can_copy: has_selection,
                    can_cut: has_selection,
                }
            }),
        }
    }
}

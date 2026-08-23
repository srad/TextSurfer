mod delivery;
mod keys;
mod navigation;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use url::Url;

use crate::app::net::Navigate;
use crate::app::startpage::start_page;
use crate::core::event::{Key, KeyEvent, KeyModifiers};
use crate::core::focus::Focus;
use crate::core::geom::Size;
use crate::net::{FetchError, FetchPayload, FetchResponse, ResourceId};
use crate::paint::DisplayList;
use crate::pipeline::page_load::STYLESHEET_DEADLINE;
use crate::ui::keymap::Action;

use super::{App, STARTUP_HINT};

pub(super) fn press(code: Key) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::default(),
    }
}

#[derive(Default)]
pub(super) struct FakeNet {
    pub(super) pending: Mutex<Vec<FetchPayload>>,
    pub(super) submitted: Mutex<Vec<(u64, Url)>>,
}

impl Navigate for FakeNet {
    fn submit(&self, tab_id: u64, generation: u64, resource_id: ResourceId, url: Url) {
        self.submitted
            .lock()
            .unwrap()
            .push((generation, url.clone()));
        self.pending.lock().unwrap().push(FetchPayload {
            tab_id,
            generation,
            resource_id,
            result: Ok(FetchResponse {
                final_url: url,
                body: b"<p>hi there</p>".to_vec(),
                content_type: None,
            }),
        });
    }

    fn poll_result(&self) -> Option<FetchPayload> {
        self.pending.lock().unwrap().pop()
    }
}

pub(super) fn ctrl(mut event: KeyEvent) -> KeyEvent {
    event.modifiers.ctrl = true;
    event
}

pub(super) fn alt(mut event: KeyEvent) -> KeyEvent {
    event.modifiers.alt = true;
    event
}

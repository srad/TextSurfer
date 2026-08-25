mod delivery;
mod keys;
mod mouse;
mod navigation;
mod state;
mod theme;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use url::Url;

use crate::app::net::Navigate;
use crate::app::startpage::start_page;
use crate::core::event::{Key, KeyEvent, KeyModifiers};
use crate::core::focus::Focus;
use crate::core::geom::Size;
use crate::net::{FetchError, FetchPayload, FetchPoll, FetchResponse, ResourceId, Submitted};
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

pub(super) struct FakeNet {
    pub(super) pending: Mutex<Vec<FetchPayload>>,
    pub(super) submitted: Mutex<Vec<(u64, Url)>>,
    body: Vec<u8>,
}

impl Default for FakeNet {
    fn default() -> Self {
        Self::serving("<p>hi there</p>")
    }
}

impl FakeNet {
    /// A fake that answers every request with the same markup, so a test can put a
    /// document with real links in front of the pointer.
    pub(super) fn serving(html: &str) -> Self {
        Self {
            pending: Mutex::default(),
            submitted: Mutex::default(),
            body: html.as_bytes().to_vec(),
        }
    }
}

impl Navigate for FakeNet {
    fn submit(&self, tab_id: u64, generation: u64, resource_id: ResourceId, url: Url) -> Submitted {
        self.submitted
            .lock()
            .unwrap()
            .push((generation, url.clone()));
        // Types are declared, because this fake stands in for a real server and real
        // servers declare them; the sniffing fallback has its own cases. A request for
        // a stylesheet is answered with one, so a test that schedules an `@import` is
        // not silently counting it as a failed resource.
        let stylesheet = url.path().ends_with(".css");
        self.pending.lock().unwrap().push(FetchPayload {
            tab_id,
            generation,
            resource_id,
            result: Ok(FetchResponse {
                final_url: url,
                status: 200,
                body: if stylesheet {
                    Vec::new()
                } else {
                    self.body.clone()
                },
                content_type: Some(if stylesheet {
                    "text/css".to_string()
                } else {
                    "text/html; charset=utf-8".to_string()
                }),
            }),
        });
        Submitted::Queued
    }

    fn poll_result(&self) -> FetchPoll {
        self.pending
            .lock()
            .unwrap()
            .pop()
            .map_or(FetchPoll::Empty, FetchPoll::Ready)
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

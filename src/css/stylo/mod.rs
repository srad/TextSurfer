//! The Stylo cascade.
//!
//! The engine sees [`dom::StyleDom`], an owned mirror of `core::dom::Document`, rather than the
//! document itself: Stylo needs interior-mutable per-element style data, state bits, selector flags
//! and a stable identity, none of which can live on `core::dom::Node` without pulling `style::`
//! into `core`.

#![cfg_attr(not(test), allow(dead_code))]

mod device;
mod dom;
mod engine;
mod invalidate;
mod map;
mod prefs;
mod session;
mod sheets;
mod traversal;

pub(crate) use session::{StyloSession, cascade_once, with_session};
pub(crate) use sheets::{discover_imports, media_matches};

#[cfg(test)]
mod tests;

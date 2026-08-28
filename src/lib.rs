// `deny` rather than `forbid`: Stylo's `TElement` declares seven `unsafe fn` methods for Gecko's
// benefit, and `forbid` cannot be overridden anywhere in the crate. `css::stylo::dom` is the sole
// module allowed to opt out, and a test pins it as the only one. Under edition 2024 an `unsafe fn`
// body is ordinary safe code, so those impls contain no `unsafe` block.
#![deny(unsafe_code)]

pub mod app;
pub mod core;
pub mod css;
pub mod html;
pub mod layout;
pub mod net;
pub mod paint;
pub mod pipeline;
pub mod script;
pub mod ui;
#[cfg(feature = "vga")]
pub mod vga;

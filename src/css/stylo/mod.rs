//! The Stylo cascade (M7). Behind the `stylo` cargo feature until S7 makes it the only cascade.
//!
//! The engine sees [`dom::StyleDom`], an owned mirror of `core::dom::Document`, rather than the
//! document itself: Stylo needs interior-mutable per-element style data, state bits, selector flags
//! and a stable identity, none of which can live on `core::dom::Node` without pulling `style::`
//! into `core`.

// S3 builds the DOM adapter and proves it against its own tests; nothing in the production cascade
// path reaches it until S4 adds `StyloCascade`. This is scoped to non-test builds so it lifts on
// its own — once S4 wires the adapter in, the expectation goes unfulfilled and the build fails
// until this attribute is deleted.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "S3 lands the DOM adapter; S4 is what makes the production cascade use it"
    )
)]

mod dom;

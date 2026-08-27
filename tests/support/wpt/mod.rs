mod dom_construction;
mod manifest;
mod render;
mod supervisor;
mod tests;

pub use dom_construction::assert_deep_dom_construction_completes;
pub use supervisor::run_worker;
#[cfg(feature = "vga")]
pub use tests::assert_vga_pixel_conformance;
pub use tests::{
    assert_corpus_conformance, assert_corpus_integrity, assert_manifest_contracts,
    assert_supervisor_contracts,
};

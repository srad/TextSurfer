use std::sync::Once;

static ENABLE: Once = Once::new();

/// Turn on the Stylo preferences the terminal engine depends on.
///
/// Stylo gates whole property families behind `stylo_static_prefs` booleans, read at *parse* time
/// with no diagnostic when they are off. `layout.grid.enabled` defaults to `false`, and every grid
/// longhand carries `servo_pref = "layout.grid.enabled"` while `display: grid` is gated by the same
/// pref — so without this call the entire Grid feature parses away to nothing and every grid page
/// silently renders as block flow.
///
/// The preferences are process-global atomics, so this runs once and must precede the first
/// `Stylesheet::from_str`.
///
pub(super) fn enable() {
    ENABLE.call_once(|| {
        stylo_static_prefs::set_pref!("layout.grid.enabled", true);
        stylo_static_prefs::set_pref!("layout.unimplemented", true);
    });
}

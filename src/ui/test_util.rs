use ratatui::buffer::Buffer;
use ratatui::style::Modifier;

use crate::core::geom::Size;
use crate::paint::DisplayList;
use crate::ui::chrome::ChromeView;
use crate::ui::mouse::ChromeGeometry;
use crate::ui::theme::NORTON;
use crate::ui::widgets::content::ContentLines;
use crate::ui::widgets::status::StatusView;

static DRAFT_CONTENT: std::sync::LazyLock<DisplayList> =
    std::sync::LazyLock::new(|| DisplayList::from_lines(&["hello".to_string()]));

/// A minimal but complete chrome view, shared by the widget tests and the framebuffer
/// frontend's tests so both render the same thing.
///
/// The geometry is fixed at 60x10 independently of the area it is later rendered into;
/// several callers rely on that to exercise clipping.
pub fn draft_view() -> ChromeView<'static> {
    ChromeView {
        geometry: ChromeGeometry::for_size(Size { cols: 60, rows: 10 }),
        theme: NORTON,
        can_back: false,
        can_forward: false,
        address: "https://example.com".to_string().into(),
        address_cursor: 0,
        address_focused: false,
        menu_open: false,
        menu_active: 0,
        menu_item: 0,
        tabs: Vec::new(),
        active_tab: 0,
        content: ContentLines {
            painted: &DRAFT_CONTENT,
            scroll: 0,
        },
        status: StatusView {
            url: "https://example.com".to_string().into(),
            message: "Ready".to_string().into(),
        },
    }
}

pub fn styled_buffer_string(buf: &Buffer) -> String {
    let mut out = String::new();
    for row in buf.area.top()..buf.area.bottom() {
        for col in buf.area.left()..buf.area.right() {
            let cell = &buf[(col, row)];
            let mut marks = String::new();
            if cell.modifier.contains(Modifier::BOLD) {
                marks.push('b');
            }
            if cell.modifier.contains(Modifier::UNDERLINED) {
                marks.push('u');
            }
            if cell.modifier.contains(Modifier::CROSSED_OUT) {
                marks.push('s');
            }
            if cell.modifier.contains(Modifier::REVERSED) {
                marks.push('r');
            }
            out.push_str(&format!(
                "{}[{:?}/{:?}{}]",
                cell.symbol(),
                cell.fg,
                cell.bg,
                if marks.is_empty() {
                    String::new()
                } else {
                    format!("/{marks}")
                }
            ));
        }
        out.push('\n');
    }
    out
}

pub fn buffer_string(buf: &Buffer) -> String {
    let mut out = String::new();
    for (index, cell) in buf.content().iter().enumerate() {
        if index > 0 && index % usize::from(buf.area.width) == 0 {
            out.push('\n');
        }
        out.push_str(cell.symbol());
    }
    out.push('\n');
    out
}

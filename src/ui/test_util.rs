use ratatui::buffer::Buffer;
use ratatui::style::Modifier;

use crate::core::geom::Size;
use crate::paint::DisplayList;
use crate::ui::chrome::{ChromeView, MainMenuView};
use crate::ui::mouse::ChromeGeometry;
use crate::ui::theme::{DEFAULT, DEFAULT_THEME_INDEX};
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
        theme: DEFAULT,
        theme_index: DEFAULT_THEME_INDEX,
        can_back: false,
        can_forward: false,
        hovered_button: None,
        address: crate::ui::widgets::text_field::TextFieldView::display("https://example.com"),
        address_focused: false,
        content_cursor: None,
        main_menu: MainMenuView {
            open: false,
            active: 0,
            selected: None,
            hovered_title: None,
            enabled: crate::ui::widgets::menu::MenuItemMask::all(
                crate::ui::widgets::menu::MENUS[0].len(),
            ),
        },
        tabs: Vec::new(),
        active_tab: 0,
        content: ContentLines {
            painted: &DRAFT_CONTENT,
            scroll: 0,
            text_fields: Vec::new(),
        },
        status: StatusView {
            url: "https://example.com".to_string().into(),
            message: "Ready".to_string().into(),
            hover: None,
            progress: None,
        },
        flash: None,
        text_field_menu: None,
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

use ratatui::buffer::Buffer;
use ratatui::style::Modifier;

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

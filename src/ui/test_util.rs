use ratatui::buffer::Buffer;

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

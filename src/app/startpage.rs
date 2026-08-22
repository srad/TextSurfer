use crate::paint::DisplayList;

pub fn start_page() -> DisplayList {
    DisplayList::from_lines(&[
        "TextSurfer - a text-mode browser".to_string(),
        String::new(),
        "  press / to type a URL or search terms, Enter to go".to_string(),
        "  Ctrl+T new tab  Ctrl+W close  Ctrl+N/P switch".to_string(),
        "  j/k or arrows scroll  Space/PageDown page  Home/End jump  q quits".to_string(),
        "  HTML and embedded CSS render as terminal text; help arrives in M2".to_string(),
    ])
}

pub fn content_for(url: &str) -> DisplayList {
    if url.is_empty() {
        return start_page();
    }
    DisplayList::from_lines(&[
        url.to_string(),
        String::new(),
        format!("  fetching {url}"),
        "  loading through the HTML, style, layout and paint pipeline".to_string(),
    ])
}

pub fn start_page() -> Vec<String> {
    vec![
        "TextSurf - a text-mode browser".to_string(),
        String::new(),
        "  press / to type a URL or search terms, Enter to go".to_string(),
        "  Ctrl+T new tab  Ctrl+W close  Ctrl+N/P switch".to_string(),
        "  j/k or arrows scroll  Home/End jump  q quits".to_string(),
        "  HTML and embedded CSS render as terminal text; help arrives in M2".to_string(),
    ]
}

pub fn content_for(url: &str) -> Vec<String> {
    if url.is_empty() {
        return start_page();
    }
    vec![
        url.to_string(),
        String::new(),
        format!("  fetching {url}"),
        "  loading through the HTML, style, layout and paint pipeline".to_string(),
    ]
}

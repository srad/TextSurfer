pub fn start_page() -> Vec<String> {
    vec![
        "TextSurf - a text-mode browser".to_string(),
        String::new(),
        "  press / to type a URL or search terms, Enter to go".to_string(),
        "  Ctrl+T new tab  Ctrl+W close  Ctrl+N/P switch".to_string(),
        "  j/k or arrows scroll  Home/End jump  q quits".to_string(),
        "  the network pipeline lands in M1-A; help overlay in M2".to_string(),
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
        "  parsing, styles and layout arrive in M1-A / M1-B".to_string(),
    ]
}

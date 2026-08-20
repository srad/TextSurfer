pub struct Tab {
    pub url: String,
    pub title: String,
    pub history: Vec<String>,
    pub scroll: u16,
    pub layout_width: u16,
    pub generation: u64,
    pub content: Vec<String>,
}

impl Tab {
    pub(crate) fn new(url: String, generation: u64, content: Vec<String>) -> Self {
        let title = if url.is_empty() {
            "start".to_string()
        } else {
            url.clone()
        };
        Self {
            title,
            history: vec![url.clone()],
            url,
            scroll: 0,
            layout_width: 0,
            generation,
            content,
        }
    }

    pub fn push_history(&mut self, url: &str) {
        if self.history.last().map(String::as_str) != Some(url) {
            self.history.push(url.to_string());
        }
    }
}

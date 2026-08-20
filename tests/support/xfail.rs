use std::collections::HashMap;

pub struct Xfail {
    map: HashMap<(String, usize), String>,
}

pub const MANIFEST: &str = include_str!("xfail.txt");

fn parse_manifest(text: &str) -> HashMap<(String, usize), String> {
    let mut map = HashMap::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (file, rest) = line
            .split_once('#')
            .unwrap_or_else(|| panic!("xfail manifest line {}: missing `#`", lineno + 1));
        let rest = rest.trim_start();
        let (idx, reason) = match rest.split_once(char::is_whitespace) {
            Some((idx, reason)) => (idx, reason.trim()),
            None => (rest, ""),
        };
        let idx: usize = idx
            .parse()
            .unwrap_or_else(|_| panic!("xfail manifest line {}: bad index `{idx}`", lineno + 1));
        assert!(
            !reason.is_empty(),
            "xfail manifest line {}: missing reason",
            lineno + 1
        );
        assert!(
            map.insert((file.to_string(), idx), reason.to_string())
                .is_none(),
            "xfail manifest line {}: duplicate entry",
            lineno + 1
        );
    }
    map
}

impl Xfail {
    pub fn load() -> Self {
        Self {
            map: parse_manifest(MANIFEST),
        }
    }

    pub fn reason(&self, file: &str, case: usize) -> Option<&str> {
        self.map.get(&(file.to_string(), case)).map(String::as_str)
    }

    pub fn keys(&self) -> impl Iterator<Item = (&str, usize)> {
        self.map.keys().map(|(file, case)| (file.as_str(), *case))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_file_case_and_reason() {
        let map = parse_manifest(
            "# comment\n\n  tests10.dat#7  foster parenting divergence\napan.dat#1  another\nempty.dat#3  third\n",
        );
        assert_eq!(
            map.get(&("tests10.dat".into(), 7)).map(String::as_str),
            Some("foster parenting divergence")
        );
        assert_eq!(
            map.get(&("apan.dat".into(), 1)).map(String::as_str),
            Some("another")
        );
        assert_eq!(
            map.get(&("empty.dat".into(), 3)).map(String::as_str),
            Some("third")
        );
        assert_eq!(map.get(&("tests10.dat".into(), 8)), None);
    }

    #[test]
    #[should_panic(expected = "missing `#`")]
    fn malformed_line_panics() {
        parse_manifest("tests10.dat 7 no hash\n");
    }

    #[test]
    #[should_panic(expected = "bad index")]
    fn bad_index_panics() {
        parse_manifest("tests10.dat#xyz\n");
    }

    #[test]
    #[should_panic(expected = "missing reason")]
    fn missing_reason_panics() {
        parse_manifest("tests10.dat#7\n");
    }
}

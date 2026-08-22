use textsurfer::core::dom::ElementNs;

pub struct DatCase {
    pub input: String,
    pub expected: String,
    pub is_fragment: bool,
    pub context: Option<(String, ElementNs)>,
    pub script_on: bool,
    pub error_count: usize,
}

const MARKERS: [&str; 7] = [
    "#data",
    "#errors",
    "#new-errors",
    "#document-fragment",
    "#script-on",
    "#script-off",
    "#document",
];

fn is_marker(line: &str) -> bool {
    MARKERS.contains(&line)
}

pub fn parse_file(file: &str, bytes: &[u8]) -> Vec<DatCase> {
    let text = String::from_utf8_lossy(bytes);
    let chunks: Vec<(String, Vec<String>)> = {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        let mut current: Option<(String, Vec<String>)> = None;
        for line in text.lines() {
            if is_marker(line) {
                if let Some(previous) = current.take() {
                    out.push(previous);
                }
                current = Some((line.to_string(), Vec::new()));
            } else if let Some(storage) = &mut current {
                storage.1.push(line.to_string());
            } else {
                panic!("{file}: content before the first #data marker: `{line}`");
            }
        }
        if let Some(last) = current {
            out.push(last);
        }
        out
    };

    let mut cases = Vec::new();
    let mut idx = 0;
    while idx < chunks.len() {
        let (marker, payload) = &chunks[idx];
        if marker != "#data" {
            panic!("{file}: section `{marker}` outside a test");
        }
        let input = payload.join("\n");

        let mut error_count = 0usize;
        let mut fragment: Option<String> = None;
        let mut script: Option<bool> = None;
        let mut dump_lines: Vec<String> = Vec::new();
        let mut next = idx + 1;
        while next < chunks.len() {
            let (name, body) = &chunks[next];
            match name.as_str() {
                "#data" => break,
                "#errors" => error_count += body.len(),
                "#new-errors" => error_count += body.len(),
                "#document-fragment" => {
                    assert_eq!(
                        body.len(),
                        1,
                        "{file}: #document-fragment must carry exactly one context line"
                    );
                    fragment = Some(body[0].clone());
                }
                "#script-on" => script = Some(true),
                "#script-off" => script = Some(false),
                "#document" => {
                    dump_lines = body.clone();
                    next += 1;
                    break;
                }
                _ => unreachable!("marker recognized by is_marker"),
            }
            next += 1;
        }
        idx = next;

        if dump_lines.last().is_some_and(|line| line.is_empty()) {
            dump_lines.pop();
        }
        let is_fragment = fragment.is_some();
        let context = fragment.map(|line| {
            if let Some(name) = line.strip_prefix("svg ") {
                (name.to_string(), ElementNs::Svg)
            } else if let Some(name) = line.strip_prefix("math ") {
                (name.to_string(), ElementNs::MathMl)
            } else {
                (line, ElementNs::Html)
            }
        });
        let expected = if is_fragment {
            dump_lines.join("\n")
        } else {
            if dump_lines.is_empty() {
                "#document\n".to_string()
            } else {
                format!("#document\n{}", dump_lines.join("\n"))
            }
        };

        cases.push(DatCase {
            input,
            expected,
            is_fragment,
            context,
            script_on: script == Some(true),
            error_count,
        });
    }
    cases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_document_case() {
        let bytes = b"#data\n<p>One<p>Two\n#errors\n3: Missing document type declaration\n#document\n| <html>\n|   <head>\n|   <body>\n|     <p>\n|       \"One\"\n|     <p>\n|       \"Two\"\n";
        let cases = parse_file("sample.dat", bytes);
        assert_eq!(cases.len(), 1);
        let case = &cases[0];
        assert_eq!(case.input, "<p>One<p>Two");
        assert_eq!(case.error_count, 1);
        assert!(!case.is_fragment);
        assert!(!case.script_on);
        assert_eq!(
            case.expected,
            "#document\n| <html>\n|   <head>\n|   <body>\n|     <p>\n|       \"One\"\n|     <p>\n|       \"Two\""
        );
    }

    #[test]
    fn trailing_blank_separator_is_dropped_from_the_dump() {
        let bytes = b"#data\nx\n#errors\n#document\n| \'root\'\n\n#data\ny\n#errors\n#document\n| \'two\'\n";
        let cases = parse_file("pair.dat", bytes);
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].expected, "#document\n| \'root\'");
        assert_eq!(cases[1].expected, "#document\n| \'two\'");
    }

    #[test]
    fn pending_script_and_fragment_sections() {
        let bytes = b"#data\n<b>x</b>\n#errors\n#document-fragment\nsvg path\n#script-on\n#document\n| <b>\n|   \"x\"\n";
        let cases = parse_file("frag.dat", bytes);
        assert_eq!(cases.len(), 1);
        let case = &cases[0];
        assert!(case.is_fragment);
        assert!(case.script_on);
        assert_eq!(case.context, Some(("path".to_string(), ElementNs::Svg)));
        assert_eq!(case.expected, "| <b>\n|   \"x\"");
    }

    #[test]
    fn script_sections_after_errors_are_optional() {
        let bytes = b"#data\na\n#errors\n1: x\n#script-off\n#document\n| \"a\"\n";
        let cases = parse_file("off.dat", bytes);
        assert!(!cases[0].script_on);
    }

    #[test]
    fn blank_lines_inside_data_are_preserved() {
        let bytes = b"#data\n<p>x</p>\n\n#errors\n#document\n| <p>\n|   \"x\"\n";
        let cases = parse_file("blank.dat", bytes);
        assert_eq!(cases[0].input, "<p>x</p>\n");
    }

    #[test]
    fn new_errors_add_to_the_count() {
        let bytes = b"#data\n<p>\n#errors\n1: a\n#new-errors\n2: b\n2: c\n#document\n| <p>\n";
        let cases = parse_file("newerr.dat", bytes);
        assert_eq!(cases[0].error_count, 3);
    }

    #[test]
    fn last_case_without_trailing_blank_line() {
        let bytes = b"#data\na\n#errors\n#document\n| \"a\"";
        let cases = parse_file("eof.dat", bytes);
        assert_eq!(cases[0].expected, "#document\n| \"a\"");
    }
}

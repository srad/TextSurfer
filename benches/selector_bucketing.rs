use std::time::{Duration, Instant};

use textsurfer::core::dom::{Attr, Document, ElementNs};
use textsurfer::css::{BasicCascade, Cascade, CssParser, CssparserParser, MediaContext};

fn main() {
    let mut css = String::new();
    for index in 0..5_000 {
        css.push_str(&format!(
            ".rule-{index} {{ color: rgb({}, {}, {}); }}\n",
            index % 255,
            (index * 3) % 255,
            (index * 7) % 255
        ));
    }
    let sheet = CssparserParser.parse(&css);
    let mut document = Document::new();
    let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
    for index in 0..2_000 {
        document.insert_element(
            Some(root),
            "p",
            ElementNs::Html,
            vec![Attr::plain("class", &format!("rule-{}", index % 5_000))],
        );
    }
    let cascade = BasicCascade;
    let media = MediaContext::screen();
    let _ = cascade.apply(std::slice::from_ref(&sheet), &document, media);
    let mut samples = Vec::new();
    for _ in 0..5 {
        let started = Instant::now();
        let _ = cascade.apply(std::slice::from_ref(&sheet), &document, media);
        samples.push(started.elapsed());
    }
    samples.sort();
    let median = samples[2];
    assert!(
        median < Duration::from_millis(200),
        "selector cascade median was {median:?}"
    );
    println!("selector cascade median: {median:?}");
}

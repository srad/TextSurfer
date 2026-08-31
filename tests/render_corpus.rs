//! Compares our rendering against a real browser's, using captures made by
//! `tools/capture-browser-reference.mjs`. The browser is the oracle for what a page *means*; we
//! never compare pixels, because we paint an 8x16 character grid and it paints glyphs.
//!
//! The unit of comparison is a word. Line breaking differs between the two engines by design — we
//! advance a fixed 8px per cell, the browser uses proportional metrics — so runs and line boxes
//! never correspond, but a word survives both.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use textsurfer::core::form::FormState;
use textsurfer::core::geom::Size;
use textsurfer::core::image::{ImageDecoder, ImageResources};
use textsurfer::core::style::{CellMetric, RenderContext};
use textsurfer::css::ColorScheme;
use textsurfer::layout::{BoxTree, TaffyLayoutEngine};
use textsurfer::net::FetchResponse;
use textsurfer::pipeline::image::RasterImageDecoder;
use textsurfer::pipeline::page_load::{PageLoad, PageLoadOptions};
use textsurfer::pipeline::render::RenderedPage;
use textsurfer::ui::PAPER_WHITE;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use url::Url;

const COLUMN_PX: f64 = 8.0;
const ROW_PX: f64 = 16.0;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bundle {
    schema: u8,
    slug: String,
    url: String,
    #[allow(dead_code)]
    requested: String,
    resources: Vec<Resource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Resource {
    url: String,
    status: u16,
    mime_type: String,
    file: Option<String>,
    sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    words: Vec<BrowserWord>,
}

/// Positional to keep a page's reference from costing megabytes:
/// `[text, x, y, width, height, visible, rtl]`, in whole CSS pixels.
#[derive(Deserialize)]
struct BrowserWord(String, f64, f64, f64, f64, u8, u8);

impl BrowserWord {
    fn text(&self) -> &str {
        &self.0
    }

    fn visible(&self) -> bool {
        self.5 == 1
    }

    /// Whether the word sits in a right-to-left block. Reading order there runs the other way
    /// along the band, and we have no bidi yet (M1-F's last bullet), so these words are counted
    /// for presence but excluded from the order check rather than reported as reordering.
    fn rtl(&self) -> bool {
        self.6 == 1
    }

    /// The 16px band this word reads in. Centre, not top: a line box taller than our row would
    /// otherwise land its words a band early.
    fn band(&self) -> i64 {
        ((self.2 + self.4 / 2.0) / ROW_PX).floor() as i64
    }
}

/// One word from either engine, in CSS pixels on a 16px band, so the two streams are directly
/// comparable. Our side converts cells at 8x16; the browser's rects are already pixels.
#[derive(Clone, Debug)]
struct Token {
    text: String,
    band: i64,
    start: f64,
    end: f64,
}

impl Token {
    fn overlaps(&self, other: &Self) -> bool {
        self.band == other.band && self.start < other.end && other.start < self.end
    }
}

fn in_reading_order(mut tokens: Vec<Token>) -> Vec<Token> {
    tokens.sort_by(|a, b| {
        a.band.cmp(&b.band).then(
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    tokens
}

/// The comparison alphabet: maximal alphanumeric runs, punctuation discarded.
///
/// A whitespace-separated word is *not* a stable identity across two engines. Where the browser
/// keeps `Linux` and `(kernel)[9][g]` on one line they read as one contiguous clump; where our
/// narrower line breaks between them they do not — so any rule based on word or contiguity
/// boundaries is width-dependent, which is precisely the thing that legitimately differs. Runs of
/// letters and digits survive both line breaking and punctuation clumping, and "is content
/// missing?" was never a question about punctuation.
fn runs(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|run| !run.is_empty())
        .map(str::to_string)
}

fn run_sequence(tokens: &[Token]) -> Vec<String> {
    tokens.iter().flat_map(|token| runs(&token.text)).collect()
}

fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/browser-corpus")
}

fn slugs() -> Vec<String> {
    let root = corpus_root();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut slugs = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("bundle.json").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    slugs.sort();
    slugs
}

fn load_bundle(slug: &str) -> Bundle {
    let path = corpus_root().join(slug).join("bundle.json");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("bundle {}: {error}", path.display()));
    let bundle: Bundle = serde_json::from_str(&source)
        .unwrap_or_else(|error| panic!("bundle {}: {error}", path.display()));
    assert_eq!(bundle.schema, 1, "unsupported bundle schema in {slug}");
    bundle
}

fn load_reference(slug: &str, cols: u16, rows: u16) -> Option<Reference> {
    let path = corpus_root()
        .join(slug)
        .join(format!("reference-{cols}x{rows}.json"));
    let source = std::fs::read_to_string(&path).ok()?;
    Some(
        serde_json::from_str(&source)
            .unwrap_or_else(|error| panic!("reference {}: {error}", path.display())),
    )
}

/// Renders the captured bytes with the browser's own resource state: anything the browser did not
/// fetch is a 404 here too, so a lazy-loaded image missing from the capture is missing from both
/// sides rather than only from one.
fn render(slug: &str, bundle: &Bundle, cols: u16, rows: u16) -> RenderedPage {
    let directory = corpus_root().join(slug);
    let by_url = bundle
        .resources
        .iter()
        .map(|resource| (resource.url.as_str(), resource))
        .collect::<HashMap<_, _>>();
    let document = bundle
        .resources
        .iter()
        .find(|resource| resource.url == bundle.url)
        .and_then(|resource| resource.file.as_ref())
        .unwrap_or_else(|| panic!("{slug} has no captured document body"));
    let source = std::fs::read_to_string(directory.join(document))
        .unwrap_or_else(|error| panic!("{slug} document: {error}"));

    let mut load = PageLoad::new(
        &source,
        Url::parse(&bundle.url).expect("captured document URL parses"),
        encoding_rs::UTF_8,
        PageLoadOptions {
            render: RenderContext::terminal(Size { cols, rows }),
            palette: PAPER_WHITE.palette(),
            scripting: false,
            color_scheme: ColorScheme::Light,
            started: Duration::ZERO,
        },
    );
    for _ in 0..=128 {
        let commands = load.take_commands();
        let had_commands = !commands.is_empty();
        for command in commands {
            let captured = by_url
                .get(command.url.as_str())
                .filter(|resource| resource.file.is_some());
            let response = match captured {
                Some(resource) => {
                    let file = resource.file.as_ref().expect("filtered above");
                    FetchResponse {
                        final_url: command.url,
                        status: resource.status,
                        body: std::fs::read(directory.join(file))
                            .unwrap_or_else(|error| panic!("{slug} resource {file}: {error}")),
                        content_type: Some(resource.mime_type.clone()),
                    }
                }
                None => FetchResponse {
                    final_url: command.url,
                    status: 404,
                    body: Vec::new(),
                    content_type: None,
                },
            };
            load.deliver(command.resource_id, Ok(response));
        }
        let decodes = load.take_image_decode_commands();
        let had_decodes = !decodes.is_empty();
        for request in decodes {
            let asset_id = request.asset_id;
            let revision = request.revision;
            let result = RasterImageDecoder.decode(request);
            load.deliver_image_decode(asset_id, revision, result);
        }
        if !had_commands && !had_decodes {
            break;
        }
    }
    assert!(
        !load.external_disabled(),
        "{slug} discarded its stylesheets, so this render proves nothing about CSS"
    );
    load.force_render()
}

fn our_tokens(tree: &BoxTree) -> Vec<Token> {
    let mut tokens = Vec::new();
    for fragment in &tree.fragments {
        let scale = usize::from(fragment.style.scale).max(1);
        let mut col = fragment.col;
        let mut start = col;
        let mut text = String::new();
        let flush = |text: &mut String, start: usize, end: usize, tokens: &mut Vec<Token>| {
            if !text.is_empty() {
                tokens.push(Token {
                    text: std::mem::take(text),
                    band: fragment.row as i64,
                    start: start as f64 * COLUMN_PX,
                    end: end as f64 * COLUMN_PX,
                });
            }
        };
        for grapheme in fragment.text.graphemes(true) {
            let width = UnicodeWidthStr::width(grapheme).saturating_mul(scale);
            if grapheme.chars().all(char::is_whitespace) {
                flush(&mut text, start, col, &mut tokens);
                col += width;
                start = col;
                continue;
            }
            text.push_str(grapheme);
            col += width;
        }
        flush(&mut text, start, col, &mut tokens);
    }
    in_reading_order(tokens)
}

fn comparison_layout(page: &RenderedPage, viewport: Size) -> BoxTree {
    let mut images = ImageResources::default();
    for placement in &page.painted.images {
        if let Some(image) = page.painted.image_assets.get(&placement.asset_id) {
            images.insert(placement.node, image.clone());
        }
    }
    TaffyLayoutEngine.layout_with_images(
        &page.document.borrow(),
        &page.styles,
        viewport,
        FormState::empty(),
        &images,
        CellMetric::DEFAULT,
    )
}

fn browser_tokens(reference: &Reference) -> Vec<Token> {
    in_reading_order(
        reference
            .words
            .iter()
            .filter(|word| word.visible())
            .map(|word| Token {
                text: word.text().to_string(),
                band: word.band(),
                start: word.1,
                end: word.1 + word.3,
            })
            .collect(),
    )
}

fn counts(texts: impl Iterator<Item = String>) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for text in texts {
        *counts.entry(text).or_insert(0) += 1;
    }
    counts
}

/// How much of the browser's reading order we preserve.
///
/// Hunt-Szymanski reduces the LCS to increasing subsequences of matching positions, avoiding the
/// 15,000 x 15,000 matrix while still handling repeated words exactly.
fn order_agreement(browser: &[String], ours: &[String]) -> usize {
    let mut positions: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, text) in ours.iter().enumerate() {
        positions.entry(text.as_str()).or_default().push(index);
    }

    let mut tails: Vec<usize> = Vec::new();
    for text in browser {
        let Some(matches) = positions.get(text.as_str()) else {
            continue;
        };
        for &position in matches.iter().rev() {
            match tails.binary_search(&position) {
                Ok(_) => {}
                Err(index) if index == tails.len() => tails.push(position),
                Err(index) => tails[index] = position,
            }
        }
    }
    tails.len()
}

#[test]
fn order_agreement_handles_reordered_duplicates() {
    let sequence = |items: &[&str]| {
        items
            .iter()
            .map(|item| (*item).to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        order_agreement(&sequence(&["A", "B", "A"]), &sequence(&["B", "A", "B"])),
        2
    );
}

struct Findings {
    missing: Vec<String>,
    shown_but_hidden: Vec<String>,
    order_matched: usize,
    order_total: usize,
    overlaps: Vec<String>,
}

fn compare(reference: &Reference, browser: &[Token], ours: &[Token]) -> Findings {
    let browser_sequence = run_sequence(browser);
    let our_sequence = run_sequence(ours);
    let our_counts = counts(our_sequence.iter().cloned());
    let browser_counts = counts(browser_sequence.iter().cloned());
    let rtl_runs = counts(
        reference
            .words
            .iter()
            .filter(|word| word.visible() && word.rtl())
            .flat_map(|word| runs(word.text())),
    );

    let mut missing = Vec::new();
    for (text, wanted) in &browser_counts {
        let got = our_counts.get(text).copied().unwrap_or(0);
        if got < *wanted {
            missing.push(format!("{text:?} browser={wanted} ours={got}"));
        }
    }
    missing.sort();

    let mut shown_but_hidden = Vec::new();
    for word in reference.words.iter().filter(|word| !word.visible()) {
        for run in runs(word.text()) {
            if browser_counts.contains_key(&run) || !our_counts.contains_key(&run) {
                continue;
            }
            shown_but_hidden.push(format!("{run:?}"));
        }
    }
    shown_but_hidden.sort();
    shown_but_hidden.dedup();

    let ltr_sequence = browser_sequence
        .iter()
        .filter(|run| !rtl_runs.contains_key(*run))
        .cloned()
        .collect::<Vec<_>>();
    let order_matched = order_agreement(&ltr_sequence, &our_sequence);

    // Only pairs that collide on our side can be defects, and there are few of them, so this walks
    // our overlaps and asks the browser about each rather than testing every pair of 15,000 words.
    // Keyed by first alphanumeric run, for the same reason A1 counts runs: the whole-word text of
    // a token is not stable between the two engines.
    let mut browser_by_run: HashMap<String, Vec<&Token>> = HashMap::new();
    for token in browser {
        if let Some(first) = runs(&token.text).next() {
            browser_by_run.entry(first).or_default().push(token);
        }
    }
    let key = |token: &Token| runs(&token.text).next();
    let mut overlaps = Vec::new();
    for (index, left) in ours.iter().enumerate() {
        for right in &ours[index + 1..] {
            if right.band != left.band {
                break;
            }
            if !left.overlaps(right) {
                continue;
            }
            let (Some(left_key), Some(right_key)) = (key(left), key(right)) else {
                continue;
            };
            let (Some(browser_left), Some(browser_right)) = (
                browser_by_run.get(&left_key).and_then(|t| t.first()),
                browser_by_run.get(&right_key).and_then(|t| t.first()),
            ) else {
                continue;
            };
            if !browser_left.overlaps(browser_right) {
                overlaps.push(format!(
                    "band {} col {}: {:?} and {:?} share cells, but the browser keeps them apart",
                    left.band,
                    (left.start / COLUMN_PX) as usize,
                    left.text,
                    right.text
                ));
            }
        }
    }
    overlaps.sort();

    Findings {
        missing,
        shown_but_hidden,
        order_matched,
        order_total: ltr_sequence.len(),
        overlaps,
    }
}

/// Ceilings on the two relations that still carry unclassified divergence, so a regression fails
/// even before every finding has been reduced to a cause.
///
/// A1 counts distinct alphanumeric runs the browser paints and we do not. These numbers are *not*
/// a target — they are today's unclassified total, mixing real defects with the known-legitimate
/// divergences (we clip narrow lines, window form values, and break lines on a fixed advance).
/// Phase 3 of the plan reduces them to owned causes; until then they may only go down.
/// A2 is the fraction of the browser's reading order we preserve, as a permille floor.
const EXPECTED: &[(&str, u16, usize, u32)] = &[
    ("example", 40, 0, 1000),
    ("example", 100, 0, 1000),
    ("example", 160, 0, 1000),
    ("wikipedia-linux", 40, 351, 952),
    ("wikipedia-linux", 100, 53, 972),
    ("wikipedia-linux", 160, 47, 970),
];

fn expectation(slug: &str, cols: u16) -> Option<(usize, u32)> {
    EXPECTED
        .iter()
        .find(|(page, width, _, _)| *page == slug && *width == cols)
        .map(|(_, _, missing, order)| (*missing, *order))
}

/// Groups A1 misses by the band they sit in, so a whole region we never render is distinguishable
/// from scattered per-element drops. Diagnostic only, behind `TEXTSURFER_CORPUS_TRIAGE=1`.
fn triage(reference: &Reference, browser: &[Token], ours: &[Token]) {
    let our_counts = counts(run_sequence(ours).into_iter());
    let mut missing_bands: Vec<(i64, String)> = Vec::new();
    for token in browser {
        for run in runs(&token.text) {
            if our_counts.contains_key(&run) {
                continue;
            }
            missing_bands.push((token.band, run));
        }
    }
    missing_bands.sort();

    let mut clusters: Vec<(i64, i64, Vec<String>)> = Vec::new();
    for (band, run) in missing_bands {
        match clusters.last_mut() {
            Some((_, end, texts)) if band - *end <= 2 => {
                *end = band;
                texts.push(run);
            }
            _ => clusters.push((band, band, vec![run])),
        }
    }
    clusters.sort_by_key(|(_, _, texts)| std::cmp::Reverse(texts.len()));

    println!(
        "      triage: {} clusters over {} bands; browser document is {} bands",
        clusters.len(),
        clusters.iter().map(|(s, e, _)| e - s + 1).sum::<i64>(),
        reference
            .words
            .iter()
            .map(BrowserWord::band)
            .max()
            .unwrap_or_default(),
    );
    // Bands are the browser's own coordinates. Ours drift from them over a long document, because
    // line breaking differs, so a band number only locates a finding on the browser side — never
    // compare the two documents at the same band index.
    for (start, end, texts) in clusters.iter().take(6) {
        let context = browser
            .iter()
            .filter(|token| token.band >= *start && token.band <= *end)
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "      bands {start}..={end} missing {} {:?}\n        context: {:?}",
            texts.len(),
            texts,
            context.chars().take(140).collect::<String>()
        );
    }
}

#[test]
fn browser_reference_comparison() {
    let slugs = slugs();
    assert!(
        !slugs.is_empty(),
        "no captures under testdata/browser-corpus; run tools/capture-browser-reference.mjs"
    );
    for slug in &slugs {
        let bundle = load_bundle(slug);
        assert_eq!(
            &bundle.slug, slug,
            "bundle slug disagrees with its directory"
        );
        for resource in &bundle.resources {
            let (Some(file), Some(expected)) = (&resource.file, &resource.sha256) else {
                continue;
            };
            let bytes = std::fs::read(corpus_root().join(slug).join(file))
                .unwrap_or_else(|error| panic!("{slug}/{file}: {error}"));
            let actual = hash_bytes(&bytes);
            assert_eq!(
                &actual, expected,
                "{slug}/{file} does not match its capture"
            );
        }

        for (cols, rows) in [(40u16, 30u16), (100, 38), (160, 45)] {
            let Some(reference) = load_reference(slug, cols, rows) else {
                continue;
            };
            let page = render(slug, &bundle, cols, rows);
            let tree = comparison_layout(&page, Size { cols, rows });
            let ours = our_tokens(&tree);
            let browser = browser_tokens(&reference);
            let findings = compare(&reference, &browser, &ours);
            println!(
                "{slug} {cols}x{rows}: browser={} ours={} A1-missing={} A1-leaked={} A2-order={}/{} A3-overlap={}",
                browser.len(),
                ours.len(),
                findings.missing.len(),
                findings.shown_but_hidden.len(),
                findings.order_matched,
                findings.order_total,
                findings.overlaps.len(),
            );
            for line in findings.missing.iter().take(8) {
                println!("    A1 missing: {line}");
            }
            for line in findings.shown_but_hidden.iter().take(8) {
                println!("    A1 leaked: {line}");
            }
            for line in findings.overlaps.iter().take(8) {
                println!("    A3: {line}");
            }
            if std::env::var("TEXTSURFER_CORPUS_TRIAGE").is_ok() {
                println!(
                    "      limits={:?} css_warnings={} parse_errors={}",
                    page.painted.limits, page.css_warnings, page.parse_errors
                );
                triage(&reference, &browser, &ours);
            }

            // A3 has no ceiling and never will: text the browser keeps apart must not collide in
            // our grid. This is the assertion that catches a paint-containment or stacking
            // regression, and it is the one the hand-written invariant could only guess at.
            assert!(
                findings.overlaps.is_empty(),
                "{slug} {cols}x{rows} overlaps text the browser separates:\n{}",
                findings.overlaps.join("\n")
            );
            let Some((missing_ceiling, order_floor)) = expectation(slug, cols) else {
                panic!("{slug} {cols}x{rows} has no recorded expectation; add one to EXPECTED");
            };
            assert!(
                findings.missing.len() <= missing_ceiling,
                "{slug} {cols}x{rows} A1 rose to {} from {missing_ceiling}",
                findings.missing.len()
            );
            let order = (findings.order_matched * 1000)
                .checked_div(findings.order_total)
                .unwrap_or(1000) as u32;
            assert!(
                order >= order_floor,
                "{slug} {cols}x{rows} A2 fell to {order}‰ from {order_floor}‰"
            );
        }
    }
}

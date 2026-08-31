use std::collections::{BTreeMap, HashMap, HashSet};

use textsurfer::layout::BoxTree;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::manifest::{Expectation, Relation};
use super::render::{Reference, digest};

const COLUMN_PX: f64 = 8.0;
const ROW_PX: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direction {
    Ltr,
    Rtl,
}

#[derive(Clone, Debug)]
pub(super) struct Token {
    pub(super) text: String,
    pub(super) band: i64,
    pub(super) start: f64,
    pub(super) end: f64,
    pub(super) direction: Direction,
    pub(super) ordinal: usize,
}

impl Token {
    fn overlaps(&self, other: &Self) -> bool {
        self.band == other.band && self.start < other.end && other.start < self.end
    }
}

#[derive(Clone)]
pub(super) struct RunOccurrence {
    pub(super) text: String,
    pub(super) token: usize,
}

pub struct Summary {
    pub browser_tokens: usize,
    pub our_tokens: usize,
    pub missing: usize,
    pub leaked: usize,
    pub order_matched: usize,
    pub order_total: usize,
}

pub fn compare_case(
    reference: &Reference,
    tree: &BoxTree,
    expectation: &Expectation,
) -> Result<Summary, String> {
    let browser = browser_tokens(reference);
    let ours = our_tokens(tree);
    let browser_visible = browser
        .iter()
        .filter(|token| token.1)
        .map(|(token, _)| token.clone())
        .collect::<Vec<_>>();
    let browser_hidden = browser
        .iter()
        .filter(|token| !token.1)
        .map(|(token, _)| token.clone())
        .collect::<Vec<_>>();
    let missing = missing_findings(&browser_visible, &ours);
    let leaked = hidden_leak_findings(&browser_visible, &browser_hidden, &ours);
    let browser_visual = directional_order(browser_visible.clone());
    let mut our_visual = ours.clone();
    our_visual.sort_by(token_position);
    let browser_order = occurrences(&browser_visual)
        .into_iter()
        .filter(|occurrence| browser_visual[occurrence.token].direction == Direction::Ltr)
        .collect::<Vec<_>>();
    let our_order = occurrences(&our_visual);
    let order_pairs = lcs_pairs(&browser_order, &our_order);
    let matched_browser = order_pairs
        .iter()
        .map(|(browser, _)| *browser)
        .collect::<HashSet<_>>();
    let order = browser_order
        .iter()
        .enumerate()
        .filter(|(index, _)| !matched_browser.contains(index))
        .map(|(index, occurrence)| {
            format!(
                "{index}:{}",
                serde_json::to_string(&occurrence.text).expect("serialize run")
            )
        })
        .collect::<Vec<_>>();
    let overlaps = overlap_findings(&browser_visible, &ours);
    if !overlaps.is_empty() {
        return Err(format!(
            "A3 overlaps text Chromium separates:\n{}",
            overlaps.join("\n")
        ));
    }
    let actual = [
        (Relation::A1Missing, &missing),
        (Relation::A1HiddenLeak, &leaked),
        (Relation::A2Order, &order),
    ];
    let expected = expectation
        .xfails
        .iter()
        .map(|value| (value.relation, value))
        .collect::<HashMap<_, _>>();
    let mut errors = Vec::new();
    for (relation, findings) in actual {
        let digest = finding_digest(findings);
        match (findings.is_empty(), expected.get(&relation)) {
            (true, None) => {}
            (true, Some(value)) => errors.push(format!(
                "{} unexpectedly passed; remove stale count={} sha256={}",
                relation.name(),
                value.count,
                value.sha256
            )),
            (false, None) => errors.push(format_finding_mismatch(
                relation,
                findings,
                &digest,
                "unclassified",
            )),
            (false, Some(value)) if value.count == findings.len() && value.sha256 == digest => {}
            (false, Some(_)) => errors.push(format_finding_mismatch(
                relation, findings, &digest, "changed",
            )),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(Summary {
        browser_tokens: browser_visible.len(),
        our_tokens: ours.len(),
        missing: missing.len(),
        leaked: leaked.len(),
        order_matched: order_pairs.len(),
        order_total: browser_order.len(),
    })
}

fn browser_tokens(reference: &Reference) -> Vec<(Token, bool)> {
    reference
        .words
        .iter()
        .enumerate()
        .map(|(ordinal, word)| {
            (
                Token {
                    text: word.0.clone(),
                    band: ((word.2 + word.4 / 2.0) / ROW_PX).floor() as i64,
                    start: word.1,
                    end: word.1 + word.3,
                    direction: if word.6 == 1 {
                        Direction::Rtl
                    } else {
                        Direction::Ltr
                    },
                    ordinal,
                },
                word.5 == 1,
            )
        })
        .collect()
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
                let ordinal = tokens.len();
                tokens.push(Token {
                    text: std::mem::take(text),
                    band: fragment.row as i64,
                    start: start as f64 * COLUMN_PX,
                    end: end as f64 * COLUMN_PX,
                    direction: Direction::Ltr,
                    ordinal,
                });
            }
        };
        for grapheme in fragment.text.graphemes(true) {
            let width = UnicodeWidthStr::width(grapheme).saturating_mul(scale);
            if grapheme.chars().all(char::is_whitespace) {
                flush(&mut text, start, col, &mut tokens);
                col += width;
                start = col;
            } else {
                text.push_str(grapheme);
                col += width;
            }
        }
        flush(&mut text, start, col, &mut tokens);
    }
    tokens
}

pub(super) fn directional_order(tokens: Vec<Token>) -> Vec<Token> {
    let mut bands = BTreeMap::<i64, Vec<Token>>::new();
    for token in tokens {
        bands.entry(token.band).or_default().push(token);
    }
    let mut ordered = Vec::new();
    for (_, mut band) in bands {
        band.sort_by_key(|token| token.ordinal);
        let mut start = 0;
        while start < band.len() {
            let direction = band[start].direction;
            let mut end = start + 1;
            while end < band.len() && band[end].direction == direction {
                end += 1;
            }
            match direction {
                Direction::Ltr => band[start..end].sort_by(token_position),
                Direction::Rtl => band[start..end].sort_by(|left, right| {
                    right
                        .start
                        .partial_cmp(&left.start)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(left.ordinal.cmp(&right.ordinal))
                }),
            }
            ordered.extend_from_slice(&band[start..end]);
            start = end;
        }
    }
    ordered
}

fn token_position(left: &Token, right: &Token) -> std::cmp::Ordering {
    left.band
        .cmp(&right.band)
        .then(
            left.start
                .partial_cmp(&right.start)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
        .then(left.ordinal.cmp(&right.ordinal))
}

fn runs(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|run| !run.is_empty())
        .map(str::to_string)
}

fn occurrences(tokens: &[Token]) -> Vec<RunOccurrence> {
    tokens
        .iter()
        .enumerate()
        .flat_map(|(token, value)| runs(&value.text).map(move |text| RunOccurrence { text, token }))
        .collect()
}

fn counts(tokens: &[Token]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for run in occurrences(tokens) {
        *counts.entry(run.text).or_insert(0) += 1;
    }
    counts
}

pub(super) fn missing_findings(browser: &[Token], ours: &[Token]) -> Vec<String> {
    let browser = counts(browser);
    let ours = counts(ours);
    let mut findings = browser
        .iter()
        .filter_map(|(text, wanted)| {
            let got = ours.get(text).copied().unwrap_or(0);
            (got < *wanted).then(|| {
                format!(
                    "{}:browser={wanted}:textsurfer={got}",
                    serde_json::to_string(text).expect("serialize run")
                )
            })
        })
        .collect::<Vec<_>>();
    findings.sort();
    findings
}

pub(super) fn hidden_leak_findings(
    visible: &[Token],
    hidden: &[Token],
    ours: &[Token],
) -> Vec<String> {
    let visible = counts(visible);
    let hidden = counts(hidden);
    let ours = counts(ours);
    let mut findings = hidden
        .iter()
        .filter_map(|(text, hidden_count)| {
            let visible_count = visible.get(text).copied().unwrap_or(0);
            let our_count = ours.get(text).copied().unwrap_or(0);
            let leaked = our_count
                .saturating_sub(visible_count)
                .min(*hidden_count);
            (leaked > 0).then(|| {
                format!(
                    "{}:visible={visible_count}:hidden={hidden_count}:textsurfer={our_count}:leaked={leaked}",
                    serde_json::to_string(text).expect("serialize run")
                )
            })
        })
        .collect::<Vec<_>>();
    findings.sort();
    findings
}

pub(super) fn lcs_pairs(left: &[RunOccurrence], right: &[RunOccurrence]) -> Vec<(usize, usize)> {
    struct Node {
        left: usize,
        right: usize,
        previous: Option<usize>,
    }
    let mut positions: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, occurrence) in right.iter().enumerate() {
        positions
            .entry(occurrence.text.as_str())
            .or_default()
            .push(index);
    }
    let mut tails = Vec::<usize>::new();
    let mut tail_nodes = Vec::<usize>::new();
    let mut nodes = Vec::<Node>::new();
    for (left_index, occurrence) in left.iter().enumerate() {
        let Some(matches) = positions.get(occurrence.text.as_str()) else {
            continue;
        };
        for &right_index in matches.iter().rev() {
            let slot = tails.partition_point(|position| *position < right_index);
            if slot < tails.len() && tails[slot] == right_index {
                continue;
            }
            let node = nodes.len();
            nodes.push(Node {
                left: left_index,
                right: right_index,
                previous: slot.checked_sub(1).map(|previous| tail_nodes[previous]),
            });
            if slot == tails.len() {
                tails.push(right_index);
                tail_nodes.push(node);
            } else {
                tails[slot] = right_index;
                tail_nodes[slot] = node;
            }
        }
    }
    let mut pairs = Vec::new();
    let mut node = tail_nodes.last().copied();
    while let Some(index) = node {
        let value = &nodes[index];
        pairs.push((value.left, value.right));
        node = value.previous;
    }
    pairs.reverse();
    pairs
}

pub(super) fn overlap_findings(browser: &[Token], ours: &[Token]) -> Vec<String> {
    let mut browser_source = browser.to_vec();
    browser_source.sort_by_key(|token| token.ordinal);
    let mut our_source = ours.to_vec();
    our_source.sort_by_key(|token| token.ordinal);
    let browser_runs = occurrences(&browser_source);
    let our_runs = occurrences(&our_source);
    let pairs = lcs_pairs(&browser_runs, &our_runs);
    let mut correspondence = HashMap::new();
    for (browser_run, our_run) in pairs {
        correspondence
            .entry(our_runs[our_run].token)
            .or_insert(browser_runs[browser_run].token);
    }
    let mut visual = our_source.iter().enumerate().collect::<Vec<_>>();
    visual.sort_by(|left, right| token_position(left.1, right.1));
    let mut findings = Vec::new();
    for (position, (left_index, left)) in visual.iter().enumerate() {
        for (right_index, right) in &visual[position + 1..] {
            if right.band != left.band {
                break;
            }
            if !left.overlaps(right) {
                continue;
            }
            let (Some(browser_left), Some(browser_right)) = (
                correspondence.get(left_index),
                correspondence.get(right_index),
            ) else {
                continue;
            };
            if browser_left == browser_right {
                continue;
            }
            let browser_left = &browser_source[*browser_left];
            let browser_right = &browser_source[*browser_right];
            if !browser_left.overlaps(browser_right) {
                findings.push(format!(
                    "band {} col {}: {:?} and {:?}",
                    left.band,
                    (left.start / COLUMN_PX) as usize,
                    left.text,
                    right.text
                ));
            }
        }
    }
    findings.sort();
    findings.dedup();
    findings
}

fn finding_digest(findings: &[String]) -> String {
    digest(findings.join("\n").as_bytes())
}

fn format_finding_mismatch(
    relation: Relation,
    findings: &[String],
    digest: &str,
    state: &str,
) -> String {
    let sample = findings
        .iter()
        .take(8)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n  ");
    format!(
        "{} {state}: count={} sha256={digest}\n  {}",
        relation.name(),
        findings.len(),
        sample
    )
}

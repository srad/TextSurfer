use textwrap::core::Fragment as WrapFragment;
use textwrap::wrap_algorithms::wrap_first_fit;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::core::dom::NodeId;
use crate::core::style::{CellStyle, WhiteSpace};

pub(super) trait Atom {
    fn width(&self) -> usize;
    fn height(&self) -> usize;

    fn baseline(&self) -> usize {
        self.height().saturating_sub(1)
    }
}

#[derive(Clone)]
pub(super) struct Piece<A> {
    pub node: NodeId,
    pub text: String,
    pub white_space: WhiteSpace,
    pub depth: usize,
    pub style: CellStyle,
    pub hidden: bool,
    pub atom: Option<A>,
}

#[derive(Clone, Debug)]
pub(super) struct Glyph {
    pub source: usize,
    pub node: NodeId,
    pub text: String,
    pub width: usize,
    pub depth: usize,
    pub white_space: WhiteSpace,
    pub style: CellStyle,
    pub hidden: bool,
    pub atom: Option<usize>,
}

#[derive(Debug)]
struct Word {
    glyphs: Vec<Glyph>,
    whitespace: Option<Glyph>,
}

impl WrapFragment for Word {
    fn width(&self) -> f64 {
        self.glyphs.iter().map(|glyph| glyph.width).sum::<usize>() as f64
    }

    fn whitespace_width(&self) -> f64 {
        self.whitespace.as_ref().map_or(0, |glyph| glyph.width) as f64
    }

    fn penalty_width(&self) -> f64 {
        0.0
    }
}

pub(super) fn flatten_glyphs<A: Atom>(pieces: &[Piece<A>]) -> Vec<Glyph> {
    let mut glyphs = Vec::new();
    let mut source = String::new();
    let mut spans = Vec::new();
    for (index, piece) in pieces.iter().enumerate() {
        if let Some(atom) = &piece.atom {
            append_text_glyphs(&mut glyphs, &source, &spans);
            source.clear();
            spans.clear();
            glyphs.push(Glyph {
                source: 0,
                node: piece.node,
                text: String::new(),
                width: atom.width(),
                depth: piece.depth,
                white_space: piece.white_space,
                style: piece.style,
                hidden: piece.hidden,
                atom: Some(index),
            });
            continue;
        }
        let start = source.len();
        source.push_str(&piece.text);
        spans.push((
            start,
            source.len(),
            piece.node,
            piece.white_space,
            piece.depth,
            piece.style,
            piece.hidden,
        ));
    }
    append_text_glyphs(&mut glyphs, &source, &spans);
    for (source, glyph) in glyphs.iter_mut().enumerate() {
        glyph.source = source;
    }
    glyphs
}

fn append_text_glyphs(
    glyphs: &mut Vec<Glyph>,
    source: &str,
    spans: &[(usize, usize, NodeId, WhiteSpace, usize, CellStyle, bool)],
) {
    if source.is_empty() {
        return;
    }
    let mut span = 0;
    glyphs.extend(source.grapheme_indices(true).map(|(offset, text)| {
        while span + 1 < spans.len() && offset >= spans[span].1 {
            span += 1;
        }
        let (_, _, node, white_space, depth, style, hidden) = spans[span];
        Glyph {
            source: 0,
            node,
            text: text.to_string(),
            width: UnicodeWidthStr::width(text).saturating_mul(usize::from(style.scale)),
            depth,
            white_space,
            style,
            hidden,
            atom: None,
        }
    }));
}

pub(super) fn format_inline<A: Atom>(pieces: &[Piece<A>], width: usize) -> Vec<Vec<Glyph>> {
    let mut glyphs = flatten_glyphs(pieces);
    format_glyphs(&mut glyphs, width)
}

pub(super) fn format_glyphs(glyphs: &mut [Glyph], width: usize) -> Vec<Vec<Glyph>> {
    apply_scale_caps(glyphs, width);
    let mode = glyphs.first().map(|glyph| glyph.white_space);
    if glyphs.iter().all(|glyph| Some(glyph.white_space) == mode) {
        match mode.unwrap_or_default() {
            WhiteSpace::Normal => format_collapsed(glyphs, width, true),
            WhiteSpace::NoWrap => format_collapsed(glyphs, width, false),
            WhiteSpace::Pre => format_preserved(glyphs, width, false),
            WhiteSpace::PreWrap | WhiteSpace::BreakSpaces => format_preserved(glyphs, width, true),
            WhiteSpace::PreLine => format_pre_line(glyphs, width),
        }
    } else {
        format_mixed(glyphs, width)
    }
}

fn apply_scale_caps(glyphs: &mut [Glyph], width: usize) {
    let mut start = 0usize;
    for index in 0..glyphs.len() {
        if forced_break(&glyphs[index]) {
            apply_scale_cap(&mut glyphs[start..index], width);
            start = index + 1;
        }
    }
    apply_scale_cap(&mut glyphs[start..], width);
}

fn apply_scale_cap(glyphs: &mut [Glyph], width: usize) {
    let cap = scale_cap(glyphs, width);
    for glyph in glyphs {
        if glyph.atom.is_none() {
            glyph.style.scale = glyph.style.scale.min(cap);
            glyph.width = UnicodeWidthStr::width(glyph.text.as_str())
                .saturating_mul(usize::from(glyph.style.scale));
        }
    }
}

fn scale_cap(glyphs: &[Glyph], width: usize) -> u8 {
    for cap in (1..=4).rev() {
        let mut current = 0usize;
        for glyph in glyphs {
            if glyph.atom.is_some() {
                current = current.saturating_add(glyph.width);
            } else {
                current = current.saturating_add(
                    UnicodeWidthStr::width(glyph.text.as_str())
                        .saturating_mul(usize::from(glyph.style.scale.min(cap))),
                );
            }
        }
        if current <= width {
            return cap;
        }
    }
    1
}

fn forced_break(glyph: &Glyph) -> bool {
    glyph.atom.is_none()
        && glyph.text == "\n"
        && matches!(
            glyph.white_space,
            WhiteSpace::Pre | WhiteSpace::PreWrap | WhiteSpace::PreLine | WhiteSpace::BreakSpaces
        )
}

fn format_collapsed(glyphs: &[Glyph], width: usize, wrap: bool) -> Vec<Vec<Glyph>> {
    let mut words = Vec::new();
    let mut current = Vec::new();
    let mut whitespace = None;
    for glyph in glyphs {
        if glyph.atom.is_none() && glyph.text.chars().all(char::is_whitespace) {
            if !current.is_empty() {
                let mut space = glyph.clone();
                space.text = " ".to_string();
                space.width = usize::from(space.style.scale);
                whitespace = Some(space);
            }
        } else {
            if whitespace.is_some() {
                words.push(Word {
                    glyphs: std::mem::take(&mut current),
                    whitespace: whitespace.take(),
                });
            }
            current.push(glyph.clone());
        }
    }
    if !current.is_empty() {
        words.push(Word {
            glyphs: current,
            whitespace: None,
        });
    }
    if words.is_empty() {
        return Vec::new();
    }
    let words = split_long_words(words, width.max(1));
    if !wrap {
        return vec![join_words(&words)];
    }
    wrap_first_fit(&words, &[width.max(1) as f64])
        .into_iter()
        .map(join_words)
        .collect()
}

fn split_long_words(words: Vec<Word>, width: usize) -> Vec<Word> {
    let mut split = Vec::new();
    for word in words {
        let mut chunk = Vec::new();
        let mut used = 0usize;
        for glyph in word.glyphs {
            if !chunk.is_empty() && used.saturating_add(glyph.width) > width {
                split.push(Word {
                    glyphs: std::mem::take(&mut chunk),
                    whitespace: None,
                });
                used = 0;
            }
            used = used.saturating_add(glyph.width);
            chunk.push(glyph);
        }
        split.push(Word {
            glyphs: chunk,
            whitespace: word.whitespace,
        });
    }
    split
}

fn join_words(words: &[Word]) -> Vec<Glyph> {
    let mut glyphs = Vec::new();
    for (index, word) in words.iter().enumerate() {
        glyphs.extend(word.glyphs.iter().cloned());
        if index + 1 < words.len()
            && let Some(whitespace) = &word.whitespace
        {
            glyphs.push(whitespace.clone());
        }
    }
    glyphs
}

fn format_pre_line(glyphs: &[Glyph], width: usize) -> Vec<Vec<Glyph>> {
    let mut lines = Vec::new();
    let mut segment = Vec::new();
    for glyph in glyphs {
        if glyph.atom.is_none() && glyph.text == "\n" {
            let wrapped = format_collapsed(&segment, width, true);
            if wrapped.is_empty() {
                lines.push(Vec::new());
            } else {
                lines.extend(wrapped);
            }
            segment.clear();
        } else {
            segment.push(glyph.clone());
        }
    }
    let wrapped = format_collapsed(&segment, width, true);
    if wrapped.is_empty()
        && !glyphs.is_empty()
        && glyphs.last().is_some_and(|glyph| glyph.text == "\n")
    {
        lines.push(Vec::new());
    } else {
        lines.extend(wrapped);
    }
    lines
}

fn format_preserved(glyphs: &[Glyph], width: usize, wrap: bool) -> Vec<Vec<Glyph>> {
    let mut items = Vec::new();
    for glyph in glyphs {
        if glyph.atom.is_none() && glyph.text == "\n" {
            items.push(LineItem::Break);
        } else if glyph.atom.is_none() && glyph.text == "\t" {
            items.push(LineItem::Tab(glyph.clone(), wrap));
        } else {
            items.push(LineItem::Glyph(glyph.clone(), wrap, false));
        }
    }
    layout_items(items, width)
}

fn format_mixed(glyphs: &[Glyph], width: usize) -> Vec<Vec<Glyph>> {
    let mut items = Vec::new();
    let mut pending = None;
    let mut has_content = false;
    for glyph in glyphs {
        let collapses = matches!(
            glyph.white_space,
            WhiteSpace::Normal | WhiteSpace::NoWrap | WhiteSpace::PreLine
        );
        if collapses && glyph.atom.is_none() && glyph.text.chars().all(char::is_whitespace) {
            if glyph.white_space == WhiteSpace::PreLine && glyph.text == "\n" {
                pending = None;
                items.push(LineItem::Break);
                has_content = false;
            } else if has_content {
                let mut space = glyph.clone();
                space.text = " ".to_string();
                space.width = usize::from(space.style.scale);
                pending = Some(space);
            }
            continue;
        }
        if let Some(space) = pending.take() {
            let wraps = space.white_space != WhiteSpace::NoWrap;
            items.push(LineItem::Glyph(space, wraps, true));
        }
        if glyph.atom.is_none() && glyph.text == "\n" {
            items.push(LineItem::Break);
            has_content = false;
        } else if glyph.atom.is_none() && glyph.text == "\t" {
            let wraps = matches!(
                glyph.white_space,
                WhiteSpace::PreWrap | WhiteSpace::BreakSpaces
            );
            items.push(LineItem::Tab(glyph.clone(), wraps));
            has_content = true;
        } else {
            let wraps = matches!(
                glyph.white_space,
                WhiteSpace::Normal
                    | WhiteSpace::PreWrap
                    | WhiteSpace::PreLine
                    | WhiteSpace::BreakSpaces
            );
            items.push(LineItem::Glyph(glyph.clone(), wraps, false));
            has_content = true;
        }
    }
    layout_items(items, width)
}

enum LineItem {
    Glyph(Glyph, bool, bool),
    Tab(Glyph, bool),
    Break,
}

fn layout_items(items: Vec<LineItem>, width: usize) -> Vec<Vec<Glyph>> {
    let width = width.max(1);
    let mut state = LineState::default();
    let mut ended_with_break = false;
    for item in items {
        match item {
            LineItem::Break => {
                trim_collapsed(&mut state.line);
                state.lines.push(std::mem::take(&mut state.line));
                state.used = 0;
                state.last_break = None;
                ended_with_break = true;
            }
            LineItem::Tab(glyph, wrap) => {
                ended_with_break = false;
                let count = 8 - state.used % 8;
                for _ in 0..count {
                    let mut space = glyph.clone();
                    space.text = " ".to_string();
                    space.width = usize::from(space.style.scale);
                    push_glyph(space, wrap, false, width, &mut state);
                }
            }
            LineItem::Glyph(glyph, wrap, trimmable) => {
                ended_with_break = false;
                push_glyph(glyph, wrap, trimmable, width, &mut state);
            }
        }
    }
    trim_collapsed(&mut state.line);
    if !state.line.is_empty() || state.lines.is_empty() || ended_with_break {
        state.lines.push(state.line);
    }
    state.lines
}

#[derive(Default)]
struct LineState {
    line: Vec<Glyph>,
    used: usize,
    last_break: Option<usize>,
    lines: Vec<Vec<Glyph>>,
}

fn push_glyph(glyph: Glyph, wrap: bool, trimmable: bool, width: usize, state: &mut LineState) {
    if wrap && !state.line.is_empty() && state.used.saturating_add(glyph.width) > width {
        if let Some(index) = state.last_break.take() {
            let remainder = state.line.split_off(index);
            trim_collapsed(&mut state.line);
            state.lines.push(std::mem::take(&mut state.line));
            state.line = remainder;
            state.used = state.line.iter().map(|glyph| glyph.width).sum();
        } else {
            trim_collapsed(&mut state.line);
            state.lines.push(std::mem::take(&mut state.line));
            state.used = 0;
        }
        state.last_break = state
            .line
            .iter()
            .rposition(is_break_opportunity)
            .map(|index| index + 1);
        if !state.line.is_empty() && state.used.saturating_add(glyph.width) > width {
            state.lines.push(std::mem::take(&mut state.line));
            state.used = 0;
            state.last_break = None;
        }
    }
    if trimmable && state.line.is_empty() {
        return;
    }
    state.used = state.used.saturating_add(glyph.width);
    let break_after = wrap && is_break_opportunity(&glyph);
    state.line.push(glyph);
    if break_after {
        state.last_break = Some(state.line.len());
    }
}

fn is_break_opportunity(glyph: &Glyph) -> bool {
    glyph.atom.is_none()
        && (glyph.text.chars().all(char::is_whitespace) || breaks_between_letters(&glyph.text))
}

fn breaks_between_letters(text: &str) -> bool {
    if UnicodeWidthStr::width(text) > 1 {
        return true;
    }
    text.chars()
        .next()
        .is_some_and(|ch| !ch.is_ascii() && !ch.is_alphanumeric() && !is_word_joiner(ch))
}

fn is_word_joiner(ch: char) -> bool {
    matches!(ch, '\u{00ad}' | '\u{2019}' | '\u{2018}' | '\u{02bc}') || ch.is_alphabetic()
}

fn trim_collapsed(line: &mut Vec<Glyph>) {
    while line.last().is_some_and(|glyph| {
        glyph.text == " "
            && matches!(
                glyph.white_space,
                WhiteSpace::Normal | WhiteSpace::NoWrap | WhiteSpace::PreLine
            )
    }) {
        line.pop();
    }
}

pub(super) fn intrinsic_width<A: Atom>(pieces: &[Piece<A>]) -> usize {
    format_inline(pieces, usize::MAX / 4)
        .iter()
        .map(|line| line.iter().map(|glyph| glyph.width).sum())
        .max()
        .unwrap_or(0)
}

pub(super) fn min_content_width<A: Atom>(pieces: &[Piece<A>]) -> usize {
    let mut glyphs = flatten_glyphs(pieces);
    for glyph in &mut glyphs {
        if glyph.atom.is_none() {
            glyph.width = if glyph.style.scale == 0 {
                0
            } else {
                UnicodeWidthStr::width(glyph.text.as_str())
            };
        }
    }
    let mode = glyphs.first().map(|glyph| glyph.white_space);
    if glyphs.iter().all(|glyph| Some(glyph.white_space) == mode)
        && matches!(mode, Some(WhiteSpace::NoWrap | WhiteSpace::Pre))
    {
        return glyphs
            .split(|glyph| glyph.atom.is_none() && glyph.text == "\n")
            .map(|line| line.iter().map(|glyph| glyph.width).sum())
            .max()
            .unwrap_or(0);
    }

    let mut widest = 0usize;
    let mut current = 0usize;
    let mut pending_collapsed_space = false;
    for glyph in &glyphs {
        if glyph.atom.is_some() {
            finish_min_segment(&mut widest, &mut current);
            widest = widest.max(glyph.width);
            pending_collapsed_space = false;
            continue;
        }
        let whitespace = glyph.text.chars().all(char::is_whitespace);
        let collapses = matches!(
            glyph.white_space,
            WhiteSpace::Normal | WhiteSpace::NoWrap | WhiteSpace::PreLine
        );
        if whitespace {
            if glyph.text == "\n"
                && matches!(
                    glyph.white_space,
                    WhiteSpace::Pre
                        | WhiteSpace::PreWrap
                        | WhiteSpace::PreLine
                        | WhiteSpace::BreakSpaces
                )
            {
                finish_min_segment(&mut widest, &mut current);
                pending_collapsed_space = false;
            } else if collapses {
                if glyph.white_space == WhiteSpace::NoWrap && current > 0 {
                    pending_collapsed_space = true;
                } else {
                    finish_min_segment(&mut widest, &mut current);
                }
            } else if glyph.white_space == WhiteSpace::Pre {
                if glyph.text == "\t" {
                    current = current.saturating_add(8 - current % 8);
                } else {
                    current = current.saturating_add(glyph.width);
                }
            } else if glyph.text == "\t" {
                current = current.saturating_add(1);
                finish_min_segment(&mut widest, &mut current);
                widest = widest.max(1);
            } else {
                current = current.saturating_add(glyph.width);
                finish_min_segment(&mut widest, &mut current);
            }
            continue;
        }
        if pending_collapsed_space {
            current = current.saturating_add(1);
            pending_collapsed_space = false;
        }
        current = current.saturating_add(glyph.width);
        if glyph.white_space != WhiteSpace::NoWrap && breaks_between_letters(&glyph.text) {
            finish_min_segment(&mut widest, &mut current);
        }
    }
    finish_min_segment(&mut widest, &mut current);
    widest
}

fn finish_min_segment(widest: &mut usize, current: &mut usize) {
    *widest = (*widest).max(*current);
    *current = 0;
}

pub(super) fn line_height<A: Atom>(line: &[Glyph], pieces: &[Piece<A>]) -> usize {
    line_metrics(line, pieces).0
}

pub(super) fn line_metrics<A: Atom>(line: &[Glyph], pieces: &[Piece<A>]) -> (usize, usize) {
    let mut baseline = 0;
    let mut below = 0;
    for atom in line
        .iter()
        .filter_map(|glyph| glyph.atom.and_then(|index| pieces[index].atom.as_ref()))
    {
        let atom_baseline = atom.baseline().min(atom.height().saturating_sub(1));
        baseline = baseline.max(atom_baseline);
        below = below.max(atom.height().saturating_sub(atom_baseline + 1));
    }
    for glyph in line.iter().filter(|glyph| glyph.atom.is_none()) {
        let height = usize::from(glyph.style.scale);
        baseline = baseline.max(height.saturating_sub(1));
    }
    (baseline + below + 1, baseline)
}

pub(super) fn formatted_height<A: Atom>(lines: &[Vec<Glyph>], pieces: &[Piece<A>]) -> usize {
    lines.iter().map(|line| line_height(line, pieces)).sum()
}

pub(super) fn normalize_segment_breaks(source: &str) -> String {
    source
        .replace("\r\n", "\n")
        .replace(['\r', '\u{000c}'], "\n")
}

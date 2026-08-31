use std::collections::HashMap;

use super::{BoxTree, LayoutRect, PaintStyleSource};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PaintIndex {
    sources: HashMap<PaintStyleSource, PaintPrimitives>,
    fills: IntervalIndex,
    strokes: IntervalIndex,
    fragments: IntervalIndex,
    float_fills: Vec<bool>,
    float_strokes: Vec<bool>,
    float_fragments: Vec<bool>,
    scaled_runs: Vec<Option<usize>>,
    unresolved: PaintPrimitives,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::layout) struct PaintPrimitives {
    pub(in crate::layout) boxes: Vec<usize>,
    pub(in crate::layout) fragments: Vec<usize>,
    pub(in crate::layout) fills: Vec<usize>,
    pub(in crate::layout) strokes: Vec<usize>,
}

impl PaintIndex {
    pub(super) fn build(tree: &BoxTree) -> Self {
        let mut index = Self::default();
        for (primitive, source) in tree.paint_sources.boxes.iter().copied().enumerate() {
            index.insert(source, |entries| entries.boxes.push(primitive));
        }
        for (primitive, source) in tree.paint_sources.fragments.iter().copied().enumerate() {
            index.insert(source, |entries| entries.fragments.push(primitive));
        }
        for (primitive, source) in tree.paint_sources.fills.iter().copied().enumerate() {
            index.insert(source, |entries| entries.fills.push(primitive));
        }
        for (primitive, source) in tree.paint_sources.strokes.iter().copied().enumerate() {
            index.insert(source, |entries| entries.strokes.push(primitive));
        }
        index.fills = IntervalIndex::build(
            tree.fills
                .iter()
                .enumerate()
                .map(|(index, fill)| (fill.rect, index)),
        );
        index.strokes = IntervalIndex::build(
            tree.strokes
                .iter()
                .enumerate()
                .map(|(index, stroke)| (stroke.rect, index)),
        );
        index.fragments = IntervalIndex::build(
            tree.fragments
                .iter()
                .enumerate()
                .map(|(index, fragment)| (fragment.rect(), index)),
        );
        index.float_fills = membership(tree.fills.len(), &tree.float_fills);
        index.float_strokes = membership(tree.strokes.len(), &tree.float_strokes);
        index.float_fragments = membership(tree.fragments.len(), &tree.float_fragments);
        let mut fragments = tree.fragments.iter().enumerate().collect::<Vec<_>>();
        fragments.sort_by_key(|(primitive, fragment)| {
            (
                index.float_fragments[*primitive],
                fragment.depth,
                fragment.row,
                fragment.col,
            )
        });
        index.scaled_runs = vec![None; tree.fragments.len()];
        let mut run = 0;
        for (primitive, fragment) in fragments {
            if fragment.style.scale > 1 && fragment.row < tree.height && fragment.col < tree.width {
                index.scaled_runs[primitive] = Some(run);
                run += 1;
            }
        }
        index
    }

    fn insert(&mut self, source: PaintStyleSource, add: impl FnOnce(&mut PaintPrimitives)) {
        if source == PaintStyleSource::Missing {
            add(&mut self.unresolved);
        } else {
            add(self.sources.entry(source).or_default());
        }
    }

    pub(super) fn source(&self, source: PaintStyleSource) -> Option<&PaintPrimitives> {
        self.sources.get(&source)
    }

    pub(super) fn unresolved(&self) -> &PaintPrimitives {
        &self.unresolved
    }

    pub(super) fn fills_at(&self, row: usize, output: &mut Vec<usize>) {
        self.fills.query(row, output);
    }

    pub(super) fn strokes_at(&self, row: usize, output: &mut Vec<usize>) {
        self.strokes.query(row, output);
    }

    pub(super) fn fragments_at(&self, row: usize, output: &mut Vec<usize>) {
        self.fragments.query(row, output);
    }

    pub(super) fn scaled_run(&self, fragment: usize) -> Option<usize> {
        self.scaled_runs.get(fragment).copied().flatten()
    }

    pub(super) fn fill_is_float(&self, fill: usize) -> bool {
        self.float_fills.get(fill).copied().unwrap_or(false)
    }

    pub(super) fn stroke_is_float(&self, stroke: usize) -> bool {
        self.float_strokes.get(stroke).copied().unwrap_or(false)
    }

    pub(super) fn fragment_is_float(&self, fragment: usize) -> bool {
        self.float_fragments.get(fragment).copied().unwrap_or(false)
    }
}

fn membership(len: usize, members: &[usize]) -> Vec<bool> {
    let mut membership = vec![false; len];
    for member in members.iter().copied().filter(|member| *member < len) {
        membership[member] = true;
    }
    membership
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Interval {
    start: usize,
    end: usize,
    value: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct IntervalIndex {
    root: Option<Box<IntervalNode>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IntervalNode {
    center: usize,
    by_start: Vec<Interval>,
    by_end: Vec<Interval>,
    left: Option<Box<Self>>,
    right: Option<Box<Self>>,
}

impl IntervalIndex {
    fn build(rects: impl Iterator<Item = (LayoutRect, usize)>) -> Self {
        let intervals = rects
            .filter_map(|(rect, value)| {
                let end = rect.row.saturating_add(rect.height);
                (end > rect.row).then_some(Interval {
                    start: rect.row,
                    end,
                    value,
                })
            })
            .collect();
        Self {
            root: IntervalNode::build(intervals),
        }
    }

    fn query(&self, row: usize, output: &mut Vec<usize>) {
        if let Some(root) = &self.root {
            root.query(row, output);
        }
    }
}

impl IntervalNode {
    fn build(mut intervals: Vec<Interval>) -> Option<Box<Self>> {
        if intervals.is_empty() {
            return None;
        }
        intervals
            .sort_unstable_by_key(|interval| interval.start + (interval.end - interval.start) / 2);
        let center = intervals[intervals.len() / 2].start
            + (intervals[intervals.len() / 2].end - intervals[intervals.len() / 2].start) / 2;
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut overlapping = Vec::new();
        for interval in intervals {
            if interval.end <= center {
                left.push(interval);
            } else if interval.start > center {
                right.push(interval);
            } else {
                overlapping.push(interval);
            }
        }
        let mut by_start = overlapping.clone();
        by_start.sort_unstable_by_key(|interval| interval.start);
        overlapping.sort_unstable_by_key(|interval| std::cmp::Reverse(interval.end));
        Some(Box::new(Self {
            center,
            by_start,
            by_end: overlapping,
            left: Self::build(left),
            right: Self::build(right),
        }))
    }

    fn query(&self, row: usize, output: &mut Vec<usize>) {
        if row < self.center {
            output.extend(
                self.by_start
                    .iter()
                    .take_while(|interval| interval.start <= row)
                    .map(|interval| interval.value),
            );
            if let Some(left) = &self.left {
                left.query(row, output);
            }
        } else {
            output.extend(
                self.by_end
                    .iter()
                    .take_while(|interval| interval.end > row)
                    .map(|interval| interval.value),
            );
            if row > self.center
                && let Some(right) = &self.right
            {
                right.query(row, output);
            }
        }
    }
}

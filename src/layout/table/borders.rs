use std::collections::BTreeMap;

use crate::core::style::{BorderEdges, BorderLineStyle, BorderSide, CellStyle};
use crate::layout::LayoutRect;

#[derive(Clone, Copy)]
pub(super) struct BorderCandidate {
    pub(super) rect: LayoutRect,
    pub(super) edges: BorderEdges,
    pub(super) side: BorderSide,
    pub(super) style: CellStyle,
    pub(super) depth: usize,
}

pub(super) fn add_collapsed_candidates(
    segments: &mut BTreeMap<(bool, usize, usize), BorderCandidate>,
    rect: LayoutRect,
    edges: BorderEdges,
    style: CellStyle,
    depth: usize,
) {
    let right = rect.col + rect.width.saturating_sub(1);
    let bottom = rect.row + rect.height.saturating_sub(1);
    for col in rect.col..right {
        choose_candidate(
            segments,
            (true, rect.row, col),
            horizontal_candidate(col, rect.row, edges.top, style, depth),
        );
        choose_candidate(
            segments,
            (true, bottom, col),
            horizontal_candidate(col, bottom, edges.bottom, style, depth),
        );
    }
    for row in rect.row..bottom {
        choose_candidate(
            segments,
            (false, row, rect.col),
            vertical_candidate(rect.col, row, edges.left, style, depth),
        );
        choose_candidate(
            segments,
            (false, row, right),
            vertical_candidate(right, row, edges.right, style, depth),
        );
    }
}

fn horizontal_candidate(
    col: usize,
    row: usize,
    side: BorderSide,
    style: CellStyle,
    depth: usize,
) -> BorderCandidate {
    BorderCandidate {
        rect: LayoutRect {
            col,
            row,
            width: 2,
            height: 1,
        },
        edges: BorderEdges {
            top: side,
            ..Default::default()
        },
        side,
        style,
        depth,
    }
}

fn vertical_candidate(
    col: usize,
    row: usize,
    side: BorderSide,
    style: CellStyle,
    depth: usize,
) -> BorderCandidate {
    BorderCandidate {
        rect: LayoutRect {
            col,
            row,
            width: 1,
            height: 2,
        },
        edges: BorderEdges {
            left: side,
            ..Default::default()
        },
        side,
        style,
        depth,
    }
}

fn choose_candidate(
    segments: &mut BTreeMap<(bool, usize, usize), BorderCandidate>,
    key: (bool, usize, usize),
    candidate: BorderCandidate,
) {
    match segments.get(&key) {
        Some(current)
            if border_priority(current.side, current.depth)
                >= border_priority(candidate.side, candidate.depth) => {}
        _ => {
            segments.insert(key, candidate);
        }
    }
}

fn border_priority(side: BorderSide, origin: usize) -> (u8, u8, usize) {
    if side.style == BorderLineStyle::Hidden {
        return (3, u8::MAX, origin);
    }
    if side.style == BorderLineStyle::None || side.width == 0 {
        return (0, 0, origin);
    }
    (2, side.style as u8, origin)
}

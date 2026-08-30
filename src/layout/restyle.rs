use crate::core::dom::{Document, Node, NodeId};
use crate::core::style::{CellStyle, PseudoElement, StyleTree};
use std::collections::HashSet;
use std::ops::Range;

use super::engine::{
    BackgroundFill, BorderStroke, BoxTree, PaintSources, PaintStyleSource, TextFragment,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestyleFailure {
    LayoutChanged,
    SourceCount,
    UnresolvedSource,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RestyleDamage {
    pub(crate) rows: Vec<Range<usize>>,
    pub(crate) sources_visited: usize,
    pub(crate) primitives_visited: usize,
}

pub(crate) fn capture_sources(tree: &mut BoxTree, document: &Document, styles: &StyleTree) {
    add_dormant_paint(tree, styles);
    tree.paint_sources = PaintSources {
        boxes: tree.boxes.iter().map(|box_| box_.paint_source).collect(),
        fragments: tree
            .fragments
            .iter()
            .map(|fragment| fragment_source(document, styles, fragment))
            .collect(),
        fills: tree
            .fills
            .iter()
            .map(|fill| box_source_for_fill(tree, fill))
            .collect(),
        strokes: tree
            .strokes
            .iter()
            .map(|stroke| box_source_for_stroke(tree, stroke))
            .collect(),
    };
    tree.rebuild_paint_index();
}

pub(crate) fn restyle(
    tree: &mut BoxTree,
    previous: &StyleTree,
    next: &StyleTree,
) -> Result<RestyleDamage, RestyleFailure> {
    restyle_impl(tree, previous, next, None)
}

pub(crate) fn restyle_nodes(
    tree: &mut BoxTree,
    previous: &StyleTree,
    next: &StyleTree,
    nodes: &[NodeId],
) -> Result<RestyleDamage, RestyleFailure> {
    restyle_impl(tree, previous, next, Some(nodes))
}

fn restyle_impl(
    tree: &mut BoxTree,
    previous: &StyleTree,
    next: &StyleTree,
    nodes: Option<&[NodeId]>,
) -> Result<RestyleDamage, RestyleFailure> {
    let layout_compatible = nodes.map_or_else(
        || previous.layout_compatible_with(next),
        |nodes| previous.layout_compatible_for(next, nodes),
    );
    if !layout_compatible {
        return Err(RestyleFailure::LayoutChanged);
    }
    if tree.paint_sources.boxes.len() != tree.boxes.len()
        || tree.paint_sources.fragments.len() != tree.fragments.len()
        || tree.paint_sources.fills.len() != tree.fills.len()
        || tree.paint_sources.strokes.len() != tree.strokes.len()
    {
        return Err(RestyleFailure::SourceCount);
    }
    let changes = nodes.map_or_else(
        || previous.paint_changes(next),
        |nodes| previous.paint_changes_for(next, nodes),
    );
    let unresolved = tree.unresolved_paint_primitives();
    let unresolved_checked = if changes.is_empty() {
        0
    } else {
        unresolved.boxes.len()
            + unresolved.fragments.len()
            + unresolved.fills.len()
            + unresolved.strokes.len()
    };
    if !changes.is_empty()
        && (unresolved.boxes.iter().any(|index| {
            changes.iter().any(|(old, new)| {
                project_style(tree.boxes[*index].style, *old) == tree.boxes[*index].style
                    && project_style(tree.boxes[*index].style, *new) != tree.boxes[*index].style
            })
        }) || unresolved.fragments.iter().any(|index| {
            changes.iter().any(|(old, new)| {
                project_style(tree.fragments[*index].style, *old) == tree.fragments[*index].style
                    && project_style(tree.fragments[*index].style, *new)
                        != tree.fragments[*index].style
            })
        }) || unresolved.fills.iter().any(|index| {
            changes.iter().any(|(old, new)| {
                old.background == tree.fills[*index].color && old.background != new.background
            })
        }) || unresolved.strokes.iter().any(|index| {
            changes.iter().any(|(old, new)| {
                old.border == tree.strokes[*index].edges
                    && (old.border != new.border || old.cell_style().fg != new.cell_style().fg)
            })
        }))
    {
        return Err(RestyleFailure::UnresolvedSource);
    }
    let mut sources = HashSet::new();
    if let Some(nodes) = nodes {
        for node in nodes {
            sources.insert(PaintStyleSource::Element(*node));
            sources.insert(PaintStyleSource::Pseudo(*node, PseudoElement::Before));
            sources.insert(PaintStyleSource::Pseudo(*node, PseudoElement::After));
            sources.insert(PaintStyleSource::Marker(*node));
        }
    } else {
        sources.extend(tree.paint_sources.boxes.iter().copied());
        sources.extend(tree.paint_sources.fragments.iter().copied());
        sources.extend(tree.paint_sources.fills.iter().copied());
        sources.extend(tree.paint_sources.strokes.iter().copied());
        sources.remove(&PaintStyleSource::Missing);
    }
    let mut damage = RestyleDamage {
        primitives_visited: unresolved_checked,
        ..RestyleDamage::default()
    };
    for source in sources {
        let Some(primitives) = tree.paint_source_primitives(source).cloned() else {
            continue;
        };
        damage.sources_visited += 1;
        let style = source_style(next, source);
        for index in primitives.boxes {
            damage.primitives_visited += 1;
            let painted = &mut tree.boxes[index].style;
            *painted = project_style(*painted, style);
        }
        for index in primitives.fragments {
            damage.primitives_visited += 1;
            let fragment = &mut tree.fragments[index];
            let before = fragment.style;
            fragment.style = project_style(fragment.style, style);
            if fragment.style != before {
                damage.rows.push(fragment.rect().row_range(tree.height));
            }
        }
        for index in primitives.fills {
            damage.primitives_visited += 1;
            let fill = &mut tree.fills[index];
            let before = fill.color;
            fill.color = style.background;
            if fill.color != before {
                damage.rows.push(fill.rect.row_range(tree.height));
            }
        }
        for index in primitives.strokes {
            damage.primitives_visited += 1;
            let stroke = &mut tree.strokes[index];
            let before = (stroke.edges, stroke.style);
            stroke.edges = style.border;
            stroke.style = project_style(stroke.style, style);
            if (stroke.edges, stroke.style) != before {
                damage.rows.push(stroke.rect.row_range(tree.height));
            }
        }
    }
    merge_ranges(&mut damage.rows);
    Ok(damage)
}

fn merge_ranges(ranges: &mut Vec<Range<usize>>) {
    ranges.retain(|range| !range.is_empty());
    ranges.sort_unstable_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges.drain(..) {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    *ranges = merged;
}

fn add_dormant_paint(tree: &mut BoxTree, styles: &StyleTree) {
    for box_ in &tree.boxes {
        if !tree
            .fills
            .iter()
            .any(|fill| fill.rect == box_.border_rect && fill.depth == box_.depth)
        {
            tree.fills.push(BackgroundFill {
                rect: box_.border_rect,
                color: match box_.paint_source {
                    PaintStyleSource::Missing => None,
                    source => source_style(styles, source).background,
                },
                depth: box_.depth,
            });
        }
    }
}

fn fragment_source(
    document: &Document,
    styles: &StyleTree,
    fragment: &TextFragment,
) -> PaintStyleSource {
    if !matches!(document.node(fragment.node), Some(Node::Text { .. })) {
        for which in [PseudoElement::Before, PseudoElement::After] {
            if styles.pseudo(fragment.node, which).is_some_and(|box_| {
                box_.text == fragment.text && box_.style.cell_style() == fragment.style
            }) {
                return PaintStyleSource::Pseudo(fragment.node, which);
            }
        }
        if styles.marker(fragment.node).is_some_and(|marker| {
            marker.text == fragment.text && marker.style.cell_style() == fragment.style
        }) {
            return PaintStyleSource::Marker(fragment.node);
        }
    }
    PaintStyleSource::Element(nearest_styled_node(document, fragment.node))
}

fn nearest_styled_node(document: &Document, mut node: NodeId) -> NodeId {
    while matches!(document.node(node), Some(Node::Text { .. })) {
        let Some(parent) = document.parent(node) else {
            break;
        };
        node = parent;
    }
    node
}

fn box_source_for_fill(tree: &BoxTree, fill: &BackgroundFill) -> PaintStyleSource {
    tree.boxes
        .iter()
        .filter(|box_| box_.depth == fill.depth && contains(box_.border_rect, fill.rect))
        .min_by_key(|box_| {
            box_.border_rect
                .width
                .saturating_mul(box_.border_rect.height)
        })
        .map_or(PaintStyleSource::Missing, |box_| box_.paint_source)
}

fn box_source_for_stroke(tree: &BoxTree, stroke: &BorderStroke) -> PaintStyleSource {
    tree.boxes
        .iter()
        .filter(|box_| box_.depth == stroke.depth && contains(box_.border_rect, stroke.rect))
        .min_by_key(|box_| {
            box_.border_rect
                .width
                .saturating_mul(box_.border_rect.height)
        })
        .map_or(PaintStyleSource::Missing, |box_| box_.paint_source)
}

fn contains(outer: super::LayoutRect, inner: super::LayoutRect) -> bool {
    inner.col >= outer.col
        && inner.row >= outer.row
        && inner.col.saturating_add(inner.width) <= outer.col.saturating_add(outer.width)
        && inner.row.saturating_add(inner.height) <= outer.row.saturating_add(outer.height)
}

fn source_style(styles: &StyleTree, source: PaintStyleSource) -> crate::core::style::ComputedStyle {
    match source {
        PaintStyleSource::Missing => unreachable!(),
        PaintStyleSource::Element(node) => styles.get(node),
        PaintStyleSource::Pseudo(node, which) => styles
            .pseudo(node, which)
            .map_or_else(|| styles.get(node), |box_| box_.style),
        PaintStyleSource::Marker(node) => styles
            .marker(node)
            .map_or_else(|| styles.get(node), |marker| marker.style),
    }
}

fn project_style(mut painted: CellStyle, computed: crate::core::style::ComputedStyle) -> CellStyle {
    let next = computed.cell_style();
    painted.fg = next.fg;
    painted.bg = next.bg;
    painted.bold = next.bold;
    painted.underline = next.underline;
    painted.strike = next.strike;
    painted.reverse = next.reverse;
    painted
}

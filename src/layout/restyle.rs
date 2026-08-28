use crate::core::dom::{Document, Node, NodeId};
use crate::core::style::{CellStyle, PseudoElement, StyleTree};

use super::engine::{
    BackgroundFill, BorderStroke, BoxTree, PaintSources, PaintStyleSource, TextFragment,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestyleFailure {
    LayoutChanged,
    SourceCount,
    AmbiguousBox(NodeId),
    AmbiguousFragment,
    AmbiguousFill,
    AmbiguousStroke,
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
}

pub(crate) fn restyle(
    tree: &mut BoxTree,
    previous: &StyleTree,
    next: &StyleTree,
) -> Result<(), RestyleFailure> {
    if !previous.layout_compatible_with(next) {
        return Err(RestyleFailure::LayoutChanged);
    }
    if tree.paint_sources.boxes.len() != tree.boxes.len()
        || tree.paint_sources.fragments.len() != tree.fragments.len()
        || tree.paint_sources.fills.len() != tree.fills.len()
        || tree.paint_sources.strokes.len() != tree.strokes.len()
    {
        return Err(RestyleFailure::SourceCount);
    }
    let changes = previous.paint_changes(next);
    if let Some(node) =
        tree.boxes
            .iter()
            .zip(&tree.paint_sources.boxes)
            .find_map(|(box_, source)| {
                (*source == PaintStyleSource::Missing
                    && changes.iter().any(|(old, new)| {
                        project_style(box_.style, *old) == box_.style
                            && project_style(box_.style, *new) != box_.style
                    }))
                .then_some(box_.node)
            })
    {
        return Err(RestyleFailure::AmbiguousBox(node));
    }
    if tree
        .fragments
        .iter()
        .zip(&tree.paint_sources.fragments)
        .any(|(fragment, source)| {
            *source == PaintStyleSource::Missing
                && changes.iter().any(|(old, new)| {
                    project_style(fragment.style, *old) == fragment.style
                        && project_style(fragment.style, *new) != fragment.style
                })
        })
    {
        return Err(RestyleFailure::AmbiguousFragment);
    }
    if tree
        .fills
        .iter()
        .zip(&tree.paint_sources.fills)
        .any(|(fill, source)| {
            *source == PaintStyleSource::Missing
                && changes.iter().any(|(old, new)| {
                    old.background == fill.color && old.background != new.background
                })
        })
    {
        return Err(RestyleFailure::AmbiguousFill);
    }
    if tree
        .strokes
        .iter()
        .zip(&tree.paint_sources.strokes)
        .any(|(stroke, source)| {
            *source == PaintStyleSource::Missing
                && changes.iter().any(|(old, new)| {
                    old.border == stroke.edges
                        && (old.border != new.border || old.cell_style().fg != new.cell_style().fg)
                })
        })
    {
        return Err(RestyleFailure::AmbiguousStroke);
    }
    for (box_, source) in tree.boxes.iter_mut().zip(&tree.paint_sources.boxes) {
        if *source != PaintStyleSource::Missing {
            box_.style = project_style(box_.style, source_style(next, *source));
        }
    }
    for (fragment, source) in tree.fragments.iter_mut().zip(&tree.paint_sources.fragments) {
        if *source != PaintStyleSource::Missing {
            fragment.style = project_style(fragment.style, source_style(next, *source));
        }
    }
    for (fill, source) in tree.fills.iter_mut().zip(&tree.paint_sources.fills) {
        if *source != PaintStyleSource::Missing {
            fill.color = source_style(next, *source).background;
        }
    }
    for (stroke, source) in tree.strokes.iter_mut().zip(&tree.paint_sources.strokes) {
        if *source != PaintStyleSource::Missing {
            let style = source_style(next, *source);
            stroke.edges = style.border;
            stroke.style = project_style(stroke.style, style);
        }
    }
    Ok(())
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

mod flow;
mod links;
mod tables;
mod taffy_style;
mod tree;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use taffy::compute_leaf_layout;
use taffy::prelude::{AvailableSpace, Size as TaffySize};
use taffy::tree::Baselines;
use unicode_width::UnicodeWidthStr;

use crate::layout::clip::ClipRegion;

use crate::core::dom::{Document, NodeId};
use crate::core::form::FormState;
use crate::core::geom::Size;
use crate::core::style::{BorderEdges, CellStyle, DisplayInside, FlexDirection, Rgb, StyleTree};
use crate::layout::LayoutInput;
use crate::layout::table::{TableFormatter, TableLimits};
use crate::layout::text_flow::{
    Atom, format_inline, formatted_height, intrinsic_width, line_metrics, min_content_width,
};

use flow::{
    AtomicLayout, FlowBox, FlowTree, InlineAtom, InlineAtomSource, ResolvedInlinePiece,
    append_inline, build_flow_subtree, build_flow_tree,
};
use links::{assign_link_rects, collect_links};
use tables::append_table_output;
use taffy_style::{TaffyStyleInput, layout_rect, taffy_style};
use tree::LayoutTree;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoxTree {
    pub width: usize,
    pub height: usize,
    pub boxes: Vec<LayoutBox>,
    pub fragments: Vec<TextFragment>,
    pub fills: Vec<BackgroundFill>,
    pub strokes: Vec<BorderStroke>,
    pub links: Vec<LinkBox>,
    pub limits: LayoutLimits,
}

/// Ways a page can outgrow what the engine will do for it. `layout` sits below `app`
/// and cannot write the status bar, so it reports here and the caller says so — the
/// same route `parse_errors` and `css_warnings` already take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutLimits {
    /// Nesting passed [`MAX_BLOCK_DEPTH`], so boxes below the cap were not built.
    pub truncated_depth: bool,
    /// Taffy refused a node or a layout. Nothing in the current tree builder should
    /// produce this; it exists so a broken invariant degrades instead of aborting.
    pub engine_failed: bool,
}

impl LayoutLimits {
    pub fn is_clear(self) -> bool {
        !self.truncated_depth && !self.engine_failed
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutRect {
    pub col: usize,
    pub row: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutBox {
    pub node: NodeId,
    pub border_rect: LayoutRect,
    pub content_rect: LayoutRect,
    pub depth: usize,
    pub style: CellStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackgroundFill {
    pub rect: LayoutRect,
    pub color: Rgb,
    pub depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorderStroke {
    pub rect: LayoutRect,
    pub edges: BorderEdges,
    pub style: CellStyle,
    pub depth: usize,
    pub merge_group: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextFragment {
    pub node: NodeId,
    pub col: usize,
    pub row: usize,
    pub text: String,
    pub depth: usize,
    pub style: CellStyle,
}

impl TextFragment {
    pub fn rect(&self) -> LayoutRect {
        LayoutRect {
            col: self.col,
            row: self.row,
            width: UnicodeWidthStr::width(self.text.as_str())
                .saturating_mul(usize::from(self.style.scale)),
            height: usize::from(self.style.scale),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkBox {
    pub node: NodeId,
    pub href: String,
    pub rects: Vec<LayoutRect>,
    pub hit_nodes: Vec<NodeId>,
}

pub trait LayoutEngine: Send + Sync {
    /// Lay the document out as authored, with no user input applied.
    fn layout(&self, document: &Document, styles: &StyleTree, viewport: Size) -> BoxTree {
        self.layout_with_form_state(document, styles, viewport, FormState::empty())
    }

    /// Lay the document out with the user's form edits applied. An empty [`FormState`] is exactly
    /// the authored state, which is why `layout` can defer to this and why `--dump` never needs one.
    fn layout_with_form_state(
        &self,
        document: &Document,
        styles: &StyleTree,
        viewport: Size,
        forms: &FormState,
    ) -> BoxTree;
}

#[derive(Default)]
pub struct TaffyLayoutEngine;

impl LayoutEngine for TaffyLayoutEngine {
    fn layout_with_form_state(
        &self,
        document: &Document,
        styles: &StyleTree,
        viewport: Size,
        forms: &FormState,
    ) -> BoxTree {
        let input = LayoutInput {
            document,
            styles,
            forms,
        };
        let width = usize::from(viewport.cols);
        let flow = build_flow_tree(input, width);
        let mut links = Vec::new();
        for root in document.roots() {
            collect_links(document, styles, *root, &mut links);
        }
        let mut tree = layout_flow(
            input,
            width,
            flow,
            0,
            AvailableSpace::Definite(width as f32),
            0,
        );
        tree.width = width;
        tree.links = links;
        assign_link_rects(document, &mut tree);
        tree
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MeasureWidth {
    MinContent,
    MaxContent,
    Definite(usize),
}

/// Lay a flow tree out, degrading instead of aborting if Taffy refuses.
///
/// Taffy owns block and flex geometry and every `None` below is one of its
/// invariants — a node id we just created, a layout for a node we just computed.
/// They are not reachable from any page we can build today, but a browser must not
/// turn a broken invariant into a dead process, so the failure becomes an empty tree
/// carrying [`LayoutLimits::engine_failed`] and the status bar says so.
fn layout_flow(
    input: LayoutInput<'_>,
    viewport_width: usize,
    flow: FlowTree,
    root_index: usize,
    available_width: AvailableSpace,
    nesting: usize,
) -> BoxTree {
    let truncated_depth = flow.truncated;
    let mut tree = try_layout_flow(
        input,
        viewport_width,
        flow.boxes,
        root_index,
        available_width,
        nesting,
    )
    .unwrap_or_else(|| BoxTree {
        width: viewport_width,
        limits: LayoutLimits {
            engine_failed: true,
            ..LayoutLimits::default()
        },
        ..BoxTree::default()
    });
    tree.limits.truncated_depth |= truncated_depth;
    tree
}

fn try_layout_flow(
    input: LayoutInput<'_>,
    viewport_width: usize,
    mut flow: Vec<FlowBox>,
    root_index: usize,
    available_width: AvailableSpace,
    nesting: usize,
) -> Option<BoxTree> {
    let LayoutInput {
        document,
        styles,
        forms: _,
    } = input;
    let parent_direction = prepare_flow(&mut flow);
    let mut table_cache = HashMap::new();
    let mut inline_cache: HashMap<(usize, MeasureWidth), Vec<ResolvedInlinePiece>> = HashMap::new();
    let mut measure = |inputs, index: usize, style: &taffy::Style| {
        let mut baseline = None;
        let mut output = compute_leaf_layout(
            inputs,
            style,
            |_, _| 0.0,
            |known, available| {
                if let Some(table) = flow[index].table {
                    let width = measured_width(known.width, available.width, viewport_width)
                        .definite(viewport_width)
                        .max(1);
                    let output = table_cache.entry((table, width)).or_insert_with(|| {
                        TableFormatter::new(input).format(
                            table,
                            width,
                            TableLimits::default(),
                            nesting,
                        )
                    });
                    baseline = Some(output.baseline() as f32);
                    return TaffySize {
                        width: output.width as f32,
                        height: output.height as f32,
                    };
                }
                let measure = measured_width(known.width, available.width, viewport_width);
                let key = (index, measure);
                inline_cache.entry(key).or_insert_with(|| {
                    resolve_inline(input, viewport_width, &flow[index].inline, measure, nesting)
                });
                let pieces = inline_cache.get(&key).expect("resolved inline");
                let natural = intrinsic_width(pieces);
                let measured_width = known.width.unwrap_or_else(|| match available.width {
                    AvailableSpace::Definite(value) => value,
                    AvailableSpace::MinContent => min_content_width(pieces) as f32,
                    AvailableSpace::MaxContent => natural as f32,
                });
                let lines = format_inline(pieces, measured_width.max(0.0) as usize);
                baseline = lines.first().map(|line| {
                    let (_, baseline) = line_metrics(line, pieces);
                    let inset = flow[index]
                        .style
                        .padding
                        .top
                        .saturating_add(flow[index].style.border.top.layout_width());
                    baseline.saturating_add(inset) as f32
                });
                TaffySize {
                    width: known.width.unwrap_or(measured_width),
                    height: known
                        .height
                        .unwrap_or(formatted_height(&lines, pieces) as f32),
                }
            },
        );
        output.baselines = Baselines::from_first(baseline);
        output
    };
    let calc_values = std::cell::RefCell::new(Vec::<crate::core::style::CssCalc>::new());
    let mut taffy = LayoutTree::new(&mut measure, |value, basis| {
        calc_values
            .borrow()
            .get((value.addr() >> 3).saturating_sub(1))
            .and_then(|value| styles.resolve_calc(*value, basis))
            .unwrap_or(0.0)
    });
    let mut taffy_nodes = vec![None; flow.len()];
    for index in (0..flow.len()).rev() {
        let style = taffy_style(TaffyStyleInput {
            flow: &flow[index],
            root: index == 0 && root_index == 0,
            viewport_width,
            parent_direction: parent_direction[index],
            calc_values: &calc_values,
            styles,
        });
        let node = if !flow[index].inline.is_empty() || flow[index].table.is_some() {
            taffy.new_leaf(style, index)
        } else {
            let children: Vec<_> = flow[index]
                .children
                .iter()
                .map(|child| taffy_nodes[*child])
                .collect::<Option<Vec<_>>>()?;
            taffy.new_with_children(style, children)
        };
        taffy_nodes[index] = Some(node);
    }
    let root = taffy_nodes[root_index]?;
    taffy.compute_layout(
        root,
        TaffySize {
            width: available_width,
            height: AvailableSpace::MaxContent,
        },
    );

    let layouts: Vec<_> = taffy_nodes
        .iter()
        .map(|node| node.map(|node| taffy.layout(node)))
        .collect::<Option<Vec<_>>>()?;
    drop(taffy);
    let root_layout = layouts[root_index];
    let mut tree = BoxTree {
        width: root_layout.size.width.max(0.0).round() as usize,
        height: 0,
        ..Default::default()
    };
    let mut layout_height = 0;
    let mut stack = vec![(
        root_index,
        0.0f32,
        0.0f32,
        ClipRegion::viewport(viewport_width),
        false,
    )];
    while let Some((index, parent_col, parent_row, inherited_clip, fixed_subtree)) = stack.pop() {
        let layout = layouts[index];
        let absolute_col = parent_col + layout.location.x;
        let absolute_row = parent_row + layout.location.y;
        let border_rect = layout_rect(absolute_col, absolute_row, layout.size);
        let padding_rect = layout_rect(
            absolute_col + layout.border.left,
            absolute_row + layout.border.top,
            TaffySize {
                width: (layout.size.width - layout.border.left - layout.border.right).max(0.0),
                height: (layout.size.height - layout.border.top - layout.border.bottom).max(0.0),
            },
        );
        let propagates = flow[index]
            .owner
            .is_some_and(|owner| propagates_overflow(document, owner));
        let child_clip = if propagates {
            inherited_clip
        } else {
            inherited_clip.intersect_axes(
                padding_rect,
                flow[index].style.overflow.x.clips(),
                flow[index].style.overflow.y.clips(),
            )
        };
        if !fixed_subtree && let Some(bottom) = inherited_clip.vertical_end(border_rect) {
            layout_height = layout_height.max(bottom);
        }
        if let Some(table) = flow[index].table {
            let width = layout.size.width.max(1.0) as usize;
            let output = table_cache
                .entry((table, width))
                .or_insert_with(|| {
                    TableFormatter::new(input).format(table, width, TableLimits::default(), 0)
                })
                .clone();
            let starts = OutputStarts::new(&tree);
            append_table_output(
                &mut tree,
                output,
                absolute_col.round() as isize,
                absolute_row.round() as isize,
                flow[index].depth,
                index.saturating_add(1),
            );
            clip_since(&mut tree, starts, child_clip);
        } else if let Some(owner) = flow[index].owner {
            let left = layout.border.left + layout.padding.left;
            let right = layout.border.right + layout.padding.right;
            let top = layout.border.top + layout.padding.top;
            let bottom = layout.border.bottom + layout.padding.bottom;
            let content_rect = layout_rect(
                absolute_col + left,
                absolute_row + top,
                TaffySize {
                    width: (layout.size.width - left - right).max(0.0),
                    height: (layout.size.height - top - bottom).max(0.0),
                },
            );
            let style = flow[index].style.cell_style();
            let visible = !flow[index].style.visibility.is_hidden();
            let painted_border = inherited_clip.box_rect(border_rect);
            if visible
                && let Some(rect) = painted_border
                && let Some(color) = style.bg
            {
                tree.fills.push(BackgroundFill {
                    rect,
                    color,
                    depth: flow[index].depth,
                });
            }
            if visible
                && flow[index].style.border.is_visible()
                && let Some(stroke) = inherited_clip.stroke(BorderStroke {
                    rect: border_rect,
                    edges: flow[index].style.border,
                    style,
                    depth: flow[index].depth,
                    merge_group: index.saturating_add(1),
                })
            {
                tree.strokes.push(stroke);
            }
            if visible && let Some(border_rect) = painted_border {
                tree.boxes.push(LayoutBox {
                    node: owner,
                    border_rect,
                    content_rect: inherited_clip.rect(content_rect).unwrap_or_default(),
                    depth: flow[index].depth,
                    style,
                });
            }
            if visible
                && flow[index].rule
                && content_rect.width > 0
                && let Some(fragment) = inherited_clip.fragment(&TextFragment {
                    node: owner,
                    col: content_rect.col,
                    row: content_rect.row,
                    text: "─".repeat(content_rect.width),
                    depth: flow[index].depth,
                    style: flow[index].style.cell_style(),
                })
            {
                tree.fragments.push(fragment);
            }
            // A replaced box paints from its *content* rect, so a border or padding of its own is
            // respected. It cannot ride `inline`, which is emitted at the border-box origin — that
            // is what painted a search field on top of its own border and put a button's label
            // outside its own `overflow: hidden` padding box, where it was clipped away.
            if visible
                && let Some(replaced) = &flow[index].replaced
                && content_rect.width > 0
                && content_rect.height > 0
            {
                let mut style = flow[index].style.cell_style();
                style.dim |= replaced.placeholder;
                let rows = replaced.rows(
                    content_rect.width,
                    content_rect.height,
                    flow[index].style.text_align,
                );
                for (offset, text) in rows.into_iter().enumerate() {
                    if let Some(fragment) = child_clip.fragment(&TextFragment {
                        node: owner,
                        col: content_rect.col,
                        row: content_rect.row.saturating_add(offset),
                        text,
                        depth: flow[index].depth,
                        style,
                    }) {
                        tree.fragments.push(fragment);
                    }
                }
            }
            if let Some(marker) = &flow[index].marker
                && !marker.style.visibility.is_hidden()
            {
                let width = UnicodeWidthStr::width(marker.text.as_str());
                if let Some(fragment) = inherited_clip.fragment(&TextFragment {
                    node: owner,
                    col: content_rect.col.saturating_sub(width),
                    row: content_rect.row,
                    text: marker.text.clone(),
                    depth: flow[index].depth,
                    style: marker.style.cell_style(),
                }) {
                    tree.fragments.push(fragment);
                }
            }
        }
        if !flow[index].inline.is_empty() && !child_clip.is_empty() {
            let rect = layout_rect(absolute_col, absolute_row, layout.size);
            let layout_width = layout.size.width.max(0.0).round() as usize;
            let measure = MeasureWidth::Definite(layout_width);
            let key = (index, measure);
            inline_cache.entry(key).or_insert_with(|| {
                resolve_inline(input, viewport_width, &flow[index].inline, measure, nesting)
            });
            let starts = OutputStarts::new(&tree);
            append_inline(
                &mut tree,
                inline_cache.get(&key).expect("resolved inline"),
                absolute_col.round() as isize,
                absolute_row.round() as isize,
                layout_width,
                flow[index].style.text_align,
                index.saturating_add(1),
            );
            clip_since(&mut tree, starts, child_clip);
            if !fixed_subtree && let Some(rect) = child_clip.rect(rect) {
                layout_height = layout_height.max(rect.row.saturating_add(rect.height));
            }
        }
        if !child_clip.is_empty() || index == root_index {
            for child in flow[index].children.iter().rev() {
                stack.push((
                    *child,
                    absolute_col,
                    absolute_row,
                    child_clip,
                    fixed_subtree
                        || matches!(
                            flow[*child].style.position,
                            crate::core::style::Position::Fixed
                        ),
                ));
            }
        }
    }
    tree.height = layout_height;
    tree.boxes.sort_by_key(|layout_box| layout_box.depth);
    tree.fragments
        .sort_by_key(|fragment| (fragment.row, fragment.col, fragment.depth));
    Some(tree)
}

#[derive(Clone, Copy)]
struct OutputStarts {
    boxes: usize,
    fills: usize,
    strokes: usize,
    fragments: usize,
}

impl OutputStarts {
    fn new(tree: &BoxTree) -> Self {
        Self {
            boxes: tree.boxes.len(),
            fills: tree.fills.len(),
            strokes: tree.strokes.len(),
            fragments: tree.fragments.len(),
        }
    }
}

fn clip_since(tree: &mut BoxTree, starts: OutputStarts, clip: ClipRegion) {
    let boxes: Vec<_> = tree
        .boxes
        .drain(starts.boxes..)
        .filter_map(|mut layout_box| {
            layout_box.border_rect = clip.box_rect(layout_box.border_rect)?;
            layout_box.content_rect = clip.box_rect(layout_box.content_rect).unwrap_or_default();
            Some(layout_box)
        })
        .collect();
    tree.boxes.extend(boxes);
    let fills: Vec<_> = tree
        .fills
        .drain(starts.fills..)
        .filter_map(|mut fill| {
            fill.rect = clip.rect(fill.rect)?;
            Some(fill)
        })
        .collect();
    tree.fills.extend(fills);
    let strokes: Vec<_> = tree
        .strokes
        .drain(starts.strokes..)
        .filter_map(|stroke| clip.stroke(stroke))
        .collect();
    tree.strokes.extend(strokes);
    let fragments: Vec<_> = tree
        .fragments
        .drain(starts.fragments..)
        .filter_map(|fragment| clip.fragment(&fragment))
        .collect();
    tree.fragments.extend(fragments);
}

fn propagates_overflow(document: &Document, node: NodeId) -> bool {
    matches!(
        document.node(node),
        Some(crate::core::dom::Node::Element { name, .. }) if matches!(name.as_str(), "html" | "body")
    )
}

/// Apply `order` to each flex or grid container's in-flow children, and record the flex direction
/// each item sits in so `flex-basis` can pick an axis. Grid items are reordered too — auto
/// placement and paint order both follow order-modified document order — but a grid has no main
/// axis, so their direction stays `None`.
fn prepare_flow(flow: &mut [FlowBox]) -> Vec<Option<FlexDirection>> {
    let mut parent_direction = vec![None; flow.len()];
    for index in 0..flow.len() {
        let inside = flow[index].style.display.inside();
        if matches!(inside, Some(DisplayInside::Flex | DisplayInside::Grid)) {
            let direction = matches!(inside, Some(DisplayInside::Flex))
                .then(|| flow[index].style.flex.direction);
            let mut children = flow[index].children.clone();
            let positions: Vec<_> = children
                .iter()
                .enumerate()
                .filter_map(|(position, child)| {
                    (!flow[*child].style.position.is_absolute()).then_some(position)
                })
                .collect();
            let mut in_flow: Vec<_> = positions
                .iter()
                .map(|position| children[*position])
                .collect();
            in_flow.sort_by_key(|child| flow[*child].style.order);
            for (position, child) in positions.into_iter().zip(in_flow) {
                children[position] = child;
                parent_direction[child] = direction;
            }
            flow[index].children = children;
        }
    }
    parent_direction
}

impl MeasureWidth {
    fn definite(self, viewport_width: usize) -> usize {
        match self {
            Self::MinContent => 1,
            Self::MaxContent => viewport_width,
            Self::Definite(width) => width,
        }
    }
}

fn measured_width(
    known: Option<f32>,
    available: AvailableSpace,
    viewport_width: usize,
) -> MeasureWidth {
    if let Some(width) = known {
        return MeasureWidth::Definite(width.max(0.0).round() as usize);
    }
    match available {
        AvailableSpace::MinContent => MeasureWidth::MinContent,
        AvailableSpace::MaxContent => MeasureWidth::MaxContent,
        AvailableSpace::Definite(width) => {
            MeasureWidth::Definite(width.max(0.0).round() as usize).max_viewport(viewport_width)
        }
    }
}

impl MeasureWidth {
    fn max_viewport(self, viewport_width: usize) -> Self {
        match self {
            Self::Definite(width) => Self::Definite(width.min(viewport_width)),
            other => other,
        }
    }
}

fn resolve_inline(
    input: LayoutInput<'_>,
    viewport_width: usize,
    pieces: &[flow::InlinePiece],
    measure: MeasureWidth,
    nesting: usize,
) -> Vec<ResolvedInlinePiece> {
    let styles = input.styles;
    let available = measure.definite(viewport_width).max(1);
    pieces
        .iter()
        .map(|piece| {
            let atom = piece.atom.as_ref().map(|source| match source {
                InlineAtomSource::Ready(output) => InlineAtom::Table(output.clone()),
                InlineAtomSource::Offset(offset) => InlineAtom::Offset(*offset),
                InlineAtomSource::Node(node)
                    if matches!(
                        styles.get(*node).display.inside(),
                        Some(DisplayInside::Flex | DisplayInside::Grid)
                    ) && nesting < TableLimits::default().max_nesting =>
                {
                    let flow = build_flow_subtree(input, viewport_width, *node);
                    let root = flow.boxes[0].children.first().copied().unwrap_or(0);
                    let width = match measure {
                        MeasureWidth::MinContent => AvailableSpace::MinContent,
                        MeasureWidth::MaxContent => AvailableSpace::MaxContent,
                        MeasureWidth::Definite(width) => AvailableSpace::Definite(width as f32),
                    };
                    let tree = layout_flow(
                        input,
                        viewport_width,
                        flow,
                        root,
                        width,
                        nesting.saturating_add(1),
                    );
                    let baseline = tree
                        .fragments
                        .iter()
                        .map(|fragment| {
                            fragment
                                .row
                                .saturating_add(usize::from(fragment.style.scale).saturating_sub(1))
                        })
                        .min()
                        .unwrap_or_else(|| tree.height.saturating_sub(1));
                    InlineAtom::Layout(Box::new(AtomicLayout { tree, baseline }))
                }
                InlineAtomSource::Node(node) => {
                    InlineAtom::Table(Box::new(TableFormatter::new(input).format_inline_atom(
                        *node,
                        available,
                        TableLimits::default(),
                        nesting,
                    )))
                }
            });
            ResolvedInlinePiece {
                node: piece.node,
                text: piece.text.clone(),
                white_space: piece.white_space,
                depth: piece.depth,
                style: piece.style,
                hidden: piece.hidden,
                atom,
            }
        })
        .collect()
}

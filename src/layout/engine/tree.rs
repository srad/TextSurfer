use taffy::prelude::{AvailableSpace, Display, Layout, NodeId, Size, Style};
use taffy::tree::{LayoutInput, LayoutOutput};
use taffy::{
    BlockContext, Cache, CacheTree, LayoutBlockContainer, LayoutFlexboxContainer,
    LayoutGridContainer, LayoutPartialTree, TraversePartialTree, compute_block_layout,
    compute_cached_layout, compute_flexbox_layout, compute_grid_layout, compute_root_layout,
};

struct Node {
    style: Style,
    context: Option<usize>,
    children: Vec<NodeId>,
    cache: Cache,
    layout: Layout,
}

pub(super) struct LayoutTree<'a, M, R> {
    nodes: Vec<Node>,
    measure: &'a mut M,
    resolve: R,
}

impl<'a, M, R> LayoutTree<'a, M, R>
where
    M: FnMut(LayoutInput, usize, &Style, Option<&mut BlockContext<'_>>) -> LayoutOutput,
    R: Fn(*const (), f32) -> f32,
{
    pub(super) fn new(measure: &'a mut M, resolve: R) -> Self {
        Self {
            nodes: Vec::new(),
            measure,
            resolve,
        }
    }

    pub(super) fn new_leaf(&mut self, style: Style, context: usize) -> NodeId {
        self.push(style, Some(context), Vec::new())
    }

    pub(super) fn new_with_children(&mut self, style: Style, children: Vec<NodeId>) -> NodeId {
        self.push(style, None, children)
    }

    fn push(&mut self, style: Style, context: Option<usize>, children: Vec<NodeId>) -> NodeId {
        let id = NodeId::from(self.nodes.len());
        self.nodes.push(Node {
            style,
            context,
            children,
            cache: Cache::new(),
            layout: Layout::with_order(0),
        });
        id
    }

    pub(super) fn compute_layout(&mut self, root: NodeId, available: Size<AvailableSpace>) {
        compute_root_layout(self, root, available);
    }

    pub(super) fn layout(&self, node: NodeId) -> Layout {
        self.nodes[usize::from(node)].layout
    }

    fn compute(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        let index = usize::from(node);
        if let Some(context) = self.nodes[index].context
            && block.is_some()
        {
            let style = self.nodes[index].style.clone();
            return (self.measure)(inputs, context, &style, block);
        }
        compute_cached_layout(self, node, inputs, |tree, node, inputs| {
            let index = usize::from(node);
            let context = tree.nodes[index].context;
            if let Some(context) = context {
                let style = tree.nodes[index].style.clone();
                return (tree.measure)(inputs, context, &style, None);
            }
            match tree.nodes[index].style.display {
                Display::Flex => compute_flexbox_layout(tree, node, inputs),
                Display::Grid => compute_grid_layout(tree, node, inputs),
                _ => compute_block_layout(tree, node, inputs, block),
            }
        })
    }
}

pub(super) struct Children<'a>(std::slice::Iter<'a, NodeId>);

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().copied()
    }
}

impl<M, R> TraversePartialTree for LayoutTree<'_, M, R> {
    type ChildIter<'a>
        = Children<'a>
    where
        Self: 'a;

    fn child_ids(&self, node: NodeId) -> Self::ChildIter<'_> {
        Children(self.nodes[usize::from(node)].children.iter())
    }

    fn child_count(&self, node: NodeId) -> usize {
        self.nodes[usize::from(node)].children.len()
    }

    fn get_child_id(&self, node: NodeId, index: usize) -> NodeId {
        self.nodes[usize::from(node)].children[index]
    }
}

impl<M, R> LayoutPartialTree for LayoutTree<'_, M, R>
where
    M: FnMut(LayoutInput, usize, &Style, Option<&mut BlockContext<'_>>) -> LayoutOutput,
    R: Fn(*const (), f32) -> f32,
{
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type CustomIdent = String;

    fn get_core_container_style(&self, node: NodeId) -> Self::CoreContainerStyle<'_> {
        &self.nodes[usize::from(node)].style
    }

    fn resolve_calc_value(&self, value: *const (), basis: f32) -> f32 {
        (self.resolve)(value, basis)
    }

    fn set_unrounded_layout(&mut self, node: NodeId, layout: &Layout) {
        self.nodes[usize::from(node)].layout = *layout;
    }

    fn compute_child_layout(&mut self, node: NodeId, inputs: LayoutInput) -> LayoutOutput {
        self.compute(node, inputs, None)
    }
}

impl<M, R> CacheTree for LayoutTree<'_, M, R> {
    fn cache_get(&mut self, node: NodeId, input: &LayoutInput) -> Option<LayoutOutput> {
        self.nodes[usize::from(node)].cache.get(input)
    }

    fn cache_store(&mut self, node: NodeId, input: &LayoutInput, output: LayoutOutput) {
        self.nodes[usize::from(node)].cache.store(input, output);
    }

    fn cache_clear(&mut self, node: NodeId) {
        self.nodes[usize::from(node)].cache.clear();
    }
}

impl<M, R> LayoutFlexboxContainer for LayoutTree<'_, M, R>
where
    M: FnMut(LayoutInput, usize, &Style, Option<&mut BlockContext<'_>>) -> LayoutOutput,
    R: Fn(*const (), f32) -> f32,
{
    type FlexboxContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type FlexboxItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_flexbox_container_style(&self, node: NodeId) -> Self::FlexboxContainerStyle<'_> {
        &self.nodes[usize::from(node)].style
    }

    fn get_flexbox_child_style(&self, node: NodeId) -> Self::FlexboxItemStyle<'_> {
        &self.nodes[usize::from(node)].style
    }
}

impl<M, R> LayoutGridContainer for LayoutTree<'_, M, R>
where
    M: FnMut(LayoutInput, usize, &Style, Option<&mut BlockContext<'_>>) -> LayoutOutput,
    R: Fn(*const (), f32) -> f32,
{
    type GridContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type GridItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_grid_container_style(&self, node: NodeId) -> Self::GridContainerStyle<'_> {
        &self.nodes[usize::from(node)].style
    }

    fn get_grid_child_style(&self, node: NodeId) -> Self::GridItemStyle<'_> {
        &self.nodes[usize::from(node)].style
    }
}

impl<M, R> LayoutBlockContainer for LayoutTree<'_, M, R>
where
    M: FnMut(LayoutInput, usize, &Style, Option<&mut BlockContext<'_>>) -> LayoutOutput,
    R: Fn(*const (), f32) -> f32,
{
    type BlockContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_block_container_style(&self, node: NodeId) -> Self::BlockContainerStyle<'_> {
        &self.nodes[usize::from(node)].style
    }

    fn get_block_child_style(&self, node: NodeId) -> Self::BlockItemStyle<'_> {
        &self.nodes[usize::from(node)].style
    }

    fn compute_block_child_layout(
        &mut self,
        node: NodeId,
        inputs: LayoutInput,
        block: Option<&mut BlockContext<'_>>,
    ) -> LayoutOutput {
        self.compute(node, inputs, block)
    }
}

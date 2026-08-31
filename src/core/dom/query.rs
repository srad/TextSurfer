use super::{AttrNs, Document, DomQuirksMode, ElementNs, Node, NodeId};

impl Document {
    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.arena.get(id.0).map(indextree::Node::get)
    }

    pub fn root(&self) -> Option<NodeId> {
        self.roots.first().copied()
    }

    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    pub fn is_template_contents(&self, id: NodeId) -> bool {
        self.template_contents
            .values()
            .any(|contents| *contents == id)
    }

    pub fn element_by_id(&self, id_attr: &str) -> Option<NodeId> {
        self.roots.iter().find_map(|root| {
            root.0.descendants(&self.arena).find_map(|id| {
                let node = self.arena.get(id)?.get();
                match node {
                    Node::Element { attrs, .. }
                        if attrs.iter().any(|attr| {
                            attr.ns == AttrNs::None && attr.name == "id" && attr.value == id_attr
                        }) =>
                    {
                        Some(NodeId(id))
                    }
                    _ => None,
                }
            })
        })
    }

    pub fn element_by_name(&self, name: &str) -> Option<NodeId> {
        self.roots.iter().find_map(|root| {
            root.0
                .descendants(&self.arena)
                .find_map(|id| match self.arena.get(id)?.get() {
                    Node::Element {
                        name: element_name,
                        ns: ElementNs::Html,
                        ..
                    } if element_name.eq_ignore_ascii_case(name) => Some(NodeId(id)),
                    _ => None,
                })
        })
    }

    pub fn element_by_class(&self, class: &str) -> Option<NodeId> {
        self.roots.iter().find_map(|root| {
            root.0
                .descendants(&self.arena)
                .find_map(|id| match self.arena.get(id)?.get() {
                    Node::Element { attrs, .. }
                        if attrs.iter().any(|attribute| {
                            attribute.ns == AttrNs::None
                                && attribute.name.eq_ignore_ascii_case("class")
                                && attribute
                                    .value
                                    .split_ascii_whitespace()
                                    .any(|token| token == class)
                        }) =>
                    {
                        Some(NodeId(id))
                    }
                    _ => None,
                })
        })
    }

    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        if self.node(id).is_none() {
            return Vec::new();
        }
        id.0.descendants(&self.arena).map(NodeId).collect()
    }

    pub fn element_name(&self, id: NodeId) -> Option<&str> {
        match self.node(id)? {
            Node::Element { name, .. } => Some(name),
            _ => None,
        }
    }

    pub fn attribute(&self, id: NodeId, name: &str) -> Option<&str> {
        match self.node(id)? {
            Node::Element { attrs, .. } => attrs
                .iter()
                .find(|attribute| {
                    attribute.ns == AttrNs::None && attribute.name.eq_ignore_ascii_case(name)
                })
                .map(|attribute| attribute.value.as_str()),
            _ => None,
        }
    }

    pub fn text_content(&self, id: NodeId) -> Option<String> {
        self.node(id)?;
        let mut result = String::new();
        let mut pending = vec![id];
        while let Some(node) = pending.pop() {
            match self.node(node) {
                Some(Node::Text { data }) => result.push_str(data),
                Some(_) => {
                    let children = self.children(node);
                    pending.extend(children.into_iter().rev());
                }
                None => return None,
            }
        }
        Some(result)
    }

    pub fn parent_chain(&self, id: NodeId) -> Vec<NodeId> {
        let mut chain = Vec::new();
        let mut current = Some(id);
        while let Some(node) = current {
            if self.node(node).is_none() {
                return Vec::new();
            }
            chain.push(node);
            current = self.parent(node);
        }
        chain
    }

    pub fn node_count(&self) -> usize {
        self.arena.len()
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.arena.get(id.0)?.parent().map(NodeId)
    }

    pub fn first_child(&self, id: NodeId) -> Option<NodeId> {
        self.arena.get(id.0)?.first_child().map(NodeId)
    }

    pub fn last_child(&self, id: NodeId) -> Option<NodeId> {
        self.arena.get(id.0)?.last_child().map(NodeId)
    }

    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        if self.parent(id).is_none() {
            let position = self.roots.iter().position(|root| *root == id)?;
            self.roots.get(position + 1).copied()
        } else {
            self.arena.get(id.0)?.next_sibling().map(NodeId)
        }
    }

    pub fn prev_sibling(&self, id: NodeId) -> Option<NodeId> {
        if self.parent(id).is_none() {
            let position = self.roots.iter().position(|root| *root == id)?;
            position
                .checked_sub(1)
                .and_then(|index| self.roots.get(index).copied())
        } else {
            self.arena.get(id.0)?.previous_sibling().map(NodeId)
        }
    }

    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        if self.arena.get(id.0).is_none() {
            return Vec::new();
        }
        id.0.children(&self.arena).map(NodeId).collect()
    }

    pub fn quirks_mode(&self) -> DomQuirksMode {
        self.quirks_mode
    }

    pub fn set_quirks_mode(&mut self, mode: DomQuirksMode) {
        self.quirks_mode = mode;
    }

    pub fn template_contents(&self, template: NodeId) -> Option<NodeId> {
        self.template_contents.get(&template).copied()
    }
}

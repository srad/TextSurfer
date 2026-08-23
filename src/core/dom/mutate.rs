use super::{Attr, Document, DomError, ElementNs, Node, NodeId};

impl Document {
    pub fn create_template_contents(&mut self, template: NodeId) -> Result<NodeId, DomError> {
        self.ensure_node(template)?;
        let fragment = self.create_detached(Node::DocumentFragment);
        self.template_contents.insert(template, fragment);
        Ok(fragment)
    }

    pub fn insert_element(
        &mut self,
        parent: Option<NodeId>,
        name: &str,
        ns: ElementNs,
        attrs: Vec<Attr>,
    ) -> NodeId {
        self.append(
            parent,
            Node::Element {
                name: name.to_string(),
                ns,
                attrs,
            },
        )
    }

    pub fn insert_text(&mut self, parent: Option<NodeId>, data: &str) -> NodeId {
        self.append(
            parent,
            Node::Text {
                data: data.to_string(),
            },
        )
    }

    pub fn insert_comment(&mut self, parent: Option<NodeId>, data: &str) -> NodeId {
        self.append(
            parent,
            Node::Comment {
                data: data.to_string(),
            },
        )
    }

    pub fn insert_pi(&mut self, parent: Option<NodeId>, target: &str, data: &str) -> NodeId {
        self.append(
            parent,
            Node::Pi {
                target: target.to_string(),
                data: data.to_string(),
            },
        )
    }

    pub fn insert_doctype(
        &mut self,
        parent: Option<NodeId>,
        name: &str,
        public_id: &str,
        system_id: &str,
    ) -> NodeId {
        self.append(
            parent,
            Node::Doctype {
                name: name.to_string(),
                public_id: public_id.to_string(),
                system_id: system_id.to_string(),
            },
        )
    }

    pub fn append(&mut self, parent: Option<NodeId>, node: Node) -> NodeId {
        let id = self.create_detached(node);
        match parent {
            None => self.roots.push(id),
            Some(parent) => parent
                .0
                .checked_append(id.0, &mut self.arena)
                .expect("parent node must exist and a new node cannot create a cycle"),
        }
        id
    }

    pub fn insert_before(&mut self, sibling: NodeId, node: Node) -> NodeId {
        self.ensure_node(sibling).expect("sibling node must exist");
        let parent = self.parent(sibling);
        let id = self.create_detached(node);
        if parent.is_some() {
            sibling
                .0
                .checked_insert_before(id.0, &mut self.arena)
                .expect("new sibling cannot create a cycle");
        } else {
            let position = self
                .roots
                .iter()
                .position(|root| *root == sibling)
                .expect("parentless sibling must be a document root");
            self.roots.insert(position, id);
        }
        id
    }

    pub fn append_based_on_parent_node(
        &mut self,
        sibling: Option<NodeId>,
        new_parent: Option<NodeId>,
        node: Node,
    ) -> NodeId {
        match sibling {
            Some(s) if self.parent(s).is_some() => self.insert_before(s, node),
            _ => self.append(new_parent, node),
        }
    }

    pub fn detach(&mut self, id: NodeId) -> Result<(), DomError> {
        self.ensure_node(id)?;
        id.0.detach(&mut self.arena);
        self.roots.retain(|root| *root != id);
        Ok(())
    }

    pub fn attach(&mut self, id: NodeId, parent: Option<NodeId>) -> Result<(), DomError> {
        self.validate_move(id, parent)?;
        id.0.detach(&mut self.arena);
        self.roots.retain(|root| *root != id);
        match parent {
            None => self.roots.push(id),
            Some(parent) => parent
                .0
                .checked_append(id.0, &mut self.arena)
                .map_err(|_| DomError::Cycle)?,
        }
        Ok(())
    }

    pub fn attach_before(&mut self, id: NodeId, sibling: NodeId) -> Result<(), DomError> {
        if id == sibling {
            return Err(DomError::SameNode);
        }
        self.ensure_node(sibling)?;
        let parent = self.parent(sibling);
        self.validate_move(id, parent)?;
        id.0.detach(&mut self.arena);
        self.roots.retain(|root| *root != id);
        match parent {
            Some(_) => sibling
                .0
                .checked_insert_before(id.0, &mut self.arena)
                .map_err(|_| DomError::Cycle)?,
            None => {
                let position = self
                    .roots
                    .iter()
                    .position(|root| *root == sibling)
                    .expect("sibling must be a root");
                self.roots.insert(position, id);
            }
        }
        Ok(())
    }

    pub fn append_merged_text(&mut self, parent: Option<NodeId>, data: &str) -> NodeId {
        let last = parent.and_then(|p| self.last_child(p));
        if let Some(last) = last
            && let Some(Node::Text { data: existing }) =
                self.arena.get_mut(last.0).map(indextree::Node::get_mut)
        {
            existing.push_str(data);
            return last;
        }
        self.insert_text(parent, data)
    }

    pub fn insert_merged_text_before(&mut self, sibling: NodeId, data: &str) -> NodeId {
        if let Some(prev) = self.prev_sibling(sibling)
            && let Some(Node::Text { data: existing }) =
                self.arena.get_mut(prev.0).map(indextree::Node::get_mut)
        {
            existing.push_str(data);
            return prev;
        }
        self.insert_before(
            sibling,
            Node::Text {
                data: data.to_string(),
            },
        )
    }

    pub fn reparent_children(
        &mut self,
        node: NodeId,
        new_parent: Option<NodeId>,
    ) -> Result<(), DomError> {
        self.ensure_node(node)?;
        if new_parent == Some(node) {
            return Ok(());
        }
        if let Some(parent) = new_parent {
            self.ensure_node(parent)?;
            if parent
                .0
                .ancestors(&self.arena)
                .any(|ancestor| ancestor == node.0)
            {
                return Err(DomError::Cycle);
            }
        }
        let kids = self.children(node);
        for child in kids {
            self.attach(child, new_parent)?;
        }
        Ok(())
    }

    pub fn add_attrs_if_missing(&mut self, id: NodeId, attrs: Vec<Attr>) {
        if let Some(Node::Element {
            attrs: existing, ..
        }) = self.arena.get_mut(id.0).map(indextree::Node::get_mut)
        {
            for attr in attrs {
                if !existing
                    .iter()
                    .any(|a| a.ns == attr.ns && a.name == attr.name)
                {
                    existing.push(attr);
                }
            }
        }
    }

    pub fn set_root(&mut self, id: NodeId) -> Result<(), DomError> {
        self.ensure_node(id)?;
        if !self.roots.contains(&id) {
            self.attach(id, None)?;
        }
        Ok(())
    }

    pub fn remove_node(&mut self, id: NodeId) -> Result<bool, DomError> {
        self.ensure_node(id)?;
        let connected = self.parent(id).is_some() || self.roots.contains(&id);
        if connected {
            self.detach(id)?;
        }
        Ok(connected)
    }

    fn create_detached(&mut self, node: Node) -> NodeId {
        NodeId(self.arena.new_node(node))
    }
}

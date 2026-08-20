use indextree::Arena;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(indextree::NodeId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ElementNs {
    Html,
    Svg,
    MathMl,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AttrNs {
    None,
    Xlink,
    Xml,
    Xmlns,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    pub name: String,
    pub ns: AttrNs,
    pub value: String,
}

impl Attr {
    pub fn plain(name: &str, value: &str) -> Self {
        Self {
            name: name.to_string(),
            ns: AttrNs::None,
            value: value.to_string(),
        }
    }

    pub fn namespaced(ns: AttrNs, name: &str, value: &str) -> Self {
        Self {
            name: name.to_string(),
            ns,
            value: value.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Element {
        name: String,
        ns: ElementNs,
        attrs: Vec<Attr>,
    },
    Text {
        data: String,
    },
    Comment {
        data: String,
    },
    Pi {
        target: String,
        data: String,
    },
    Doctype {
        name: String,
        public_id: String,
        system_id: String,
    },
    DocumentFragment,
}

impl Node {}

pub type SharedDocument = Rc<RefCell<Document>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DomQuirksMode {
    Quirks,
    LimitedQuirks,
    #[default]
    NoQuirks,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum DomError {
    #[error("node does not exist")]
    InvalidNode,
    #[error("mutation would create a cycle")]
    Cycle,
    #[error("a node cannot be inserted relative to itself")]
    SameNode,
}

#[derive(Debug, Default)]
pub struct Document {
    arena: Arena<Node>,
    roots: Vec<NodeId>,
    template_contents: HashMap<NodeId, NodeId>,
    quirks_mode: DomQuirksMode,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

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

    fn ensure_node(&self, id: NodeId) -> Result<(), DomError> {
        if self.arena.get(id.0).is_some() {
            Ok(())
        } else {
            Err(DomError::InvalidNode)
        }
    }

    fn validate_move(&self, id: NodeId, parent: Option<NodeId>) -> Result<(), DomError> {
        self.ensure_node(id)?;
        if let Some(parent) = parent {
            self.ensure_node(parent)?;
            if parent
                .0
                .ancestors(&self.arena)
                .any(|ancestor| ancestor == id.0)
            {
                return Err(DomError::Cycle);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(name: &str) -> Node {
        Node::Element {
            name: name.to_string(),
            ns: ElementNs::Html,
            attrs: vec![],
        }
    }

    #[test]
    fn empty_document_has_no_root() {
        let document = Document::new();
        assert!(document.is_empty());
        assert_eq!(document.root(), None);
    }

    #[test]
    fn children_follow_sibling_order() {
        let mut document = Document::new();
        let parent = document.insert_element(None, "html", ElementNs::Html, vec![]);
        document.set_root(parent).unwrap();
        let head = document.insert_element(Some(parent), "head", ElementNs::Html, vec![]);
        let body = document.insert_element(Some(parent), "body", ElementNs::Html, vec![]);
        assert_eq!(document.children(parent), vec![head, body]);
        assert_eq!(document.next_sibling(head), Some(body));
        assert_eq!(document.prev_sibling(body), Some(head));
        assert_eq!(document.parent(head), Some(parent));
    }

    #[test]
    fn namespaced_element_is_still_visible_to_tests() {
        let mut document = Document::new();
        let element =
            document.insert_element(None, "g", ElementNs::Svg, vec![Attr::plain("id", "shapes")]);
        assert_eq!(document.element_by_id("shapes"), Some(element));
        assert_eq!(
            document.node(element),
            Some(&Node::Element {
                name: "g".to_string(),
                ns: ElementNs::Svg,
                attrs: vec![Attr::plain("id", "shapes")],
            })
        );
    }

    #[test]
    fn text_nodes_live_below_elements() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let text = document.insert_text(Some(p), "hello");
        assert_eq!(
            document.node(text),
            Some(&Node::Text {
                data: "hello".to_string()
            })
        );
        assert_eq!(document.children(p), vec![text]);
    }

    #[test]
    fn comment_pi_and_doctype_nodes_roundtrip() {
        let mut document = Document::new();
        let dt = document.insert_doctype(None, "html", "", "");
        let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
        let comment = document.insert_comment(None, "hi");
        let pi = document.insert_pi(None, "target", "data");
        assert_eq!(
            document.node(dt),
            Some(&Node::Doctype {
                name: "html".to_string(),
                public_id: String::new(),
                system_id: String::new(),
            })
        );
        assert_eq!(
            document.node(comment),
            Some(&Node::Comment {
                data: "hi".to_string()
            })
        );
        assert_eq!(
            document.node(pi),
            Some(&Node::Pi {
                target: "target".to_string(),
                data: "data".to_string(),
            })
        );
        assert_eq!(document.roots(), &[dt, html, comment, pi]);
    }

    #[test]
    fn removal_unlinks_nodes_and_ids() {
        let mut document = Document::new();
        let parent =
            document.insert_element(None, "div", ElementNs::Html, vec![Attr::plain("id", "x")]);
        document.set_root(parent).unwrap();
        let a = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
        let b = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
        assert_eq!(document.remove_node(a), Ok(true));
        assert_eq!(document.remove_node(a), Ok(false));
        assert_eq!(document.children(parent), vec![b]);
        assert_eq!(document.prev_sibling(b), None);
        assert_eq!(document.remove_node(parent), Ok(true));
        assert_eq!(document.element_by_id("x"), None);
        assert_eq!(document.root(), None);
        assert!(document.node(parent).is_some());
        assert!(document.node(a).is_some());
    }

    #[test]
    fn removal_detaches_a_subtree_without_invalidating_handles() {
        let mut document = Document::new();
        let article = document.insert_element(None, "article", ElementNs::Html, vec![]);
        let section = document.insert_element(Some(article), "section", ElementNs::Html, vec![]);
        let paragraph = document.insert_element(Some(section), "p", ElementNs::Html, vec![]);
        let text = document.insert_text(Some(paragraph), "deep text");
        assert_eq!(document.remove_node(section), Ok(true));
        assert_eq!(document.children(article), Vec::<NodeId>::new());
        assert!(document.node(section).is_some());
        assert!(document.node(paragraph).is_some());
        assert!(document.node(text).is_some());
        assert_eq!(document.children(section), vec![paragraph]);
        assert!(document.node(article).is_some());
        assert_eq!(document.root(), Some(article));
    }

    #[test]
    fn insert_before_places_node_in_sibling_chain() {
        let mut document = Document::new();
        let parent = document.insert_element(None, "ul", ElementNs::Html, vec![]);
        let a = document.insert_element(Some(parent), "li", ElementNs::Html, vec![]);
        let c = document.insert_element(Some(parent), "li", ElementNs::Html, vec![]);
        let b = document.insert_before(c, html("li"));
        assert_eq!(document.children(parent), vec![a, b, c]);
        assert_eq!(document.prev_sibling(c), Some(b));
        assert_eq!(document.next_sibling(b), Some(c));
        assert_eq!(document.parent(b), Some(parent));
        let z = document.insert_before(a, html("li"));
        assert_eq!(document.children(parent), vec![z, a, b, c]);
        assert_eq!(document.first_child(parent), Some(z));
        assert_eq!(document.prev_sibling(z), None);
    }

    #[test]
    fn insert_before_on_root_keeps_root_order() {
        let mut document = Document::new();
        let first = document.insert_element(None, "html", ElementNs::Html, vec![]);
        let last = document.insert_element(None, "body", ElementNs::Html, vec![]);
        let mid = document.insert_before(last, html("head"));
        assert_eq!(document.roots(), &[first, mid, last]);
        assert_eq!(document.parent(mid), None);
        assert_eq!(document.root(), Some(first));
    }

    #[test]
    fn append_based_on_parent_node_fosters_before_sibling() {
        let mut document = Document::new();
        let body = document.insert_element(None, "body", ElementNs::Html, vec![]);
        document.set_root(body).unwrap();
        let table = document.insert_element(Some(body), "table", ElementNs::Html, vec![]);
        let row = document.insert_element(Some(table), "tr", ElementNs::Html, vec![]);
        let revolution = document.insert_element(Some(row), "div", ElementNs::Html, vec![]);
        let text = document.append_based_on_parent_node(
            Some(table),
            Some(revolution),
            Node::Text {
                data: "text".to_string(),
            },
        );
        assert_eq!(document.children(body), vec![text, table]);
        assert_eq!(document.parent(text), Some(body));
        assert_eq!(document.next_sibling(text), Some(table));
        let late = document.append_based_on_parent_node(Some(row), Some(revolution), html("p"));
        assert_eq!(document.children(table), vec![late, row]);
        assert_eq!(document.parent(late), Some(table));
        assert_eq!(document.prev_sibling(row), Some(late));
    }

    #[test]
    fn append_based_on_parent_node_without_parent_goes_to_new_parent() {
        let mut document = Document::new();
        let div = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let pending = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let id = document.append_based_on_parent_node(Some(pending), Some(div), html("span"));
        assert_eq!(document.children(div), vec![id]);
        assert_eq!(document.parent(id), Some(div));
        assert!(document.node(pending).is_some());
        assert_eq!(document.roots(), &[div, pending]);
    }

    #[test]
    fn reparent_children_moves_the_whole_chain() {
        let mut document = Document::new();
        let head = document.insert_element(None, "head", ElementNs::Html, vec![]);
        let title = document.insert_element(Some(head), "title", ElementNs::Html, vec![]);
        let meta = document.insert_element(Some(head), "meta", ElementNs::Html, vec![]);
        let template = document.insert_element(None, "template", ElementNs::Html, vec![]);
        document.reparent_children(head, Some(template)).unwrap();
        assert_eq!(document.children(head), Vec::<NodeId>::new());
        assert_eq!(document.children(template), vec![title, meta]);
        assert_eq!(document.parent(title), Some(template));
        assert_eq!(document.parent(meta), Some(template));
        assert_eq!(document.first_child(head), None);
        assert_eq!(document.last_child(head), None);
    }

    #[test]
    fn reparent_children_to_none_appends_to_roots() {
        let mut document = Document::new();
        let html = document.insert_element(None, "html", ElementNs::Html, vec![]);
        let f1 = document.insert_element(Some(html), "frameset", ElementNs::Html, vec![]);
        let f2 = document.insert_element(Some(html), "frameset", ElementNs::Html, vec![]);
        document.reparent_children(html, None).unwrap();
        assert_eq!(document.children(html), Vec::<NodeId>::new());
        assert_eq!(document.roots(), &[html, f1, f2]);
        assert_eq!(document.prev_sibling(f1), Some(html));
        assert_eq!(document.next_sibling(f2), None);
    }

    #[test]
    fn add_attrs_if_missing_skips_existing_and_is_ns_aware() {
        let mut document = Document::new();
        let div = document.insert_element(
            None,
            "div",
            ElementNs::Html,
            vec![
                Attr::plain("id", "main"),
                Attr::namespaced(AttrNs::Xlink, "href", "x"),
            ],
        );
        document.add_attrs_if_missing(
            div,
            vec![
                Attr::plain("id", "other"),
                Attr::plain("class", "b"),
                Attr::namespaced(AttrNs::None, "href", "y"),
            ],
        );
        assert_eq!(
            document.node(div),
            Some(&Node::Element {
                name: "div".to_string(),
                ns: ElementNs::Html,
                attrs: vec![
                    Attr::plain("id", "main"),
                    Attr::namespaced(AttrNs::Xlink, "href", "x"),
                    Attr::plain("class", "b"),
                    Attr::plain("href", "y"),
                ],
            })
        );
        assert_eq!(document.element_by_id("main"), Some(div));
        assert_eq!(document.element_by_id("other"), None);
    }

    #[test]
    fn add_attrs_indexes_a_fresh_id() {
        let mut document = Document::new();
        let div = document.insert_element(None, "div", ElementNs::Html, vec![]);
        document.add_attrs_if_missing(div, vec![Attr::plain("id", "late")]);
        assert_eq!(document.element_by_id("late"), Some(div));
        document.remove_node(div).unwrap();
        assert_eq!(document.element_by_id("late"), None);
    }

    #[test]
    fn detach_unlinks_but_keeps_node_and_children() {
        let mut document = Document::new();
        let parent = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let a = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
        let text = document.insert_text(Some(a), "x");
        let b = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
        document.detach(a).unwrap();
        assert_eq!(document.children(parent), vec![b]);
        assert_eq!(document.prev_sibling(b), None);
        assert_eq!(document.first_child(parent), Some(b));
        assert_eq!(document.last_child(parent), Some(b));
        assert_eq!(document.parent(a), None);
        assert_eq!(document.children(a), vec![text]);
        assert!(document.node(a).is_some());
        let root = document.insert_element(None, "section", ElementNs::Html, vec![]);
        document.detach(root).unwrap();
        assert_eq!(document.roots(), &[parent]);
        assert!(document.node(root).is_some());
    }

    #[test]
    fn attach_moves_node_to_end_of_new_parent() {
        let mut document = Document::new();
        let div = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let span = document.insert_element(None, "span", ElementNs::Html, vec![]);
        document.insert_text(Some(span), "words");
        document.attach(span, Some(div)).unwrap();
        assert_eq!(document.children(div), vec![span]);
        assert_eq!(document.parent(span), Some(div));
        assert_eq!(document.root(), Some(div));
        let p = document.insert_element(Some(div), "p", ElementNs::Html, vec![]);
        document.attach(span, Some(div)).unwrap();
        assert_eq!(document.children(div), vec![p, span]);
        assert_eq!(document.last_child(div), Some(span));
        assert_eq!(document.prev_sibling(span), Some(p));
    }

    #[test]
    fn attach_to_none_turns_node_into_root() {
        let mut document = Document::new();
        let section = document.insert_element(None, "section", ElementNs::Html, vec![]);
        let p = document.insert_element(Some(section), "p", ElementNs::Html, vec![]);
        document.attach(p, None).unwrap();
        assert_eq!(document.roots(), &[section, p]);
        assert_eq!(document.parent(p), None);
        assert_eq!(document.children(section), Vec::<NodeId>::new());
    }

    #[test]
    fn attach_before_splices_between_siblings() {
        let mut document = Document::new();
        let ul = document.insert_element(None, "ul", ElementNs::Html, vec![]);
        let a = document.insert_element(Some(ul), "li", ElementNs::Html, vec![]);
        let c = document.insert_element(Some(ul), "li", ElementNs::Html, vec![]);
        let b = document.insert_element(None, "li", ElementNs::Html, vec![]);
        document.attach_before(b, c).unwrap();
        assert_eq!(document.children(ul), vec![a, b, c]);
        assert_eq!(document.parent(b), Some(ul));
        assert_eq!(document.prev_sibling(b), Some(a));
        assert_eq!(document.next_sibling(a), Some(b));
        let z = document.insert_element(None, "li", ElementNs::Html, vec![]);
        document.attach_before(z, a).unwrap();
        assert_eq!(document.children(ul), vec![z, a, b, c]);
        assert_eq!(document.first_child(ul), Some(z));
        assert_eq!(document.prev_sibling(z), None);
    }

    #[test]
    fn attach_before_between_roots_keeps_root_order() {
        let mut document = Document::new();
        let head = document.insert_element(None, "head", ElementNs::Html, vec![]);
        let body = document.insert_element(None, "body", ElementNs::Html, vec![]);
        let title = document.insert_element(None, "title", ElementNs::Html, vec![]);
        document.attach_before(title, body).unwrap();
        assert_eq!(document.roots(), &[head, title, body]);
        assert_eq!(document.next_sibling(title), Some(body));
        assert_eq!(document.prev_sibling(body), Some(title));
        let meta = document.insert_element(None, "meta", ElementNs::Html, vec![]);
        document.attach_before(meta, head).unwrap();
        assert_eq!(document.roots(), &[meta, head, title, body]);
        assert_eq!(document.prev_sibling(meta), None);
    }

    #[test]
    fn append_merged_text_extends_trailing_text() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let first = document.append_merged_text(Some(p), "Hello ");
        let second = document.append_merged_text(Some(p), "world");
        assert_eq!(first, second);
        assert_eq!(
            document.node(first),
            Some(&Node::Text {
                data: "Hello world".to_string()
            })
        );
        assert_eq!(document.children(p), vec![first]);
        let mid = document.insert_element(Some(p), "br", ElementNs::Html, vec![]);
        let third = document.append_merged_text(Some(p), "after");
        assert_ne!(third, first);
        assert_eq!(document.children(p), vec![first, mid, third]);
    }

    #[test]
    fn append_merged_text_on_document_appends_roots() {
        let mut document = Document::new();
        let first = document.append_merged_text(None, "a");
        let second = document.append_merged_text(None, "b");
        assert_ne!(first, second);
        assert_eq!(document.roots(), &[first, second]);
    }

    #[test]
    fn insert_merged_text_before_merges_into_previous_text() {
        let mut document = Document::new();
        let p = document.insert_element(None, "p", ElementNs::Html, vec![]);
        let first = document.insert_text(Some(p), "ab");
        let br = document.insert_element(Some(p), "br", ElementNs::Html, vec![]);
        let third = document.insert_text(Some(p), "cd");
        let merged = document.insert_merged_text_before(br, "xy");
        assert_eq!(merged, first);
        assert_eq!(
            document.node(first),
            Some(&Node::Text {
                data: "abxy".to_string()
            })
        );
        let tail = document.insert_merged_text_before(third, "ef");
        assert_ne!(tail, first);
        assert_eq!(
            document.node(tail),
            Some(&Node::Text {
                data: "ef".to_string()
            })
        );
        let draft = document.insert_element(None, "span", ElementNs::Html, vec![]);
        document.attach_before(draft, br).unwrap();
        let final_merge = document.insert_merged_text_before(draft, "zz");
        assert_eq!(final_merge, first);
        assert_eq!(
            document.node(first),
            Some(&Node::Text {
                data: "abxyzz".to_string()
            })
        );
        assert_eq!(document.children(p), vec![first, draft, br, tail, third]);
    }

    #[test]
    fn checked_moves_reject_cycles_without_changing_the_tree() {
        let mut document = Document::new();
        let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
        let child = document.insert_element(Some(root), "section", ElementNs::Html, vec![]);
        let grandchild = document.insert_element(Some(child), "p", ElementNs::Html, vec![]);
        assert_eq!(
            document.attach(root, Some(grandchild)),
            Err(DomError::Cycle)
        );
        assert_eq!(
            document.attach_before(root, grandchild),
            Err(DomError::Cycle)
        );
        assert_eq!(
            document.reparent_children(root, Some(grandchild)),
            Err(DomError::Cycle)
        );
        assert_eq!(document.root(), Some(root));
        assert_eq!(document.children(root), vec![child]);
        assert_eq!(document.children(child), vec![grandchild]);
    }

    #[test]
    fn duplicate_ids_resolve_to_the_first_connected_element_in_tree_order() {
        let mut document = Document::new();
        let root = document.insert_element(None, "main", ElementNs::Html, vec![]);
        let first = document.insert_element(
            Some(root),
            "p",
            ElementNs::Html,
            vec![Attr::plain("id", "duplicate")],
        );
        let second = document.insert_element(
            Some(root),
            "p",
            ElementNs::Html,
            vec![Attr::plain("id", "duplicate")],
        );
        assert_eq!(document.element_by_id("duplicate"), Some(first));
        document.remove_node(first).unwrap();
        assert_eq!(document.element_by_id("duplicate"), Some(second));
    }
}

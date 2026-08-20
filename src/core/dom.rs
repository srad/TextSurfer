use slotmap::{SlotMap, new_key_type};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

new_key_type! {
    pub struct NodeId;
}

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
}

impl Node {}

pub type SharedDocument = Rc<RefCell<Document>>;

#[derive(Debug, Default)]
pub struct Document {
    arena: SlotMap<NodeId, Node>,
    parent: HashMap<NodeId, Option<NodeId>>,
    first_child: HashMap<NodeId, Option<NodeId>>,
    last_child: HashMap<NodeId, Option<NodeId>>,
    prev_sibling: HashMap<NodeId, Option<NodeId>>,
    next_sibling: HashMap<NodeId, Option<NodeId>>,
    id_index: HashMap<String, NodeId>,
    roots: Vec<NodeId>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.arena.get(id)
    }

    pub fn root(&self) -> Option<NodeId> {
        self.roots.first().copied()
    }

    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    pub fn element_by_id(&self, id_attr: &str) -> Option<NodeId> {
        self.id_index.get(id_attr).copied()
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.parent.get(&id).copied().flatten()
    }

    pub fn first_child(&self, id: NodeId) -> Option<NodeId> {
        self.first_child.get(&id).copied().flatten()
    }

    pub fn last_child(&self, id: NodeId) -> Option<NodeId> {
        self.last_child.get(&id).copied().flatten()
    }

    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.next_sibling.get(&id).copied().flatten()
    }

    pub fn prev_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.prev_sibling.get(&id).copied().flatten()
    }

    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cursor = self.first_child(id);
        while let Some(child) = cursor {
            out.push(child);
            cursor = self.next_sibling(child);
        }
        out
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
        if let Some(p) = parent {
            assert!(self.arena.contains_key(p), "parent node must exist");
        }
        let id = self.insert_raw(node);
        self.parent.insert(id, parent);
        match parent {
            None => self.roots.push(id),
            Some(p) => match self.last_child(p) {
                None => {
                    self.first_child.insert(p, Some(id));
                    self.last_child.insert(p, Some(id));
                }
                Some(last) => {
                    self.next_sibling.insert(last, Some(id));
                    self.prev_sibling.insert(id, Some(last));
                    self.last_child.insert(p, Some(id));
                }
            },
        }
        id
    }

    pub fn insert_before(&mut self, sibling: NodeId, node: Node) -> NodeId {
        assert!(self.arena.contains_key(sibling), "sibling node must exist");
        let parent = self.parent(sibling);
        let id = self.insert_raw(node);
        self.parent.insert(id, parent);
        let prev = self.prev_sibling(sibling);
        if let Some(pr) = prev {
            self.next_sibling.insert(pr, Some(id));
        }
        self.prev_sibling.insert(id, prev);
        self.next_sibling.insert(id, Some(sibling));
        self.prev_sibling.insert(sibling, Some(id));
        if prev.is_none() {
            if let Some(p) = parent {
                self.first_child.insert(p, Some(id));
            } else {
                let pos = self
                    .roots
                    .iter()
                    .position(|&root| root == sibling)
                    .expect("sibling must be a root");
                self.roots.insert(pos, id);
            }
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

    pub fn detach(&mut self, id: NodeId) {
        assert!(self.arena.contains_key(id), "node must exist");
        self.unlink(id);
        self.parent.insert(id, None);
        self.prev_sibling.insert(id, None);
        self.next_sibling.insert(id, None);
    }

    pub fn attach(&mut self, id: NodeId, parent: Option<NodeId>) {
        assert!(self.arena.contains_key(id), "node must exist");
        if let Some(p) = parent {
            assert!(self.arena.contains_key(p), "parent node must exist");
        }
        self.unlink(id);
        self.parent.insert(id, parent);
        match parent {
            None => self.roots.push(id),
            Some(p) => {
                let prev = self.last_child(p);
                self.prev_sibling.insert(id, prev);
                self.next_sibling.insert(id, None);
                if let Some(pr) = prev {
                    self.next_sibling.insert(pr, Some(id));
                } else {
                    self.first_child.insert(p, Some(id));
                }
                self.last_child.insert(p, Some(id));
            }
        }
    }

    pub fn attach_before(&mut self, id: NodeId, sibling: NodeId) {
        assert!(self.arena.contains_key(id), "node must exist");
        assert!(self.arena.contains_key(sibling), "sibling node must exist");
        let parent = self.parent(sibling);
        self.unlink(id);
        self.parent.insert(id, parent);
        let prev = self.prev_sibling(sibling);
        if let Some(pr) = prev {
            self.next_sibling.insert(pr, Some(id));
        }
        self.prev_sibling.insert(id, prev);
        self.next_sibling.insert(id, Some(sibling));
        self.prev_sibling.insert(sibling, Some(id));
        match parent {
            Some(p) => {
                if prev.is_none() {
                    self.first_child.insert(p, Some(id));
                }
            }
            None => {
                let pos = self
                    .roots
                    .iter()
                    .position(|&root| root == sibling)
                    .expect("sibling must be a root");
                self.roots.insert(pos, id);
            }
        }
    }

    pub fn append_merged_text(&mut self, parent: Option<NodeId>, data: &str) -> NodeId {
        let last = parent.and_then(|p| self.last_child(p));
        if let Some(last) = last
            && let Some(Node::Text { data: existing }) = self.arena.get_mut(last)
        {
            existing.push_str(data);
            return last;
        }
        self.insert_text(parent, data)
    }

    pub fn insert_merged_text_before(&mut self, sibling: NodeId, data: &str) -> NodeId {
        if let Some(prev) = self.prev_sibling(sibling)
            && let Some(Node::Text { data: existing }) = self.arena.get_mut(prev)
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

    fn unlink(&mut self, id: NodeId) {
        if let Some(p) = self.parent(id) {
            if self.first_child(p) == Some(id) {
                self.first_child.insert(p, self.next_sibling(id));
            }
            if self.last_child(p) == Some(id) {
                self.last_child.insert(p, self.prev_sibling(id));
            }
        }
        if let Some(pr) = self.prev_sibling(id) {
            self.next_sibling.insert(pr, self.next_sibling(id));
        }
        if let Some(nx) = self.next_sibling(id) {
            self.prev_sibling.insert(nx, self.prev_sibling(id));
        }
        if let Some(pos) = self.roots.iter().position(|&root| root == id) {
            self.roots.remove(pos);
        }
    }

    pub fn reparent_children(&mut self, node: NodeId, new_parent: Option<NodeId>) {
        let kids = self.children(node);
        if kids.is_empty() {
            return;
        }
        assert!(
            !new_parent.is_some_and(|p| kids.contains(&p)),
            "new parent must not be inside the moved subtree"
        );
        self.first_child.insert(node, None);
        self.last_child.insert(node, None);
        let first = kids[0];
        let last = kids[kids.len() - 1];
        for &kid in &kids {
            self.parent.insert(kid, new_parent);
        }
        match new_parent {
            Some(p) => {
                let old_last = self.last_child(p);
                if let Some(ol) = old_last {
                    self.next_sibling.insert(ol, Some(first));
                }
                self.prev_sibling.insert(first, old_last);
                self.last_child.insert(p, Some(last));
                if old_last.is_none() {
                    self.first_child.insert(p, Some(first));
                }
            }
            None => {
                self.prev_sibling.insert(first, None);
                self.next_sibling.insert(last, None);
                self.roots.extend(kids);
            }
        }
    }

    pub fn add_attrs_if_missing(&mut self, id: NodeId, attrs: Vec<Attr>) {
        let mut added_id: Option<String> = None;
        if let Some(Node::Element {
            attrs: existing, ..
        }) = self.arena.get_mut(id)
        {
            for attr in attrs {
                if !existing
                    .iter()
                    .any(|a| a.ns == attr.ns && a.name == attr.name)
                {
                    if attr.ns == AttrNs::None && attr.name == "id" {
                        added_id = Some(attr.value.clone());
                    }
                    existing.push(attr);
                }
            }
        }
        if let Some(value) = added_id {
            self.id_index.insert(value, id);
        }
    }

    pub fn set_root(&mut self, id: NodeId) {
        if self.arena.contains_key(id) && !self.roots.contains(&id) {
            self.roots.push(id);
        }
    }

    pub fn remove_node(&mut self, id: NodeId) -> bool {
        if !self.arena.contains_key(id) {
            return false;
        }
        for child in self.children(id) {
            self.remove_node(child);
        }
        let parent = self.parent(id);
        let prev = self.prev_sibling(id);
        let next = self.next_sibling(id);
        if let Some(p) = parent {
            if self.first_child(p) == Some(id) {
                self.first_child.insert(p, next);
            }
            if self.last_child(p) == Some(id) {
                self.last_child.insert(p, next);
            }
        }
        if let Some(pr) = prev {
            self.next_sibling.insert(pr, next);
        }
        if let Some(nx) = next {
            self.prev_sibling.insert(nx, prev);
        }
        if let Some(pos) = self.roots.iter().position(|&root| root == id) {
            self.roots.remove(pos);
        }
        let id_attr = match self.arena.get(id) {
            Some(Node::Element { attrs, .. }) => attrs
                .iter()
                .find(|attr| attr.ns == AttrNs::None && attr.name == "id")
                .map(|attr| attr.value.clone()),
            _ => None,
        };
        if let Some(value) = id_attr {
            self.id_index.remove(&value);
        }
        self.arena.remove(id);
        self.parent.remove(&id);
        self.first_child.remove(&id);
        self.last_child.remove(&id);
        self.prev_sibling.remove(&id);
        self.next_sibling.remove(&id);
        true
    }

    fn insert_raw(&mut self, node: Node) -> NodeId {
        let id_attr = match &node {
            Node::Element { attrs, .. } => attrs
                .iter()
                .find(|attr| attr.ns == AttrNs::None && attr.name == "id")
                .map(|attr| attr.value.clone()),
            _ => None,
        };
        let id = self.arena.insert(node);
        self.parent.insert(id, None);
        self.first_child.insert(id, None);
        self.last_child.insert(id, None);
        self.prev_sibling.insert(id, None);
        self.next_sibling.insert(id, None);
        if let Some(value) = id_attr {
            self.id_index.insert(value, id);
        }
        id
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
        document.set_root(parent);
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
        document.set_root(parent);
        let a = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
        let b = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
        assert!(document.remove_node(a));
        assert!(!document.remove_node(a));
        assert_eq!(document.children(parent), vec![b]);
        assert_eq!(document.prev_sibling(b), None);
        assert!(document.remove_node(parent));
        assert_eq!(document.element_by_id("x"), None);
        assert_eq!(document.root(), None);
        assert!(document.is_empty());
    }

    #[test]
    fn removal_drops_descendants() {
        let mut document = Document::new();
        let article = document.insert_element(None, "article", ElementNs::Html, vec![]);
        let section = document.insert_element(Some(article), "section", ElementNs::Html, vec![]);
        let paragraph = document.insert_element(Some(section), "p", ElementNs::Html, vec![]);
        let text = document.insert_text(Some(paragraph), "deep text");
        assert!(document.remove_node(section));
        assert_eq!(document.children(article), Vec::<NodeId>::new());
        assert!(document.node(section).is_none());
        assert!(document.node(paragraph).is_none());
        assert!(document.node(text).is_none());
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
        document.set_root(body);
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
        document.reparent_children(head, Some(template));
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
        document.reparent_children(html, None);
        assert_eq!(document.children(html), Vec::<NodeId>::new());
        assert_eq!(document.roots(), &[html, f1, f2]);
        assert_eq!(document.prev_sibling(f1), None);
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
        document.remove_node(div);
        assert_eq!(document.element_by_id("late"), None);
    }

    #[test]
    fn detach_unlinks_but_keeps_node_and_children() {
        let mut document = Document::new();
        let parent = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let a = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
        let text = document.insert_text(Some(a), "x");
        let b = document.insert_element(Some(parent), "p", ElementNs::Html, vec![]);
        document.detach(a);
        assert_eq!(document.children(parent), vec![b]);
        assert_eq!(document.prev_sibling(b), None);
        assert_eq!(document.first_child(parent), Some(b));
        assert_eq!(document.last_child(parent), Some(b));
        assert_eq!(document.parent(a), None);
        assert_eq!(document.children(a), vec![text]);
        assert!(document.node(a).is_some());
        let root = document.insert_element(None, "section", ElementNs::Html, vec![]);
        document.detach(root);
        assert_eq!(document.roots(), &[parent]);
        assert!(document.node(root).is_some());
    }

    #[test]
    fn attach_moves_node_to_end_of_new_parent() {
        let mut document = Document::new();
        let div = document.insert_element(None, "div", ElementNs::Html, vec![]);
        let span = document.insert_element(None, "span", ElementNs::Html, vec![]);
        document.insert_text(Some(span), "words");
        document.attach(span, Some(div));
        assert_eq!(document.children(div), vec![span]);
        assert_eq!(document.parent(span), Some(div));
        assert_eq!(document.root(), Some(div));
        let p = document.insert_element(Some(div), "p", ElementNs::Html, vec![]);
        document.attach(span, Some(div));
        assert_eq!(document.children(div), vec![p, span]);
        assert_eq!(document.last_child(div), Some(span));
        assert_eq!(document.prev_sibling(span), Some(p));
    }

    #[test]
    fn attach_to_none_turns_node_into_root() {
        let mut document = Document::new();
        let section = document.insert_element(None, "section", ElementNs::Html, vec![]);
        let p = document.insert_element(Some(section), "p", ElementNs::Html, vec![]);
        document.attach(p, None);
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
        document.attach_before(b, c);
        assert_eq!(document.children(ul), vec![a, b, c]);
        assert_eq!(document.parent(b), Some(ul));
        assert_eq!(document.prev_sibling(b), Some(a));
        assert_eq!(document.next_sibling(a), Some(b));
        let z = document.insert_element(None, "li", ElementNs::Html, vec![]);
        document.attach_before(z, a);
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
        document.attach_before(title, body);
        assert_eq!(document.roots(), &[head, title, body]);
        assert_eq!(document.next_sibling(title), Some(body));
        assert_eq!(document.prev_sibling(body), Some(title));
        let meta = document.insert_element(None, "meta", ElementNs::Html, vec![]);
        document.attach_before(meta, head);
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
        document.attach_before(draft, br);
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
}

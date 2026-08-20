use crate::core::dom::{
    Attr, AttrNs, Document, DomQuirksMode, ElementNs, Node, NodeId, SharedDocument,
};
use crate::html::parser::ParseOutcome;
use html5ever::tendril::StrTendril;
use html5ever::tree_builder::{ElemName, ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, LocalName, Namespace, QualName};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";
const SVG_NS: &str = "http://www.w3.org/2000/svg";
const MATHML_NS: &str = "http://www.w3.org/1998/Math/MathML";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Handle {
    Doc,
    Node(NodeId),
    Template(NodeId),
}

impl Handle {
    pub(crate) fn node(&self) -> NodeId {
        match self {
            Handle::Node(id) | Handle::Template(id) => *id,
            Handle::Doc => panic!("handle does not denote a real node"),
        }
    }

    fn parent_slot(&self) -> Option<NodeId> {
        match self {
            Handle::Doc => None,
            Handle::Node(id) | Handle::Template(id) => Some(*id),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ArenaElemName {
    ns: Namespace,
    local: LocalName,
}

impl ElemName for ArenaElemName {
    fn ns(&self) -> &Namespace {
        &self.ns
    }

    fn local_name(&self) -> &LocalName {
        &self.local
    }
}

pub(crate) struct ArenaTreeSink {
    document: SharedDocument,
    errors: Cell<usize>,
    fragment: bool,
    context: Option<NodeId>,
    integration_points: RefCell<HashSet<NodeId>>,
}

fn map_element_ns(ns: &Namespace) -> ElementNs {
    if ns.as_ref() == HTML_NS {
        ElementNs::Html
    } else if ns.as_ref() == SVG_NS {
        ElementNs::Svg
    } else if ns.as_ref() == MATHML_NS {
        ElementNs::MathMl
    } else {
        ElementNs::Other
    }
}

fn map_attr_ns(ns: &Namespace) -> AttrNs {
    if ns.as_ref() == XLINK_NS {
        AttrNs::Xlink
    } else if ns.as_ref() == XML_NS {
        AttrNs::Xml
    } else if ns.as_ref() == XMLNS_NS {
        AttrNs::Xmlns
    } else {
        AttrNs::None
    }
}

fn map_attr(attr: &Attribute) -> Attr {
    Attr {
        name: attr.name.local.as_ref().to_string(),
        ns: map_attr_ns(&attr.name.ns),
        value: attr.value.to_string(),
    }
}

fn ns_string(ns: ElementNs) -> &'static str {
    match ns {
        ElementNs::Html => HTML_NS,
        ElementNs::Svg => SVG_NS,
        ElementNs::MathMl => MATHML_NS,
        ElementNs::Other => "",
    }
}

fn base_href(document: &Document) -> Option<String> {
    fn walk(document: &Document, id: NodeId, found: &mut Option<String>) -> bool {
        if let Some(Node::Element { name, ns, attrs }) = document.node(id) {
            if *ns == ElementNs::Html
                && name == "base"
                && found.is_none()
                && let Some(href) = attrs
                    .iter()
                    .find(|attr| attr.ns == AttrNs::None && attr.name == "href")
            {
                *found = Some(href.value.clone());
                return true;
            }
            if !(*ns == ElementNs::Html && name == "template") {
                for child in document.children(id) {
                    if walk(document, child, found) {
                        return true;
                    }
                }
            }
        }
        false
    }
    let mut found = None;
    for root in document.roots() {
        if walk(document, *root, &mut found) {
            break;
        }
    }
    found
}

impl ArenaTreeSink {
    pub fn new(fragment: bool, context: Option<NodeId>) -> Self {
        Self {
            document: Rc::new(RefCell::new(Document::new())),
            errors: Cell::new(0),
            fragment,
            context,
            integration_points: RefCell::new(HashSet::new()),
        }
    }

    pub(crate) fn set_context(&mut self, id: NodeId) {
        self.context = Some(id);
    }

    pub fn finish(self) -> ParseOutcome {
        let document = &self.document;
        if self.fragment {
            let context = self.context.expect("fragment sink requires a context node");
            let html_root = document
                .borrow()
                .roots()
                .iter()
                .copied()
                .find(|root| {
                    *root != context
                        && matches!(
                            document.borrow().node(*root),
                            Some(Node::Element {
                                name,
                                ns: ElementNs::Html,
                                ..
                            }) if name == "html"
                        )
                })
                .expect("fragment parse must produce the synthetic html root");
            document
                .borrow_mut()
                .reparent_children(html_root, Some(context))
                .expect("fragment reparenting must be acyclic");
            document
                .borrow_mut()
                .remove_node(html_root)
                .expect("synthetic fragment root must exist");
        }
        let doc = document.borrow();
        let base = base_href(&doc);
        ParseOutcome {
            document: Rc::clone(&self.document),
            base_href: base,
            parse_errors: self.errors.get(),
        }
    }
}

impl TreeSink for ArenaTreeSink {
    type Handle = Handle;
    type Output = ParseOutcome;
    type ElemName<'a> = ArenaElemName;

    fn finish(self) -> Self::Output {
        ArenaTreeSink::finish(self)
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {
        self.errors.set(self.errors.get() + 1);
    }

    fn get_document(&self) -> Self::Handle {
        Handle::Doc
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        let id = target.node();
        let document = self.document.borrow();
        match document.node(id) {
            Some(Node::Element { name, ns, .. }) => ArenaElemName {
                ns: Namespace::from(ns_string(*ns)),
                local: LocalName::from(name.as_str()),
            },
            _ => panic!("elem_name called on a non-element node"),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> Self::Handle {
        let attr = attrs.iter().map(map_attr).collect();
        let id = self.document.borrow_mut().insert_element(
            None,
            name.local.as_ref(),
            map_element_ns(&name.ns),
            attr,
        );
        if flags.mathml_annotation_xml_integration_point {
            self.integration_points.borrow_mut().insert(id);
        }
        if flags.template {
            self.document
                .borrow_mut()
                .create_template_contents(id)
                .expect("template element must exist");
        }
        Handle::Node(id)
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        let id = self
            .document
            .borrow_mut()
            .insert_comment(None, text.as_ref());
        Handle::Node(id)
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        let id = self
            .document
            .borrow_mut()
            .insert_pi(None, target.as_ref(), data.as_ref());
        Handle::Node(id)
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        let parent_id = parent.parent_slot();
        let mut document = self.document.borrow_mut();
        match child {
            NodeOrText::AppendNode(node) => document
                .attach(node.node(), parent_id)
                .expect("tree builder append must be acyclic"),
            NodeOrText::AppendText(text) => {
                document.append_merged_text(parent_id, text.as_ref());
            }
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        let element_id = element.node();
        let prev_id = prev_element.node();
        let has_parent = self.document.borrow().parent(element_id).is_some();
        let mut document = self.document.borrow_mut();
        match child {
            NodeOrText::AppendNode(node) if has_parent => {
                document
                    .attach_before(node.node(), element_id)
                    .expect("foster parenting must be acyclic");
            }
            NodeOrText::AppendNode(node) => document
                .attach(node.node(), Some(prev_id))
                .expect("tree builder append must be acyclic"),
            NodeOrText::AppendText(text) if has_parent => {
                document.insert_merged_text_before(element_id, text.as_ref());
            }
            NodeOrText::AppendText(text) => {
                document.append_merged_text(Some(prev_id), text.as_ref());
            }
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        self.document.borrow_mut().insert_doctype(
            None,
            name.as_ref(),
            public_id.as_ref(),
            system_id.as_ref(),
        );
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        let template = target.node();
        Handle::Template(
            self.document
                .borrow()
                .template_contents(template)
                .expect("template contents must be created with the element"),
        )
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        let mode = match mode {
            QuirksMode::Quirks => DomQuirksMode::Quirks,
            QuirksMode::LimitedQuirks => DomQuirksMode::LimitedQuirks,
            QuirksMode::NoQuirks => DomQuirksMode::NoQuirks,
        };
        self.document.borrow_mut().set_quirks_mode(mode);
    }

    fn append_before_sibling(&self, sibling: &Self::Handle, new_node: NodeOrText<Self::Handle>) {
        let sibling_id = sibling.node();
        let mut document = self.document.borrow_mut();
        match new_node {
            NodeOrText::AppendNode(node) => document
                .attach_before(node.node(), sibling_id)
                .expect("tree builder sibling insertion must be acyclic"),
            NodeOrText::AppendText(text) => {
                document.insert_merged_text_before(sibling_id, text.as_ref());
            }
        }
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        let id = target.node();
        let mapped = attrs.iter().map(map_attr).collect();
        self.document.borrow_mut().add_attrs_if_missing(id, mapped);
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        let id = target.node();
        self.document
            .borrow_mut()
            .detach(id)
            .expect("tree builder target must exist");
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        let node_id = node.node();
        self.document
            .borrow_mut()
            .reparent_children(node_id, new_parent.parent_slot())
            .expect("tree builder reparenting must be acyclic");
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &Self::Handle) -> bool {
        self.integration_points.borrow().contains(&handle.node())
    }

    fn allow_declarative_shadow_roots(&self, _intended_parent: &Self::Handle) -> bool {
        false
    }
}

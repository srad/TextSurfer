mod attrs;
mod mutate;
mod query;
mod validate;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use indextree::Arena;

pub use attrs::{attr_number, attr_value, has_attr};
pub use validate::DomError;

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

pub type SharedDocument = Rc<RefCell<Document>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DomQuirksMode {
    Quirks,
    LimitedQuirks,
    #[default]
    NoQuirks,
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
}

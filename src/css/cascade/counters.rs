use cssparser::{Parser, ParserInput};

use crate::core::dom::{Document, ElementNs, Node, NodeId, attr_number, has_attr};
use crate::core::style::ComputedStyle;
use crate::css::Declaration;
use crate::css::values::{is_css_wide_keyword, parse_ident};

pub const LIST_ITEM_COUNTER: &str = "list-item";

/// One counter instance. A counter created at `depth` stays in scope for that element, its
/// descendants and its following siblings, which is exactly "entries deeper than the element
/// being visited are out of scope".
struct CounterEntry {
    depth: usize,
    name: String,
    value: i64,
}

#[derive(Default)]
pub(super) struct CounterScopes {
    entries: Vec<CounterEntry>,
}

impl CounterScopes {
    pub(super) fn enter(&mut self, depth: usize) {
        self.entries.retain(|entry| entry.depth <= depth);
    }

    pub(super) fn run(&mut self, depth: usize, ops: &CounterOps) {
        for (name, value) in &ops.reset {
            self.reset(depth, name, *value);
        }
        for (name, value) in &ops.increment {
            self.adjust(name, *value);
        }
        for (name, value) in &ops.set {
            self.assign(name, *value);
        }
    }

    fn reset(&mut self, depth: usize, name: &str, value: i64) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.depth == depth && entry.name == name)
        {
            entry.value = value;
            return;
        }
        self.entries.push(CounterEntry {
            depth,
            name: name.to_string(),
            value,
        });
    }

    fn adjust(&mut self, name: &str, by: i64) {
        match self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.name == name)
        {
            Some(entry) => entry.value = entry.value.saturating_add(by),
            None => self.entries.push(CounterEntry {
                depth: 0,
                name: name.to_string(),
                value: by,
            }),
        }
    }

    fn assign(&mut self, name: &str, value: i64) {
        match self
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.name == name)
        {
            Some(entry) => entry.value = value,
            None => self.entries.push(CounterEntry {
                depth: 0,
                name: name.to_string(),
                value,
            }),
        }
    }

    pub(super) fn value(&self, name: &str) -> i64 {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.name == name)
            .map_or(0, |entry| entry.value)
    }

    pub(super) fn values(&self, name: &str) -> Vec<i64> {
        self.entries
            .iter()
            .filter(|entry| entry.name == name)
            .map(|entry| entry.value)
            .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct CounterOps {
    reset: Vec<(String, i64)>,
    increment: Vec<(String, i64)>,
    set: Vec<(String, i64)>,
}

/// The counter properties an author declared on one element. `None` means "not declared", so the
/// UA-derived list for that property survives; a declared list replaces it outright.
#[derive(Default)]
pub(super) struct AuthoredCounterOps {
    reset: Option<Vec<(String, i64)>>,
    increment: Option<Vec<(String, i64)>>,
    set: Option<Vec<(String, i64)>>,
}

impl AuthoredCounterOps {
    pub(super) fn apply(&mut self, declaration: &Declaration) {
        let slot = match declaration.name.as_str() {
            "counter-reset" => &mut self.reset,
            "counter-increment" => &mut self.increment,
            "counter-set" => &mut self.set,
            _ => return,
        };
        let default = if declaration.name == "counter-increment" {
            1
        } else {
            0
        };
        if parse_ident(&declaration.value).is_some_and(|value| value == "revert") {
            *slot = None;
            return;
        }
        if let Some(values) = parse_counter_values(&declaration.value, default) {
            *slot = Some(values);
        }
    }

    pub(super) fn resolve(
        &self,
        document: &Document,
        id: NodeId,
        style: ComputedStyle,
    ) -> CounterOps {
        let ua = ua_counter_ops(document, id, style);
        CounterOps {
            reset: merge_counter_ops(self.reset.as_deref(), ua.reset),
            increment: merge_counter_ops(self.increment.as_deref(), ua.increment),
            set: merge_counter_ops(self.set.as_deref(), ua.set),
        }
    }
}

/// The `list-item` operations implied by `display: list-item` and by `ol`/`ul` are *implicit*: an
/// author who increments some counter of their own does not thereby stop a list from numbering.
/// Only naming the same counter overrides the implicit operation.
fn merge_counter_ops(
    authored: Option<&[(String, i64)]>,
    implicit: Vec<(String, i64)>,
) -> Vec<(String, i64)> {
    let Some(authored) = authored else {
        return implicit;
    };
    let mut merged: Vec<(String, i64)> = implicit
        .into_iter()
        .filter(|(name, _)| !authored.iter().any(|(other, _)| other == name))
        .collect();
    merged.extend(authored.iter().cloned());
    merged
}

/// `ol`/`ul` open a `list-item` scope, list items step it, and the HTML ordinal attributes
/// (`start`, `reversed`, `value`) are expressed as counter operations at UA origin.
fn ua_counter_ops(document: &Document, id: NodeId, style: ComputedStyle) -> CounterOps {
    let mut ops = CounterOps::default();
    let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
        return ops;
    };
    if *ns == ElementNs::Html && matches!(name.as_str(), "ol" | "ul" | "menu") {
        let reversed = name == "ol" && has_attr(attrs, "reversed");
        let start = attr_number(attrs, "start");
        let first = if reversed {
            start.unwrap_or_else(|| list_item_count(document, id))
        } else {
            start.unwrap_or(1)
        };
        let step: i64 = if reversed { -1 } else { 1 };
        ops.reset
            .push((LIST_ITEM_COUNTER.to_string(), first.saturating_sub(step)));
    }
    if style.display.is_list_item() {
        let reversed = document
            .parent(id)
            .and_then(|parent| document.node(parent))
            .is_some_and(|node| match node {
                Node::Element { name, ns, attrs } => {
                    *ns == ElementNs::Html && name == "ol" && has_attr(attrs, "reversed")
                }
                _ => false,
            });
        ops.increment
            .push((LIST_ITEM_COUNTER.to_string(), if reversed { -1 } else { 1 }));
        if *ns == ElementNs::Html
            && name == "li"
            && let Some(value) = attr_number(attrs, "value")
        {
            ops.set.push((LIST_ITEM_COUNTER.to_string(), value));
        }
    }
    ops
}

fn list_item_count(document: &Document, list: NodeId) -> i64 {
    document
        .children(list)
        .into_iter()
        .filter(|child| match document.node(*child) {
            Some(Node::Element { name, ns, .. }) => *ns == ElementNs::Html && name == "li",
            _ => false,
        })
        .count() as i64
}

/// `none` and the CSS-wide keywords all mean "this declaration names no counter". Returning an
/// empty list rather than `None` matters: `None` means "invalid, ignore me", which would leave an
/// earlier declaration standing instead of letting the later one win. An empty list still merges
/// with the implicit `list-item` operation, so `li { counter-increment: initial }` does not stop a
/// list numbering — which is also why `revert` needs no separate treatment, the UA origin it
/// reverts to *is* that implicit operation. `inherit` is approximated the same way; we do not model
/// parent counter values.
pub(super) fn parse_counter_values(source: &str, default: i64) -> Option<Vec<(String, i64)>> {
    if parse_ident(source).is_some_and(|value| is_css_wide_keyword(&value)) {
        return Some(Vec::new());
    }
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut values: Vec<(String, i64)> = Vec::new();
    while !parser.is_exhausted() {
        let name = parser.expect_ident_cloned().ok()?.to_string();
        if is_css_wide_keyword(&name) {
            return None;
        }
        let value = parser
            .try_parse(|input| input.expect_integer())
            .map_or(default, i64::from);
        values.push((name, value));
    }
    (!values.is_empty()).then_some(values)
}

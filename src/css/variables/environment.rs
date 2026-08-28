use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cssparser::{Parser, ParserInput};

use super::syntax::{dependencies, substitute};
use super::{Environment, Lookup, MAX_VALUE_BYTES};

pub(crate) fn derive_environment(
    parent: Arc<Environment>,
    winners: HashMap<String, String>,
) -> Arc<Environment> {
    if winners.is_empty() {
        return parent;
    }
    let mut raw = HashMap::new();
    let mut values: HashMap<String, Option<Arc<str>>> = HashMap::new();
    for (name, value) in winners {
        match css_wide(&value) {
            Some("initial") => {
                values.insert(name, None);
            }
            Some("inherit" | "unset" | "revert") => {}
            _ => {
                raw.insert(name, value);
            }
        }
    }
    let cycles = cycle_members(&raw);
    for name in &cycles {
        values.insert(name.clone(), None);
    }
    for name in resolution_order(&raw, &cycles) {
        let result = substitute(
            &raw[&name],
            |reference| {
                values.get(reference).map_or_else(
                    || parent.lookup(reference),
                    |value| {
                        value
                            .as_ref()
                            .map_or(Lookup::Invalid, |value| Lookup::Value(value.clone()))
                    },
                )
            },
            MAX_VALUE_BYTES,
        );
        if let Ok(value) = result {
            match css_wide(&value) {
                Some("initial") => {
                    values.insert(name, None);
                }
                Some("inherit" | "unset" | "revert") => {}
                _ => {
                    values.insert(name, Some(Arc::from(value)));
                }
            }
        } else {
            values.insert(name, None);
        }
    }
    Arc::new(Environment {
        parent: Some(parent),
        values,
    })
}

fn css_wide(source: &str) -> Option<&str> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let source = parser.expect_ident_cloned().ok()?;
    if !parser.is_exhausted() {
        return None;
    }
    ["initial", "inherit", "unset", "revert"]
        .into_iter()
        .find(|keyword| source.eq_ignore_ascii_case(keyword))
}

fn cycle_members(raw: &HashMap<String, String>) -> HashSet<String> {
    let graph = dependency_graph(raw);
    let mut cycles = HashSet::new();
    let mut complete = HashSet::new();
    for start in raw.keys() {
        if complete.contains(start) {
            continue;
        }
        let mut path = Vec::new();
        let mut positions = HashMap::new();
        let mut stack = vec![(start.clone(), 0usize)];
        while let Some((node, edge)) = stack.pop() {
            if edge == 0 {
                positions.insert(node.clone(), path.len());
                path.push(node.clone());
            }
            let edges = &graph[&node];
            if edge < edges.len() {
                stack.push((node.clone(), edge + 1));
                let next = edges[edge].clone();
                if let Some(position) = positions.get(&next).copied() {
                    cycles.extend(path[position..].iter().cloned());
                } else if !complete.contains(&next) {
                    stack.push((next, 0));
                }
            } else {
                positions.remove(&node);
                path.pop();
                complete.insert(node);
            }
        }
    }
    cycles
}

fn resolution_order(raw: &HashMap<String, String>, cycles: &HashSet<String>) -> Vec<String> {
    let graph = dependency_graph(raw);
    let mut complete = cycles.clone();
    let mut order = Vec::new();
    for start in raw.keys() {
        if complete.contains(start) {
            continue;
        }
        let mut stack = vec![(start.clone(), 0usize)];
        while let Some((node, edge)) = stack.pop() {
            if complete.contains(&node) {
                continue;
            }
            let edges = &graph[&node];
            if edge < edges.len() {
                stack.push((node, edge + 1));
                let next = &edges[edge];
                if !complete.contains(next) {
                    stack.push((next.clone(), 0));
                }
            } else {
                complete.insert(node.clone());
                order.push(node);
            }
        }
    }
    order
}

fn dependency_graph(raw: &HashMap<String, String>) -> HashMap<String, Vec<String>> {
    raw.iter()
        .map(|(name, value)| {
            (
                name.clone(),
                dependencies(value)
                    .into_iter()
                    .filter(|dependency| raw.contains_key(dependency))
                    .collect(),
            )
        })
        .collect()
}

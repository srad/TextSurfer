mod environment;
mod syntax;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

pub(super) use environment::derive_environment;
pub(super) use syntax::{
    contains_var, is_custom_name, trim_css_whitespace, validate_declaration_value,
};

pub(super) const MAX_VALUE_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_COMPONENT_DEPTH: usize = 64;

#[derive(Clone)]
pub(super) struct Environment {
    parent: Option<Arc<Self>>,
    values: HashMap<String, Option<Arc<str>>>,
}

impl Environment {
    pub(super) fn root() -> Arc<Self> {
        Arc::new(Self {
            parent: None,
            values: HashMap::new(),
        })
    }

    pub(super) fn substitute(&self, source: &str) -> Result<String, ()> {
        syntax::substitute(source, |name| self.lookup(name), MAX_VALUE_BYTES)
    }

    fn lookup(&self, name: &str) -> Lookup {
        let mut environment = Some(self);
        while let Some(current) = environment {
            if let Some(value) = current.values.get(name) {
                return value
                    .as_ref()
                    .map_or(Lookup::Invalid, |value| Lookup::Value(value.clone()));
            }
            environment = current.parent.as_deref();
        }
        Lookup::Missing
    }
}

#[derive(Clone)]
enum Lookup {
    Value(Arc<str>),
    Invalid,
    Missing,
}

use std::borrow::Borrow;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

use cssparser::{ToCss, serialize_identifier};
use precomputed_hash::PrecomputedHash;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Atom(String);

impl Atom {
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for Atom {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Atom {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Atom {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl ToCss for Atom {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        serialize_identifier(&self.0, dest)
    }
}

impl PrecomputedHash for Atom {
    fn precomputed_hash(&self) -> u32 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish() as u32
    }
}

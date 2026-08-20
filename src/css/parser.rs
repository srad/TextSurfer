#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleSheet;

pub trait CssParser: Send + Sync {
    fn parse(&self, source: &str) -> StyleSheet;
}

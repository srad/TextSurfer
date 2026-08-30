#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    pub source: String,
}

pub trait CssParser: Send + Sync {
    fn parse(&self, source: &str) -> StyleSheet;
}

#[derive(Default)]
pub struct CssparserParser;

impl CssParser for CssparserParser {
    fn parse(&self, source: &str) -> StyleSheet {
        StyleSheet {
            source: source.to_string(),
        }
    }
}

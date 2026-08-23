use crate::core::geom::Size;
use crate::core::style::Palette;
use crate::css::StyleSheet;
use crate::css::parser::{
    ColorScheme, CssRule, MediaAxis, MediaComparison, MediaFeature, MediaQuery, MediaQueryList,
    ScriptingValue, StyleRule,
};
use crate::css::selectors::DynamicState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MediaTarget {
    Screen,
    Print,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaContext {
    target: MediaTarget,
    pub palette: Palette,
    pub state: DynamicState,
    pub scripting: bool,
    pub color_scheme: ColorScheme,
    pub viewport: Size,
}

impl MediaContext {
    pub const fn screen() -> Self {
        Self {
            target: MediaTarget::Screen,
            palette: Palette::DEFAULT,
            state: DynamicState::INERT,
            scripting: false,
            color_scheme: ColorScheme::Dark,
            viewport: Size { cols: 80, rows: 24 },
        }
    }

    pub const fn print() -> Self {
        Self {
            target: MediaTarget::Print,
            palette: Palette::DEFAULT,
            state: DynamicState::INERT,
            scripting: false,
            color_scheme: ColorScheme::Dark,
            viewport: Size { cols: 80, rows: 24 },
        }
    }

    pub fn with_palette(self, palette: Palette) -> Self {
        Self { palette, ..self }
    }

    pub fn with_state(self, state: DynamicState) -> Self {
        Self { state, ..self }
    }

    pub fn with_scripting(self, scripting: bool) -> Self {
        Self { scripting, ..self }
    }

    pub fn with_color_scheme(self, color_scheme: ColorScheme) -> Self {
        Self {
            color_scheme,
            ..self
        }
    }

    pub fn with_viewport(self, viewport: Size) -> Self {
        Self { viewport, ..self }
    }
}

pub(super) fn active_style_rules(sheets: &[StyleSheet], media: MediaContext) -> Vec<&StyleRule> {
    let mut stack = Vec::new();
    for sheet in sheets.iter().rev() {
        stack.extend(sheet.rules.iter().rev());
    }
    let mut active = Vec::new();
    while let Some(rule) = stack.pop() {
        match rule {
            CssRule::Style(rule) => active.push(rule),
            CssRule::Media(rule) if media_query_list_matches(&rule.queries, media) => {
                stack.extend(rule.rules.iter().rev());
            }
            CssRule::Media(_) => {}
            CssRule::Import(_) => {}
        }
    }
    active
}

pub fn media_query_list_matches(queries: &MediaQueryList, media: MediaContext) -> bool {
    match queries {
        MediaQueryList::Always => true,
        MediaQueryList::Any(queries) => queries.iter().any(|query| match query {
            MediaQuery::Type {
                negated,
                media_type,
            } => {
                let matched = match media_type.as_str() {
                    "all" => true,
                    "screen" => media.target == MediaTarget::Screen,
                    "print" => media.target == MediaTarget::Print,
                    _ => false,
                };
                matched != *negated
            }
            MediaQuery::Condition {
                negated,
                media_type,
                features,
            } => {
                let type_matches = media_type
                    .as_deref()
                    .is_none_or(|media_type| media_type_matches(media_type, media));
                let matched = type_matches
                    && features
                        .iter()
                        .all(|feature| media_feature_matches(*feature, media));
                matched != *negated
            }
            MediaQuery::Never => false,
        }),
    }
}

fn media_type_matches(media_type: &str, media: MediaContext) -> bool {
    match media_type {
        "all" => true,
        "screen" => media.target == MediaTarget::Screen,
        "print" => media.target == MediaTarget::Print,
        _ => false,
    }
}

fn media_feature_matches(feature: MediaFeature, media: MediaContext) -> bool {
    match feature {
        MediaFeature::Scripting(None) => media.scripting,
        MediaFeature::Scripting(Some(ScriptingValue::None)) => !media.scripting,
        MediaFeature::Scripting(Some(ScriptingValue::InitialOnly)) => false,
        MediaFeature::Scripting(Some(ScriptingValue::Enabled)) => media.scripting,
        MediaFeature::PrefersColorScheme(None) => true,
        MediaFeature::PrefersColorScheme(Some(scheme)) => media.color_scheme == scheme,
        MediaFeature::Dimension {
            axis,
            comparison,
            value,
        } => {
            let actual = match axis {
                MediaAxis::Width => media.viewport.cols,
                MediaAxis::Height => media.viewport.rows,
            };
            match value {
                None => actual != 0,
                Some(expected) => match comparison {
                    MediaComparison::Equal => actual == expected,
                    MediaComparison::Minimum => actual >= expected,
                    MediaComparison::Maximum => actual <= expected,
                },
            }
        }
    }
}

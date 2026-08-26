use cssparser::{Parser, ParserInput, Token};

use super::super::MediaContext;
use crate::core::style::{
    CssNumber, GridIdent, GridLength, GridRepeat, GridStore, GridTemplateComponent,
    GridTemplateData, LengthAxis, RepeatCount, StyleStore, TrackBreadthMax, TrackBreadthMin,
    TrackSize,
};
use crate::css::math::parse_length_percentage_parser;
use crate::css::values::{parse_length_token, percentage_value};

/// A `<track-list>` may not name more components than this. Taffy separately clamps the realised
/// grid to 10 000 tracks per axis; this only bounds what the cascade stores.
const MAX_COMPONENTS: usize = 1_024;

/// `repeat()` counts above this are clamped rather than rejected, which is what the spec's
/// overlarge-grid rule allows.
const MAX_REPEAT_COUNT: u16 = 10_000;

/// Run `parse` inside the block the parser has just entered, treating a `None` as a parse error so
/// the block is consumed either way.
fn nested<'i, T>(
    parser: &mut Parser<'i, '_>,
    parse: impl for<'t> FnOnce(&mut Parser<'i, 't>) -> Option<T>,
) -> Option<T> {
    parser
        .parse_nested_block(|input| parse(input).ok_or_else(|| input.new_custom_error::<_, ()>(())))
        .ok()
}

/// Parse a `<track-list>` — the value of `grid-template-rows` / `grid-template-columns`.
///
/// Returns `None` for `none` as well as for an invalid list; the caller distinguishes them,
/// because `none` is a valid value that clears the template.
pub(super) fn parse_template(
    source: &str,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<GridTemplateData> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_template_parser(&mut parser, media, axis, store)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

pub(super) fn parse_template_parser(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<GridTemplateData> {
    let mut components = Vec::new();
    let mut line_names = vec![parse_line_names(parser, store)?];
    while let Some(component) = parse_component(parser, media, axis, store)? {
        if components.len() >= MAX_COMPONENTS {
            return None;
        }
        components.push(component);
        line_names.push(parse_line_names(parser, store)?);
    }
    if components.is_empty() {
        return None;
    }
    let template = GridTemplateData {
        components,
        line_names,
    };
    is_valid_template(&template).then_some(template)
}

pub(super) fn parse_explicit_template_parser(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<GridTemplateData> {
    let mut components = Vec::new();
    let mut line_names = vec![parse_line_names(parser, store)?];
    while let Some(track) = parse_track_size(parser, media, axis, store) {
        if components.len() >= MAX_COMPONENTS {
            return None;
        }
        components.push(GridTemplateComponent::Single(track));
        line_names.push(parse_line_names(parser, store)?);
    }
    (!components.is_empty()).then_some(GridTemplateData {
        components,
        line_names,
    })
}

/// A track list may hold at most one auto-repeat, and only if every one of its tracks has a fixed
/// component. Taffy discards such a list wholesale at layout time; rejecting it here instead keeps
/// the cascade honest, so an invalid declaration leaves the previously cascaded value in place.
fn is_valid_template(template: &GridTemplateData) -> bool {
    let auto_repeats = template
        .components
        .iter()
        .filter(|component| match component {
            GridTemplateComponent::Single(_) => false,
            GridTemplateComponent::Repeat(repeat) => repeat.count.is_auto(),
        })
        .count();
    match auto_repeats {
        0 => true,
        1 => template.components.iter().all(|component| match component {
            GridTemplateComponent::Single(track) => track.has_fixed_component(),
            GridTemplateComponent::Repeat(repeat) => repeat
                .tracks
                .iter()
                .all(|track| track.has_fixed_component()),
        }),
        _ => false,
    }
}

/// `Ok(None)` means "no component here", which ends the list; `None` means the input was invalid.
fn parse_component(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<Option<GridTemplateComponent>> {
    if parser.is_exhausted() {
        return Some(None);
    }
    let state = parser.state();
    if let Ok(Token::Function(name)) = parser.next().cloned()
        && name.eq_ignore_ascii_case("repeat")
    {
        let repeat = nested(parser, |input| parse_repeat(input, media, axis, store))?;
        return Some(Some(GridTemplateComponent::Repeat(repeat)));
    }
    parser.reset(&state);
    match parse_track_size(parser, media, axis, store) {
        Some(track) => Some(Some(GridTemplateComponent::Single(track))),
        None => {
            parser.reset(&state);
            Some(None)
        }
    }
}

fn parse_repeat(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<GridRepeat> {
    let count = parse_repeat_count(parser)?;
    parser.expect_comma().ok()?;
    let mut tracks = Vec::new();
    let mut line_names = vec![parse_line_names(parser, store)?];
    while let Some(track) = parse_track_size(parser, media, axis, store) {
        if tracks.len() >= MAX_COMPONENTS {
            return None;
        }
        tracks.push(track);
        line_names.push(parse_line_names(parser, store)?);
    }
    parser.expect_exhausted().ok()?;
    if tracks.is_empty() {
        return None;
    }
    Some(GridRepeat {
        count,
        tracks,
        line_names,
    })
}

fn parse_repeat_count(parser: &mut Parser<'_, '_>) -> Option<RepeatCount> {
    if let Ok(word) = parser.try_parse(|input| input.expect_ident_cloned()) {
        return match word.to_ascii_lowercase().as_str() {
            "auto-fill" => Some(RepeatCount::AutoFill),
            "auto-fit" => Some(RepeatCount::AutoFit),
            _ => None,
        };
    }
    let count = parser.expect_integer().ok()?;
    (1..=i32::from(MAX_REPEAT_COUNT))
        .contains(&count)
        .then_some(RepeatCount::Count(count as u16))
}

/// A `[name other]` line-name set. An absent set is the empty set, never a parse failure, so every
/// line of a template gets one and the positional invariant Taffy asserts on holds by construction.
pub(super) fn parse_line_names(
    parser: &mut Parser<'_, '_>,
    store: &mut StyleStore,
) -> Option<Vec<GridIdent>> {
    let mut names = Vec::new();
    while parser
        .try_parse(|input| input.expect_square_bracket_block())
        .is_ok()
    {
        let block = nested(parser, |input| {
            let mut block = Vec::new();
            while !input.is_exhausted() {
                let name = input.expect_ident_cloned().ok()?;
                block.push(custom_ident(&name, &mut store.grid)?);
            }
            Some(block)
        })?;
        names.extend(block);
    }
    Some(names)
}

/// A `<custom-ident>` excludes the CSS-wide keywords and, in grid, `span` and `auto`.
pub(super) fn custom_ident(name: &str, store: &mut GridStore) -> Option<GridIdent> {
    let lowered = name.to_ascii_lowercase();
    if matches!(
        lowered.as_str(),
        "span" | "auto" | "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default"
    ) {
        return None;
    }
    store.insert_ident(name)
}

/// Leaves the parser exactly where it started when there is no track size here, so a caller can
/// go on to read whatever else the grammar allows in that position — a row string, or the `/` that
/// separates the two halves of a shorthand.
pub(super) fn parse_track_size(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<TrackSize> {
    let state = parser.state();
    let value = parse_track_size_inner(parser, media, axis, store);
    if value.is_none() {
        parser.reset(&state);
    }
    value
}

fn parse_track_size_inner(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<TrackSize> {
    let state = parser.state();
    if let Ok(Token::Function(name)) = parser.next().cloned() {
        if name.eq_ignore_ascii_case("minmax") {
            return nested(parser, |input| parse_minmax(input, media, axis, store));
        }
        if name.eq_ignore_ascii_case("fit-content") {
            return nested(parser, |input| parse_fit_content(input, media, axis, store));
        }
        parser.reset(&state);
    }
    parser.reset(&state);
    let breadth = parse_breadth(parser, media, axis, store)?;
    Some(breadth.into_track_size())
}

/// `fit-content(x)` is `minmax(auto, fit-content(x))`; it is never a `minmax()` argument itself.
fn parse_fit_content(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<TrackSize> {
    let length = parse_grid_length(parser, media, axis, store)?;
    parser.expect_exhausted().ok()?;
    Some(TrackSize::breadth(
        TrackBreadthMin::Auto,
        TrackBreadthMax::FitContent(length),
    ))
}

fn parse_minmax(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<TrackSize> {
    let min = parse_breadth(parser, media, axis, store)?;
    parser.expect_comma().ok()?;
    let max = parse_breadth(parser, media, axis, store)?;
    parser.expect_exhausted().ok()?;
    // An `<inflexible-breadth>` is required on the min side: `minmax(1fr, …)` is invalid.
    let min = match min {
        Breadth::Auto => TrackBreadthMin::Auto,
        Breadth::MinContent => TrackBreadthMin::MinContent,
        Breadth::MaxContent => TrackBreadthMin::MaxContent,
        Breadth::Length(value) => TrackBreadthMin::Length(value),
        Breadth::Fr(_) => return None,
    };
    Some(TrackSize::breadth(min, max.into_max()))
}

#[derive(Clone, Copy)]
enum Breadth {
    Auto,
    MinContent,
    MaxContent,
    Length(GridLength),
    Fr(CssNumber),
}

impl Breadth {
    fn into_max(self) -> TrackBreadthMax {
        match self {
            Self::Auto => TrackBreadthMax::Auto,
            Self::MinContent => TrackBreadthMax::MinContent,
            Self::MaxContent => TrackBreadthMax::MaxContent,
            Self::Length(value) => TrackBreadthMax::Length(value),
            Self::Fr(value) => TrackBreadthMax::Fr(value),
        }
    }

    /// A bare breadth is both the min and the max, except `fr`, whose min is `auto`.
    fn into_track_size(self) -> TrackSize {
        let min = match self {
            Self::Auto | Self::Fr(_) => TrackBreadthMin::Auto,
            Self::MinContent => TrackBreadthMin::MinContent,
            Self::MaxContent => TrackBreadthMin::MaxContent,
            Self::Length(value) => TrackBreadthMin::Length(value),
        };
        TrackSize::breadth(min, self.into_max())
    }
}

fn parse_breadth(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<Breadth> {
    let state = parser.state();
    if let Ok(word) = parser.try_parse(|input| input.expect_ident_cloned()) {
        return match word.to_ascii_lowercase().as_str() {
            "auto" => Some(Breadth::Auto),
            "min-content" => Some(Breadth::MinContent),
            "max-content" => Some(Breadth::MaxContent),
            _ => {
                parser.reset(&state);
                None
            }
        };
    }
    if let Ok(Token::Dimension { value, unit, .. }) = parser.next().cloned()
        && unit.eq_ignore_ascii_case("fr")
    {
        return CssNumber::new(value).map(Breadth::Fr);
    }
    parser.reset(&state);
    parse_grid_length(parser, media, axis, store).map(Breadth::Length)
}

fn parse_grid_length(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<GridLength> {
    let state = parser.state();
    if let Ok(Token::Percentage { unit_value, .. }) = parser.next().cloned() {
        return percentage_value(unit_value).map(GridLength::Percent);
    }
    parser.reset(&state);
    if let Some(expression) = parse_length_percentage_parser(parser, media, axis) {
        return store
            .calculations
            .insert(expression, crate::core::style::CalcRange::NonNegative)
            .map(GridLength::Calc);
    }
    parser.reset(&state);
    let length = parse_length_token(parser)?;
    Some(GridLength::Cells(media.resolve_cells(length, axis)))
}

/// `grid-auto-rows` / `grid-auto-columns`: a bare `<track-size>+`, with no repeats or line names.
pub(super) fn parse_auto_tracks(
    source: &str,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<Vec<TrackSize>> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_auto_tracks_parser(&mut parser, media, axis, store)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

pub(super) fn parse_auto_tracks_parser(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<Vec<TrackSize>> {
    let mut tracks = Vec::new();
    while let Some(track) = parse_track_size(parser, media, axis, store) {
        if tracks.len() >= MAX_COMPONENTS {
            return None;
        }
        tracks.push(track);
    }
    (!tracks.is_empty()).then_some(tracks)
}

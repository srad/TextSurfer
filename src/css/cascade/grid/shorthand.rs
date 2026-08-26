use cssparser::{Parser, ParserInput};

use super::super::MediaContext;
use super::areas;
use super::tracks::{
    parse_auto_tracks_parser, parse_explicit_template_parser, parse_line_names,
    parse_template_parser, parse_track_size,
};
use crate::core::style::{
    GridAreasData, GridAutoFlow, GridTemplateComponent, GridTemplateData, LengthAxis, StyleStore,
    TrackSize,
};

/// Everything the `grid` and `grid-template` shorthands can set. Both are resetting shorthands, so
/// a field left `None` here is reset to its initial value by the caller.
#[derive(Default)]
pub(super) struct GridShorthand {
    pub(super) template_rows: Option<GridTemplateData>,
    pub(super) template_columns: Option<GridTemplateData>,
    pub(super) template_areas: Option<GridAreasData>,
    pub(super) auto_rows: Option<Vec<TrackSize>>,
    pub(super) auto_columns: Option<Vec<TrackSize>>,
    pub(super) auto_flow: Option<GridAutoFlow>,
}

pub(super) fn parse_grid_template(
    source: &str,
    media: MediaContext,
    store: &mut StyleStore,
) -> Option<GridShorthand> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_grid_template_parser(&mut parser, media, store)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

fn parse_grid_template_parser(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    store: &mut StyleStore,
) -> Option<GridShorthand> {
    let start = parser.state();
    if parser
        .try_parse(|input| input.expect_ident_matching("none"))
        .is_ok()
        && parser.is_exhausted()
    {
        return Some(GridShorthand::default());
    }
    parser.reset(&start);
    let state = parser.state();
    let checkpoint = store.checkpoint();
    if let Some(value) = parse_area_form(parser, media, store) {
        return Some(value);
    }
    parser.reset(&state);
    store.rollback(checkpoint);
    let template_rows = parse_template_axis(parser, media, LengthAxis::Vertical, store)?;
    parser.expect_delim('/').ok()?;
    let template_columns = parse_template_axis(parser, media, LengthAxis::Horizontal, store)?;
    Some(GridShorthand {
        template_rows,
        template_columns,
        ..Default::default()
    })
}

fn parse_template_axis(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
) -> Option<Option<GridTemplateData>> {
    if parser
        .try_parse(|input| input.expect_ident_matching("none"))
        .is_ok()
    {
        Some(None)
    } else {
        parse_template_parser(parser, media, axis, store).map(Some)
    }
}

/// `[ <line-names>? <string> <track-size>? <line-names>? ]+ [ / <explicit-track-list> ]?`
///
/// Each row string contributes one row track, defaulting to `auto`, and the strings together are
/// the `grid-template-areas` value.
fn parse_area_form(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    store: &mut StyleStore,
) -> Option<GridShorthand> {
    let mut rows = Vec::new();
    let mut components = Vec::new();
    let mut line_names = Vec::new();
    loop {
        let names = parse_line_names(parser, store)?;
        line_names.push(names);
        let Ok(row) = parser.try_parse(|input| input.expect_string_cloned()) else {
            break;
        };
        rows.push(row.as_ref().to_owned());
        let track =
            parse_track_size(parser, media, LengthAxis::Vertical, store).unwrap_or(TrackSize::AUTO);
        components.push(GridTemplateComponent::Single(track));
    }
    if rows.is_empty() {
        return None;
    }
    let template_areas = areas::from_strings(&rows, &mut store.grid)?;
    let template_columns = if parser.is_exhausted() {
        None
    } else {
        parser.expect_delim('/').ok()?;
        Some(parse_explicit_template_parser(
            parser,
            media,
            LengthAxis::Horizontal,
            store,
        )?)
    };
    Some(GridShorthand {
        template_rows: Some(GridTemplateData {
            components,
            line_names,
        }),
        template_columns,
        template_areas: Some(template_areas),
        ..Default::default()
    })
}

pub(super) fn parse_grid(
    source: &str,
    media: MediaContext,
    store: &mut StyleStore,
) -> Option<GridShorthand> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let state = parser.state();
    let checkpoint = store.checkpoint();
    if let Some(value) = parse_grid_template_parser(&mut parser, media, store)
        && parser.expect_exhausted().is_ok()
    {
        return Some(value);
    }
    parser.reset(&state);
    store.rollback(checkpoint);
    let value = parse_auto_flow_form(&mut parser, media, store)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

/// `<rows> / [ auto-flow && dense? ] <auto-columns>?`
/// or `[ auto-flow && dense? ] <auto-rows>? / <columns>`.
fn parse_auto_flow_form(
    parser: &mut Parser<'_, '_>,
    media: MediaContext,
    store: &mut StyleStore,
) -> Option<GridShorthand> {
    if let Some(dense) = parse_auto_flow(parser) {
        let auto_rows = parse_auto_tracks_parser(parser, media, LengthAxis::Vertical, store);
        parser.expect_delim('/').ok()?;
        let template_columns = parse_template_axis(parser, media, LengthAxis::Horizontal, store)?;
        return Some(GridShorthand {
            template_columns,
            auto_rows,
            auto_flow: Some(if dense {
                GridAutoFlow::RowDense
            } else {
                GridAutoFlow::Row
            }),
            ..Default::default()
        });
    }
    let template_rows = parse_template_axis(parser, media, LengthAxis::Vertical, store)?;
    parser.expect_delim('/').ok()?;
    let dense = parse_auto_flow(parser)?;
    let auto_columns = parse_auto_tracks_parser(parser, media, LengthAxis::Horizontal, store);
    Some(GridShorthand {
        template_rows,
        auto_columns,
        auto_flow: Some(if dense {
            GridAutoFlow::ColumnDense
        } else {
            GridAutoFlow::Column
        }),
        ..Default::default()
    })
}

/// `auto-flow && dense?` in either order; returns whether `dense` was present.
fn parse_auto_flow(parser: &mut Parser<'_, '_>) -> Option<bool> {
    let state = parser.state();
    let leading_dense = parser
        .try_parse(|input| input.expect_ident_matching("dense"))
        .is_ok();
    if parser
        .try_parse(|input| input.expect_ident_matching("auto-flow"))
        .is_err()
    {
        parser.reset(&state);
        return None;
    }
    if leading_dense {
        return Some(true);
    }
    Some(
        parser
            .try_parse(|input| input.expect_ident_matching("dense"))
            .is_ok(),
    )
}

use super::super::MediaContext;
use super::shorthand::GridShorthand;
use super::{areas, placement, shorthand, tracks};
use crate::core::style::{
    ComputedStyle, GridAutoFlow, GridStore, GridStyle, LengthAxis, StyleStore,
};
use crate::css::values::parse_ident;

pub(in crate::css::cascade) fn apply_grid_declaration(
    style: &mut ComputedStyle,
    property: &str,
    source: &str,
    media: MediaContext,
    store: &mut StyleStore,
) -> bool {
    if !matches!(
        property,
        "grid-template-columns"
            | "grid-template-rows"
            | "grid-template-areas"
            | "grid-auto-columns"
            | "grid-auto-rows"
            | "grid-auto-flow"
            | "grid-row-start"
            | "grid-row-end"
            | "grid-column-start"
            | "grid-column-end"
            | "grid-row"
            | "grid-column"
            | "grid-area"
            | "grid-template"
            | "grid"
    ) {
        return false;
    }
    let checkpoint = store.checkpoint();
    let mut candidate = *style;
    let valid = match property {
        "grid-template-columns" => assign_template(
            &mut candidate.grid,
            source,
            media,
            LengthAxis::Horizontal,
            store,
            false,
        ),
        "grid-template-rows" => assign_template(
            &mut candidate.grid,
            source,
            media,
            LengthAxis::Vertical,
            store,
            true,
        ),
        "grid-template-areas" => assign_areas(&mut candidate.grid, source, store),
        "grid-auto-columns" => assign_auto_tracks(
            &mut candidate.grid,
            source,
            media,
            LengthAxis::Horizontal,
            store,
            false,
        ),
        "grid-auto-rows" => assign_auto_tracks(
            &mut candidate.grid,
            source,
            media,
            LengthAxis::Vertical,
            store,
            true,
        ),
        "grid-auto-flow" => assign(&mut candidate.grid.auto_flow, parse_auto_flow(source)),
        "grid-row-start" => assign(
            &mut candidate.grid.row.start,
            placement::parse_placement(source, &mut store.grid),
        ),
        "grid-row-end" => assign(
            &mut candidate.grid.row.end,
            placement::parse_placement(source, &mut store.grid),
        ),
        "grid-column-start" => assign(
            &mut candidate.grid.column.start,
            placement::parse_placement(source, &mut store.grid),
        ),
        "grid-column-end" => assign(
            &mut candidate.grid.column.end,
            placement::parse_placement(source, &mut store.grid),
        ),
        "grid-row" => assign(
            &mut candidate.grid.row,
            placement::parse_lines(source, &mut store.grid),
        ),
        "grid-column" => assign(
            &mut candidate.grid.column,
            placement::parse_lines(source, &mut store.grid),
        ),
        "grid-area" => assign_area(&mut candidate.grid, source, &mut store.grid),
        "grid-template" => assign_shorthand(
            &mut candidate.grid,
            shorthand::parse_grid_template(source, media, store),
            store,
            false,
        ),
        "grid" => assign_shorthand(
            &mut candidate.grid,
            shorthand::parse_grid(source, media, store),
            store,
            true,
        ),
        _ => unreachable!(),
    };
    if valid {
        *style = candidate;
    } else {
        store.rollback(checkpoint);
    }
    true
}

fn assign<T>(target: &mut T, value: Option<T>) -> bool {
    if let Some(value) = value {
        *target = value;
        true
    } else {
        false
    }
}

fn assign_template(
    style: &mut GridStyle,
    source: &str,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
    rows: bool,
) -> bool {
    let target = if rows {
        &mut style.template_rows
    } else {
        &mut style.template_columns
    };
    if parse_ident(source).as_deref() == Some("none") {
        *target = None;
        return true;
    }
    let Some(template) = tracks::parse_template(source, media, axis, store) else {
        return false;
    };
    if let Some(handle) = store.grid.insert_template(template) {
        *target = Some(handle);
        true
    } else {
        false
    }
}

fn assign_areas(style: &mut GridStyle, source: &str, store: &mut StyleStore) -> bool {
    if parse_ident(source).as_deref() == Some("none") {
        style.template_areas = None;
        return true;
    }
    let Some(areas) = areas::parse_areas(source, &mut store.grid) else {
        return false;
    };
    if let Some(handle) = store.grid.insert_areas(areas) {
        style.template_areas = Some(handle);
        true
    } else {
        false
    }
}

fn assign_auto_tracks(
    style: &mut GridStyle,
    source: &str,
    media: MediaContext,
    axis: LengthAxis,
    store: &mut StyleStore,
    rows: bool,
) -> bool {
    let target = if rows {
        &mut style.auto_rows
    } else {
        &mut style.auto_columns
    };
    if parse_ident(source).as_deref() == Some("auto") {
        *target = None;
        return true;
    }
    let Some(value) = tracks::parse_auto_tracks(source, media, axis, store) else {
        return false;
    };
    if let Some(handle) = store.grid.insert_tracks(value) {
        *target = Some(handle);
        true
    } else {
        false
    }
}

fn assign_area(style: &mut GridStyle, source: &str, store: &mut GridStore) -> bool {
    if let Some((row, column)) = placement::parse_area(source, store) {
        style.row = row;
        style.column = column;
        true
    } else {
        false
    }
}

fn assign_shorthand(
    style: &mut GridStyle,
    value: Option<GridShorthand>,
    store: &mut StyleStore,
    resets_auto: bool,
) -> bool {
    let Some(value) = value else {
        return false;
    };
    let template_rows = match value.template_rows {
        Some(template) => match store.grid.insert_template(template) {
            Some(handle) => Some(handle),
            None => return false,
        },
        None => None,
    };
    let template_columns = match value.template_columns {
        Some(template) => match store.grid.insert_template(template) {
            Some(handle) => Some(handle),
            None => return false,
        },
        None => None,
    };
    let template_areas = match value.template_areas {
        Some(areas) => match store.grid.insert_areas(areas) {
            Some(handle) => Some(handle),
            None => return false,
        },
        None => None,
    };
    let auto_rows = match value.auto_rows {
        Some(auto) => match store.grid.insert_tracks(auto) {
            Some(handle) => Some(handle),
            None => return false,
        },
        None => None,
    };
    let auto_columns = match value.auto_columns {
        Some(auto) => match store.grid.insert_tracks(auto) {
            Some(handle) => Some(handle),
            None => return false,
        },
        None => None,
    };
    style.template_rows = template_rows;
    style.template_columns = template_columns;
    style.template_areas = template_areas;
    if resets_auto {
        style.auto_rows = auto_rows;
        style.auto_columns = auto_columns;
        style.auto_flow = value.auto_flow.unwrap_or_default();
    }
    true
}

fn parse_auto_flow(source: &str) -> Option<GridAutoFlow> {
    let mut input = cssparser::ParserInput::new(source);
    let mut parser = cssparser::Parser::new(&mut input);
    let mut direction = None;
    let mut dense = false;
    while !parser.is_exhausted() {
        let word = parser.expect_ident_cloned().ok()?.to_ascii_lowercase();
        match word.as_str() {
            "row" if direction.is_none() => direction = Some(false),
            "column" if direction.is_none() => direction = Some(true),
            "dense" if !dense => dense = true,
            _ => return None,
        }
    }
    if direction.is_none() && !dense {
        return None;
    }
    Some(match (direction.unwrap_or(false), dense) {
        (false, false) => GridAutoFlow::Row,
        (false, true) => GridAutoFlow::RowDense,
        (true, false) => GridAutoFlow::Column,
        (true, true) => GridAutoFlow::ColumnDense,
    })
}

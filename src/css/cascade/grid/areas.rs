use std::collections::HashMap;

use cssparser::{Parser, ParserInput};

use crate::core::style::{GridArea, GridAreasData, GridStore};

/// A `grid-template-areas` template may not exceed this in either axis.
const MAX_TRACKS: usize = 1_024;

pub(super) fn parse_areas(source: &str, store: &mut GridStore) -> Option<GridAreasData> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let value = parse_areas_parser(&mut parser, store)?;
    parser.expect_exhausted().ok()?;
    Some(value)
}

pub(super) fn parse_areas_parser(
    parser: &mut Parser<'_, '_>,
    store: &mut GridStore,
) -> Option<GridAreasData> {
    let mut rows: Vec<String> = Vec::new();
    while let Ok(row) = parser.try_parse(|input| input.expect_string_cloned()) {
        rows.push(row.as_ref().to_owned());
    }
    from_strings(&rows, store)
}

/// Shared with the `grid-template` shorthand, which interleaves the row strings with track sizes.
pub(super) fn from_strings(rows: &[String], store: &mut GridStore) -> Option<GridAreasData> {
    if rows.is_empty() || rows.len() > MAX_TRACKS {
        return None;
    }
    let rows: Vec<Vec<Option<String>>> = rows
        .iter()
        .map(|row| parse_row(row))
        .collect::<Option<_>>()?;
    let columns = rows.first()?.len();
    if columns == 0 || rows.iter().any(|row| row.len() != columns) {
        return None;
    }
    build(&rows, columns, store)
}

/// One row string is a sequence of cell tokens: a `<custom-ident>` name, or a run of one or more
/// `.` standing for a single null cell.
fn parse_row(row: &str) -> Option<Vec<Option<String>>> {
    let mut cells = Vec::new();
    let mut rest = row.trim_matches(is_css_whitespace);
    while !rest.is_empty() {
        if rest.starts_with('.') {
            rest = rest.trim_start_matches('.');
            cells.push(None);
        } else {
            let end = rest
                .find(|value: char| value == '.' || is_css_whitespace(value))
                .unwrap_or(rest.len());
            let (name, tail) = rest.split_at(end);
            if !is_ident(name) {
                return None;
            }
            cells.push(Some(name.to_owned()));
            rest = tail;
        }
        rest = rest.trim_start_matches(is_css_whitespace);
        if cells.len() > MAX_TRACKS {
            return None;
        }
    }
    Some(cells)
}

fn is_css_whitespace(value: char) -> bool {
    matches!(value, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

fn is_ident(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|value| {
            value.is_alphanumeric() || value == '_' || value == '-' || !value.is_ascii()
        })
}

/// Every name must cover a rectangle, and each name may appear only once. Anything else makes the
/// whole declaration invalid.
fn build(
    rows: &[Vec<Option<String>>],
    columns: usize,
    store: &mut GridStore,
) -> Option<GridAreasData> {
    let mut bounds: HashMap<&str, (usize, usize, usize, usize)> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for (row, cells) in rows.iter().enumerate() {
        for (column, cell) in cells.iter().enumerate() {
            let Some(name) = cell.as_deref() else {
                continue;
            };
            match bounds.get_mut(name) {
                Some(area) => {
                    area.1 = area.1.max(row);
                    area.3 = area.3.max(column);
                }
                None => {
                    bounds.insert(name, (row, row, column, column));
                    order.push(name);
                }
            }
        }
    }
    let mut areas = Vec::with_capacity(order.len());
    for name in order {
        let (row_start, row_end, column_start, column_end) = bounds[name];
        // Every cell inside the bounding box must carry the name, or the area is not a rectangle.
        let rectangular = rows[row_start..=row_end].iter().all(|cells| {
            cells[column_start..=column_end]
                .iter()
                .all(|cell| cell.as_deref() == Some(name))
        });
        if !rectangular {
            return None;
        }
        areas.push(GridArea {
            name: store.insert_ident(name)?,
            row_start: line(row_start)?,
            row_end: line(row_end + 1)?,
            column_start: line(column_start)?,
            column_end: line(column_end + 1)?,
        });
    }
    Some(GridAreasData {
        areas,
        row_count: u16::try_from(rows.len()).ok()?,
        column_count: u16::try_from(columns).ok()?,
    })
}

/// Cell indices are zero-based; Taffy wants 1-based grid lines.
fn line(index: usize) -> Option<u16> {
    u16::try_from(index + 1).ok()
}

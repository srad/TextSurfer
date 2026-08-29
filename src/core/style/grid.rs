use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use super::{CssCalc, CssNumber, CssPercentage};

/// Total interned grid payload, in components/tracks/areas/idents, across the whole document.
const MAX_STORED_NODES: usize = 65_536;

/// A `<length-percentage>` in a track sizing function. Cells are already resolved against the
/// render context; `calc()` stays a handle into the calculation store until the axis is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridLength {
    Cells(usize),
    Percent(CssPercentage),
    Calc(CssCalc),
}

impl GridLength {
    /// Whether this counts as a "fixed component" for the auto-repeat validity rule. `calc()` does
    /// not, matching Taffy's `is_length_or_percentage`, which is what decides whether a track list
    /// containing `repeat(auto-fill, …)` is usable at all.
    const fn is_fixed(self) -> bool {
        matches!(self, Self::Cells(_) | Self::Percent(_))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TrackBreadthMin {
    #[default]
    Auto,
    MinContent,
    MaxContent,
    Length(GridLength),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TrackBreadthMax {
    #[default]
    Auto,
    MinContent,
    MaxContent,
    Length(GridLength),
    Fr(CssNumber),
    /// Only reachable from a standalone `fit-content()` track size, never from inside `minmax()`.
    FitContent(GridLength),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TrackSize {
    pub min: TrackBreadthMin,
    pub max: TrackBreadthMax,
}

impl TrackSize {
    pub const AUTO: Self = Self {
        min: TrackBreadthMin::Auto,
        max: TrackBreadthMax::Auto,
    };

    pub const fn breadth(min: TrackBreadthMin, max: TrackBreadthMax) -> Self {
        Self { min, max }
    }

    /// Whether either breadth is a definite length or percentage. A track list containing an
    /// auto-repeat is invalid unless every one of its tracks satisfies this.
    pub const fn has_fixed_component(self) -> bool {
        matches!(self.min, TrackBreadthMin::Length(value) if value.is_fixed())
            || matches!(self.max, TrackBreadthMax::Length(value) if value.is_fixed())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RepeatCount {
    Count(u16),
    AutoFill,
    AutoFit,
}

impl RepeatCount {
    pub const fn is_auto(self) -> bool {
        matches!(self, Self::AutoFill | Self::AutoFit)
    }
}

/// One `repeat()` clause.
///
/// `line_names` always holds exactly `tracks.len() + 1` sets — one per line of a single
/// repetition, including both edges, padded with empty sets where the author named nothing. Taffy
/// asserts on any other non-zero length, so the invariant is maintained at construction rather
/// than checked at use.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GridRepeat {
    pub count: RepeatCount,
    pub tracks: Vec<TrackSize>,
    pub line_names: Vec<Vec<GridIdent>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GridTemplateComponent {
    Single(TrackSize),
    Repeat(GridRepeat),
}

/// A `grid-template-rows` / `grid-template-columns` value.
///
/// `line_names` holds exactly `components.len() + 1` sets. Taffy walks this vector to drive its
/// own component iterator, so a shorter one silently misnumbers every line past the gap.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct GridTemplateData {
    pub components: Vec<GridTemplateComponent>,
    pub line_names: Vec<Vec<GridIdent>>,
}

impl GridTemplateData {
    /// A track list may hold at most one auto-repeat, and only if every one of its tracks has a
    /// fixed component.
    ///
    /// Taffy discards such a list wholesale at layout time, so both cascades reject it up front
    /// instead, which leaves the previously cascaded value in place. Note this is stricter than
    /// CSS: `<fixed-breadth>` is a `<length-percentage>` and so includes `calc()`, which
    /// [`GridLength::is_fixed`] excludes because Taffy's fixed-component contract cannot carry it.
    pub(crate) fn is_valid(&self) -> bool {
        let auto_repeats = self
            .components
            .iter()
            .filter(|component| match component {
                GridTemplateComponent::Single(_) => false,
                GridTemplateComponent::Repeat(repeat) => repeat.count.is_auto(),
            })
            .count();
        match auto_repeats {
            0 => true,
            1 => self.components.iter().all(|component| match component {
                GridTemplateComponent::Single(track) => track.has_fixed_component(),
                GridTemplateComponent::Repeat(repeat) => repeat
                    .tracks
                    .iter()
                    .all(|track| track.has_fixed_component()),
            }),
            _ => false,
        }
    }
}

/// One named area of a `grid-template-areas` value, in 1-based grid line coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GridArea {
    pub name: GridIdent,
    pub row_start: u16,
    pub row_end: u16,
    pub column_start: u16,
    pub column_end: u16,
}

/// The template may be larger than its named areas, because null cells (`.`) name nothing, so the
/// extent is carried separately.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct GridAreasData {
    pub areas: Vec<GridArea>,
    pub row_count: u16,
    pub column_count: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GridPlacement {
    #[default]
    Auto,
    /// A 1-based grid line, negative counting back from the end. Never zero.
    Line(i16),
    NamedLine(GridIdent, i16),
    /// A span of at least one track.
    Span(u16),
    NamedSpan(GridIdent, u16),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GridLines {
    pub start: GridPlacement,
    pub end: GridPlacement,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GridAutoFlow {
    #[default]
    Row,
    Column,
    RowDense,
    ColumnDense,
}

/// The grid properties that only a grid container or grid item reads. Box alignment and the gap
/// properties live on [`super::AlignmentStyle`], because the flex formatting context shares them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GridStyle {
    pub template_columns: Option<GridTemplate>,
    pub template_rows: Option<GridTemplate>,
    pub template_areas: Option<GridAreas>,
    pub auto_columns: Option<GridTracks>,
    pub auto_rows: Option<GridTracks>,
    pub auto_flow: GridAutoFlow,
    pub row: GridLines,
    pub column: GridLines,
}

macro_rules! handle {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(u32);
    };
}

handle!(GridTemplate);
handle!(GridTracks);
handle!(GridAreas);
handle!(GridIdent);

/// A content-addressed table: equal payloads share one handle, so a rule that applies the same
/// track list to a thousand elements stores it once.
#[derive(Clone, Debug)]
struct Table<T> {
    values: Vec<Arc<T>>,
    ids: HashMap<Arc<T>, u32>,
}

impl<T> Default for Table<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            ids: HashMap::new(),
        }
    }
}

/// `ids` is an index derived from `values`, so equality is decided by the values alone. A derive
/// would also demand `T: Hash` of every caller that only wants to compare two style trees.
impl<T: PartialEq> PartialEq for Table<T> {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

impl<T: Eq> Eq for Table<T> {}

impl<T: Eq + Hash> Table<T> {
    fn id(&self, value: &T) -> Option<u32> {
        self.ids.get(value).copied()
    }

    fn insert(&mut self, value: T) -> Option<u32> {
        if let Some(id) = self.ids.get(&value) {
            return Some(*id);
        }
        let id = u32::try_from(self.values.len()).ok()?;
        let value = Arc::new(value);
        self.values.push(value.clone());
        self.ids.insert(value, id);
        Some(id)
    }

    fn get(&self, id: u32) -> Option<&T> {
        self.values.get(id as usize).map(Arc::as_ref)
    }

    fn rollback(&mut self, length: usize) {
        while self.values.len() > length {
            if let Some(value) = self.values.pop() {
                self.ids.remove(value.as_ref());
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GridStore {
    templates: Table<GridTemplateData>,
    tracks: Table<Vec<TrackSize>>,
    areas: Table<GridAreasData>,
    idents: Table<String>,
    nodes: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct GridStoreCheckpoint {
    templates: usize,
    tracks: usize,
    areas: usize,
    idents: usize,
    nodes: usize,
}

impl GridStore {
    pub(crate) fn checkpoint(&self) -> GridStoreCheckpoint {
        GridStoreCheckpoint {
            templates: self.templates.values.len(),
            tracks: self.tracks.values.len(),
            areas: self.areas.values.len(),
            idents: self.idents.values.len(),
            nodes: self.nodes,
        }
    }

    pub(crate) fn rollback(&mut self, checkpoint: GridStoreCheckpoint) {
        self.templates.rollback(checkpoint.templates);
        self.tracks.rollback(checkpoint.tracks);
        self.areas.rollback(checkpoint.areas);
        self.idents.rollback(checkpoint.idents);
        self.nodes = checkpoint.nodes;
    }

    pub(crate) fn insert_ident(&mut self, name: &str) -> Option<GridIdent> {
        if let Some(id) = self.idents.id(&name.to_owned()) {
            return Some(GridIdent(id));
        }
        self.charge(1)?;
        self.idents.insert(name.to_owned()).map(GridIdent)
    }

    pub(crate) fn ident(&self, handle: GridIdent) -> Option<&str> {
        self.idents.get(handle.0).map(String::as_str)
    }

    pub(crate) fn insert_template(&mut self, value: GridTemplateData) -> Option<GridTemplate> {
        if let Some(id) = self.templates.id(&value) {
            return Some(GridTemplate(id));
        }
        let outer_names =
            value.line_names.len() + value.line_names.iter().map(Vec::len).sum::<usize>();
        let repeated = value
            .components
            .iter()
            .map(|component| match component {
                GridTemplateComponent::Single(_) => 1,
                GridTemplateComponent::Repeat(repeat) => {
                    1 + repeat.tracks.len()
                        + repeat.line_names.len()
                        + repeat.line_names.iter().map(Vec::len).sum::<usize>()
                }
            })
            .sum::<usize>();
        let cost = outer_names.saturating_add(repeated);
        self.charge(cost)?;
        self.templates.insert(value).map(GridTemplate)
    }

    pub(crate) fn template(&self, handle: GridTemplate) -> Option<&GridTemplateData> {
        self.templates.get(handle.0)
    }

    pub(crate) fn insert_tracks(&mut self, value: Vec<TrackSize>) -> Option<GridTracks> {
        if let Some(id) = self.tracks.id(&value) {
            return Some(GridTracks(id));
        }
        self.charge(value.len())?;
        self.tracks.insert(value).map(GridTracks)
    }

    pub(crate) fn tracks(&self, handle: GridTracks) -> Option<&[TrackSize]> {
        self.tracks.get(handle.0).map(Vec::as_slice)
    }

    pub(crate) fn insert_areas(&mut self, value: GridAreasData) -> Option<GridAreas> {
        if let Some(id) = self.areas.id(&value) {
            return Some(GridAreas(id));
        }
        self.charge(value.areas.len())?;
        self.areas.insert(value).map(GridAreas)
    }

    pub(crate) fn areas(&self, handle: GridAreas) -> Option<&GridAreasData> {
        self.areas.get(handle.0)
    }

    #[cfg(test)]
    pub(crate) fn stored_nodes(&self) -> usize {
        self.nodes
    }

    /// Spend from the shared budget. A declaration whose payload does not fit is refused, which
    /// the cascade turns into an invalid declaration, exactly as an over-long `calc()` is.
    fn charge(&mut self, nodes: usize) -> Option<()> {
        let total = self.nodes.saturating_add(nodes);
        if total > MAX_STORED_NODES {
            return None;
        }
        self.nodes = total;
        Some(())
    }
}

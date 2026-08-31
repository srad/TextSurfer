#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

impl Visibility {
    pub const fn parse(keyword: &str) -> Option<Self> {
        Some(match keyword.as_bytes() {
            b"visible" => Self::Visible,
            b"hidden" => Self::Hidden,
            b"collapse" => Self::Collapse,
            _ => return None,
        })
    }

    pub const fn is_hidden(self) -> bool {
        !matches!(self, Self::Visible)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayOutside {
    Block,
    Inline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayInside {
    Flow,
    FlowRoot,
    Table,
    Flex,
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayBox {
    None,
    Contents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayInternal {
    TableHeaderGroup,
    TableRowGroup,
    TableFooterGroup,
    TableRow,
    TableCell,
    TableColumn,
    TableColumnGroup,
    TableCaption,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayMode {
    outside: DisplayOutside,
    inside: DisplayInside,
    list_item: bool,
}

impl DisplayMode {
    pub const fn outside(self) -> DisplayOutside {
        self.outside
    }

    pub const fn inside(self) -> DisplayInside {
        self.inside
    }

    pub const fn is_list_item(self) -> bool {
        self.list_item
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    OutsideInside(DisplayMode),
    Box(DisplayBox),
    Internal(DisplayInternal),
}

impl Default for Display {
    fn default() -> Self {
        Self::INLINE
    }
}

impl Display {
    pub const NONE: Self = Self::Box(DisplayBox::None);
    pub const CONTENTS: Self = Self::Box(DisplayBox::Contents);
    pub const INLINE: Self = Self::flow(DisplayOutside::Inline, false, false);
    pub const BLOCK: Self = Self::flow(DisplayOutside::Block, false, false);
    pub const FLOW_ROOT: Self = Self::flow(DisplayOutside::Block, true, false);
    pub const INLINE_BLOCK: Self = Self::flow(DisplayOutside::Inline, true, false);
    pub const LIST_ITEM: Self = Self::flow(DisplayOutside::Block, false, true);
    pub const TABLE: Self = Self::table(DisplayOutside::Block);
    pub const INLINE_TABLE: Self = Self::table(DisplayOutside::Inline);
    pub const TABLE_HEADER_GROUP: Self = Self::Internal(DisplayInternal::TableHeaderGroup);
    pub const TABLE_ROW_GROUP: Self = Self::Internal(DisplayInternal::TableRowGroup);
    pub const TABLE_FOOTER_GROUP: Self = Self::Internal(DisplayInternal::TableFooterGroup);
    pub const TABLE_ROW: Self = Self::Internal(DisplayInternal::TableRow);
    pub const TABLE_CELL: Self = Self::Internal(DisplayInternal::TableCell);
    pub const TABLE_COLUMN: Self = Self::Internal(DisplayInternal::TableColumn);
    pub const TABLE_COLUMN_GROUP: Self = Self::Internal(DisplayInternal::TableColumnGroup);
    pub const TABLE_CAPTION: Self = Self::Internal(DisplayInternal::TableCaption);

    pub const fn flow(outside: DisplayOutside, flow_root: bool, list_item: bool) -> Self {
        Self::OutsideInside(DisplayMode {
            outside,
            inside: if flow_root {
                DisplayInside::FlowRoot
            } else {
                DisplayInside::Flow
            },
            list_item,
        })
    }

    pub const fn table(outside: DisplayOutside) -> Self {
        Self::OutsideInside(DisplayMode {
            outside,
            inside: DisplayInside::Table,
            list_item: false,
        })
    }

    pub const fn flex(outside: DisplayOutside) -> Self {
        Self::OutsideInside(DisplayMode {
            outside,
            inside: DisplayInside::Flex,
            list_item: false,
        })
    }

    pub const fn grid(outside: DisplayOutside) -> Self {
        Self::OutsideInside(DisplayMode {
            outside,
            inside: DisplayInside::Grid,
            list_item: false,
        })
    }

    pub const fn outside(self) -> Option<DisplayOutside> {
        match self {
            Self::OutsideInside(mode) => Some(mode.outside),
            Self::Box(_) | Self::Internal(_) => None,
        }
    }

    pub const fn inside(self) -> Option<DisplayInside> {
        match self {
            Self::OutsideInside(mode) => Some(mode.inside),
            Self::Box(_) | Self::Internal(_) => None,
        }
    }

    pub const fn is_none(self) -> bool {
        matches!(self, Self::Box(DisplayBox::None))
    }

    pub const fn is_contents(self) -> bool {
        matches!(self, Self::Box(DisplayBox::Contents))
    }

    pub const fn is_list_item(self) -> bool {
        matches!(self, Self::OutsideInside(mode) if mode.list_item)
    }

    pub const fn is_inline_level(self) -> bool {
        matches!(self.outside(), Some(DisplayOutside::Inline))
    }

    pub const fn is_block_level(self) -> bool {
        matches!(self.outside(), Some(DisplayOutside::Block))
    }

    pub const fn is_block_container(self) -> bool {
        matches!(
            self,
            Self::OutsideInside(DisplayMode {
                outside: DisplayOutside::Block,
                inside: DisplayInside::Flow
                    | DisplayInside::FlowRoot
                    | DisplayInside::Flex
                    | DisplayInside::Grid,
                ..
            })
        )
    }

    pub const fn is_inline_flow(self) -> bool {
        matches!(
            self,
            Self::OutsideInside(DisplayMode {
                outside: DisplayOutside::Inline,
                inside: DisplayInside::Flow,
                ..
            })
        )
    }

    pub const fn is_atomic_inline(self) -> bool {
        matches!(
            self,
            Self::OutsideInside(DisplayMode {
                outside: DisplayOutside::Inline,
                inside: DisplayInside::FlowRoot
                    | DisplayInside::Table
                    | DisplayInside::Flex
                    | DisplayInside::Grid,
                ..
            })
        )
    }

    pub const fn is_table(self) -> bool {
        matches!(
            self,
            Self::OutsideInside(DisplayMode {
                inside: DisplayInside::Table,
                ..
            })
        )
    }

    pub const fn is_table_internal(self) -> bool {
        matches!(self, Self::Internal(_))
    }

    pub const fn blockify(self) -> Self {
        match self {
            Self::OutsideInside(mode) => Self::OutsideInside(DisplayMode {
                outside: DisplayOutside::Block,
                inside: mode.inside,
                list_item: mode.list_item,
            }),
            Self::Internal(_) => Self::BLOCK,
            Self::Box(_) => self,
        }
    }
}

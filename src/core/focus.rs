#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Focus {
    Menu,
    Tabs,
    #[default]
    Content,
    Address,
}

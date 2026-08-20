#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Focus {
    Tabs,
    #[default]
    Content,
    Address,
}

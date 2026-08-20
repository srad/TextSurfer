#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Capabilities {
    pub executes_scripts: bool,
    pub async_host_ops: bool,
}

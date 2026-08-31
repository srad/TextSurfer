use std::rc::Rc;

use crate::core::dom::NodeId;
use crate::script::Capabilities;
use crate::script::host::{HostCompletion, JsHost};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptTask {
    pub source: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsEvent {
    Click { target: NodeId },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptReport {
    pub error: Option<String>,
    pub jobs_run: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventOutcome {
    pub canceled: bool,
}

pub trait JsEngine {
    fn capabilities(&self) -> Capabilities;
    fn run_script(&mut self, task: &ScriptTask) -> ScriptReport;
    fn run_job_pump(&mut self, budget: u32) -> ScriptReport;
    fn dispatch_event(&mut self, event: &JsEvent) -> EventOutcome;
    fn complete_host(&mut self, completion: HostCompletion) -> ScriptReport;
}

pub trait JsEngineFactory: Send + Sync {
    fn create(&self, host: Rc<dyn JsHost>) -> Result<Box<dyn JsEngine>, String>;
}

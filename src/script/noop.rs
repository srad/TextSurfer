use std::rc::Rc;

use crate::script::host::{HostCompletion, JsHost};
use crate::script::{
    Capabilities, EventOutcome, JsEngine, JsEngineFactory, JsEvent, ScriptReport, ScriptTask,
};

pub struct NoopEngine;
pub struct NoopEngineFactory;

impl JsEngine for NoopEngine {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn run_script(&mut self, _task: &ScriptTask) -> ScriptReport {
        ScriptReport::default()
    }

    fn run_job_pump(&mut self, _budget: u32) -> ScriptReport {
        ScriptReport::default()
    }

    fn dispatch_event(&mut self, _event: &JsEvent) -> EventOutcome {
        EventOutcome::default()
    }

    fn complete_host(&mut self, _completion: HostCompletion) -> ScriptReport {
        ScriptReport::default()
    }
}

impl JsEngineFactory for NoopEngineFactory {
    fn create(&self, _host: Rc<dyn JsHost>) -> Result<Box<dyn JsEngine>, String> {
        Ok(Box::new(NoopEngine))
    }
}

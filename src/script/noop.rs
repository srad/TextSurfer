use crate::script::host::{JsEvent, JsHost};
use crate::script::{Capabilities, JsEngine};

pub struct NoopEngine;

impl JsEngine for NoopEngine {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn run_script(&mut self, _source: &str, _host: &mut dyn JsHost) {}

    fn run_job_pump(&mut self, _budget: u32, _host: &mut dyn JsHost) -> u32 {
        0
    }

    fn dispatch_event(&mut self, _event: &JsEvent, _host: &mut dyn JsHost) {}
}

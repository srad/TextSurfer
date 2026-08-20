use crate::script::Capabilities;
use crate::script::host::{JsEvent, JsHost};

pub trait JsEngine: Send {
    fn capabilities(&self) -> Capabilities;
    fn run_script(&mut self, source: &str, host: &mut dyn JsHost);
    fn run_job_pump(&mut self, budget: u32, host: &mut dyn JsHost) -> u32;
    fn dispatch_event(&mut self, event: &JsEvent, host: &mut dyn JsHost);
}

use crate::core::dom::{Document, ElementNs};
use crate::script::host::{JsEvent, JsHost};
use crate::script::{Capabilities, JsEngine, MutateOp};

pub struct RecordingHost {
    pub logs: Vec<String>,
    pub mutations: Vec<MutateOp>,
}

impl JsHost for RecordingHost {
    fn apply_mutation(&mut self, op: MutateOp) {
        self.mutations.push(op);
    }

    fn log(&mut self, message: &str) {
        self.logs.push(message.to_string());
    }
}

pub fn assert_capability_report(engine: &dyn JsEngine, expected: Capabilities) {
    let actual = engine.capabilities();
    assert_eq!(
        actual, expected,
        "engine capability report must match its contract tier"
    );
}

pub fn run_inert_suite<E: JsEngine>(engine: &mut E, host: &mut RecordingHost) {
    let caps = engine.capabilities();
    assert!(
        !caps.executes_scripts,
        "inert engines must not execute scripts"
    );
    assert!(
        !caps.async_host_ops,
        "inert engines must not run async host ops"
    );
    engine.run_script("1 + 1;", host);
    engine.run_script("", host);
    assert_eq!(
        engine.run_job_pump(256, host),
        0,
        "inert engines perform no steps"
    );
    let mut document = Document::new();
    let target = document.insert_element(None, "button", ElementNs::Html, vec![]);
    engine.dispatch_event(&JsEvent::Click { target }, host);
    assert!(
        host.logs.is_empty() && host.mutations.is_empty(),
        "inert engines must leave the host untouched"
    );
}

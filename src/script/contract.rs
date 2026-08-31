use std::cell::{Cell, RefCell};

use crate::core::dom::{Document, ElementNs};
use crate::script::{
    Capabilities, DomQuery, DomValue, HostEffect, HostError, HostOpId, HostRequest, JsEngine,
    JsEvent, JsHost, MutateOp, MutateResult, ScriptTask,
};

#[derive(Default)]
pub struct RecordingHost {
    pub effects: RefCell<Vec<HostEffect>>,
    pub mutations: RefCell<Vec<MutateOp>>,
    pub requests: RefCell<Vec<(HostOpId, HostRequest)>>,
    next_operation: Cell<u64>,
}

impl JsHost for RecordingHost {
    fn query(&self, _query: DomQuery) -> Result<DomValue, HostError> {
        Ok(DomValue::Node(None))
    }

    fn mutate(&self, op: MutateOp) -> Result<MutateResult, HostError> {
        self.mutations.borrow_mut().push(op);
        Ok(MutateResult::None)
    }

    fn emit(&self, effect: HostEffect) -> Result<(), HostError> {
        self.effects.borrow_mut().push(effect);
        Ok(())
    }

    fn request(&self, request: HostRequest) -> Result<HostOpId, HostError> {
        let id = HostOpId(self.next_operation.get().saturating_add(1));
        self.next_operation.set(id.0);
        self.requests.borrow_mut().push((id, request));
        Ok(id)
    }
}

#[cfg(feature = "js")]
pub fn run_executing_suite(engine: &mut dyn JsEngine, host: &RecordingHost) {
    let caps = engine.capabilities();
    assert!(caps.executes_scripts);
    assert!(caps.async_host_ops);
    let report = engine.run_script(&ScriptTask {
        source: "document.title = 'contract title'; console.log('contract log'); Promise.resolve().then(() => console.log('contract job')); fetch('/contract'); setTimeout(() => console.log('contract timer'), 1);".to_string(),
        url: "about:contract".to_string(),
    });
    assert_eq!(report.error, None);
    let jobs = engine.run_job_pump(256);
    assert_eq!(jobs.error, None);
    assert!(jobs.jobs_run > 0);
    assert!(
        host.mutations
            .borrow()
            .contains(&MutateOp::SetTitle("contract title".to_string()))
    );
    assert!(
        host.effects
            .borrow()
            .contains(&HostEffect::Log("contract log".to_string()))
    );
    assert!(
        host.effects
            .borrow()
            .contains(&HostEffect::Log("contract job".to_string()))
    );
    assert!(host.requests.borrow().iter().any(|(_, request)| matches!(
        request,
        HostRequest::Fetch { url } if url == "/contract"
    )));
    assert!(host.requests.borrow().iter().any(|(_, request)| matches!(
        request,
        HostRequest::Timer {
            delay_ms: 1,
            repeat: false
        }
    )));
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
    engine.run_script(&ScriptTask {
        source: "1 + 1;".to_string(),
        url: "about:test".to_string(),
    });
    engine.run_script(&ScriptTask {
        source: String::new(),
        url: "about:test".to_string(),
    });
    assert_eq!(
        engine.run_job_pump(256).jobs_run,
        0,
        "inert engines perform no steps"
    );
    let mut document = Document::new();
    let target = document.insert_element(None, "button", ElementNs::Html, vec![]);
    engine.dispatch_event(&JsEvent::Click { target });
    assert!(
        host.effects.borrow().is_empty() && host.mutations.borrow().is_empty(),
        "inert engines must leave the host untouched"
    );
}

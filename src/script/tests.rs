use crate::script::Capabilities;
use crate::script::contract;
use crate::script::noop::NoopEngine;

#[test]
fn noop_engine_reports_the_inert_capabilities() {
    let engine = NoopEngine;
    contract::assert_capability_report(&engine, Capabilities::default());
}

#[cfg(feature = "js")]
#[test]
fn boa_executes_scripts_and_pumps_promise_jobs() {
    use std::rc::Rc;

    use crate::script::{HostEffect, JsEngineFactory, ScriptTask};

    let host = Rc::new(contract::RecordingHost::default());
    let engine_host: Rc<dyn crate::script::JsHost> = host.clone();
    let mut engine = crate::script::BoaEngineFactory.create(engine_host).unwrap();
    let report = engine.run_script(&ScriptTask {
        source: "console.log('sync'); Promise.resolve().then(() => console.log('async'));"
            .to_string(),
        url: "about:test".to_string(),
    });
    assert_eq!(report.error, None);
    assert_eq!(engine.run_job_pump(1).jobs_run, 1);
    assert_eq!(
        *host.effects.borrow(),
        vec![
            HostEffect::Log("sync".to_string()),
            HostEffect::Log("async".to_string())
        ]
    );
}

#[cfg(feature = "js")]
#[test]
fn boa_passes_the_executing_contract_suite() {
    use std::rc::Rc;

    use crate::script::JsEngineFactory;

    let host = Rc::new(contract::RecordingHost::default());
    let engine_host: Rc<dyn crate::script::JsHost> = host.clone();
    let mut engine = crate::script::BoaEngineFactory.create(engine_host).unwrap();
    contract::run_executing_suite(engine.as_mut(), &host);
}

#[cfg(feature = "js")]
#[test]
fn boa_promise_work_is_sliced_at_the_requested_budget() {
    use std::rc::Rc;

    use crate::script::{JsEngineFactory, ScriptTask};

    let host = Rc::new(contract::RecordingHost::default());
    let mut engine = crate::script::BoaEngineFactory.create(host).unwrap();
    let report = engine.run_script(&ScriptTask {
        source: "let work = Promise.resolve(); for (let index = 0; index < 600; index++) work = work.then(() => {});".to_string(),
        url: "about:responsiveness".to_string(),
    });
    assert_eq!(report.error, None);
    assert_eq!(engine.run_job_pump(256).jobs_run, 256);
    assert_eq!(engine.run_job_pump(256).jobs_run, 256);
    assert!(engine.run_job_pump(256).jobs_run < 256);
}

#[test]
fn noop_engine_passes_the_inert_contract_suite() {
    let mut engine = NoopEngine;
    let mut host = contract::RecordingHost::default();
    contract::run_inert_suite(&mut engine, &mut host);
}

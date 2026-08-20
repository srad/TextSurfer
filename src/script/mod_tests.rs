use crate::script::Capabilities;
use crate::script::contract;
use crate::script::noop::NoopEngine;

#[test]
fn noop_engine_reports_the_inert_capabilities() {
    let engine = NoopEngine;
    contract::assert_capability_report(&engine, Capabilities::default());
}

#[test]
fn noop_engine_passes_the_inert_contract_suite() {
    let mut engine = NoopEngine;
    let mut host = contract::RecordingHost {
        logs: Vec::new(),
        mutations: Vec::new(),
    };
    contract::run_inert_suite(&mut engine, &mut host);
}

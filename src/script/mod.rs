#[cfg(feature = "js")]
mod boa;
pub mod capabilities;
pub mod engine;
pub mod host;
pub mod noop;

#[cfg(test)]
mod contract;
#[cfg(test)]
mod tests;

#[cfg(feature = "js")]
pub use boa::BoaEngineFactory;
pub use capabilities::Capabilities;
pub use engine::{EventOutcome, JsEngine, JsEngineFactory, JsEvent, ScriptReport, ScriptTask};
pub use host::{
    DomQuery, DomValue, HostCompletion, HostEffect, HostError, HostOpId, HostRequest, HostResponse,
    JsHost, MutateOp, MutateResult, MutationImpact, NavigationKind,
};
pub use noop::{NoopEngine, NoopEngineFactory};

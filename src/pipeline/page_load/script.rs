use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use url::Url;

use crate::core::dom::{Document, ElementNs, Node, NodeId, SharedDocument};
use crate::script::{
    DomQuery, DomValue, EventOutcome, HostCompletion, HostEffect, HostError, HostOpId, HostRequest,
    JsEngine, JsEngineFactory, JsEvent, JsHost, MutateOp, MutateResult, MutationImpact,
    NavigationKind, ScriptReport, ScriptTask,
};

const MAX_MUTATIONS_PER_TURN: usize = 1_024;
const MAX_HOST_STRING_BYTES: usize = 1024 * 1024;
const MAX_SCRIPT_CREATED_NODES: usize = 100_000;
const MAX_MESSAGES_PER_TURN: usize = 64;

#[derive(Default)]
pub(super) struct BrowserEffects {
    pub(super) impact: MutationImpact,
    pub(super) messages: Vec<String>,
    pub(super) navigation: Option<(String, NavigationKind)>,
    pub(super) requests: Vec<(HostOpId, HostRequest)>,
}

struct BrowserHost {
    document: SharedDocument,
    location: String,
    effects: RefCell<BrowserEffects>,
    mutations: Cell<usize>,
    next_operation: Cell<u64>,
}

impl BrowserHost {
    fn new(document: SharedDocument, location: String) -> Self {
        Self {
            document,
            location,
            effects: RefCell::new(BrowserEffects::default()),
            mutations: Cell::new(0),
            next_operation: Cell::new(1),
        }
    }

    fn take_effects(&self) -> BrowserEffects {
        self.mutations.set(0);
        std::mem::take(&mut *self.effects.borrow_mut())
    }

    fn check_string(value: &str) -> Result<(), HostError> {
        if value.len() > MAX_HOST_STRING_BYTES {
            Err(HostError::Limit)
        } else {
            Ok(())
        }
    }

    fn begin_mutation(&self) -> Result<(), HostError> {
        let mutations = self.mutations.get();
        if mutations >= MAX_MUTATIONS_PER_TURN {
            return Err(HostError::Limit);
        }
        self.mutations.set(mutations + 1);
        Ok(())
    }

    fn mark_render(&self) {
        self.effects.borrow_mut().impact.render = true;
    }

    fn title(document: &Document) -> String {
        document
            .element_by_name("title")
            .and_then(|node| document.text_content(node))
            .unwrap_or_default()
    }

    fn selector(document: &Document, selector: &str) -> Result<Option<NodeId>, HostError> {
        let selector = selector.trim();
        if selector.is_empty()
            || selector
                .chars()
                .any(|character| character.is_ascii_whitespace() || ">+~,:[]".contains(character))
        {
            return Err(HostError::Dom("unsupported selector".to_string()));
        }
        Ok(if let Some(id) = selector.strip_prefix('#') {
            document.element_by_id(id)
        } else if let Some(class) = selector.strip_prefix('.') {
            document.element_by_class(class)
        } else {
            document.element_by_name(selector)
        })
    }
}

impl JsHost for BrowserHost {
    fn query(&self, query: DomQuery) -> Result<DomValue, HostError> {
        let document = self
            .document
            .try_borrow()
            .map_err(|_| HostError::BorrowConflict)?;
        Ok(match query {
            DomQuery::DocumentElement => DomValue::Node(document.element_by_name("html")),
            DomQuery::Body => DomValue::Node(document.element_by_name("body")),
            DomQuery::ById(id) => DomValue::Node(document.element_by_id(&id)),
            DomQuery::BySelector(selector) => DomValue::Node(Self::selector(&document, &selector)?),
            DomQuery::Parent(node) => DomValue::Node(document.parent(node)),
            DomQuery::TextContent(node) => DomValue::Text(document.text_content(node)),
            DomQuery::Attribute(node, name) => {
                DomValue::Text(document.attribute(node, &name).map(str::to_string))
            }
            DomQuery::Title => DomValue::Text(Some(Self::title(&document))),
            DomQuery::Location => DomValue::Text(Some(self.location.clone())),
        })
    }

    fn mutate(&self, op: MutateOp) -> Result<MutateResult, HostError> {
        self.begin_mutation()?;
        let mut document = self
            .document
            .try_borrow_mut()
            .map_err(|_| HostError::BorrowConflict)?;
        let result = match op {
            MutateOp::CreateElement(name) => {
                Self::check_string(&name)?;
                if name.is_empty()
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                    || document.node_count() >= MAX_SCRIPT_CREATED_NODES
                {
                    return Err(HostError::Limit);
                }
                MutateResult::Node(document.create_element(&name))
            }
            MutateOp::SetTextContent { node, value } => {
                Self::check_string(&value)?;
                document.set_text_content(node, value).map_err(dom_error)?;
                self.mark_render();
                MutateResult::None
            }
            MutateOp::SetAttribute { node, name, value } => {
                Self::check_string(&name)?;
                Self::check_string(&value)?;
                document
                    .set_attribute(node, &name, value)
                    .map_err(dom_error)?;
                self.mark_render();
                MutateResult::None
            }
            MutateOp::RemoveAttribute { node, name } => {
                Self::check_string(&name)?;
                let removed = document.remove_attribute(node, &name).map_err(dom_error)?;
                if removed {
                    self.mark_render();
                }
                MutateResult::Removed(removed)
            }
            MutateOp::AppendChild { parent, child } => {
                document.attach(child, Some(parent)).map_err(dom_error)?;
                self.mark_render();
                MutateResult::Node(child)
            }
            MutateOp::InsertBefore {
                parent,
                child,
                sibling,
            } => {
                match sibling {
                    Some(sibling) if document.parent(sibling) == Some(parent) => {
                        document.attach_before(child, sibling).map_err(dom_error)?;
                    }
                    Some(_) => {
                        return Err(HostError::Dom(
                            "reference node is not a child of the parent".to_string(),
                        ));
                    }
                    None => document.attach(child, Some(parent)).map_err(dom_error)?,
                }
                self.mark_render();
                MutateResult::Node(child)
            }
            MutateOp::Remove(node) => {
                let removed = document.remove_node(node).map_err(dom_error)?;
                if removed {
                    self.mark_render();
                }
                MutateResult::Removed(removed)
            }
            MutateOp::SetTitle(value) => {
                Self::check_string(&value)?;
                let title = if let Some(title) = document.element_by_name("title") {
                    title
                } else {
                    let parent = document
                        .element_by_name("head")
                        .or_else(|| document.element_by_name("html"))
                        .ok_or_else(|| {
                            HostError::Dom("document has no root element".to_string())
                        })?;
                    let title = document.create_element("title");
                    document.attach(title, Some(parent)).map_err(dom_error)?;
                    title
                };
                document.set_text_content(title, value).map_err(dom_error)?;
                let mut effects = self.effects.borrow_mut();
                effects.impact.render = true;
                effects.impact.title = true;
                MutateResult::None
            }
        };
        Ok(result)
    }

    fn emit(&self, effect: HostEffect) -> Result<(), HostError> {
        let mut effects = self.effects.borrow_mut();
        match effect {
            HostEffect::Log(message) | HostEffect::Alert(message) => {
                Self::check_string(&message)?;
                if effects.messages.len() >= MAX_MESSAGES_PER_TURN {
                    return Err(HostError::Limit);
                }
                effects.messages.push(message);
            }
            HostEffect::Navigate { target, kind } => {
                Self::check_string(&target)?;
                effects.navigation = Some((target, kind));
            }
        }
        Ok(())
    }

    fn request(&self, request: HostRequest) -> Result<HostOpId, HostError> {
        let next = self.next_operation.get();
        let id = HostOpId(next);
        self.next_operation.set(next.saturating_add(1));
        self.effects.borrow_mut().requests.push((id, request));
        Ok(id)
    }
}

pub(super) struct ScriptAdvance {
    pub(super) report: ScriptReport,
    pub(super) effects: BrowserEffects,
}

pub(super) struct ScriptController {
    engine: Box<dyn JsEngine>,
    host: Rc<BrowserHost>,
    tasks: VecDeque<PendingScript>,
    pump_again: bool,
}

enum PendingScript {
    Ready(ScriptTask),
    External {
        index: usize,
        url: Url,
        task: Option<ScriptTask>,
        failed: bool,
    },
}

impl ScriptController {
    pub(super) fn new(
        document: SharedDocument,
        document_url: &Url,
        base_url: &Url,
        factory: Arc<dyn JsEngineFactory>,
    ) -> Result<Self, String> {
        let host = Rc::new(BrowserHost::new(document.clone(), document_url.to_string()));
        let engine_host: Rc<dyn JsHost> = host.clone();
        let engine = factory.create(engine_host)?;
        let tasks = discover_scripts(&document.borrow(), document_url, base_url);
        Ok(Self {
            engine,
            host,
            tasks,
            pump_again: false,
        })
    }

    pub(super) fn is_idle(&self) -> bool {
        self.tasks.is_empty()
    }

    pub(super) fn is_runnable(&self) -> bool {
        self.pump_again
            || matches!(
                self.tasks.front(),
                Some(PendingScript::Ready(_))
                    | Some(PendingScript::External { task: Some(_), .. })
                    | Some(PendingScript::External { failed: true, .. })
            )
    }

    pub(super) fn external_requests(&self) -> Vec<(usize, Url)> {
        self.tasks
            .iter()
            .filter_map(|task| match task {
                PendingScript::External { index, url, .. } => Some((*index, url.clone())),
                PendingScript::Ready(_) => None,
            })
            .collect()
    }

    pub(super) fn deliver_external(
        &mut self,
        index: usize,
        source: Option<String>,
        final_url: String,
    ) -> bool {
        let Some(PendingScript::External { task, failed, .. }) = self.tasks.iter_mut().find(|task| {
            matches!(task, PendingScript::External { index: candidate, .. } if *candidate == index)
        }) else {
            return false;
        };
        if task.is_some() || *failed {
            return false;
        }
        match source {
            Some(source) => {
                *task = Some(ScriptTask {
                    source,
                    url: final_url,
                });
            }
            None => *failed = true,
        }
        true
    }

    pub(super) fn advance(&mut self) -> ScriptAdvance {
        let runnable = match self.tasks.front_mut() {
            Some(PendingScript::Ready(_)) => match self.tasks.pop_front() {
                Some(PendingScript::Ready(task)) => Some(task),
                _ => None,
            },
            Some(PendingScript::External { task: Some(_), .. }) => match self.tasks.pop_front() {
                Some(PendingScript::External {
                    task: Some(task), ..
                }) => Some(task),
                _ => None,
            },
            Some(PendingScript::External { failed: true, .. }) => {
                self.tasks.pop_front();
                None
            }
            Some(PendingScript::External { .. }) | None => None,
        };
        let mut report = runnable
            .as_ref()
            .map_or_else(ScriptReport::default, |task| self.engine.run_script(task));
        let jobs = self.engine.run_job_pump(256);
        self.pump_again = jobs.jobs_run == 256;
        report.jobs_run = report.jobs_run.saturating_add(jobs.jobs_run);
        if report.error.is_none() {
            report.error = jobs.error;
        }
        ScriptAdvance {
            report,
            effects: self.host.take_effects(),
        }
    }

    pub(super) fn dispatch_click(&mut self, target: NodeId) -> (EventOutcome, BrowserEffects) {
        let outcome = self.engine.dispatch_event(&JsEvent::Click { target });
        self.pump_again = true;
        (outcome, self.host.take_effects())
    }

    pub(super) fn complete(&mut self, completion: HostCompletion) -> ScriptAdvance {
        let mut report = self.engine.complete_host(completion);
        let jobs = self.engine.run_job_pump(256);
        self.pump_again = jobs.jobs_run == 256;
        report.jobs_run = report.jobs_run.saturating_add(jobs.jobs_run);
        if report.error.is_none() {
            report.error = jobs.error;
        }
        ScriptAdvance {
            report,
            effects: self.host.take_effects(),
        }
    }
}

fn discover_scripts(
    document: &Document,
    document_url: &Url,
    base_url: &Url,
) -> VecDeque<PendingScript> {
    let mut tasks = VecDeque::new();
    let mut external_index = 0;
    for root in document.roots() {
        if document.is_template_contents(*root) {
            continue;
        }
        for node in document.descendants(*root) {
            let Some(Node::Element {
                name,
                ns: ElementNs::Html,
                attrs,
            }) = document.node(node)
            else {
                continue;
            };
            if !name.eq_ignore_ascii_case("script")
                || attrs.iter().any(|attribute| {
                    attribute.name.eq_ignore_ascii_case("type")
                        && !matches!(
                            attribute.value.trim().to_ascii_lowercase().as_str(),
                            "" | "text/javascript" | "application/javascript"
                        )
                })
            {
                continue;
            }
            if let Some(source) = attrs
                .iter()
                .find(|attribute| attribute.name.eq_ignore_ascii_case("src"))
                .map(|attribute| attribute.value.trim())
                .filter(|source| !source.is_empty())
                && let Ok(url) = base_url.join(source)
            {
                let index = external_index;
                external_index += 1;
                tasks.push_back(PendingScript::External {
                    index,
                    url,
                    task: None,
                    failed: false,
                });
                continue;
            }
            let source = document.text_content(node).unwrap_or_default();
            if !source.trim().is_empty() {
                tasks.push_back(PendingScript::Ready(ScriptTask {
                    source,
                    url: document_url.to_string(),
                }));
            }
        }
    }
    tasks
}

fn dom_error(error: impl ToString) -> HostError {
    HostError::Dom(error.to_string())
}

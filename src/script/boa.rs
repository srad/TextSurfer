use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use boa_engine::job::{Job, JobExecutor};
use boa_engine::{Context, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source};

use crate::core::dom::NodeId;

use super::{
    Capabilities, DomQuery, DomValue, EventOutcome, HostCompletion, HostEffect, HostRequest,
    JsEngine, JsEngineFactory, JsEvent, JsHost, MutateOp, MutateResult, NavigationKind,
    ScriptReport, ScriptTask,
};

const BOOTSTRAP: &str = r#"
const __ts_nodes = new Map();
function __ts_wrap(handle) {
  if (!handle) return null;
  if (!__ts_nodes.has(handle)) __ts_nodes.set(handle, new Element(handle));
  return __ts_nodes.get(handle);
}
function __ts_tokens(value) {
  return String(value).trim().split(/\s+/).filter(Boolean);
}
function __ts_class_list(element) {
  return {
    add(...tokens) {
      const values = new Set(__ts_tokens(element.className));
      tokens.flatMap(__ts_tokens).forEach(token => values.add(token));
      element.className = [...values].join(' ');
    },
    remove(...tokens) {
      const removed = new Set(tokens.flatMap(__ts_tokens));
      element.className = __ts_tokens(element.className).filter(token => !removed.has(token)).join(' ');
    },
    contains(token) { return __ts_tokens(element.className).includes(String(token)); },
    toggle(token, force) {
      token = String(token);
      const present = this.contains(token);
      const enabled = force === undefined ? !present : Boolean(force);
      if (enabled && !present) this.add(token);
      if (!enabled && present) this.remove(token);
      return enabled;
    }
  };
}
function __ts_style_map(element) {
  const values = new Map();
  for (const declaration of (element.getAttribute('style') ?? '').split(';')) {
    const separator = declaration.indexOf(':');
    if (separator > 0) values.set(declaration.slice(0, separator).trim(), declaration.slice(separator + 1).trim());
  }
  return values;
}
function __ts_style_name(name) {
  return String(name).replace(/[A-Z]/g, letter => '-' + letter.toLowerCase());
}
function __ts_style(element) {
  const write = values => element.setAttribute('style', [...values].map(([name, value]) => `${name}: ${value}`).join('; '));
  return new Proxy({}, {
    get(_target, name) {
      if (name === 'cssText') return element.getAttribute('style') ?? '';
      if (name === 'setProperty') return (property, value) => { const values = __ts_style_map(element); values.set(String(property), String(value)); write(values); };
      if (name === 'removeProperty') return property => { const values = __ts_style_map(element); const old = values.get(String(property)) ?? ''; values.delete(String(property)); write(values); return old; };
      return __ts_style_map(element).get(__ts_style_name(name)) ?? '';
    },
    set(_target, name, value) {
      if (name === 'cssText') { element.setAttribute('style', String(value)); return true; }
      const values = __ts_style_map(element);
      values.set(__ts_style_name(name), String(value));
      write(values);
      return true;
    }
  });
}
class Element {
  constructor(handle) { this.__handle = handle; this.onclick = null; }
  get textContent() { return __ts_text(this.__handle) ?? ''; }
  set textContent(value) { __ts_set_text(this.__handle, String(value)); }
  get parentNode() { return __ts_wrap(__ts_parent(this.__handle)); }
  get id() { return this.getAttribute('id') ?? ''; }
  set id(value) { this.setAttribute('id', value); }
  get className() { return this.getAttribute('class') ?? ''; }
  set className(value) { this.setAttribute('class', value); }
  get classList() { return __ts_class_list(this); }
  get style() { return __ts_style(this); }
  get value() { return this.getAttribute('value') ?? ''; }
  set value(value) { this.setAttribute('value', value); }
  get checked() { return this.getAttribute('checked') !== null; }
  set checked(value) { value ? this.setAttribute('checked', '') : this.removeAttribute('checked'); }
  get disabled() { return this.getAttribute('disabled') !== null; }
  set disabled(value) { value ? this.setAttribute('disabled', '') : this.removeAttribute('disabled'); }
  get href() { return this.getAttribute('href') ?? ''; }
  set href(value) { this.setAttribute('href', value); }
  get src() { return this.getAttribute('src') ?? ''; }
  set src(value) { this.setAttribute('src', value); }
  get name() { return this.getAttribute('name') ?? ''; }
  set name(value) { this.setAttribute('name', value); }
  getAttribute(name) { return __ts_attr(this.__handle, String(name)); }
  setAttribute(name, value) { __ts_set_attr(this.__handle, String(name), String(value)); }
  removeAttribute(name) { __ts_remove_attr(this.__handle, String(name)); }
  appendChild(child) { __ts_append(this.__handle, child.__handle); return child; }
  removeChild(child) { __ts_remove(child.__handle); return child; }
  querySelector(selector) { return document.querySelector(selector); }
  insertBefore(child, sibling) {
    __ts_insert_before(this.__handle, child.__handle, sibling ? sibling.__handle : 0);
    return child;
  }
  remove() { __ts_remove(this.__handle); }
}
class Document {
  get documentElement() { return __ts_wrap(__ts_document_element()); }
  get body() { return __ts_wrap(__ts_body()); }
  get title() { return __ts_title(); }
  set title(value) { __ts_set_title(String(value)); }
  get location() { return globalThis.location; }
  get URL() { return globalThis.location.href; }
  getElementById(id) { return __ts_wrap(__ts_by_id(String(id))); }
  querySelector(selector) { return __ts_wrap(__ts_query_selector(String(selector))); }
  createElement(name) { return __ts_wrap(__ts_create_element(String(name))); }
}
globalThis.Element = Element;
globalThis.document = new Document();
globalThis.window = globalThis;
globalThis.self = globalThis;
globalThis.console = {
  log(...values) { __ts_log(values.map(String).join(' ')); },
  error(...values) { __ts_log(values.map(String).join(' ')); }
};
globalThis.alert = value => __ts_alert(String(value));
const __ts_async = new Map();
globalThis.fetch = value => new Promise((resolve, reject) => {
  const id = __ts_fetch(String(value));
  __ts_async.set(id, { kind: 'fetch', resolve, reject });
});
globalThis.setTimeout = (callback, delay = 0, ...args) => {
  const id = __ts_timer(Number(delay), false);
  __ts_async.set(id, { kind: 'timer', callback, args, repeat: false });
  return id;
};
globalThis.setInterval = (callback, delay = 0, ...args) => {
  const id = __ts_timer(Number(delay), true);
  __ts_async.set(id, { kind: 'timer', callback, args, repeat: true });
  return id;
};
globalThis.clearTimeout = globalThis.clearInterval = id => {
  __ts_async.delete(Number(id));
  __ts_cancel_timer(Number(id));
};
function __ts_complete_fetch(id, ok, status, url, text, error) {
  const pending = __ts_async.get(id);
  if (!pending) return;
  __ts_async.delete(id);
  if (!ok) { pending.reject(new TypeError(error)); return; }
  pending.resolve({
    status,
    ok: status >= 200 && status < 300,
    url,
    text: () => Promise.resolve(text),
    json: () => Promise.resolve(JSON.parse(text))
  });
}
function __ts_complete_timer(id) {
  const pending = __ts_async.get(id);
  if (!pending) return;
  if (!pending.repeat) __ts_async.delete(id);
  pending.callback(...pending.args);
}
const __ts_location_object = {
  get href() { return __ts_location(); },
  set href(value) { __ts_navigate(String(value), 0); },
  assign(value) { __ts_navigate(String(value), 0); },
  replace(value) { __ts_navigate(String(value), 1); },
  reload() { __ts_navigate('', 2); }
};
Object.defineProperty(globalThis, 'location', {
  get() { return __ts_location_object; },
  set(value) { __ts_navigate(String(value), 0); },
  configurable: false
});
function __ts_dispatchClick(handle) {
  const target = __ts_wrap(handle);
  const event = {
    target,
    currentTarget: null,
    defaultPrevented: false,
    propagationStopped: false,
    preventDefault() { this.defaultPrevented = true; },
    stopPropagation() { this.propagationStopped = true; }
  };
  let current = target;
  while (current) {
    event.currentTarget = current;
    let handler = current.onclick;
    if (typeof handler !== 'function') {
      const source = current.getAttribute('onclick');
      if (source) handler = Function('event', source);
    }
    if (typeof handler === 'function' && handler.call(current, event) === false) {
      event.preventDefault();
    }
    if (event.propagationStopped) break;
    current = current.parentNode;
  }
  return event.defaultPrevented;
}
"#;

#[derive(Clone)]
struct HostSlot(Rc<HostSlotInner>);

struct HostSlotInner {
    host: Rc<dyn JsHost>,
    handles: RefCell<HashMap<u32, NodeId>>,
    reverse: RefCell<HashMap<NodeId, u32>>,
    next: RefCell<u32>,
}

impl HostSlot {
    fn new(host: Rc<dyn JsHost>) -> Self {
        Self(Rc::new(HostSlotInner {
            host,
            handles: RefCell::new(HashMap::new()),
            reverse: RefCell::new(HashMap::new()),
            next: RefCell::new(1),
        }))
    }

    fn node(&self, handle: u32) -> JsResult<NodeId> {
        self.0
            .handles
            .borrow()
            .get(&handle)
            .copied()
            .ok_or_else(|| type_error("invalid DOM handle"))
    }

    fn handle(&self, node: Option<NodeId>) -> u32 {
        let Some(node) = node else {
            return 0;
        };
        if let Some(handle) = self.0.reverse.borrow().get(&node).copied() {
            return handle;
        }
        let mut next = self.0.next.borrow_mut();
        let handle = *next;
        *next = next.saturating_add(1);
        self.0.handles.borrow_mut().insert(handle, node);
        self.0.reverse.borrow_mut().insert(node, handle);
        handle
    }
}

#[derive(Default)]
struct BoundedJobs {
    jobs: RefCell<VecDeque<Job>>,
}

impl BoundedJobs {
    fn run_bounded(&self, context: &mut Context, budget: u32) -> ScriptReport {
        let mut report = ScriptReport::default();
        while report.jobs_run < budget {
            let Some(job) = self.jobs.borrow_mut().pop_front() else {
                break;
            };
            let result = match job {
                Job::PromiseJob(job) => job.call(context),
                Job::GenericJob(job) => job.call(context),
                other => {
                    self.jobs.borrow_mut().push_front(other);
                    break;
                }
            };
            report.jobs_run += 1;
            if let Err(error) = result {
                report.error = Some(error.to_string());
                break;
            }
        }
        report
    }
}

impl JobExecutor for BoundedJobs {
    fn enqueue_job(self: Rc<Self>, job: Job, _context: &mut Context) {
        self.jobs.borrow_mut().push_back(job);
    }

    fn run_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<()> {
        let report = self.run_bounded(context, u32::MAX);
        match report.error {
            Some(error) => Err(type_error(error)),
            None => Ok(()),
        }
    }
}

pub struct BoaEngineFactory;

impl JsEngineFactory for BoaEngineFactory {
    fn create(&self, host: Rc<dyn JsHost>) -> Result<Box<dyn JsEngine>, String> {
        BoaEngine::new(host).map(|engine| Box::new(engine) as Box<dyn JsEngine>)
    }
}

struct BoaEngine {
    context: Context,
    jobs: Rc<BoundedJobs>,
    host: HostSlot,
}

impl BoaEngine {
    fn new(host: Rc<dyn JsHost>) -> Result<Self, String> {
        let jobs = Rc::new(BoundedJobs::default());
        let slot = HostSlot::new(host);
        let mut context = Context::builder()
            .job_executor(Rc::clone(&jobs))
            .build()
            .map_err(|error| error.to_string())?;
        context
            .runtime_limits_mut()
            .set_loop_iteration_limit(1_000_000);
        context.insert_data(slot.clone());
        register_host_functions(&mut context).map_err(|error| error.to_string())?;
        context
            .eval(Source::from_bytes(BOOTSTRAP))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            context,
            jobs,
            host: slot,
        })
    }
}

impl JsEngine for BoaEngine {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            executes_scripts: true,
            async_host_ops: true,
        }
    }

    fn run_script(&mut self, task: &ScriptTask) -> ScriptReport {
        match self.context.eval(Source::from_bytes(&task.source)) {
            Ok(_) => ScriptReport::default(),
            Err(error) => ScriptReport {
                error: Some(format!("{}: {error}", task.url)),
                jobs_run: 0,
            },
        }
    }

    fn run_job_pump(&mut self, budget: u32) -> ScriptReport {
        self.jobs.run_bounded(&mut self.context, budget)
    }

    fn dispatch_event(&mut self, event: &JsEvent) -> EventOutcome {
        let JsEvent::Click { target } = event;
        let handle = self.host.handle(Some(*target));
        let source = format!("__ts_dispatchClick({handle})");
        let canceled = self
            .context
            .eval(Source::from_bytes(&source))
            .is_ok_and(|value| value.to_boolean());
        EventOutcome { canceled }
    }

    fn complete_host(&mut self, completion: HostCompletion) -> ScriptReport {
        let result = match completion {
            HostCompletion::Fetch { id, result } => match result {
                Ok(response) => self.call_global(
                    "__ts_complete_fetch",
                    &[
                        JsValue::from(id.0),
                        JsValue::from(true),
                        JsValue::from(response.status),
                        JsValue::from(JsString::from(response.url)),
                        JsValue::from(JsString::from(response.text)),
                        JsValue::undefined(),
                    ],
                ),
                Err(error) => self.call_global(
                    "__ts_complete_fetch",
                    &[
                        JsValue::from(id.0),
                        JsValue::from(false),
                        JsValue::from(0),
                        JsValue::from(JsString::from("")),
                        JsValue::from(JsString::from("")),
                        JsValue::from(JsString::from(error)),
                    ],
                ),
            },
            HostCompletion::Timer { id } => {
                self.call_global("__ts_complete_timer", &[JsValue::from(id.0)])
            }
        };
        match result {
            Ok(_) => ScriptReport::default(),
            Err(error) => ScriptReport {
                error: Some(error.to_string()),
                jobs_run: 0,
            },
        }
    }
}

impl BoaEngine {
    fn call_global(&mut self, name: &str, arguments: &[JsValue]) -> JsResult<JsValue> {
        let global = self.context.global_object().clone();
        let function = global.get(JsString::from(name), &mut self.context)?;
        let callable = function
            .as_callable()
            .ok_or_else(|| type_error(format!("missing host callback {name}")))?;
        callable.call(&JsValue::undefined(), arguments, &mut self.context)
    }
}

fn register_host_functions(context: &mut Context) -> JsResult<()> {
    for (name, length, function) in [
        ("__ts_document_element", 0, native_document_element as _),
        ("__ts_body", 0, native_body as _),
        ("__ts_by_id", 1, native_by_id as _),
        ("__ts_query_selector", 1, native_query_selector as _),
        ("__ts_parent", 1, native_parent as _),
        ("__ts_text", 1, native_text as _),
        ("__ts_attr", 2, native_attr as _),
        ("__ts_title", 0, native_title as _),
        ("__ts_create_element", 1, native_create_element as _),
        ("__ts_set_text", 2, native_set_text as _),
        ("__ts_set_attr", 3, native_set_attr as _),
        ("__ts_remove_attr", 2, native_remove_attr as _),
        ("__ts_append", 2, native_append as _),
        ("__ts_insert_before", 3, native_insert_before as _),
        ("__ts_remove", 1, native_remove as _),
        ("__ts_set_title", 1, native_set_title as _),
        ("__ts_log", 1, native_log as _),
        ("__ts_alert", 1, native_alert as _),
        ("__ts_location", 0, native_location as _),
        ("__ts_navigate", 2, native_navigate as _),
        ("__ts_fetch", 1, native_fetch as _),
        ("__ts_timer", 2, native_timer as _),
        ("__ts_cancel_timer", 1, native_cancel_timer as _),
    ] {
        context.register_global_builtin_callable(
            JsString::from(name),
            length,
            NativeFunction::from_fn_ptr(function),
        )?;
    }
    Ok(())
}

fn slot(context: &Context) -> JsResult<HostSlot> {
    context
        .get_data::<HostSlot>()
        .cloned()
        .ok_or_else(|| type_error("missing browser host"))
}

fn argument_string(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    args.get(index)
        .unwrap_or(&JsValue::undefined())
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
}

fn argument_handle(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<u32> {
    Ok(args
        .get(index)
        .unwrap_or(&JsValue::undefined())
        .to_number(context)? as u32)
}

fn query_handle(context: &Context, query: DomQuery) -> JsResult<JsValue> {
    let slot = slot(context)?;
    match slot.0.host.query(query).map_err(host_error)? {
        DomValue::Node(node) => Ok(JsValue::from(slot.handle(node))),
        DomValue::Text(_) => Err(type_error("host returned text for a node query")),
    }
}

fn query_text(context: &Context, query: DomQuery) -> JsResult<JsValue> {
    let slot = slot(context)?;
    match slot.0.host.query(query).map_err(host_error)? {
        DomValue::Text(Some(value)) => Ok(JsValue::from(JsString::from(value))),
        DomValue::Text(None) => Ok(JsValue::null()),
        DomValue::Node(_) => Err(type_error("host returned a node for a text query")),
    }
}

fn mutate(context: &Context, op: MutateOp) -> JsResult<MutateResult> {
    slot(context)?.0.host.mutate(op).map_err(host_error)
}

fn native_document_element(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    query_handle(context, DomQuery::DocumentElement)
}

fn native_body(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    query_handle(context, DomQuery::Body)
}

fn native_by_id(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let id = argument_string(args, 0, context)?;
    query_handle(context, DomQuery::ById(id))
}

fn native_query_selector(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let selector = argument_string(args, 0, context)?;
    query_handle(context, DomQuery::BySelector(selector))
}

fn native_parent(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let slot = slot(context)?;
    let node = slot.node(argument_handle(args, 0, context)?)?;
    query_handle(context, DomQuery::Parent(node))
}

fn native_text(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let slot = slot(context)?;
    let node = slot.node(argument_handle(args, 0, context)?)?;
    query_text(context, DomQuery::TextContent(node))
}

fn native_attr(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let slot = slot(context)?;
    let node = slot.node(argument_handle(args, 0, context)?)?;
    let name = argument_string(args, 1, context)?;
    query_text(context, DomQuery::Attribute(node, name))
}

fn native_title(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    query_text(context, DomQuery::Title)
}

fn native_create_element(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let name = argument_string(args, 0, context)?;
    let slot = slot(context)?;
    match mutate(context, MutateOp::CreateElement(name))? {
        MutateResult::Node(node) => Ok(JsValue::from(slot.handle(Some(node)))),
        _ => Err(type_error("host did not create an element")),
    }
}

fn native_set_text(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let slot = slot(context)?;
    let node = slot.node(argument_handle(args, 0, context)?)?;
    let value = argument_string(args, 1, context)?;
    mutate(context, MutateOp::SetTextContent { node, value })?;
    Ok(JsValue::undefined())
}

fn native_set_attr(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let slot = slot(context)?;
    let node = slot.node(argument_handle(args, 0, context)?)?;
    let name = argument_string(args, 1, context)?;
    let value = argument_string(args, 2, context)?;
    mutate(context, MutateOp::SetAttribute { node, name, value })?;
    Ok(JsValue::undefined())
}

fn native_remove_attr(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let slot = slot(context)?;
    let node = slot.node(argument_handle(args, 0, context)?)?;
    let name = argument_string(args, 1, context)?;
    mutate(context, MutateOp::RemoveAttribute { node, name })?;
    Ok(JsValue::undefined())
}

fn native_append(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let slot = slot(context)?;
    let parent = slot.node(argument_handle(args, 0, context)?)?;
    let child = slot.node(argument_handle(args, 1, context)?)?;
    mutate(context, MutateOp::AppendChild { parent, child })?;
    Ok(JsValue::undefined())
}

fn native_insert_before(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let slot = slot(context)?;
    let parent = slot.node(argument_handle(args, 0, context)?)?;
    let child = slot.node(argument_handle(args, 1, context)?)?;
    let sibling_handle = argument_handle(args, 2, context)?;
    let sibling = (sibling_handle != 0)
        .then(|| slot.node(sibling_handle))
        .transpose()?;
    mutate(
        context,
        MutateOp::InsertBefore {
            parent,
            child,
            sibling,
        },
    )?;
    Ok(JsValue::undefined())
}

fn native_remove(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let slot = slot(context)?;
    let node = slot.node(argument_handle(args, 0, context)?)?;
    mutate(context, MutateOp::Remove(node))?;
    Ok(JsValue::undefined())
}

fn native_set_title(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let title = argument_string(args, 0, context)?;
    mutate(context, MutateOp::SetTitle(title))?;
    Ok(JsValue::undefined())
}

fn native_log(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let message = argument_string(args, 0, context)?;
    slot(context)?
        .0
        .host
        .emit(HostEffect::Log(message))
        .map_err(host_error)?;
    Ok(JsValue::undefined())
}

fn native_alert(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let message = argument_string(args, 0, context)?;
    slot(context)?
        .0
        .host
        .emit(HostEffect::Alert(message))
        .map_err(host_error)?;
    Ok(JsValue::undefined())
}

fn native_location(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    query_text(context, DomQuery::Location)
}

fn native_navigate(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = argument_string(args, 0, context)?;
    let kind = match argument_handle(args, 1, context)? {
        1 => NavigationKind::Replace,
        2 => NavigationKind::Reload,
        _ => NavigationKind::Push,
    };
    slot(context)?
        .0
        .host
        .emit(HostEffect::Navigate { target, kind })
        .map_err(host_error)?;
    Ok(JsValue::undefined())
}

fn native_fetch(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let url = argument_string(args, 0, context)?;
    let id = slot(context)?
        .0
        .host
        .request(HostRequest::Fetch { url })
        .map_err(host_error)?;
    Ok(JsValue::from(id.0))
}

fn native_timer(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let delay = args
        .first()
        .unwrap_or(&JsValue::undefined())
        .to_number(context)?
        .max(0.0) as u64;
    let repeat = args.get(1).is_some_and(JsValue::to_boolean);
    let id = slot(context)?
        .0
        .host
        .request(HostRequest::Timer {
            delay_ms: delay,
            repeat,
        })
        .map_err(host_error)?;
    Ok(JsValue::from(id.0))
}

fn native_cancel_timer(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let id = super::HostOpId(argument_handle(args, 0, context)?.into());
    slot(context)?
        .0
        .host
        .request(HostRequest::CancelTimer { id })
        .map_err(host_error)?;
    Ok(JsValue::undefined())
}

fn type_error(message: impl Into<String>) -> boa_engine::JsError {
    JsNativeError::typ().with_message(message.into()).into()
}

fn host_error(error: impl ToString) -> boa_engine::JsError {
    type_error(error.to_string())
}

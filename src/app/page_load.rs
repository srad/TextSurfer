use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use cssparser::EncodingSupport;
use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};
use mediatype::{MediaType, names};
use url::Url;

use crate::core::dom::{Attr, AttrNs, DomQuirksMode, ElementNs, Node, SharedDocument};
use crate::core::geom::Size;
use crate::core::style::Palette;
use crate::css::cascade::media_query_list_matches;
use crate::css::{
    ColorScheme, CssParser, CssRule, CssparserParser, MediaContext, MediaQueryList, MediaRule,
    StyleSheet, parse_media_queries,
};
use crate::html::{Html5everParser, HtmlParser};
use crate::net::{
    FetchError, FetchResponse, MAX_BODY_BYTES, ResourceId, charset_from_content_type,
};

use super::render::{RenderedPage, render_document};

pub const STYLESHEET_DEADLINE: Duration = Duration::from_secs(5);
pub const MAX_EXTERNAL_OCCURRENCES: usize = 64;
pub const MAX_IMPORT_DEPTH: usize = 8;
pub const MAX_EXTERNAL_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchCommand {
    pub resource_id: ResourceId,
    pub url: Url,
}

#[derive(Clone, Copy, Debug)]
pub struct PageLoadOptions {
    pub viewport: Size,
    pub palette: Palette,
    pub scripting: bool,
    pub color_scheme: ColorScheme,
    pub started: Duration,
}

enum RootSource {
    Inline {
        sheet: StyleSheet,
        queries: MediaQueryList,
        imports: Vec<usize>,
    },
    External(usize),
}

struct Occurrence {
    fetch_id: ResourceId,
    queries: MediaQueryList,
    depth: usize,
    ancestors: Vec<String>,
    environment: &'static Encoding,
    sheet: Option<StyleSheet>,
    imports: Vec<usize>,
    failed: bool,
}

enum FetchState {
    Pending,
    Ready(FetchResponse),
    Failed,
}

struct FetchEntry {
    requested: Url,
    state: FetchState,
    occurrences: Vec<usize>,
}

pub struct PageLoad {
    document: SharedDocument,
    document_url: Url,
    html_encoding: &'static Encoding,
    parse_errors: usize,
    roots: Vec<RootSource>,
    occurrences: Vec<Occurrence>,
    fetches: Vec<FetchEntry>,
    fetch_index: HashMap<ResourceId, usize>,
    url_cache: HashMap<String, ResourceId>,
    decoded_cache: HashMap<(ResourceId, &'static str), StyleSheet>,
    commands: Vec<FetchCommand>,
    next_resource_id: u64,
    raw_bytes: usize,
    decoded_bytes: usize,
    failed_resources: usize,
    external_disabled: bool,
    cancel_requested: bool,
    media: MediaContext,
    palette: Palette,
    deadline: Duration,
    first_painted: bool,
    final_painted: bool,
    dirty: bool,
}

impl PageLoad {
    pub fn new(
        source: &str,
        document_url: Url,
        html_encoding: &'static Encoding,
        options: PageLoadOptions,
    ) -> Self {
        let PageLoadOptions {
            viewport,
            palette,
            scripting,
            color_scheme,
            started,
        } = options;
        let outcome = Html5everParser::new(scripting).parse_document(source);
        let effective_base = outcome
            .base_href
            .as_deref()
            .and_then(|base| document_url.join(base).ok())
            .unwrap_or_else(|| document_url.clone());
        let mut load = Self {
            document: outcome.document,
            document_url,
            html_encoding,
            parse_errors: outcome.parse_errors,
            roots: Vec::new(),
            occurrences: Vec::new(),
            fetches: Vec::new(),
            fetch_index: HashMap::new(),
            url_cache: HashMap::new(),
            decoded_cache: HashMap::new(),
            commands: Vec::new(),
            next_resource_id: 1,
            raw_bytes: 0,
            decoded_bytes: 0,
            failed_resources: 0,
            external_disabled: false,
            cancel_requested: false,
            media: MediaContext::screen()
                .with_palette(palette)
                .with_scripting(scripting)
                .with_color_scheme(color_scheme)
                .with_viewport(viewport),
            palette,
            deadline: started.saturating_add(STYLESHEET_DEADLINE),
            first_painted: false,
            final_painted: false,
            dirty: true,
        };
        load.discover_document_sources(&effective_base);
        load.process_materializations();
        load
    }

    pub fn take_commands(&mut self) -> Vec<FetchCommand> {
        std::mem::take(&mut self.commands)
    }

    pub fn take_cancel_requested(&mut self) -> bool {
        std::mem::take(&mut self.cancel_requested)
    }

    pub fn deliver(
        &mut self,
        resource_id: ResourceId,
        result: Result<FetchResponse, FetchError>,
    ) -> bool {
        if resource_id == ResourceId::DOCUMENT || self.external_disabled {
            return false;
        }
        let Some(index) = self.fetch_index.get(&resource_id).copied() else {
            return false;
        };
        if !matches!(self.fetches[index].state, FetchState::Pending) {
            return false;
        }
        match result {
            Ok(response)
                if response.body.len() <= MAX_BODY_BYTES
                    && self.accepts_stylesheet_response(&response) =>
            {
                if self
                    .raw_bytes
                    .checked_add(response.body.len())
                    .is_none_or(|total| total > MAX_EXTERNAL_BYTES)
                {
                    self.disable_external();
                    return true;
                }
                self.raw_bytes += response.body.len();
                let final_key = normalized_url(&response.final_url);
                self.url_cache.entry(final_key).or_insert(resource_id);
                self.fetches[index].state = FetchState::Ready(response);
            }
            _ => {
                self.fetches[index].state = FetchState::Failed;
                self.failed_resources = self.failed_resources.saturating_add(1);
            }
        }
        self.process_materializations();
        self.dirty = true;
        true
    }

    pub fn render_if_ready(&mut self, now: Duration) -> Option<RenderedPage> {
        let settled = self.applicable_graph_settled();
        if !self.first_painted {
            if !settled && now < self.deadline && !self.external_disabled {
                return None;
            }
            self.first_painted = true;
            self.final_painted = settled;
            self.dirty = false;
            return Some(self.render_page());
        }
        if !self.final_painted && settled && self.dirty {
            self.final_painted = true;
            self.dirty = false;
            return Some(self.render_page());
        }
        None
    }

    pub fn force_render(&mut self) -> RenderedPage {
        self.first_painted = true;
        self.final_painted = self.applicable_graph_settled();
        self.dirty = false;
        self.render_page()
    }

    pub fn resize(&mut self, viewport: Size) -> Option<RenderedPage> {
        if self.media.viewport == viewport {
            return None;
        }
        self.set_viewport(viewport);
        if self.first_painted {
            self.final_painted = self.applicable_graph_settled();
            self.dirty = false;
            Some(self.render_page())
        } else {
            None
        }
    }

    pub fn set_viewport(&mut self, viewport: Size) {
        if self.media.viewport != viewport {
            self.media = self.media.with_viewport(viewport);
            self.dirty = true;
        }
    }

    pub fn parse_errors(&self) -> usize {
        self.parse_errors
    }

    pub fn has_painted(&self) -> bool {
        self.first_painted
    }

    pub fn is_settled(&self) -> bool {
        self.external_disabled
            || self
                .fetches
                .iter()
                .all(|fetch| !matches!(fetch.state, FetchState::Pending))
    }

    pub fn applicable_is_settled(&self) -> bool {
        self.applicable_graph_settled()
    }

    pub fn external_occurrences(&self) -> usize {
        self.occurrences.len()
    }

    pub fn failed_resources(&self) -> usize {
        self.failed_resources
    }

    pub fn external_disabled(&self) -> bool {
        self.external_disabled
    }

    fn discover_document_sources(&mut self, effective_base: &Url) {
        let document = self.document.borrow();
        let mut discovered = Vec::new();
        let mut stack: Vec<_> = document.roots().iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let Some(Node::Element { name, ns, attrs }) = document.node(id) else {
                continue;
            };
            if *ns != ElementNs::Html {
                stack.extend(document.children(id).into_iter().rev());
                continue;
            }
            if name == "style" && css_type_allowed(attrs) {
                let source = document
                    .children(id)
                    .iter()
                    .filter_map(|child| match document.node(*child) {
                        Some(Node::Text { data }) => Some(data.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                discovered.push(DiscoveredRoot::Inline {
                    source,
                    media: attr_value(attrs, "media").unwrap_or_default().to_string(),
                });
            } else if name == "link"
                && stylesheet_link(attrs)
                && let Some(href) = attr_value(attrs, "href").filter(|href| !href.trim().is_empty())
                && let Ok(url) = effective_base.join(href)
            {
                discovered.push(DiscoveredRoot::External {
                    url,
                    media: attr_value(attrs, "media").unwrap_or_default().to_string(),
                });
            }
            if name != "template" {
                stack.extend(document.children(id).into_iter().rev());
            }
        }
        drop(document);
        for root in discovered {
            match root {
                DiscoveredRoot::Inline { source, media } => {
                    let sheet = CssparserParser.parse(&source);
                    let imports =
                        self.discover_imports(&sheet, effective_base, 1, &[], self.html_encoding);
                    self.roots.push(RootSource::Inline {
                        sheet,
                        queries: parse_media_queries(&media),
                        imports,
                    });
                }
                DiscoveredRoot::External { mut url, media } => {
                    url.set_fragment(None);
                    if let Some(occurrence) = self.add_occurrence(
                        url,
                        parse_media_queries(&media),
                        0,
                        Vec::new(),
                        self.html_encoding,
                    ) {
                        self.roots.push(RootSource::External(occurrence));
                    }
                }
            }
        }
    }

    fn discover_imports(
        &mut self,
        sheet: &StyleSheet,
        base: &Url,
        depth: usize,
        ancestors: &[String],
        environment: &'static Encoding,
    ) -> Vec<usize> {
        let mut imports = Vec::new();
        for rule in &sheet.rules {
            let CssRule::Import(rule) = rule else {
                continue;
            };
            if depth > MAX_IMPORT_DEPTH {
                self.failed_resources = self.failed_resources.saturating_add(1);
                continue;
            }
            let Ok(mut url) = base.join(&rule.url) else {
                self.failed_resources = self.failed_resources.saturating_add(1);
                continue;
            };
            url.set_fragment(None);
            if let Some(occurrence) = self.add_occurrence(
                url,
                rule.queries.clone(),
                depth,
                ancestors.to_vec(),
                environment,
            ) {
                imports.push(occurrence);
            }
        }
        imports
    }

    fn add_occurrence(
        &mut self,
        mut url: Url,
        queries: MediaQueryList,
        depth: usize,
        ancestors: Vec<String>,
        environment: &'static Encoding,
    ) -> Option<usize> {
        if self.external_disabled {
            return None;
        }
        if self.occurrences.len() >= MAX_EXTERNAL_OCCURRENCES {
            self.disable_external();
            return None;
        }
        url.set_fragment(None);
        let key = normalized_url(&url);
        let known = self.url_cache.get(&key).copied();
        let cycle = ancestors.iter().any(|ancestor| ancestor == &key);
        let fetch_id = known.unwrap_or_else(|| {
            let id = ResourceId(self.next_resource_id);
            self.next_resource_id += 1;
            let index = self.fetches.len();
            self.fetches.push(FetchEntry {
                requested: url.clone(),
                state: FetchState::Pending,
                occurrences: Vec::new(),
            });
            self.fetch_index.insert(id, index);
            self.url_cache.insert(key.clone(), id);
            self.commands.push(FetchCommand {
                resource_id: id,
                url: url.clone(),
            });
            id
        });
        let occurrence = self.occurrences.len();
        self.occurrences.push(Occurrence {
            fetch_id,
            queries,
            depth,
            ancestors,
            environment,
            sheet: None,
            imports: Vec::new(),
            failed: cycle,
        });
        if !cycle && let Some(index) = self.fetch_index.get(&fetch_id).copied() {
            self.fetches[index].occurrences.push(occurrence);
        }
        Some(occurrence)
    }

    fn process_materializations(&mut self) {
        let mut queue: VecDeque<_> = self
            .fetches
            .iter()
            .flat_map(|fetch| fetch.occurrences.iter().copied())
            .filter(|occurrence| {
                !self.occurrences[*occurrence].failed
                    && self.occurrences[*occurrence].sheet.is_none()
            })
            .collect();
        let mut queued: HashSet<_> = queue.iter().copied().collect();
        while let Some(occurrence_id) = queue.pop_front() {
            if self.external_disabled {
                return;
            }
            let fetch_id = self.occurrences[occurrence_id].fetch_id;
            let Some(fetch_index) = self.fetch_index.get(&fetch_id).copied() else {
                continue;
            };
            let response = match &self.fetches[fetch_index].state {
                FetchState::Pending => continue,
                FetchState::Failed => {
                    self.occurrences[occurrence_id].failed = true;
                    continue;
                }
                FetchState::Ready(response) => response.clone(),
            };
            let environment = self.occurrences[occurrence_id].environment;
            let protocol = response
                .content_type
                .as_deref()
                .and_then(charset_from_content_type);
            let selected = cssparser::stylesheet_encoding::<EncodingRs>(
                &response.body,
                protocol.as_deref().map(str::as_bytes),
                Some(environment),
            );
            let (decoded, used_encoding, _) = selected.decode(&response.body);
            let cache_key = (fetch_id, used_encoding.name());
            let sheet = if let Some(sheet) = self.decoded_cache.get(&cache_key) {
                sheet.clone()
            } else {
                if self
                    .decoded_bytes
                    .checked_add(decoded.len())
                    .is_none_or(|total| total > MAX_EXTERNAL_BYTES)
                {
                    self.disable_external();
                    return;
                }
                self.decoded_bytes += decoded.len();
                let sheet = CssparserParser.parse(&decoded);
                self.decoded_cache.insert(cache_key, sheet.clone());
                sheet
            };
            self.occurrences[occurrence_id].sheet = Some(sheet.clone());
            let mut ancestors = self.occurrences[occurrence_id].ancestors.clone();
            ancestors.push(normalized_url(&self.fetches[fetch_index].requested));
            ancestors.push(normalized_url(&response.final_url));
            ancestors.sort();
            ancestors.dedup();
            let children = self.discover_imports(
                &sheet,
                &response.final_url,
                self.occurrences[occurrence_id].depth + 1,
                &ancestors,
                used_encoding,
            );
            self.occurrences[occurrence_id].imports = children.clone();
            for child in children {
                if queued.insert(child) {
                    queue.push_back(child);
                }
            }
        }
    }

    fn accepts_stylesheet_response(&self, response: &FetchResponse) -> bool {
        let Some(content_type) = response.content_type.as_deref() else {
            return true;
        };
        let Ok(media_type) = MediaType::parse(content_type) else {
            return true;
        };
        if media_type.ty == names::TEXT && media_type.subty == names::CSS {
            return true;
        }
        self.document.borrow().quirks_mode() == DomQuirksMode::Quirks
            && self.document_url.origin() == response.final_url.origin()
    }

    fn applicable_graph_settled(&self) -> bool {
        if self.external_disabled {
            return true;
        }
        self.roots.iter().all(|root| match root {
            RootSource::Inline {
                queries, imports, ..
            } => {
                !media_query_list_matches(queries, self.media)
                    || imports
                        .iter()
                        .all(|occurrence| self.occurrence_settled(*occurrence, true))
            }
            RootSource::External(occurrence) => self.occurrence_settled(*occurrence, true),
        })
    }

    fn occurrence_settled(&self, occurrence_id: usize, inherited_match: bool) -> bool {
        let occurrence = &self.occurrences[occurrence_id];
        if occurrence.failed
            || !inherited_match
            || !media_query_list_matches(&occurrence.queries, self.media)
        {
            return true;
        }
        let Some(fetch_index) = self.fetch_index.get(&occurrence.fetch_id).copied() else {
            return true;
        };
        if matches!(self.fetches[fetch_index].state, FetchState::Pending) {
            return false;
        }
        occurrence
            .imports
            .iter()
            .all(|child| self.occurrence_settled(*child, true))
    }

    fn render_page(&self) -> RenderedPage {
        let sheets = self.ordered_sheets();
        render_document(
            self.document.clone(),
            &sheets,
            self.media,
            self.palette,
            self.parse_errors,
        )
    }

    fn ordered_sheets(&self) -> Vec<StyleSheet> {
        let mut sheets = Vec::new();
        for root in &self.roots {
            match root {
                RootSource::Inline {
                    sheet,
                    queries,
                    imports,
                } => {
                    if !self.external_disabled {
                        for occurrence in imports {
                            self.flatten_occurrence(
                                *occurrence,
                                std::slice::from_ref(queries),
                                &mut sheets,
                            );
                        }
                    }
                    sheets.push(sheet_without_imports(sheet, std::slice::from_ref(queries)));
                }
                RootSource::External(occurrence) if !self.external_disabled => {
                    self.flatten_occurrence(*occurrence, &[], &mut sheets);
                }
                RootSource::External(_) => {}
            }
        }
        sheets
    }

    fn flatten_occurrence(
        &self,
        occurrence_id: usize,
        inherited: &[MediaQueryList],
        output: &mut Vec<StyleSheet>,
    ) {
        let occurrence = &self.occurrences[occurrence_id];
        if occurrence.failed {
            return;
        }
        let mut chain = inherited.to_vec();
        chain.push(occurrence.queries.clone());
        for child in &occurrence.imports {
            self.flatten_occurrence(*child, &chain, output);
        }
        if let Some(sheet) = &occurrence.sheet {
            output.push(sheet_without_imports(sheet, &chain));
        }
    }

    fn disable_external(&mut self) {
        self.external_disabled = true;
        self.cancel_requested = true;
        self.raw_bytes = 0;
        self.decoded_bytes = 0;
        self.decoded_cache.clear();
        self.commands.clear();
        for fetch in &mut self.fetches {
            fetch.state = FetchState::Failed;
        }
        for occurrence in &mut self.occurrences {
            occurrence.sheet = None;
            occurrence.imports.clear();
        }
        self.dirty = true;
    }
}

enum DiscoveredRoot {
    Inline { source: String, media: String },
    External { url: Url, media: String },
}

struct EncodingRs;

impl EncodingSupport for EncodingRs {
    type Encoding = &'static Encoding;

    fn from_label(label: &[u8]) -> Option<Self::Encoding> {
        Encoding::for_label(label)
    }

    fn utf8() -> Self::Encoding {
        UTF_8
    }

    fn is_utf16_be_or_le(encoding: &Self::Encoding) -> bool {
        *encoding == UTF_16BE || *encoding == UTF_16LE
    }
}

fn normalized_url(url: &Url) -> String {
    let mut normalized = url.clone();
    normalized.set_fragment(None);
    normalized.into()
}

fn attr_value<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|attr| attr.ns == AttrNs::None && attr.name.eq_ignore_ascii_case(name))
        .map(|attr| attr.value.as_str())
}

fn css_type_allowed(attrs: &[Attr]) -> bool {
    attr_value(attrs, "type").is_none_or(|value| {
        MediaType::parse(value.trim())
            .is_ok_and(|media_type| media_type.ty == names::TEXT && media_type.subty == names::CSS)
    })
}

fn stylesheet_link(attrs: &[Attr]) -> bool {
    let Some(rel) = attr_value(attrs, "rel") else {
        return false;
    };
    let tokens: Vec<_> = rel.split_ascii_whitespace().collect();
    tokens
        .iter()
        .any(|token| token.eq_ignore_ascii_case("stylesheet"))
        && !tokens
            .iter()
            .any(|token| token.eq_ignore_ascii_case("alternate"))
        && attr_value(attrs, "disabled").is_none()
        && css_type_allowed(attrs)
}

fn sheet_without_imports(sheet: &StyleSheet, queries: &[MediaQueryList]) -> StyleSheet {
    let mut rules: Vec<_> = sheet
        .rules
        .iter()
        .filter(|rule| !matches!(rule, CssRule::Import(_)))
        .cloned()
        .collect();
    for query in queries.iter().rev() {
        if !matches!(query, MediaQueryList::Always) {
            rules = vec![CssRule::Media(MediaRule {
                queries: query.clone(),
                rules,
            })];
        }
    }
    StyleSheet {
        rules,
        diagnostics: sheet.diagnostics.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::FetchResponse;

    fn load(source: &str) -> PageLoad {
        PageLoad::new(
            source,
            Url::parse("https://example.com/dir/page").unwrap(),
            UTF_8,
            PageLoadOptions {
                viewport: Size { cols: 80, rows: 24 },
                palette: Palette::default(),
                scripting: false,
                color_scheme: ColorScheme::Dark,
                started: Duration::ZERO,
            },
        )
    }

    #[test]
    fn discovers_ordered_links_and_honors_base_filters_and_fragments() {
        let mut load = load(
            "<base href='/assets/'><link rel='alternate stylesheet' href='skip.css'>
             <link rel='stylesheet' href='a.css#one'><link rel='STYLESHEET' href='a.css#two'>
             <link rel='stylesheet' disabled href='disabled.css'>
             <link rel='stylesheet' type='text/plain' href='plain.css'>",
        );
        let commands = load.take_commands();
        assert_eq!(load.external_occurrences(), 2);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].url.as_str(), "https://example.com/assets/a.css");
    }

    #[test]
    fn applicable_sheets_block_until_ready_and_nonmatching_sheets_do_not() {
        let mut matching = load("<link rel=stylesheet href='a.css'><p>x</p>");
        assert!(matching.render_if_ready(Duration::from_secs(4)).is_none());
        assert!(matching.render_if_ready(Duration::from_secs(5)).is_some());

        let mut print = load("<link rel=stylesheet media=print href='a.css'><p>x</p>");
        assert!(print.render_if_ready(Duration::ZERO).is_some());
    }

    #[test]
    fn imports_resolve_against_final_url_and_cycles_stop() {
        let mut load = load("<link rel=stylesheet href='redirect.css'>");
        let root = load.take_commands().pop().unwrap();
        assert!(load.deliver(
            root.resource_id,
            Ok(FetchResponse {
                final_url: Url::parse("https://cdn.example/css/main.css").unwrap(),
                body: b"@import 'child.css'; p { display: block }".to_vec(),
                content_type: Some("text/css".to_string()),
            }),
        ));
        let child = load.take_commands().pop().unwrap();
        assert_eq!(child.url.as_str(), "https://cdn.example/css/child.css");
        assert!(load.deliver(
            child.resource_id,
            Ok(FetchResponse {
                final_url: child.url,
                body: b"@import 'main.css';".to_vec(),
                content_type: Some("text/css".to_string()),
            }),
        ));
        assert!(load.is_settled());
        assert_eq!(load.take_commands().len(), 0);
    }

    #[test]
    fn invalid_non_css_mime_fails_only_that_resource() {
        let mut load = load(
            "<!doctype html><link rel=stylesheet href='a.css'><style>p { display:block }</style>",
        );
        let command = load.take_commands().pop().unwrap();
        assert!(load.deliver(
            command.resource_id,
            Ok(FetchResponse {
                final_url: command.url,
                body: b"p { display:none }".to_vec(),
                content_type: Some("image/png".to_string()),
            }),
        ));
        assert_eq!(load.failed_resources(), 1);
        assert!(load.render_if_ready(Duration::ZERO).is_some());
    }

    fn css_response(command: FetchCommand, body: &[u8]) -> (ResourceId, FetchResponse) {
        (
            command.resource_id,
            FetchResponse {
                final_url: command.url,
                body: body.to_vec(),
                content_type: Some("text/css".to_string()),
            },
        )
    }

    #[test]
    fn completion_order_never_changes_document_cascade_order() {
        let mut load = load(
            "<!doctype html><link rel=stylesheet href='a.css'>
             <link rel=stylesheet href='b.css'><p>visible</p>",
        );
        let commands = load.take_commands();
        let a = commands
            .iter()
            .find(|command| command.url.path().ends_with("a.css"))
            .unwrap();
        let b = commands
            .iter()
            .find(|command| command.url.path().ends_with("b.css"))
            .unwrap();
        let (id, response) = css_response(b.clone(), b"p { display: block }");
        assert!(load.deliver(id, Ok(response)));
        assert!(load.render_if_ready(Duration::ZERO).is_none());
        let (id, response) = css_response(a.clone(), b"p { display: none }");
        assert!(load.deliver(id, Ok(response)));
        let page = load.render_if_ready(Duration::ZERO).unwrap();
        assert!(
            page.painted
                .text_lines()
                .iter()
                .any(|line| line == "visible")
        );
    }

    #[test]
    fn repeated_occurrences_fetch_once_and_keep_each_source_position() {
        let mut load = load(
            "<!doctype html><link rel=stylesheet href='same.css'>
             <style>p { display: none }</style>
             <link rel=stylesheet href='same.css'><p>visible</p>",
        );
        let commands = load.take_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(load.external_occurrences(), 2);
        let (id, response) = css_response(commands[0].clone(), b"p { display: block }");
        assert!(load.deliver(id, Ok(response)));
        let page = load.render_if_ready(Duration::ZERO).unwrap();
        assert!(
            page.painted
                .text_lines()
                .iter()
                .any(|line| line == "visible")
        );
    }

    #[test]
    fn deadline_and_late_repaint_are_exact_and_coalesced() {
        let mut load = load(
            "<!doctype html><link rel=stylesheet href='a.css'>
             <link rel=stylesheet href='b.css'><p>visible</p>",
        );
        let commands = load.take_commands();
        assert!(
            load.render_if_ready(STYLESHEET_DEADLINE - Duration::from_nanos(1))
                .is_none()
        );
        assert!(load.render_if_ready(STYLESHEET_DEADLINE).is_some());
        let (id, response) = css_response(commands[0].clone(), b"p { display: none }");
        assert!(load.deliver(id, Ok(response)));
        assert!(load.render_if_ready(STYLESHEET_DEADLINE).is_none());
        let (id, response) = css_response(commands[1].clone(), b"p { display: block }");
        assert!(load.deliver(id, Ok(response)));
        assert!(load.render_if_ready(STYLESHEET_DEADLINE).is_some());
        assert!(load.render_if_ready(STYLESHEET_DEADLINE).is_none());
        let (_, duplicate) = css_response(commands[1].clone(), b"p { display: none }");
        assert!(!load.deliver(id, Ok(duplicate)));
        assert!(!load.deliver(
            ResourceId(999),
            Err(FetchError::Network("unknown".to_string()))
        ));
    }

    #[test]
    fn newly_applicable_pending_sheet_repaints_without_blank_loading_state() {
        let mut load = load(
            "<!doctype html><link rel=stylesheet media='(min-width: 100px)' href='wide.css'>
             <p>visible</p>",
        );
        let command = load.take_commands().pop().unwrap();
        assert!(load.render_if_ready(Duration::ZERO).is_some());
        assert!(
            load.resize(Size {
                cols: 120,
                rows: 24
            })
            .is_some()
        );
        let (id, response) = css_response(command, b"p { display: none }");
        assert!(load.deliver(id, Ok(response)));
        let page = load.render_if_ready(Duration::ZERO).unwrap();
        assert!(
            !page
                .painted
                .text_lines()
                .iter()
                .any(|line| line == "visible")
        );
    }

    #[test]
    fn occurrence_and_byte_ceiling_failures_discard_all_external_css() {
        let links = (0..=MAX_EXTERNAL_OCCURRENCES)
            .map(|index| format!("<link rel=stylesheet href='{index}.css'>"))
            .collect::<String>();
        let mut too_many = load(&format!("<!doctype html>{links}<p>visible</p>"));
        assert!(too_many.external_disabled());
        assert!(too_many.take_commands().is_empty());

        let mut too_large = load(
            "<!doctype html><style>p { display:block }</style>
             <link rel=stylesheet href='large.css'><p>visible</p>",
        );
        let command = too_large.take_commands().pop().unwrap();
        too_large.raw_bytes = MAX_EXTERNAL_BYTES;
        let (id, response) = css_response(command, b"p { display:none }");
        assert!(too_large.deliver(id, Ok(response)));
        assert!(too_large.external_disabled());
        assert!(too_large.take_cancel_requested());
        let page = too_large.force_render();
        assert!(
            page.painted
                .text_lines()
                .iter()
                .any(|line| line == "visible")
        );
    }

    #[test]
    fn one_fetch_can_produce_two_environment_decodings() {
        let mut load = load(
            "<!doctype html><link rel=stylesheet href='one.css'>
             <link rel=stylesheet href='two.css'><p>x</p>",
        );
        let commands = load.take_commands();
        for command in commands {
            let charset = if command.url.path().ends_with("one.css") {
                "windows-1252"
            } else {
                "utf-8"
            };
            assert!(load.deliver(
                command.resource_id,
                Ok(FetchResponse {
                    final_url: command.url,
                    body: b"@import 'shared.css';".to_vec(),
                    content_type: Some(format!("text/css; charset={charset}")),
                }),
            ));
        }
        let shared = load.take_commands();
        assert_eq!(shared.len(), 1);
        let (id, response) = css_response(shared[0].clone(), b"p { display:block }");
        assert!(load.deliver(id, Ok(response)));
        let variants = load
            .decoded_cache
            .keys()
            .filter(|(resource, _)| *resource == shared[0].resource_id)
            .count();
        assert_eq!(variants, 2);
    }

    #[test]
    fn import_depth_is_bounded_to_eight_edges() {
        let mut load = load("<!doctype html><link rel=stylesheet href='0.css'><p>x</p>");
        for depth in 0..=MAX_IMPORT_DEPTH {
            let commands = load.take_commands();
            assert_eq!(commands.len(), 1, "depth {depth}");
            let command = commands[0].clone();
            let body = format!("@import '{}.css';", depth + 1);
            assert!(load.deliver(
                command.resource_id,
                Ok(FetchResponse {
                    final_url: command.url,
                    body: body.into_bytes(),
                    content_type: Some("text/css".to_string()),
                }),
            ));
        }
        assert!(load.take_commands().is_empty());
        assert!(load.is_settled());
    }

    #[test]
    fn css_decoding_obeys_protocol_charset_at_charset_and_bom_precedence() {
        let cases = [
            (
                b"p { display:block }".to_vec(),
                Some("text/css; charset=windows-1252".to_string()),
                "windows-1252",
            ),
            (
                b"@charset \"windows-1252\"; p { display:block }".to_vec(),
                Some("text/css".to_string()),
                "windows-1252",
            ),
            (
                vec![0xFF, 0xFE, b'p', 0, b' ', 0, b'{', 0, b'}', 0],
                Some("text/css".to_string()),
                "UTF-16LE",
            ),
        ];
        for (body, content_type, expected) in cases {
            let mut load = load("<!doctype html><link rel=stylesheet href='sheet.css'><p>x</p>");
            let command = load.take_commands().pop().unwrap();
            assert!(load.deliver(
                command.resource_id,
                Ok(FetchResponse {
                    final_url: command.url,
                    body,
                    content_type,
                }),
            ));
            assert!(
                load.decoded_cache
                    .keys()
                    .any(|(_, encoding)| *encoding == expected)
            );
        }
    }
}

use encoding_rs::Encoding;
use mediatype::{MediaType, names};
use url::Url;

use crate::core::dom::{Attr, AttrNs, ElementNs, Node};
use crate::net::ResourceId;

use super::resource_url::normalized_url;
use super::{
    FetchCommand, FetchEntry, FetchState, MAX_EXTERNAL_OCCURRENCES, MAX_IMPORT_DEPTH, Occurrence,
    PageLoad, RootSource,
};

enum DiscoveredRoot {
    Inline { source: String, media: String },
    External { url: Url, media: String },
}

impl PageLoad {
    pub(super) fn discover_document_sources(&mut self, effective_base: &Url) {
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
                    let imports =
                        self.discover_imports(&source, effective_base, 1, &[], self.html_encoding);
                    self.roots.push(RootSource::Inline {
                        source,
                        media,
                        imports,
                    });
                }
                DiscoveredRoot::External { mut url, media } => {
                    url.set_fragment(None);
                    if let Some(occurrence) =
                        self.add_occurrence(url, String::new(), 0, Vec::new(), self.html_encoding)
                    {
                        self.roots.push(RootSource::External { occurrence, media });
                    }
                }
            }
        }
    }

    pub(super) fn discover_imports(
        &mut self,
        source: &str,
        base: &Url,
        depth: usize,
        ancestors: &[String],
        environment: &'static Encoding,
    ) -> Vec<usize> {
        let mut imports = Vec::new();
        let quirks_mode = self.document.borrow().quirks_mode();
        for import in crate::css::stylo::discover_imports(source, base, quirks_mode) {
            if depth > MAX_IMPORT_DEPTH {
                self.failed_resources = self.failed_resources.saturating_add(1);
                continue;
            }
            let mut url = import.url;
            url.set_fragment(None);
            if let Some(occurrence) =
                self.add_occurrence(url, import.media, depth, ancestors.to_vec(), environment)
            {
                imports.push(occurrence);
            }
        }
        imports
    }

    /// A page may only pull subresources from its own scheme. Without this an `http(s)` document
    /// could name `file:///…` in a `<link>` and have the browser read local files for it.
    pub(super) fn scheme_allowed(&self, url: &Url) -> bool {
        match self.document_url.scheme() {
            "http" | "https" => matches!(url.scheme(), "http" | "https"),
            scheme => url.scheme() == scheme,
        }
    }

    fn add_occurrence(
        &mut self,
        mut url: Url,
        media: String,
        depth: usize,
        ancestors: Vec<String>,
        environment: &'static Encoding,
    ) -> Option<usize> {
        if self.external_disabled {
            return None;
        }
        if !self.scheme_allowed(&url) {
            self.failed_resources = self.failed_resources.saturating_add(1);
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
            media,
            depth,
            ancestors,
            environment,
            source: None,
            base_url: url,
            imports: Vec::new(),
            failed: cycle,
        });
        if !cycle && let Some(index) = self.fetch_index.get(&fetch_id).copied() {
            self.fetches[index].occurrences.push(occurrence);
        }
        Some(occurrence)
    }
}

pub(super) fn attr_value<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
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

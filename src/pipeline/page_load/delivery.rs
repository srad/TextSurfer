use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use cssparser::EncodingSupport;
use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};
use mediatype::{MediaType, names};

use crate::core::dom::DomQuirksMode;
use crate::net::{
    FetchError, FetchResponse, MAX_BODY_BYTES, ResourceId, charset_from_content_type, decode_text,
};
use crate::script::{HostCompletion, HostResponse};

use super::super::render::RenderCause;
use super::resource_url::normalized_url;
use super::{FetchState, MAX_EXTERNAL_BYTES, PageLoad, RenderInvalidation};

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

impl PageLoad {
    pub fn deliver(
        &mut self,
        resource_id: ResourceId,
        result: Result<FetchResponse, FetchError>,
    ) -> bool {
        if self.script_host_fetch_index.contains_key(&resource_id) {
            return self.deliver_script_host_fetch(resource_id, result);
        }
        if resource_id == ResourceId::DOCUMENT || self.external_disabled {
            if resource_id == ResourceId::DOCUMENT {
                return false;
            }
            if self.image_fetch_index.contains_key(&resource_id) {
                return self.deliver_image_fetch(resource_id, result);
            }
            if self.script_fetch_index.contains_key(&resource_id) {
                return self.deliver_script_fetch(resource_id, result);
            }
            return false;
        }
        if self.script_fetch_index.contains_key(&resource_id) {
            return self.deliver_script_fetch(resource_id, result);
        }
        if self.image_fetch_index.contains_key(&resource_id) {
            return self.deliver_image_fetch(resource_id, result);
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
        self.invalidate_soft(RenderInvalidation::STYLE, RenderCause::Stylesheet);
        true
    }

    fn deliver_script_host_fetch(
        &mut self,
        resource_id: ResourceId,
        result: Result<FetchResponse, FetchError>,
    ) -> bool {
        let Some(id) = self.script_host_fetch_index.remove(&resource_id) else {
            return false;
        };
        let result = match result {
            Ok(response) if response.body.len() <= MAX_BODY_BYTES => {
                let charset = response
                    .content_type
                    .as_deref()
                    .and_then(charset_from_content_type);
                Ok(HostResponse {
                    status: response.status,
                    url: response.final_url.to_string(),
                    text: decode_text(&response.body, charset.as_deref()).text,
                })
            }
            Ok(_) => Err("fetch response exceeded the body limit".to_string()),
            Err(error) => Err(error.to_string()),
        };
        self.script_completions
            .push_back(HostCompletion::Fetch { id, result });
        true
    }

    fn deliver_script_fetch(
        &mut self,
        resource_id: ResourceId,
        result: Result<FetchResponse, FetchError>,
    ) -> bool {
        let Some(index) = self.script_fetch_index.remove(&resource_id) else {
            return false;
        };
        let (source, final_url) = match result {
            Ok(response) if response.is_success() && response.body.len() <= MAX_BODY_BYTES => {
                let charset = response
                    .content_type
                    .as_deref()
                    .and_then(charset_from_content_type);
                (
                    Some(decode_text(&response.body, charset.as_deref()).text),
                    response.final_url.to_string(),
                )
            }
            Ok(response) => {
                self.failed_resources = self.failed_resources.saturating_add(1);
                (None, response.final_url.to_string())
            }
            Err(_) => {
                self.failed_resources = self.failed_resources.saturating_add(1);
                (None, self.document_url.to_string())
            }
        };
        self.script
            .as_mut()
            .is_some_and(|script| script.deliver_external(index, source, final_url))
    }

    pub(super) fn process_materializations(&mut self) {
        let mut queue: VecDeque<_> = self
            .fetches
            .iter()
            .flat_map(|fetch| fetch.occurrences.iter().copied())
            .filter(|occurrence| {
                !self.occurrences[*occurrence].failed
                    && self.occurrences[*occurrence].source.is_none()
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
            let source = if let Some(source) = self.decoded_cache.get(&cache_key) {
                source.clone()
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
                let source: Arc<str> = Arc::from(decoded.as_ref());
                self.decoded_cache.insert(cache_key, source.clone());
                source
            };
            self.occurrences[occurrence_id].source = Some(source.clone());
            self.occurrences[occurrence_id].base_url = response.final_url.clone();
            let mut ancestors = self.occurrences[occurrence_id].ancestors.clone();
            ancestors.push(normalized_url(&self.fetches[fetch_index].requested));
            ancestors.push(normalized_url(&response.final_url));
            ancestors.sort();
            ancestors.dedup();
            let children = self.discover_imports(
                &source,
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
        self.style_revision = self.style_revision.wrapping_add(1);
    }

    fn accepts_stylesheet_response(&self, response: &FetchResponse) -> bool {
        // A 4xx/5xx body now reaches us instead of being discarded as an error. It is
        // the server's error page, not a stylesheet — and because a missing or
        // unparseable type defaults to CSS below, without this a 404 page would be
        // parsed as CSS.
        if !response.is_success() {
            return false;
        }
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

    pub(super) fn disable_external(&mut self) {
        self.style_revision = self.style_revision.wrapping_add(1);
        self.external_disabled = true;
        self.cancel_requested = self.images.is_empty();
        self.raw_bytes = 0;
        self.decoded_bytes = 0;
        self.decoded_cache.clear();
        self.commands
            .retain(|command| self.image_fetch_index.contains_key(&command.resource_id));
        for fetch in &mut self.fetches {
            fetch.state = FetchState::Failed;
        }
        for occurrence in &mut self.occurrences {
            occurrence.source = None;
            occurrence.imports.clear();
        }
        self.invalidate_soft(RenderInvalidation::STYLE, RenderCause::Stylesheet);
    }
}

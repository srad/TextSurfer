use encoding_rs::{CoderResult, Decoder, Encoding};
use url::Url;

use crate::html::IncrementalHtmlParser;
use crate::net::html_encoding;

use super::{PageLoad, PageLoadOptions};

pub struct PendingPageLoad {
    parser: IncrementalHtmlParser,
    body: Vec<u8>,
    offset: usize,
    decoder: Decoder,
    document_url: Url,
    html_encoding: &'static Encoding,
    options: PageLoadOptions,
}

impl PendingPageLoad {
    pub fn new(
        source: String,
        document_url: Url,
        html_encoding: &'static Encoding,
        options: PageLoadOptions,
    ) -> Self {
        Self::from_encoded_bytes(source.into_bytes(), document_url, html_encoding, options)
    }

    pub fn from_bytes(
        body: Vec<u8>,
        header_charset: Option<&str>,
        document_url: Url,
        options: PageLoadOptions,
    ) -> Self {
        let encoding = html_encoding(&body, header_charset);
        Self::from_encoded_bytes(body, document_url, encoding, options)
    }

    fn from_encoded_bytes(
        body: Vec<u8>,
        document_url: Url,
        html_encoding: &'static Encoding,
        options: PageLoadOptions,
    ) -> Self {
        Self {
            parser: IncrementalHtmlParser::new(options.scripting),
            decoder: html_encoding.new_decoder_with_bom_removal(),
            body,
            offset: 0,
            document_url,
            html_encoding,
            options,
        }
    }

    pub fn step(&mut self, max_bytes: usize) -> Option<PageLoad> {
        let end = self
            .offset
            .saturating_add(max_bytes.max(1))
            .min(self.body.len());
        let last = end == self.body.len();
        loop {
            let input = &self.body[self.offset..end];
            let capacity = self
                .decoder
                .max_utf8_buffer_length(input.len())
                .unwrap_or(input.len().saturating_mul(3).saturating_add(16))
                .max(16);
            let mut decoded = String::with_capacity(capacity);
            let (result, read, _) = self.decoder.decode_to_string(input, &mut decoded, last);
            self.offset += read;
            if !decoded.is_empty() {
                self.parser.feed(&decoded);
            }
            if result == CoderResult::InputEmpty {
                break;
            }
        }
        if self.offset < self.body.len() {
            return None;
        }
        let outcome = self.parser.finish()?;
        let mut load = PageLoad::from_outcome(
            outcome,
            self.document_url.clone(),
            self.html_encoding,
            self.options,
        );
        load.defer_rendering();
        Some(load)
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.offset, self.body.len())
    }
}

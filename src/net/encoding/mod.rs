mod decoder;
mod prescan;

#[cfg(test)]
mod tests;

use encoding_rs::Encoding;

pub use decoder::{charset_from_content_type, decode, decode_text, html_encoding};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub encoding: &'static Encoding,
}

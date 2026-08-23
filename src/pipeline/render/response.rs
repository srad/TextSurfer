use mediatype::{MediaType, names};

use super::ResponseKind;

pub fn response_kind(content_type: Option<&str>) -> ResponseKind {
    let Some(content_type) = content_type else {
        return ResponseKind::Html;
    };
    let Ok(media_type) = MediaType::parse(content_type) else {
        return ResponseKind::Html;
    };
    if media_type.ty == names::TEXT && media_type.subty == names::HTML
        || media_type.ty == names::APPLICATION
            && media_type.subty == names::XHTML
            && media_type.suffix == Some(names::XML)
    {
        ResponseKind::Html
    } else if media_type.ty == names::TEXT && media_type.subty == names::PLAIN {
        ResponseKind::PlainText
    } else {
        ResponseKind::Unsupported(media_type.essence().to_string())
    }
}

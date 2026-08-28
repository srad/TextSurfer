use servo_arc::Arc as ServoArc;
use style::context::QuirksMode;
use style::media_queries::MediaList;
use style::shared_lock::SharedRwLock;
use style::stylesheets::{AllowImportRules, DocumentStyleSheet, Origin, Stylesheet, UrlExtraData};

/// A minimal user-agent sheet.
///
/// `src/css/ua.rs` is the real one, and it is 329 lines of typed policy that reads the palette for
/// link colours. Turning it into CSS text needs the palette split into a separate user-origin sheet,
/// which is S4/S5 work. This holds only enough to prove the rule database resolves, while the
/// plumbing around it — origin, lock, url data — is what the full sheet will reuse unchanged.
pub(super) const UA_CSS: &str = "\
html, body, div, p, h1, h2, h3, h4, h5, h6, ul, ol, li, table, form { display: block }
li { display: list-item }
head, script, style, title { display: none }
span, a, em, strong, b, i { display: inline }
p { margin-top: 1em; margin-bottom: 1em }
";

/// The base URL every synthetic sheet is parsed against. Sheets that come from the network carry
/// their own; nothing here resolves a relative URL, but `Stylesheet::from_str` requires one.
pub(super) fn base_url() -> UrlExtraData {
    UrlExtraData::from(
        url::Url::parse("about:style").expect("the static synthetic stylesheet base parses"),
    )
}

pub(super) fn parse(
    css: &str,
    origin: Origin,
    lock: &SharedRwLock,
    quirks_mode: QuirksMode,
) -> DocumentStyleSheet {
    let media = ServoArc::new(lock.wrap(MediaList::empty()));
    DocumentStyleSheet(ServoArc::new(Stylesheet::from_str(
        css,
        base_url(),
        origin,
        media,
        lock.clone(),
        // `@import` is refused here: the resource graph in `pipeline::page_load` owns import
        // discovery and its depth and occurrence budgets. S5 decides how Stylo's `StylesheetLoader`
        // meets that graph.
        None,
        None,
        quirks_mode,
        AllowImportRules::No,
    )))
}

pub(super) fn user_agent_sheet(lock: &SharedRwLock, quirks_mode: QuirksMode) -> DocumentStyleSheet {
    parse(UA_CSS, Origin::UserAgent, lock, quirks_mode)
}

pub(super) fn author_sheet(
    css: &str,
    lock: &SharedRwLock,
    quirks_mode: QuirksMode,
) -> DocumentStyleSheet {
    parse(css, Origin::Author, lock, quirks_mode)
}

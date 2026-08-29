use servo_arc::Arc as ServoArc;
use style::context::QuirksMode;
use style::media_queries::MediaList;
use style::shared_lock::SharedRwLock;
use style::stylesheets::{AllowImportRules, DocumentStyleSheet, Origin, Stylesheet, UrlExtraData};

use crate::core::style::Palette;

pub(super) const UA_CSS: &str = "\
@namespace \"http://www.w3.org/1999/xhtml\";
head, base, link, meta, title, style, script, template { display: none }
html, body, main, article, section, nav, aside, header, footer, address, div, center, p, pre,
blockquote, ul, ol, dl, dt, dd, figure, figcaption, form, fieldset, h1, h2, h3, h4, h5, h6, hr {
    display: block
}
li { display: list-item }
table { display: table; border-spacing: 8px 0; margin-bottom: 16px }
thead { display: table-header-group }
tbody { display: table-row-group }
tfoot { display: table-footer-group }
tr { display: table-row }
td, th { display: table-cell }
col { display: table-column }
colgroup { display: table-column-group }
caption { display: table-caption; text-align: center }
thead, tbody, tfoot { vertical-align: middle }
tr, td, th { vertical-align: inherit }
th { font-weight: bold; text-align: -moz-center-or-inherit }
pre { white-space: pre }
ul, menu { list-style-type: disc }
:is(ul, menu) :is(ul, menu) { list-style-type: circle }
:is(ul, menu) :is(ul, menu) :is(ul, menu) { list-style-type: square }
ol { list-style-type: decimal }
a:link { text-decoration-line: underline; cursor: pointer }
b, strong { font-weight: bold }
u, ins { text-decoration-line: underline }
s, del, strike { text-decoration-line: line-through }
p, pre, blockquote, ul, ol { margin-bottom: 16px }
blockquote { margin-left: 16px }
h1, h2, h3, h4, h5, h6 { margin-top: 16px; margin-bottom: 16px; font-weight: bold }
h1 { font-size: 2em }
h2 { font-size: 1.5em }
h3 { font-size: 1.17em }
h4 { font-size: 1em }
h5 { font-size: .83em }
h6 { font-size: .67em }
hr { margin-left: auto; margin-right: auto }
input:not([type=hidden i]), textarea, select, button { white-space: pre }
input:not([type=hidden i]), textarea { cursor: text }
button, select,
input:is([type=checkbox i], [type=radio i], [type=submit i], [type=reset i],
    [type=button i], [type=file i], [type=image i], [type=color i], [type=range i], [type=date i],
    [type=time i], [type=month i], [type=week i], [type=datetime-local i]) {
    cursor: pointer;
    text-align: center
}
textarea { display: block }
input[type=hidden i], option, optgroup, datalist { display: none }
";

/// The user-agent sheet plus the rule that seeds `ComputedStyle::color`'s "theme decides" sentinel.
///
/// Kept out of [`UA_CSS`] so the constant stays readable as a policy list while the sentinel stays
/// beside the mapper constant it has to agree with.
fn ua_css() -> String {
    format!("{UA_CSS}html {{ color: {} }}\n", super::map::SENTINEL_CSS)
}

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
    parse(&ua_css(), Origin::UserAgent, lock, quirks_mode)
}

pub(super) fn user_sheet(
    palette: Palette,
    lock: &SharedRwLock,
    quirks_mode: QuirksMode,
) -> DocumentStyleSheet {
    let link = palette.link;
    let hover = palette.link_hover;
    let css = format!(
        "@namespace \"http://www.w3.org/1999/xhtml\";\n\
         a:link {{ color: #{:02x}{:02x}{:02x} }}\n\
         a:link:hover {{ color: #{:02x}{:02x}{:02x} }}",
        link.r, link.g, link.b, hover.r, hover.g, hover.b
    );
    parse(&css, Origin::User, lock, quirks_mode)
}

pub(super) fn author_sheet(
    css: &str,
    lock: &SharedRwLock,
    quirks_mode: QuirksMode,
) -> DocumentStyleSheet {
    parse(css, Origin::Author, lock, quirks_mode)
}

use std::borrow::Cow;
use std::cell::{Cell, RefCell};

use cssparser::{Parser, ParserInput, SourceLocation};
use servo_arc::Arc as ServoArc;
use style::context::QuirksMode;
use style::custom_properties::AttrTaint;
use style::error_reporting::{ContextualParseError, ParseErrorReporter};
use style::media_queries::MediaList;
use style::parser::ParserContext;
use style::shared_lock::SharedRwLock;
use style::stylesheets::import_rule::{
    ImportLayer, ImportRule, ImportSheet, ImportSupportsCondition,
};
use style::stylesheets::{
    AllowImportRules, DocumentStyleSheet, Namespaces, Origin, Stylesheet, StylesheetLoader,
    UrlExtraData,
};
use style::values::CssUrl;
use style_traits::{ParsingMode, ToCss};

use crate::core::dom::DomQuirksMode;
use crate::core::style::Palette;
use crate::pipeline::render::StyleSource;

pub(crate) struct DiscoveredImport {
    pub url: url::Url,
    pub media: String,
}

#[derive(Default)]
pub(super) struct WarningCounter(Cell<usize>);

impl WarningCounter {
    pub(super) fn count(&self) -> usize {
        self.0.get()
    }
}

impl ParseErrorReporter for WarningCounter {
    fn report_error(
        &self,
        _url: &UrlExtraData,
        _location: SourceLocation,
        _error: ContextualParseError,
    ) {
        self.0.set(self.0.get().saturating_add(1));
    }
}

pub(crate) fn discover_imports(
    source: &str,
    base_url: &url::Url,
    quirks_mode: DomQuirksMode,
) -> Vec<DiscoveredImport> {
    super::engine::mark_layout_thread();
    super::prefs::enable();
    let lock = SharedRwLock::new();
    let loader = DiscoveryLoader {
        imports: RefCell::new(Vec::new()),
    };
    let sheet = Stylesheet::from_str(
        source,
        UrlExtraData::from(base_url.clone()),
        Origin::Author,
        ServoArc::new(lock.wrap(MediaList::empty())),
        lock,
        Some(&loader),
        None,
        stylo_quirks_mode(quirks_mode),
        AllowImportRules::Yes,
    );
    drop(sheet);
    loader.imports.into_inner()
}

pub(crate) fn media_matches(
    source: &str,
    media: crate::css::MediaContext,
    quirks_mode: DomQuirksMode,
) -> bool {
    super::engine::mark_layout_thread();
    super::prefs::enable();
    let quirks_mode = stylo_quirks_mode(quirks_mode);
    let url_data = base_url();
    parse_media(source, &url_data, quirks_mode).evaluate(
        &super::device::device_for_media(media, quirks_mode),
        quirks_mode,
        &mut style::stylesheets::CustomMediaEvaluator::none(),
    )
}

struct DiscoveryLoader {
    imports: RefCell<Vec<DiscoveredImport>>,
}

impl StylesheetLoader for DiscoveryLoader {
    fn request_stylesheet(
        &self,
        url: CssUrl,
        source_location: SourceLocation,
        lock: &SharedRwLock,
        media: ServoArc<style::shared_lock::Locked<MediaList>>,
        supports: Option<ImportSupportsCondition>,
        _layer: ImportLayer,
    ) -> ServoArc<style::shared_lock::Locked<ImportRule>> {
        if supports.as_ref().is_none_or(|condition| condition.enabled)
            && let Some(url) = url.url()
        {
            let guard = lock.read();
            self.imports.borrow_mut().push(DiscoveredImport {
                url: url.as_ref().clone(),
                media: media.read_with(&guard).to_css_string(),
            });
        }
        ServoArc::new(lock.wrap(ImportRule {
            url,
            stylesheet: ImportSheet::new_refused(),
            supports,
            layer: ImportLayer::None,
            source_location,
        }))
    }
}

fn stylo_quirks_mode(mode: DomQuirksMode) -> QuirksMode {
    match mode {
        DomQuirksMode::Quirks => QuirksMode::Quirks,
        DomQuirksMode::LimitedQuirks => QuirksMode::LimitedQuirks,
        DomQuirksMode::NoQuirks => QuirksMode::NoQuirks,
    }
}

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

pub(super) fn author_source(
    source: &StyleSource,
    lock: &SharedRwLock,
    quirks_mode: QuirksMode,
    reporter: &WarningCounter,
) -> DocumentStyleSheet {
    let url_data = UrlExtraData::from(source.base_url.clone());
    let media = ServoArc::new(lock.wrap(parse_media(&source.media, &url_data, quirks_mode)));
    DocumentStyleSheet(ServoArc::new(parse_source(
        source,
        media,
        lock,
        quirks_mode,
        reporter,
    )))
}

fn parse_source(
    source: &StyleSource,
    media: ServoArc<style::shared_lock::Locked<MediaList>>,
    lock: &SharedRwLock,
    quirks_mode: QuirksMode,
    reporter: &WarningCounter,
) -> Stylesheet {
    let loader = GraphLoader {
        children: &source.imports,
        next: Cell::new(0),
        quirks_mode,
        reporter,
    };
    Stylesheet::from_str(
        source.source.as_deref().unwrap_or(""),
        UrlExtraData::from(source.base_url.clone()),
        Origin::Author,
        media,
        lock.clone(),
        Some(&loader),
        Some(reporter),
        quirks_mode,
        AllowImportRules::Yes,
    )
}

struct GraphLoader<'a> {
    children: &'a [StyleSource],
    next: Cell<usize>,
    quirks_mode: QuirksMode,
    reporter: &'a WarningCounter,
}

impl StylesheetLoader for GraphLoader<'_> {
    fn request_stylesheet(
        &self,
        url: CssUrl,
        source_location: SourceLocation,
        lock: &SharedRwLock,
        media: ServoArc<style::shared_lock::Locked<MediaList>>,
        supports: Option<ImportSupportsCondition>,
        layer: ImportLayer,
    ) -> ServoArc<style::shared_lock::Locked<ImportRule>> {
        let child = self.children.get(self.next.get()).filter(|child| {
            url.url()
                .is_some_and(|resolved| resolved.as_ref() == &child.base_url)
        });
        if child.is_some() {
            self.next.set(self.next.get() + 1);
        }
        let stylesheet = if supports
            .as_ref()
            .is_some_and(|condition| !condition.enabled)
        {
            ImportSheet::new_refused()
        } else if let Some(child) = child.filter(|child| child.source.is_some()) {
            ImportSheet::new(ServoArc::new(parse_source(
                child,
                media,
                lock,
                self.quirks_mode,
                self.reporter,
            )))
        } else {
            ImportSheet::new_pending()
        };
        ServoArc::new(lock.wrap(ImportRule {
            url,
            stylesheet,
            supports,
            layer,
            source_location,
        }))
    }
}

fn parse_media(source: &str, url_data: &UrlExtraData, quirks_mode: QuirksMode) -> MediaList {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut context = ParserContext::new(
        Origin::Author,
        url_data,
        None,
        ParsingMode::empty(),
        quirks_mode,
        Cow::Owned(Namespaces::default()),
        None,
        None,
        AttrTaint::default(),
    );
    MediaList::parse(&mut context, &mut parser)
}

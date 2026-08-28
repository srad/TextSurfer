use crate::core::dom::{ElementNs, SharedDocument};
use crate::html::sink::ArenaTreeSink;
use html5ever::driver::Parser;
use html5ever::driver::parse_fragment_for_element;
use html5ever::tendril::StrTendril;
use html5ever::tendril::stream::TendrilSink;
use html5ever::tree_builder::{TreeBuilderOpts, create_element_with_flags};
use html5ever::{Attribute, LocalName, ParseOpts, QualName, ns, parse_document};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementContext {
    pub name: String,
    pub ns: ElementNs,
    pub attrs: Vec<(String, String)>,
}

pub struct ParseOutcome {
    pub document: SharedDocument,
    pub base_href: Option<String>,
    pub parse_errors: usize,
}

pub trait HtmlParser: Send + Sync {
    fn parse_document(&self, source: &str) -> ParseOutcome;
    fn parse_fragment(&self, source: &str, context: &ElementContext) -> ParseOutcome;
}

pub struct Html5everParser {
    scripting: bool,
}

pub struct IncrementalHtmlParser {
    parser: Option<Parser<ArenaTreeSink>>,
}

impl IncrementalHtmlParser {
    pub fn new(scripting: bool) -> Self {
        let parser = Html5everParser::new(scripting);
        Self {
            parser: Some(parse_document(
                ArenaTreeSink::new(false, None),
                parser.opts(),
            )),
        }
    }

    pub fn feed(&mut self, source: &str) {
        self.parser
            .as_mut()
            .expect("an unfinished parser exists")
            .process(StrTendril::from(source));
    }

    pub fn finish(&mut self) -> Option<ParseOutcome> {
        Some(self.parser.take()?.finish())
    }
}

impl Html5everParser {
    pub fn new(scripting: bool) -> Self {
        Self { scripting }
    }

    fn opts(&self) -> ParseOpts {
        ParseOpts {
            tokenizer: Default::default(),
            tree_builder: TreeBuilderOpts {
                scripting_enabled: self.scripting,
                exact_errors: false,
                ..Default::default()
            },
        }
    }

    fn context_qualname(context: &ElementContext) -> QualName {
        let ns = match context.ns {
            ElementNs::Html => ns!(html),
            ElementNs::Svg => ns!(svg),
            ElementNs::MathMl => ns!(mathml),
            ElementNs::Other => ns!(),
        };
        QualName::new(None, ns, LocalName::from(context.name.as_str()))
    }
}

impl HtmlParser for Html5everParser {
    fn parse_document(&self, source: &str) -> ParseOutcome {
        let sink = ArenaTreeSink::new(false, None);
        parse_document(sink, self.opts()).one(source.to_string())
    }

    fn parse_fragment(&self, source: &str, context: &ElementContext) -> ParseOutcome {
        let mut sink = ArenaTreeSink::new(true, None);
        let attrs: Vec<Attribute> = context
            .attrs
            .iter()
            .map(|(name, value)| Attribute {
                name: QualName::new(None, ns!(), LocalName::from(name.as_str())),
                value: StrTendril::from(value.as_str()),
            })
            .collect();
        let context_handle =
            create_element_with_flags(&sink, Self::context_qualname(context), attrs, false);
        sink.set_context(context_handle.node());
        parse_fragment_for_element(sink, self.opts(), context_handle, false, None)
            .one(source.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dom::{Document, DomQuirksMode, Node, NodeId};
    use crate::html::tree_dump;

    fn parser() -> Html5everParser {
        Html5everParser::new(false)
    }

    fn document_dump(html: &str) -> String {
        let outcome = parser().parse_document(html);
        tree_dump(&outcome.document.borrow(), false)
    }

    fn fragment_dump(html: &str, name: &str, ns: ElementNs) -> String {
        let context = ElementContext {
            name: name.to_string(),
            ns,
            attrs: vec![],
        };
        let outcome = parser().parse_fragment(html, &context);
        tree_dump(&outcome.document.borrow(), true)
    }

    fn elements(document: &Document) -> Vec<(String, ElementNs)> {
        fn walk(document: &Document, id: NodeId, out: &mut Vec<(String, ElementNs)>) {
            if let Some(Node::Element { name, ns, .. }) = document.node(id) {
                out.push((name.clone(), *ns));
                for child in document.children(id) {
                    walk(document, child, out);
                }
            }
        }
        let mut names = Vec::new();
        for root in document.roots() {
            walk(document, *root, &mut names);
        }
        names
    }

    #[test]
    fn misnesting_content_goes_through_adoption_agency() {
        insta::assert_snapshot!("parse_misnesting", document_dump("<b><i>x</b>y</i>"));
    }

    #[test]
    fn foster_parenting_hoists_table_content_before_table() {
        insta::assert_snapshot!(
            "parse_foster_table",
            document_dump("<table><b><tr><td>aaa</td></tr>bbb</table>ccc")
        );
    }

    #[test]
    fn template_contents_are_kept_inside_template() {
        insta::assert_snapshot!(
            "parse_template",
            document_dump("<template><div>Hello</div></template>")
        );
    }

    #[test]
    fn svg_and_foreign_object_are_namespaced() {
        insta::assert_snapshot!(
            "parse_svg",
            document_dump(
                "<svg viewBox=\"0 0 1 1\"><linearGradient/><foreignObject><p>x</p></foreignObject></svg>"
            )
        );
    }

    #[test]
    fn mathml_integration_point_breaks_out_to_html() {
        insta::assert_snapshot!(
            "parse_mathml",
            document_dump(
                "<math><mi>x</mi><annotation-xml encoding=\"text/html\"><p>y</p></annotation-xml></math>"
            )
        );
    }

    #[test]
    fn processing_instruction_variants() {
        insta::assert_snapshot!("parse_pi", document_dump("<?target data?><div>x</div>"));
    }

    #[test]
    fn entity_references_resolve() {
        let dump = document_dump("<p>&amp;&#65;&#x42;&auml;&ltopen;</p>");
        assert!(dump.contains("&AB\u{e4}"));
        insta::assert_snapshot!(
            "parse_entities",
            document_dump("<p>&amp;&#65;&#x42;&auml;</p>")
        );
    }

    #[test]
    fn doctype_variants_roundtrip() {
        insta::assert_snapshot!(
            "parse_doctype_public",
            document_dump(
                "<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\" \"http://www.w3.org/TR/html4/strict.dtd\"><p>x</p>"
            )
        );
        insta::assert_snapshot!(
            "parse_doctype_system",
            document_dump("<!DOCTYPE html SYSTEM \"about:legacy-compat\"><p>x</p>")
        );
    }

    #[test]
    fn duplicate_attributes_keep_first_value() {
        let dump = document_dump("<div a=\"1\" a=\"2\">x</div>");
        assert!(dump.contains("a=\"1\""));
        assert!(!dump.contains("a=\"2\""));
    }

    #[test]
    fn textarea_and_pre_preserve_leading_whitespace() {
        insta::assert_snapshot!(
            "parse_textarea_pre",
            document_dump("<textarea>\n  indented\n</textarea><pre>\n  \tpre\n</pre>")
        );
    }

    #[test]
    fn implied_end_tags_close_li() {
        insta::assert_snapshot!("parse_li", document_dump("<ul><li>a<li>b</ul>"));
    }

    #[test]
    fn select_in_table_switches_modes() {
        insta::assert_snapshot!(
            "parse_select_table",
            document_dump("<table><td><select><option>a</select></td></table>")
        );
    }

    #[test]
    fn simple_document_tree_is_exact() {
        let dump = document_dump(
            "<!DOCTYPE html><html><head><title>x</title></head><body><p>Hello</p></body></html>",
        );
        assert_eq!(
            dump,
            "#document\n\
             | <!DOCTYPE html>\n\
             | <html>\n\
             |   <head>\n\
             |     <title>\n\
             |       \"x\"\n\
             |   <body>\n\
             |     <p>\n\
             |       \"Hello\""
        );
    }

    #[test]
    fn omitted_tags_are_synthesized() {
        let dump = document_dump("<p>a<p>b");
        assert_eq!(
            dump,
            "#document\n\
             | <html>\n\
             |   <head>\n\
             |   <body>\n\
             |     <p>\n\
             |       \"a\"\n\
             |     <p>\n\
             |       \"b\""
        );
    }

    #[test]
    fn parser_builds_html_svg_mathml_elements() {
        let outcome = parser().parse_document("<svg><g></g></svg>");
        let names = elements(&outcome.document.borrow());
        assert_eq!(
            names,
            vec![
                ("html".into(), ElementNs::Html),
                ("head".into(), ElementNs::Html),
                ("body".into(), ElementNs::Html),
                ("svg".into(), ElementNs::Svg),
                ("g".into(), ElementNs::Svg),
            ]
        );
    }

    #[test]
    fn base_href_is_taken_from_first_base_element() {
        let outcome = parser().parse_document(
            "<head><base href=\"https://x.example/foo/\"></head><body><p>a</p></body>",
        );
        assert_eq!(outcome.base_href.as_deref(), Some("https://x.example/foo/"));
    }

    #[test]
    fn base_href_inside_template_is_ignored() {
        let outcome = parser().parse_document("<template><base href=\"https://bad.example/\"></template><base href=\"https://good.example/\">");
        assert_eq!(outcome.base_href.as_deref(), Some("https://good.example/"));
    }

    #[test]
    fn base_href_missing_when_no_base() {
        let outcome = parser().parse_document("<p>x</p>");
        assert_eq!(outcome.base_href, None);
    }

    #[test]
    fn parse_error_counter_ticks_on_malformed_input() {
        let clean = parser().parse_document("<!DOCTYPE html><p>x</p>");
        assert_eq!(clean.parse_errors, 0);
        let broken = parser().parse_document("<!DOCTYPE html><p>x</p");
        assert!(broken.parse_errors > 0);
    }

    #[test]
    fn parser_preserves_the_document_quirks_mode() {
        let standards = parser().parse_document("<!doctype html><p>x</p>");
        assert_eq!(
            standards.document.borrow().quirks_mode(),
            DomQuirksMode::NoQuirks
        );
        let quirks = parser().parse_document("<p>x</p>");
        assert_eq!(
            quirks.document.borrow().quirks_mode(),
            DomQuirksMode::Quirks
        );
        let limited = parser().parse_document(
            "<!DOCTYPE HTML PUBLIC \"-//W3C//DTD XHTML 1.0 Transitional//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd\"><p>x</p>",
        );
        assert_eq!(
            limited.document.borrow().quirks_mode(),
            DomQuirksMode::LimitedQuirks
        );
    }

    #[test]
    fn incremental_parser_preserves_tokens_split_across_chunks() {
        let mut parser = IncrementalHtmlParser::new(false);
        parser.feed("<!doctype html><p id=target>Gr");
        parser.feed("ü");
        parser.feed("ße &amp; hello</p>");
        let outcome = parser.finish().unwrap();
        assert!(outcome.document.borrow().element_by_id("target").is_some());
        assert_eq!(outcome.parse_errors, 0);
    }

    #[test]
    fn fragment_in_html_context_emits_children_directly() {
        assert_eq!(
            fragment_dump("<span>x</span>", "div", ElementNs::Html),
            "| <span>\n|   \"x\""
        );
    }

    #[test]
    fn fragment_multi_root_children() {
        let dump = fragment_dump("<p>a</p><p>b</p><!-- c -->", "body", ElementNs::Html);
        assert_eq!(dump, "| <p>\n|   \"a\"\n| <p>\n|   \"b\"\n| <!--  c  -->");
    }

    #[test]
    fn fragment_template_context_keeps_inner_html() {
        insta::assert_snapshot!(
            "parse_fragment_template",
            fragment_dump("<div>Hi</div><td>x</td>", "template", ElementNs::Html)
        );
    }

    #[test]
    fn fragment_table_cells_foster_in_context() {
        insta::assert_snapshot!(
            "parse_fragment_table",
            fragment_dump("<td>aaa</td>bbb", "table", ElementNs::Html)
        );
    }

    #[test]
    fn fragment_foreign_context_keeps_native_structure() {
        insta::assert_snapshot!(
            "parse_fragment_foreign",
            fragment_dump("<path d=\"\"/><div>x</div>", "svg", ElementNs::Svg)
        );
    }
}

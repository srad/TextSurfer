use crate::core::dom::{AttrNs, Document, ElementNs, Node, NodeId};

pub fn tree_dump(document: &Document, fragment: bool) -> String {
    let mut out = String::new();
    if !fragment {
        out.push_str("#document\n");
    }
    for root in document.roots() {
        let heads = if fragment {
            document.children(*root)
        } else {
            vec![*root]
        };
        for id in heads {
            dump_node(document, id, 0, &mut out);
        }
    }
    out.trim_end_matches('\n').to_string()
}

fn dump_node(document: &Document, id: NodeId, depth: usize, out: &mut String) {
    match document.node(id) {
        Some(Node::Element { name, ns, attrs }) => {
            indent(out, depth);
            out.push('<');
            out.push_str(ns_prefix(*ns));
            out.push_str(name);
            out.push_str(">\n");
            let mut attrs = attrs.clone();
            attrs.sort_by_key(|a| qualified_name(a.ns, &a.name));
            for attr in attrs {
                indent(out, depth + 1);
                out.push_str(&qualified_name(attr.ns, &attr.name));
                out.push_str("=\"");
                out.push_str(&attr.value);
                out.push_str("\"\n");
            }
            if name == "template" && *ns == ElementNs::Html {
                indent(out, depth + 1);
                out.push_str("content\n");
                for child in document.children(id) {
                    dump_node(document, child, depth + 2, out);
                }
            } else {
                for child in document.children(id) {
                    dump_node(document, child, depth + 1, out);
                }
            }
        }
        Some(Node::Text { data }) => {
            indent(out, depth);
            out.push('"');
            out.push_str(data);
            out.push_str("\"\n");
        }
        Some(Node::Comment { data }) => {
            indent(out, depth);
            out.push_str("<!-- ");
            out.push_str(data);
            out.push_str(" -->\n");
        }
        Some(Node::Pi { target, data }) => {
            indent(out, depth);
            out.push_str("<?");
            out.push_str(target);
            out.push(' ');
            out.push_str(data);
            out.push_str("?>\n");
        }
        Some(Node::Doctype {
            name,
            public_id,
            system_id,
        }) => {
            indent(out, depth);
            out.push_str("<!DOCTYPE ");
            out.push_str(name);
            if !public_id.is_empty() || !system_id.is_empty() {
                out.push_str(" \"");
                out.push_str(public_id);
                out.push_str("\" \"");
                out.push_str(system_id);
                out.push('"');
            }
            out.push_str(">\n");
        }
        None => {}
    }
}

fn indent(out: &mut String, depth: usize) {
    out.push_str("| ");
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn ns_prefix(ns: ElementNs) -> &'static str {
    match ns {
        ElementNs::Html => "",
        ElementNs::Svg => "svg ",
        ElementNs::MathMl => "math ",
        ElementNs::Other => "",
    }
}

fn qualified_name(ns: AttrNs, name: &str) -> String {
    match ns {
        AttrNs::None => name.to_string(),
        AttrNs::Xlink => format!("xlink {name}"),
        AttrNs::Xml => format!("xml {name}"),
        AttrNs::Xmlns => format!("xmlns {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dom::Attr;

    fn doc() -> Document {
        Document::new()
    }

    fn elem(parent: Option<NodeId>, document: &mut Document, name: &str) -> NodeId {
        document.insert_element(parent, name, ElementNs::Html, vec![])
    }

    #[test]
    fn empty_document_dumps_header_only() {
        let document = doc();
        assert_eq!(tree_dump(&document, false), "#document");
    }

    #[test]
    fn text_root_is_quoted() {
        let mut document = doc();
        document.insert_text(None, "Hello");
        assert_eq!(tree_dump(&document, false), "#document\n| \"Hello\"");
    }

    #[test]
    fn attributes_sort_and_indent_below_element() {
        let mut document = doc();
        let div = document.insert_element(
            None,
            "div",
            ElementNs::Html,
            vec![Attr::plain("id", "b"), Attr::plain("class", "a")],
        );
        document.insert_text(Some(div), "x");
        assert_eq!(
            tree_dump(&document, false),
            "#document\n| <div>\n|   class=\"a\"\n|   id=\"b\"\n|   \"x\""
        );
    }

    #[test]
    fn attribute_sort_is_utf16_code_unit_order() {
        let mut document = doc();
        document.insert_element(
            None,
            "div",
            ElementNs::Html,
            vec![
                Attr::plain("\u{e9}", "1"),
                Attr::plain("z", "2"),
                Attr::plain("a", "3"),
            ],
        );
        assert_eq!(
            tree_dump(&document, false),
            "#document\n| <div>\n|   a=\"3\"\n|   z=\"2\"\n|   \u{e9}=\"1\""
        );
    }

    #[test]
    fn nested_elements_indent() {
        let mut document = doc();
        let html = elem(None, &mut document, "html");
        elem(Some(html), &mut document, "head");
        let body = elem(Some(html), &mut document, "body");
        elem(Some(body), &mut document, "p");
        assert_eq!(
            tree_dump(&document, false),
            "#document\n| <html>\n|   <head>\n|   <body>\n|     <p>"
        );
    }

    #[test]
    fn text_with_embedded_newline_is_verbatim() {
        let mut document = doc();
        let p = elem(None, &mut document, "p");
        document.insert_text(Some(p), "line1\nline2");
        assert_eq!(
            tree_dump(&document, false),
            "#document\n| <p>\n|   \"line1\nline2\""
        );
    }

    #[test]
    fn comment_pi_and_doctypes() {
        let mut document = doc();
        document.insert_doctype(None, "html", "", "");
        document.insert_comment(None, "hello");
        document.insert_pi(None, "target", "data");
        assert_eq!(
            tree_dump(&document, false),
            "#document\n| <!DOCTYPE html>\n| <!-- hello -->\n| <?target data?>"
        );
    }

    #[test]
    fn doctype_with_public_and_system_ids() {
        let mut document = doc();
        document.insert_doctype(
            None,
            "html",
            "-//W3C//DTD HTML 4.01//EN",
            "http://www.w3.org/TR/html4/strict.dtd",
        );
        assert_eq!(
            tree_dump(&document, false),
            "#document\n| <!DOCTYPE html \"-//W3C//DTD HTML 4.01//EN\" \"http://www.w3.org/TR/html4/strict.dtd\">"
        );
    }

    #[test]
    fn doctype_with_system_only() {
        let mut document = doc();
        document.insert_doctype(None, "html", "", "about:legacy-compat");
        assert_eq!(
            tree_dump(&document, false),
            "#document\n| <!DOCTYPE html \"\" \"about:legacy-compat\">"
        );
    }

    #[test]
    fn svg_and_math_namespaces_get_prefixes() {
        let mut document = doc();
        let svg = document.insert_element(None, "svg", ElementNs::Svg, vec![]);
        let g = document.insert_element(Some(svg), "g", ElementNs::Svg, vec![]);
        document.insert_text(Some(g), "text");
        let math = document.insert_element(None, "math", ElementNs::MathMl, vec![]);
        document.insert_element(Some(math), "mi", ElementNs::MathMl, vec![]);
        assert_eq!(
            tree_dump(&document, false),
            "#document\n\
             | <svg svg>\n\
             |   <svg g>\n\
             |     \"text\"\n\
             | <math math>\n\
             |   <math mi>"
        );
    }

    #[test]
    fn namespaced_attributes_keep_prefixes() {
        let mut document = doc();
        let svg = document.insert_element(None, "svg", ElementNs::Svg, vec![]);
        document.insert_element(
            Some(svg),
            "g",
            ElementNs::Svg,
            vec![
                Attr::namespaced(AttrNs::Xlink, "href", "#x"),
                Attr::namespaced(AttrNs::Xml, "lang", "en"),
                Attr::namespaced(AttrNs::Xmlns, "xlink", "http://x"),
                Attr::plain("fill", "none"),
            ],
        );
        assert_eq!(
            tree_dump(&document, false),
            "#document\n\
             | <svg svg>\n\
             |   <svg g>\n\
             |     fill=\"none\"\n\
             |     xlink href=\"#x\"\n\
             |     xml lang=\"en\"\n\
             |     xmlns xlink=\"http://x\""
        );
    }

    #[test]
    fn template_emits_content_pseudo_child() {
        let mut document = doc();
        let html = elem(None, &mut document, "html");
        let head = elem(Some(html), &mut document, "head");
        let template = document.insert_element(Some(head), "template", ElementNs::Html, vec![]);
        document.insert_text(Some(template), "Hello");
        assert_eq!(
            tree_dump(&document, false),
            "#document\n\
             | <html>\n\
             |   <head>\n\
             |     <template>\n\
             |       content\n\
             |         \"Hello\""
        );
    }

    #[test]
    fn empty_template_still_emits_content() {
        let mut document = doc();
        let html = elem(None, &mut document, "html");
        let head = elem(Some(html), &mut document, "head");
        document.insert_element(Some(head), "template", ElementNs::Html, vec![]);
        let body = elem(Some(html), &mut document, "body");
        document.insert_element(Some(body), "div", ElementNs::Html, vec![]);
        assert_eq!(
            tree_dump(&document, false),
            "#document\n\
             | <html>\n\
             |   <head>\n\
             |     <template>\n\
             |       content\n\
             |   <body>\n\
             |     <div>"
        );
    }

    #[test]
    fn fragment_mode_dumps_context_children_at_depth_zero() {
        let mut document = doc();
        let context = document.insert_element(None, "body", ElementNs::Html, vec![]);
        let p = elem(Some(context), &mut document, "p");
        document.insert_text(Some(p), "Hi");
        assert_eq!(tree_dump(&document, true), "| <p>\n|   \"Hi\"");
    }

    #[test]
    fn fixture_page_dumps_to_snapshot() {
        let mut document = doc();
        let html = document.insert_element(
            None,
            "html",
            ElementNs::Html,
            vec![Attr::namespaced(AttrNs::Xml, "lang", "en")],
        );
        let head = elem(Some(html), &mut document, "head");
        document.insert_element(Some(head), "title", ElementNs::Html, vec![]);
        document.insert_text(Some(head), "\u{a0}");
        let body = elem(Some(html), &mut document, "body");
        let ul = document.insert_element(
            Some(body),
            "ul",
            ElementNs::Html,
            vec![Attr::plain("data-n", "1")],
        );
        let li = document.insert_element(Some(ul), "li", ElementNs::Html, vec![]);
        document.insert_text(Some(li), "one");
        let li2 = document.insert_element(Some(ul), "li", ElementNs::Html, vec![]);
        document.insert_text(Some(li2), "two");
        insta::assert_snapshot!("fixture_page_dump", tree_dump(&document, false));
    }
}

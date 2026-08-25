use encoding_rs::UTF_8;

use super::*;
use crate::css::{DynamicState, FocusSource, FocusedNode};
use crate::net::FetchError;
use crate::ui::theme::PAPER_WHITE;

fn load(source: &str) -> PageLoad {
    PageLoad::new(
        source,
        Url::parse("https://example.com/dir/page").unwrap(),
        UTF_8,
        PageLoadOptions {
            render: crate::core::style::RenderContext::terminal(Size { cols: 80, rows: 24 }),
            palette: Palette::default(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
        },
    )
}

#[test]
fn discovers_ordered_links_and_honors_base_filters_and_fragments() {
    let mut load = load(
        "<base href='/assets/'><link rel='alternate stylesheet' href='skip.css'>
             <link rel='stylesheet' href='a.css#one'><link rel='STYLESHEET' href='a.css#two'>
             <link rel='stylesheet' disabled href='disabled.css'>
             <link rel='stylesheet' type='text/plain' href='plain.css'>",
    );
    let commands = load.take_commands();
    assert_eq!(load.external_occurrences(), 2);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].url.as_str(), "https://example.com/assets/a.css");
}

#[test]
fn a_remote_page_cannot_pull_a_subresource_from_another_scheme() {
    let mut load = load(
        "<link rel=stylesheet href='file:///C:/secrets.css'>
             <link rel=stylesheet href='https://cdn.example/ok.css'>",
    );
    let commands = load.take_commands();
    assert_eq!(
        commands
            .iter()
            .map(|command| command.url.as_str())
            .collect::<Vec<_>>(),
        ["https://cdn.example/ok.css"],
        "only the same-scheme sheet is fetched"
    );
    assert_eq!(load.failed_resources(), 1);
}

#[test]
fn a_local_page_may_still_load_its_own_local_stylesheets() {
    let mut load = PageLoad::new(
        "<link rel=stylesheet href='theme.css'>",
        Url::parse("file:///C:/site/page.html").unwrap(),
        UTF_8,
        PageLoadOptions {
            render: crate::core::style::RenderContext::terminal(Size { cols: 80, rows: 24 }),
            palette: Palette::default(),
            scripting: false,
            color_scheme: ColorScheme::Dark,
            started: Duration::ZERO,
        },
    );
    let commands = load.take_commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].url.as_str(), "file:///C:/site/theme.css");
}

#[test]
fn applicable_sheets_block_until_ready_and_nonmatching_sheets_do_not() {
    let mut matching = load("<link rel=stylesheet href='a.css'><p>x</p>");
    assert!(matching.render_if_ready(Duration::from_secs(4)).is_none());
    assert!(matching.render_if_ready(Duration::from_secs(5)).is_some());

    let mut print = load("<link rel=stylesheet media=print href='a.css'><p>x</p>");
    assert!(print.render_if_ready(Duration::ZERO).is_some());
}

#[test]
fn imports_resolve_against_final_url_and_cycles_stop() {
    let mut load = load("<link rel=stylesheet href='redirect.css'>");
    let root = load.take_commands().pop().unwrap();
    assert!(load.deliver(
        root.resource_id,
        Ok(FetchResponse {
            final_url: Url::parse("https://cdn.example/css/main.css").unwrap(),
            status: 200,
            body: b"@import 'child.css'; p { display: block }".to_vec(),
            content_type: Some("text/css".to_string()),
        }),
    ));
    let child = load.take_commands().pop().unwrap();
    assert_eq!(child.url.as_str(), "https://cdn.example/css/child.css");
    assert!(load.deliver(
        child.resource_id,
        Ok(FetchResponse {
            final_url: child.url,
            status: 200,
            body: b"@import 'main.css';".to_vec(),
            content_type: Some("text/css".to_string()),
        }),
    ));
    assert!(load.is_settled());
    assert_eq!(load.take_commands().len(), 0);
}

#[test]
fn invalid_non_css_mime_fails_only_that_resource() {
    let mut load =
        load("<!doctype html><link rel=stylesheet href='a.css'><style>p { display:block }</style>");
    let command = load.take_commands().pop().unwrap();
    assert!(load.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: command.url,
            status: 200,
            body: b"p { display:none }".to_vec(),
            content_type: Some("image/png".to_string()),
        }),
    ));
    assert_eq!(load.failed_resources(), 1);
    assert!(load.render_if_ready(Duration::ZERO).is_some());
}

#[test]
fn an_error_status_is_never_accepted_as_a_stylesheet() {
    // Non-2xx bodies now reach us instead of being discarded as errors, and a missing
    // or unparseable type otherwise defaults to CSS — so without the status check the
    // server's 404 page would be parsed as a stylesheet and its rules applied.
    for content_type in [
        None,
        Some("text/css".to_string()),
        Some("text/html".to_string()),
    ] {
        let mut load = load("<!doctype html><link rel=stylesheet href='a.css'><p>x</p>");
        let command = load.take_commands().pop().unwrap();
        assert!(load.deliver(
            command.resource_id,
            Ok(FetchResponse {
                final_url: command.url,
                status: 404,
                body: b"p { display:none }".to_vec(),
                content_type: content_type.clone(),
            }),
        ));
        assert_eq!(
            load.failed_resources(),
            1,
            "a {content_type:?} 404 is a failed resource, not a stylesheet"
        );
        let page = load
            .render_if_ready(Duration::ZERO)
            .expect("the page still renders");
        assert!(
            page.painted
                .text_lines()
                .iter()
                .any(|line| line.contains('x')),
            "and the 404's rules were not applied to it"
        );
    }
}

fn css_response(command: FetchCommand, body: &[u8]) -> (ResourceId, FetchResponse) {
    (
        command.resource_id,
        FetchResponse {
            final_url: command.url,
            status: 200,
            body: body.to_vec(),
            content_type: Some("text/css".to_string()),
        },
    )
}

#[test]
fn completion_order_never_changes_document_cascade_order() {
    let mut load = load(
        "<!doctype html><link rel=stylesheet href='a.css'>
             <link rel=stylesheet href='b.css'><p>visible</p>",
    );
    let commands = load.take_commands();
    let a = commands
        .iter()
        .find(|command| command.url.path().ends_with("a.css"))
        .unwrap();
    let b = commands
        .iter()
        .find(|command| command.url.path().ends_with("b.css"))
        .unwrap();
    let (id, response) = css_response(b.clone(), b"p { display: block }");
    assert!(load.deliver(id, Ok(response)));
    assert!(load.render_if_ready(Duration::ZERO).is_none());
    let (id, response) = css_response(a.clone(), b"p { display: none }");
    assert!(load.deliver(id, Ok(response)));
    let page = load.render_if_ready(Duration::ZERO).unwrap();
    assert!(
        page.painted
            .text_lines()
            .iter()
            .any(|line| line == "visible")
    );
}

#[test]
fn repeated_occurrences_fetch_once_and_keep_each_source_position() {
    let mut load = load(
        "<!doctype html><link rel=stylesheet href='same.css'>
             <style>p { display: none }</style>
             <link rel=stylesheet href='same.css'><p>visible</p>",
    );
    let commands = load.take_commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(load.external_occurrences(), 2);
    let (id, response) = css_response(commands[0].clone(), b"p { display: block }");
    assert!(load.deliver(id, Ok(response)));
    let page = load.render_if_ready(Duration::ZERO).unwrap();
    assert!(
        page.painted
            .text_lines()
            .iter()
            .any(|line| line == "visible")
    );
}

#[test]
fn deadline_and_late_repaint_are_exact_and_coalesced() {
    let mut load = load(
        "<!doctype html><link rel=stylesheet href='a.css'>
             <link rel=stylesheet href='b.css'><p>visible</p>",
    );
    let commands = load.take_commands();
    assert!(
        load.render_if_ready(STYLESHEET_DEADLINE - Duration::from_nanos(1))
            .is_none()
    );
    assert!(load.render_if_ready(STYLESHEET_DEADLINE).is_some());
    let (id, response) = css_response(commands[0].clone(), b"p { display: none }");
    assert!(load.deliver(id, Ok(response)));
    assert!(load.render_if_ready(STYLESHEET_DEADLINE).is_none());
    let (id, response) = css_response(commands[1].clone(), b"p { display: block }");
    assert!(load.deliver(id, Ok(response)));
    assert!(load.render_if_ready(STYLESHEET_DEADLINE).is_some());
    assert!(load.render_if_ready(STYLESHEET_DEADLINE).is_none());
    let (_, duplicate) = css_response(commands[1].clone(), b"p { display: none }");
    assert!(!load.deliver(id, Ok(duplicate)));
    assert!(!load.deliver(
        ResourceId(999),
        Err(FetchError::Network("unknown".to_string()))
    ));
}

#[test]
fn newly_applicable_pending_sheet_repaints_without_blank_loading_state() {
    let mut load = load(
        "<!doctype html><link rel=stylesheet media='(min-width: 100ch)' href='wide.css'>
             <p>visible</p>",
    );
    let command = load.take_commands().pop().unwrap();
    assert!(load.render_if_ready(Duration::ZERO).is_some());
    assert!(
        load.resize(Size {
            cols: 120,
            rows: 24
        })
        .is_some()
    );
    let (id, response) = css_response(command, b"p { display: none }");
    assert!(load.deliver(id, Ok(response)));
    let page = load.render_if_ready(Duration::ZERO).unwrap();
    assert!(
        !page
            .painted
            .text_lines()
            .iter()
            .any(|line| line == "visible")
    );
}

#[test]
fn newly_applicable_color_scheme_sheet_repaints_without_blank_loading_state() {
    let mut load = load(
        "<!doctype html><link rel=stylesheet media='(prefers-color-scheme: light)' href='light.css'>
             <p>visible</p>",
    );
    let command = load.take_commands().pop().unwrap();
    assert!(load.render_if_ready(Duration::ZERO).is_some());
    let page = load
        .recolor(PAPER_WHITE.palette(), ColorScheme::Light)
        .expect("the settled page repaints immediately");
    assert!(
        page.painted
            .text_lines()
            .iter()
            .any(|line| line == "visible")
    );
    assert!(!load.final_painted);

    let (id, response) = css_response(command, b"p { display: none }");
    assert!(load.deliver(id, Ok(response)));
    let page = load.render_if_ready(Duration::ZERO).unwrap();
    assert!(
        !page
            .painted
            .text_lines()
            .iter()
            .any(|line| line == "visible")
    );
}

#[test]
fn deferred_color_context_marks_a_painted_page_dirty_and_noops_when_unchanged() {
    let mut load = load("<p>visible</p>");
    assert!(load.render_if_ready(Duration::ZERO).is_some());
    assert!(load.set_color_context(PAPER_WHITE.palette(), ColorScheme::Light));
    assert!(load.dirty);
    assert!(!load.set_color_context(PAPER_WHITE.palette(), ColorScheme::Light));
}

#[test]
fn occurrence_and_byte_ceiling_failures_discard_all_external_css() {
    let links = (0..=MAX_EXTERNAL_OCCURRENCES)
        .map(|index| format!("<link rel=stylesheet href='{index}.css'>"))
        .collect::<String>();
    let mut too_many = load(&format!("<!doctype html>{links}<p>visible</p>"));
    assert!(too_many.external_disabled());
    assert!(too_many.take_commands().is_empty());

    let mut too_large = load(
        "<!doctype html><style>p { display:block }</style>
             <link rel=stylesheet href='large.css'><p>visible</p>",
    );
    let command = too_large.take_commands().pop().unwrap();
    too_large.raw_bytes = MAX_EXTERNAL_BYTES;
    let (id, response) = css_response(command, b"p { display:none }");
    assert!(too_large.deliver(id, Ok(response)));
    assert!(too_large.external_disabled());
    assert!(too_large.take_cancel_requested());
    let page = too_large.force_render();
    assert!(
        page.painted
            .text_lines()
            .iter()
            .any(|line| line == "visible")
    );
}

#[test]
fn one_fetch_can_produce_two_environment_decodings() {
    let mut load = load(
        "<!doctype html><link rel=stylesheet href='one.css'>
             <link rel=stylesheet href='two.css'><p>x</p>",
    );
    let commands = load.take_commands();
    for command in commands {
        let charset = if command.url.path().ends_with("one.css") {
            "windows-1252"
        } else {
            "utf-8"
        };
        assert!(load.deliver(
            command.resource_id,
            Ok(FetchResponse {
                final_url: command.url,
                status: 200,
                body: b"@import 'shared.css';".to_vec(),
                content_type: Some(format!("text/css; charset={charset}")),
            }),
        ));
    }
    let shared = load.take_commands();
    assert_eq!(shared.len(), 1);
    let (id, response) = css_response(shared[0].clone(), b"p { display:block }");
    assert!(load.deliver(id, Ok(response)));
    let variants = load
        .decoded_cache
        .keys()
        .filter(|(resource, _)| *resource == shared[0].resource_id)
        .count();
    assert_eq!(variants, 2);
}

#[test]
fn import_depth_is_bounded_to_eight_edges() {
    let mut load = load("<!doctype html><link rel=stylesheet href='0.css'><p>x</p>");
    for depth in 0..=MAX_IMPORT_DEPTH {
        let commands = load.take_commands();
        assert_eq!(commands.len(), 1, "depth {depth}");
        let command = commands[0].clone();
        let body = format!("@import '{}.css';", depth + 1);
        assert!(load.deliver(
            command.resource_id,
            Ok(FetchResponse {
                final_url: command.url,
                status: 200,
                body: body.into_bytes(),
                content_type: Some("text/css".to_string()),
            }),
        ));
    }
    assert!(load.take_commands().is_empty());
    assert!(load.is_settled());
}

#[test]
fn dynamic_state_restyles_only_when_author_or_ua_rules_can_observe_it() {
    let mut inert = load("<!doctype html><p id=x>x</p>");
    inert.force_render();
    let x = inert.document.borrow().element_by_id("x").unwrap();
    assert!(
        inert
            .set_dynamic_state(DynamicState {
                hover: Some(x),
                ..Default::default()
            })
            .is_none()
    );

    let mut authored = load("<!doctype html><style>#x:hover { color: red }</style><p id=x>x</p>");
    authored.force_render();
    let x = authored.document.borrow().element_by_id("x").unwrap();
    let progress = (
        authored.first_painted,
        authored.final_painted,
        authored.dirty,
    );
    assert!(
        authored
            .set_dynamic_state(DynamicState {
                hover: Some(x),
                ..Default::default()
            })
            .is_some()
    );
    assert_eq!(
        (
            authored.first_painted,
            authored.final_painted,
            authored.dirty
        ),
        progress
    );
}

#[test]
fn ua_link_hover_uses_the_ancestor_chain_and_suppresses_same_link_rehits() {
    let mut load = load("<!doctype html><a id=link href=/><span id=span>target</span></a>");
    load.force_render();
    let document = load.document.borrow();
    let link = document.element_by_id("link").unwrap();
    let span = document.element_by_id("span").unwrap();
    let text = document.first_child(span).unwrap();
    drop(document);
    assert!(
        load.set_dynamic_state(DynamicState {
            hover: Some(text),
            ..Default::default()
        })
        .is_some()
    );
    assert!(
        load.set_dynamic_state(DynamicState {
            hover: Some(link),
            ..Default::default()
        })
        .is_none()
    );
}

#[test]
fn prepaint_state_is_retained_and_focus_source_changes_are_observable() {
    let mut load = load(
        "<!doctype html><style>#x:focus-visible { color: red }</style><button id=x>x</button>",
    );
    let x = load.document.borrow().element_by_id("x").unwrap();
    assert!(
        load.set_dynamic_state(DynamicState {
            focus: Some(FocusedNode {
                node: x,
                source: FocusSource::Pointer,
            }),
            ..Default::default()
        })
        .is_none()
    );
    let first = load.force_render();
    assert_eq!(first.styles.get(x).color, None);
    let keyboard = load.set_dynamic_state(DynamicState {
        focus: Some(FocusedNode {
            node: x,
            source: FocusSource::Keyboard,
        }),
        ..Default::default()
    });
    assert!(keyboard.is_some());
}

#[test]
fn css_decoding_obeys_protocol_charset_at_charset_and_bom_precedence() {
    let cases = [
        (
            b"p { display:block }".to_vec(),
            Some("text/css; charset=windows-1252".to_string()),
            "windows-1252",
        ),
        (
            b"@charset \"windows-1252\"; p { display:block }".to_vec(),
            Some("text/css".to_string()),
            "windows-1252",
        ),
        (
            vec![0xFF, 0xFE, b'p', 0, b' ', 0, b'{', 0, b'}', 0],
            Some("text/css".to_string()),
            "UTF-16LE",
        ),
    ];
    for (body, content_type, expected) in cases {
        let mut load = load("<!doctype html><link rel=stylesheet href='sheet.css'><p>x</p>");
        let command = load.take_commands().pop().unwrap();
        assert!(load.deliver(
            command.resource_id,
            Ok(FetchResponse {
                final_url: command.url,
                status: 200,
                body,
                content_type,
            }),
        ));
        assert!(
            load.decoded_cache
                .keys()
                .any(|(_, encoding)| *encoding == expected)
        );
    }
}

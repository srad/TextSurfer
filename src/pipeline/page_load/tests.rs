use encoding_rs::UTF_8;

use super::*;
use crate::core::image::ImageDecodeError;
use crate::css::{DynamicState, FocusSource, FocusedNode};
use crate::net::{FetchError, FetchResponse};
use crate::pipeline::render::RenderKey;
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
fn pending_documents_decode_character_boundaries_incrementally() {
    for (body, charset, expected) in [
        (
            "<!doctype html><p>Grüße</p>".as_bytes().to_vec(),
            None,
            "Grüße",
        ),
        (b"\xFF\xFE<\0p\0>\0h\0i\0<\0/\0p\0>\0".to_vec(), None, "hi"),
        (
            b"<!doctype html><p>bad \xFF byte</p>".to_vec(),
            Some("utf-8"),
            "bad � byte",
        ),
    ] {
        let total = body.len();
        let mut pending = PendingPageLoad::from_bytes(
            body,
            charset,
            Url::parse("https://example.com/incremental").unwrap(),
            PageLoadOptions {
                render: crate::core::style::RenderContext::terminal(Size { cols: 80, rows: 24 }),
                palette: Palette::default(),
                scripting: false,
                color_scheme: ColorScheme::Dark,
                started: Duration::ZERO,
            },
        );
        let mut completed = None;
        while completed.is_none() {
            completed = pending.step(1);
        }
        assert_eq!(pending.progress(), (total, total));
        let page = completed.unwrap().force_render();
        assert!(page.painted.text_lines().join("\n").contains(expected));
    }
}

#[test]
fn a_render_result_from_an_old_epoch_is_discarded() {
    let mut load = load("<p>old layout</p>");
    load.defer_rendering();
    assert!(load.render_if_ready(Duration::ZERO).is_none());
    let key = RenderKey {
        tab_id: 7,
        generation: 11,
        epoch: load.render_epoch(),
        hard_epoch: load.hard_epoch(),
    };
    let result = load.take_render_job(key).unwrap().execute();
    assert!(
        load.resize(Size { cols: 40, rows: 24 }).is_none(),
        "deferred rendering must not publish on the owner thread"
    );
    assert!(load.apply_render_result(result).is_none());
    assert!(load.has_render_work());
}

#[test]
fn a_coherent_render_survives_a_late_stylesheet_soft_revision() {
    let mut load = load("<link rel=stylesheet href=a.css><p>first paint</p>");
    let command = load.take_commands().pop().unwrap();
    load.defer_rendering();
    assert!(load.render_if_ready(STYLESHEET_DEADLINE).is_none());
    let key = RenderKey {
        tab_id: 7,
        generation: 11,
        epoch: load.render_epoch(),
        hard_epoch: load.hard_epoch(),
    };
    let result = load.take_render_job(key).unwrap().execute();
    assert!(load.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: command.url,
            status: 200,
            body: b"p { color: red }".to_vec(),
            content_type: Some("text/css".to_string()),
        }),
    ));
    assert!(load.apply_render_result(result).is_some());
    assert!(load.has_render_work());
}

#[test]
fn coalesced_render_causes_follow_the_job_without_leaking_across_epochs() {
    let mut load = load("<p>causes</p>");
    load.defer_rendering();
    assert!(load.render_if_ready(Duration::ZERO).is_none());
    let initial_key = RenderKey {
        tab_id: 7,
        generation: 11,
        epoch: load.render_epoch(),
        hard_epoch: load.hard_epoch(),
    };
    let initial = load.take_render_job(initial_key).unwrap();
    assert!(initial.causes.contains(RenderCause::Initial));
    assert!(!initial.causes.contains(RenderCause::Viewport));

    load.set_viewport(Size { cols: 40, rows: 24 });
    load.invalidate_soft(RenderInvalidation::Layout, RenderCause::Image);
    load.invalidate_soft(RenderInvalidation::Style, RenderCause::Stylesheet);
    assert!(load.apply_render_result(initial.execute()).is_none());

    let next_key = RenderKey {
        tab_id: 7,
        generation: 11,
        epoch: load.render_epoch(),
        hard_epoch: load.hard_epoch(),
    };
    let next = load.take_render_job(next_key).unwrap();
    assert!(next.causes.contains(RenderCause::Viewport));
    assert!(next.causes.contains(RenderCause::Image));
    assert!(next.causes.contains(RenderCause::Stylesheet));
    assert!(!next.causes.contains(RenderCause::Initial));
}

#[test]
fn scripting_off_discovers_duckduckgos_noscript_refresh() {
    let load = load(
        "<script>window.parent.location.replace('bad')</script>
         <noscript><meta http-equiv='refresh'
         content='0;URL=https://en.wikipedia.org/wiki/Central_processing_unit'></noscript>",
    );
    assert_eq!(
        load.immediate_refresh().map(Url::as_str),
        Some("https://en.wikipedia.org/wiki/Central_processing_unit")
    );
}

#[test]
fn declarative_refresh_uses_the_document_base_and_whatwg_prefix_grammar() {
    for (content, expected) in [
        ("0; URL=../next", Some("https://example.com/next")),
        ("0,URL=\"/quoted\" tail", Some("https://example.com/quoted")),
        (".5 /fraction", Some("https://example.com/fraction")),
        ("nope;url=/bad", None),
    ] {
        let load = load(&format!(
            "<base href='/dir/'><meta http-equiv=ReFrEsH content='{content}'>"
        ));
        assert_eq!(load.immediate_refresh().map(Url::as_str), expected);
    }

    let self_refresh = load("<base href='/other/'><meta http-equiv=refresh content='0'>");
    assert_eq!(
        self_refresh.immediate_refresh().map(Url::as_str),
        Some("https://example.com/dir/page")
    );
}

#[test]
fn the_first_accepted_refresh_wins_and_delayed_or_javascript_targets_are_inert() {
    let delayed = load(
        "<meta http-equiv=refresh content='999999999999999999999999999999;url=/later'>
         <meta http-equiv=refresh content='0;url=/wrong'>",
    );
    assert_eq!(delayed.immediate_refresh(), None);

    let safe = load(
        "<meta http-equiv=refresh content='0;javascript:alert(1)'>
         <meta http-equiv=refresh content='0;url=/safe'>",
    );
    assert_eq!(
        safe.immediate_refresh().map(Url::as_str),
        Some("https://example.com/safe")
    );
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
    assert!(
        matching
            .render_if_ready(Duration::from_millis(249))
            .is_none()
    );
    assert!(
        matching
            .render_if_ready(Duration::from_millis(250))
            .is_some()
    );

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
    assert!(load.is_dirty());
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
fn image_fetches_deduplicate_resolve_and_never_block_first_paint() {
    let mut load =
        load("<base href='/assets/'><img id=one src='cat.png'><img id=two src='./cat.png'>");
    let commands = load.take_commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0].url.as_str(),
        "https://example.com/assets/cat.png"
    );
    assert!(load.render_if_ready(Duration::ZERO).is_some());
    assert!(!load.is_settled());
}

#[test]
fn image_fetches_require_success_and_have_an_independent_byte_budget() {
    let mut failed = load("<img src='missing.png'>");
    let command = failed.take_commands().pop().unwrap();
    assert!(failed.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: command.url,
            status: 404,
            body: vec![1, 2, 3],
            content_type: Some("image/png".to_string()),
        })
    ));
    assert_eq!(failed.failed_images(), 1);
    assert!(failed.take_image_decode_commands().is_empty());

    let mut too_large = load("<img src='large.png'>");
    let command = too_large.take_commands().pop().unwrap();
    too_large.image_raw_bytes = MAX_IMAGE_FETCH_BYTES;
    assert!(too_large.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: command.url,
            status: 200,
            body: vec![0],
            content_type: Some("text/plain".to_string()),
        })
    ));
    assert_eq!(too_large.failed_images(), 1);
    assert!(!too_large.external_disabled());
}

#[test]
fn image_failures_preserve_rate_limit_and_decode_categories() {
    let mut page_load = load("<img src='limited.png'><img src='unknown.svg'>");
    let commands = page_load.take_commands();
    let limited = commands
        .iter()
        .find(|command| command.url.path().ends_with("limited.png"))
        .unwrap();
    assert!(page_load.deliver(
        limited.resource_id,
        Ok(FetchResponse {
            final_url: limited.url.clone(),
            status: 429,
            body: b"slow down".to_vec(),
            content_type: Some("text/plain".to_string()),
        })
    ));
    let unknown = commands
        .iter()
        .find(|command| command.url.path().ends_with("unknown.svg"))
        .unwrap();
    assert!(page_load.deliver(
        unknown.resource_id,
        Ok(FetchResponse {
            final_url: unknown.url.clone(),
            status: 200,
            body: b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec(),
            content_type: Some("image/svg+xml".to_string()),
        })
    ));
    let decode = page_load.take_image_decode_commands().pop().unwrap();
    assert!(page_load.deliver_image_decode(
        decode.asset_id,
        decode.revision,
        Err(ImageDecodeError::UnknownFormat)
    ));
    assert_eq!(
        page_load.image_failures(),
        ImageFailureSummary {
            rate_limited: 1,
            unknown_format: 1,
            ..ImageFailureSummary::default()
        }
    );
    assert_eq!(page_load.failed_images(), 2);
}

#[test]
fn image_redirects_recheck_the_final_scheme() {
    let mut accepted = load("<img src='/image'>");
    let command = accepted.take_commands().pop().unwrap();
    assert!(accepted.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: Url::parse("https://cdn.example/image.png").unwrap(),
            status: 200,
            body: vec![1],
            content_type: None,
        })
    ));
    assert_eq!(accepted.take_image_decode_commands().len(), 1);

    let mut rejected = load("<img src='/image'>");
    let command = rejected.take_commands().pop().unwrap();
    assert!(rejected.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: Url::parse("file:///private/image.png").unwrap(),
            status: 200,
            body: vec![1],
            content_type: None,
        })
    ));
    assert!(rejected.take_image_decode_commands().is_empty());
    assert_eq!(rejected.failed_images(), 1);
}

#[test]
fn image_decode_delivery_is_revision_checked_and_budgeted() {
    let mut page_load = load("<img src='pixel.png'>");
    let command = page_load.take_commands().pop().unwrap();
    assert!(page_load.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: command.url,
            status: 200,
            body: vec![1, 2, 3],
            content_type: Some("text/plain".to_string()),
        })
    ));
    let decode = page_load.take_image_decode_commands().pop().unwrap();
    assert!(!page_load.deliver_image_decode(
        decode.asset_id,
        decode.revision + 1,
        Ok(crate::core::image::DecodedImage {
            asset_id: decode.asset_id,
            revision: decode.revision + 1,
            width: 1,
            height: 1,
            rgba: std::sync::Arc::from([0, 0, 0, 255]),
        })
    ));
    assert!(page_load.deliver_image_decode(
        decode.asset_id,
        decode.revision,
        Ok(crate::core::image::DecodedImage {
            asset_id: decode.asset_id,
            revision: decode.revision,
            width: 1,
            height: 1,
            rgba: std::sync::Arc::from([0, 0, 0, 255]),
        })
    ));
    assert_eq!(page_load.image_decoded_bytes, 4);
    assert!(page_load.is_settled());

    let mut over_budget = load("<img src='pixel.png'>");
    let command = over_budget.take_commands().pop().unwrap();
    assert!(over_budget.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: command.url,
            status: 200,
            body: vec![1],
            content_type: None,
        })
    ));
    let decode = over_budget.take_image_decode_commands().pop().unwrap();
    over_budget.image_decoded_bytes = MAX_IMAGE_DECODED_BYTES;
    assert!(over_budget.deliver_image_decode(
        decode.asset_id,
        decode.revision,
        Ok(crate::core::image::DecodedImage {
            asset_id: decode.asset_id,
            revision: decode.revision,
            width: 1,
            height: 1,
            rgba: std::sync::Arc::from([0, 0, 0, 255]),
        })
    ));
    assert_eq!(over_budget.failed_images(), 1);
}

#[test]
fn decoded_images_replace_fallbacks_with_intrinsic_ordered_hit_geometry() {
    let mut load = load(
        "<a href='/target'><img src='pixel.png' alt='fallback' style='width:32px;height:auto'></a>",
    );
    let before = load.force_render();
    let before_styles = before.styles.clone();
    assert!(
        before
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains("[fallback]"))
    );
    let command = load.take_commands().pop().unwrap();
    assert!(load.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: command.url,
            status: 200,
            body: vec![1],
            content_type: Some("text/plain".to_string()),
        })
    ));
    let decode = load.take_image_decode_commands().pop().unwrap();
    assert!(load.deliver_image_decode(
        decode.asset_id,
        decode.revision,
        Ok(crate::core::image::DecodedImage {
            asset_id: decode.asset_id,
            revision: decode.revision,
            width: 16,
            height: 16,
            rgba: std::sync::Arc::from(vec![255; 16 * 16 * 4]),
        })
    ));
    let page = load.render_after_image().unwrap();
    assert!(std::sync::Arc::ptr_eq(&before_styles, &page.styles));
    let node = *load.image_node_index.keys().next().unwrap();
    assert_eq!(
        page.styles.get(node).width,
        crate::core::style::CssSize::Cells(4)
    );
    assert_eq!(page.painted.images.len(), 1);
    assert_eq!(page.painted.images[0].rect.width, 4);
    assert_eq!(page.painted.images[0].rect.height, 2);
    assert!(page.painted.image_assets.contains_key(&decode.asset_id));
    let image = page.painted.images[0];
    assert_eq!(
        page.painted
            .link_at(image.clip.col, image.clip.row)
            .map(|link| link.href.as_str()),
        Some("/target")
    );
    assert!(
        !page
            .painted
            .text_lines()
            .iter()
            .any(|line| line.contains('\u{fffc}'))
    );
}

#[test]
fn decoded_images_participate_in_block_table_flex_and_grid_layout() {
    let mut load = load(
        "<img src='block.png' style='display:block'>
         <table><tr><td><img src='table.png'></td></tr></table>
         <div style='display:flex'><img src='flex.png'></div>
         <div style='display:grid'><img src='grid.png'></div>",
    );
    load.force_render();
    let commands = load.take_commands();
    for command in commands {
        assert!(load.deliver(
            command.resource_id,
            Ok(FetchResponse {
                final_url: command.url,
                status: 200,
                body: vec![1],
                content_type: None,
            })
        ));
        let decode = load.take_image_decode_commands().pop().unwrap();
        assert!(load.deliver_image_decode(
            decode.asset_id,
            decode.revision,
            Ok(crate::core::image::DecodedImage {
                asset_id: decode.asset_id,
                revision: decode.revision,
                width: 8,
                height: 16,
                rgba: std::sync::Arc::from(vec![255; 8 * 16 * 4]),
            })
        ));
    }
    let page = load.render_after_image().unwrap();
    assert_eq!(page.painted.images.len(), 4);
    assert!(
        page.painted
            .images
            .iter()
            .all(|image| image.clip.width > 0 && image.clip.height > 0)
    );
}

#[test]
fn decoded_images_do_not_inherit_the_control_content_height_floor() {
    let mut load = load(
        "<img src='tall.png' style='display:block;box-sizing:border-box;
         border:1px solid;max-height:48px'>",
    );
    load.force_render();
    let command = load.take_commands().pop().unwrap();
    assert!(load.deliver(
        command.resource_id,
        Ok(FetchResponse {
            final_url: command.url,
            status: 200,
            body: vec![1],
            content_type: None,
        })
    ));
    let decode = load.take_image_decode_commands().pop().unwrap();
    assert!(load.deliver_image_decode(
        decode.asset_id,
        decode.revision,
        Ok(crate::core::image::DecodedImage {
            asset_id: decode.asset_id,
            revision: decode.revision,
            width: 16,
            height: 64,
            rgba: std::sync::Arc::from(vec![255; 16 * 64 * 4]),
        })
    ));
    let page = load.render_after_image().unwrap();
    assert_eq!(page.painted.images.len(), 1);
    assert_eq!(page.painted.images[0].rect.height, 1);
}

/// `min-*`/`max-*` on a decoded image resize the picture along both axes at once, measure the
/// element's own chrome when `box-sizing` says to, and quantise the way an authored length does.
/// Clamping the axes independently used to stretch the picture, which is what made the WPT
/// replaced-sizing references disagree with us by whole cells.
#[test]
fn decoded_image_constraints_scale_the_picture_instead_of_stretching_it() {
    let mut load = load(
        "<img src='ratio.png' style='display:block;max-height:32px'>
         <img src='chrome.png'
              style='display:block;box-sizing:border-box;padding:0 8px;max-width:40px'>
         <img src='intrinsic.png' style='display:block'>
         <img src='authored.png' style='display:block;width:75px'>",
    );
    load.force_render();
    for command in load.take_commands() {
        // 64 is a whole number of cells either way; 75 is the WPT size that falls between them.
        let square = if command.url.path().ends_with("ratio.png")
            || command.url.path().ends_with("chrome.png")
        {
            64u32
        } else {
            75u32
        };
        assert!(load.deliver(
            command.resource_id,
            Ok(FetchResponse {
                final_url: command.url,
                status: 200,
                body: vec![1],
                content_type: None,
            })
        ));
        let decode = load.take_image_decode_commands().pop().unwrap();
        assert!(load.deliver_image_decode(
            decode.asset_id,
            decode.revision,
            Ok(crate::core::image::DecodedImage {
                asset_id: decode.asset_id,
                revision: decode.revision,
                width: square,
                height: square,
                rgba: std::sync::Arc::from(vec![255; (square * square * 4) as usize]),
            })
        ));
    }
    let page = load.render_after_image().unwrap();
    let mut placements = page.painted.images.clone();
    placements.sort_by_key(|image| image.rect.row);
    assert_eq!(
        placements
            .iter()
            .map(|image| (image.rect.width, image.rect.height))
            .collect::<Vec<_>>(),
        [
            // Two rows of `max-height` leave four columns, not the eight the picture asked for.
            (4, 2),
            // `max-width: 40px` is five columns of border box, and the 8px padding is two of them.
            (3, 2),
            // A 75px picture and an authored `width: 75px` agree on nine columns.
            (9, 5),
            (9, 5),
        ]
    );
}

#[test]
fn image_percentages_use_only_definite_layout_bases() {
    let mut load = load(
        "<table><tr><td><img src='table.png' style='width:50%'></td></tr></table>
         <div><img src='inline.png' style='width:32px;height:50%'></div>",
    );
    load.force_render();
    for command in load.take_commands() {
        assert!(load.deliver(
            command.resource_id,
            Ok(FetchResponse {
                final_url: command.url,
                status: 200,
                body: vec![1],
                content_type: None,
            })
        ));
        let decode = load.take_image_decode_commands().pop().unwrap();
        assert!(load.deliver_image_decode(
            decode.asset_id,
            decode.revision,
            Ok(crate::core::image::DecodedImage {
                asset_id: decode.asset_id,
                revision: decode.revision,
                width: 16,
                height: 16,
                rgba: std::sync::Arc::from(vec![255; 16 * 16 * 4]),
            })
        ));
    }
    let page = load.render_after_image().unwrap();
    let sizes = page
        .painted
        .images
        .iter()
        .map(|image| (image.rect.width, image.rect.height))
        .collect::<Vec<_>>();
    assert!(sizes.contains(&(2, 1)), "{sizes:?}");
    assert!(sizes.contains(&(4, 2)), "{sizes:?}");
}

#[test]
fn a_lost_decoder_fails_pending_images_without_sticking_the_page() {
    let mut load = load("<img src='one.png'><img src='two.png'>");
    let commands = load.take_commands();
    for command in commands {
        assert!(load.deliver(
            command.resource_id,
            Ok(FetchResponse {
                final_url: command.url,
                status: 200,
                body: vec![1, 2, 3],
                content_type: None,
            })
        ));
    }
    assert!(load.fail_pending_image_decodes());
    assert_eq!(load.failed_images(), 2);
    assert!(!load.fail_pending_image_decodes());
}

#[test]
fn a_lost_fetcher_fails_pending_images_without_sticking_the_page() {
    let mut load = load("<img src='one.png'><img src='two.png'>");
    assert!(load.fail_pending_image_fetches());
    assert_eq!(load.failed_images(), 2);
    assert!(load.is_settled());
    assert!(!load.fail_pending_image_fetches());
}

#[test]
fn image_url_limit_refuses_only_excess_images() {
    let images = (0..=MAX_IMAGE_URLS)
        .map(|index| format!("<img src='{index}.png'>"))
        .collect::<String>();
    let mut load = load(&images);
    assert_eq!(load.take_commands().len(), MAX_IMAGE_URLS);
    assert_eq!(load.failed_images(), 1);
    assert!(!load.external_disabled());
}

#[test]
fn empty_invalid_and_cross_scheme_image_urls_fail_without_network_work() {
    let mut load = load(
        "<img src=''><img src='http://[invalid'><img src='file:///private.png'><img src='ok.png'>",
    );
    assert_eq!(load.take_commands().len(), 1);
    assert_eq!(load.failed_images(), 2);
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
        authored.is_dirty(),
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
            authored.is_dirty()
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

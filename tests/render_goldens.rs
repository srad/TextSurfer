use std::path::Path;

use textsurfer::app::render::render_html;
use textsurfer::core::style::{Palette, Rgb};
use textsurfer::paint::DisplayList;

const WIDTH: usize = 40;

fn palette() -> Palette {
    Palette {
        text: Rgb::new(255, 255, 255),
        background: Rgb::new(0, 0, 128),
        link: Rgb::new(255, 255, 0),
    }
}

fn render(name: &str) -> DisplayList {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("fixture {}: {error}", path.display()));
    render_html(
        &source,
        textsurfer::core::geom::Size {
            cols: WIDTH as u16,
            rows: 24,
        },
        palette(),
        false,
    )
    .painted
}

fn golden(name: &str) -> String {
    render(name).text_lines().join("\n")
}

#[test]
fn margins_and_padding_golden() {
    insta::assert_snapshot!(golden("margins.html"));
}

#[test]
fn headings_lists_and_rules_golden() {
    insta::assert_snapshot!(golden("headings.html"));
}

#[test]
fn borders_golden() {
    insta::assert_snapshot!(golden("borders.html"));
}

#[test]
fn links_golden() {
    insta::assert_snapshot!(golden("links.html"));
}

#[test]
fn wide_characters_golden() {
    insta::assert_snapshot!(golden("wide.html"));
}

#[test]
fn preformatted_golden() {
    insta::assert_snapshot!(golden("pre.html"));
}

#[test]
fn painted_rows_never_exceed_the_render_width() {
    for fixture in [
        "margins.html",
        "headings.html",
        "borders.html",
        "links.html",
        "wide.html",
        "pre.html",
    ] {
        for line in render(fixture).text_lines() {
            assert!(
                unicode_width::UnicodeWidthStr::width(line.as_str()) <= WIDTH,
                "{fixture} painted past the viewport: {line:?}"
            );
        }
    }
}

#[test]
fn link_styling_and_geometry_survive_into_the_display_list() {
    let painted = render("links.html");
    assert_eq!(
        painted.links.len(),
        2,
        "only anchors with href are interactive"
    );
    let first = &painted.links[0];
    assert_eq!(first.href, "https://example.com/");
    assert!(!first.rects.is_empty(), "links must carry their geometry");
    let rect = first.rects[0];
    assert_eq!(
        painted.link_at(rect.col, rect.row).map(|link| &link.href),
        Some(&first.href)
    );

    let accent = palette().link;
    let styled = painted.rows[rect.row]
        .spans
        .iter()
        .find(|span| span.col == rect.col)
        .expect("a span starts where the link starts");
    assert_eq!(styled.style.fg, Some(accent), "UA link colour is painted");
    assert!(styled.style.underline);
}

#[test]
fn dump_mode_prints_the_same_page_the_painter_produced() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/headings.html");
    let url = url::Url::from_file_path(&path).expect("fixture path is absolute");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_textsurfer"))
        .args(["--dump", "--cols", "40", "--url", url.as_str()])
        .output()
        .expect("dump run");
    assert!(
        output.status.success(),
        "dump failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8(output.stdout).expect("utf-8 dump");
    let expected = golden("headings.html");
    assert_eq!(
        printed.replace("\r\n", "\n").trim_end(),
        expected.trim_end(),
        "--dump must agree with the in-process painter"
    );
}

#[test]
fn unreadable_author_colours_are_corrected_before_painting() {
    let painted = render("links.html");
    let dark = painted
        .rows
        .iter()
        .flat_map(|row| row.spans.iter())
        .find(|span| span.text.contains("nearly black"))
        .expect("the dark paragraph is painted");
    let foreground = dark.style.fg.expect("author colour applied");
    assert!(
        foreground.contrast_ratio(palette().background) >= 3.0,
        "author colour {foreground:?} must be corrected against the field"
    );
}

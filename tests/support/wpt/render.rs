use std::collections::HashSet;
use std::fs;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Cell;
use ratatui::style::Style;
use ratatui::widgets::Widget;
use textsurfer::core::geom::Size;
use textsurfer::core::style::{RenderContext, RenderMetrics};
use textsurfer::css::ColorScheme;
use textsurfer::net::FetchResponse;
use textsurfer::paint::DisplayList;
use textsurfer::pipeline::page_load::{PageLoad, PageLoadOptions};
use textsurfer::ui::PAPER_WHITE;
use textsurfer::ui::widgets::content::{Content, ContentLines};
use url::Url;

use super::manifest::{
    Case, CaseKind, CaseOutcome, Manifest, Relation, corpus_root, local_url_path,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct VisualGrid {
    columns: u16,
    rows: u16,
    cells: Vec<Cell>,
}

impl VisualGrid {
    fn has_authored_background(&self) -> bool {
        self.cells.iter().any(|cell| {
            cell.style()
                .bg
                .is_some_and(|background| background != PAPER_WHITE.bg)
        })
    }
}

pub fn evaluate_case(manifest: &Manifest, case: &Case) -> CaseOutcome {
    match evaluate_case_inner(manifest, case) {
        Ok(outcome) => outcome,
        Err(error) => CaseOutcome::HarnessError(error),
    }
}

fn evaluate_case_inner(manifest: &Manifest, case: &Case) -> Result<CaseOutcome, String> {
    let allowed = allowed_files(case);
    match case.kind {
        CaseKind::Crashtest => {
            let _ = render_grid(manifest, &case.path, &allowed)?;
            render_vga(manifest, &case.path, &allowed)?;
            Ok(CaseOutcome::Pass)
        }
        CaseKind::Reftest => {
            let test = render_grid(manifest, &case.path, &allowed)?;
            if case
                .capabilities
                .iter()
                .any(|capability| capability == "authored-background-sentinel")
                && !test.has_authored_background()
            {
                return Err(format!(
                    "{} did not paint its authored-background sentinel",
                    case.path
                ));
            }
            render_vga(manifest, &case.path, &allowed)?;
            let mut matched = false;
            let mut mismatch_equal = Vec::new();
            let mut first_difference = None;
            for reference in &case.references {
                let expected = render_grid(manifest, &reference.path, &allowed)?;
                render_vga(manifest, &reference.path, &allowed)?;
                let equal = test == expected;
                match reference.relation {
                    Relation::Match if equal => matched = true,
                    Relation::Match => {
                        first_difference.get_or_insert_with(|| {
                            format!(
                                "match {} differs at {}",
                                reference.path,
                                first_difference_between(&test, &expected)
                            )
                        });
                    }
                    Relation::Mismatch if equal => mismatch_equal.push(reference.path.as_str()),
                    Relation::Mismatch => {}
                }
            }
            if matched && mismatch_equal.is_empty() {
                Ok(CaseOutcome::Pass)
            } else {
                let detail = if !mismatch_equal.is_empty() {
                    format!("mismatch references unexpectedly agree: {mismatch_equal:?}")
                } else {
                    first_difference.unwrap_or_else(|| "no match reference agreed".to_string())
                };
                Ok(CaseOutcome::AssertionMismatch(detail))
            }
        }
        CaseKind::Testharness => Err(format!("{} is not a runnable static case", case.path)),
    }
}

fn allowed_files(case: &Case) -> HashSet<&str> {
    let mut allowed = HashSet::from([case.path.as_str()]);
    allowed.extend(
        case.references
            .iter()
            .map(|reference| reference.path.as_str()),
    );
    allowed.extend(case.resources.iter().map(String::as_str));
    allowed
}

fn render_grid(
    manifest: &Manifest,
    path: &str,
    allowed: &HashSet<&str>,
) -> Result<VisualGrid, String> {
    let painted = render_page(manifest, path, allowed, RenderMetrics::TERMINAL)?;
    visible_grid(&painted, manifest.profile.columns, manifest.profile.rows)
}

fn visible_grid(painted: &DisplayList, columns: u16, rows: u16) -> Result<VisualGrid, String> {
    let backend = TestBackend::new(columns.saturating_add(2), rows);
    let mut terminal = Terminal::new(backend).map_err(|error| format!("terminal: {error}"))?;
    terminal
        .draw(|frame| {
            let area = frame.area();
            frame.buffer_mut().set_style(
                area,
                Style::default().fg(PAPER_WHITE.text).bg(PAPER_WHITE.bg),
            );
            Content {
                lines: &ContentLines { painted, scroll: 0 },
                theme: &PAPER_WHITE,
            }
            .render(area, frame.buffer_mut());
        })
        .map_err(|error| format!("terminal draw: {error}"))?;
    let buffer = terminal.backend().buffer();
    let mut cells = Vec::with_capacity(usize::from(columns) * usize::from(rows));
    for row in 0..rows {
        for column in 0..columns {
            cells.push(buffer[(column + 1, row)].clone());
        }
    }
    Ok(VisualGrid {
        columns,
        rows,
        cells,
    })
}

pub fn visually_equal(
    left: &DisplayList,
    right: &DisplayList,
    columns: u16,
    rows: u16,
) -> Result<bool, String> {
    Ok(visible_grid(left, columns, rows)? == visible_grid(right, columns, rows)?)
}

fn render_page(
    manifest: &Manifest,
    path: &str,
    allowed: &HashSet<&str>,
    metrics: RenderMetrics,
) -> Result<DisplayList, String> {
    let root = corpus_root();
    let source_path = root.join(path);
    let source = fs::read_to_string(&source_path)
        .map_err(|error| format!("page {}: {error}", source_path.display()))?;
    let document_url = Url::parse("https://wpt.test/")
        .expect("fixed origin")
        .join(path)
        .map_err(|error| format!("document URL {path}: {error}"))?;
    let mut load = PageLoad::new(
        &source,
        document_url,
        encoding_rs::UTF_8,
        PageLoadOptions {
            render: RenderContext {
                viewport: Size {
                    cols: manifest.profile.columns,
                    rows: manifest.profile.rows,
                },
                metrics,
            },
            palette: PAPER_WHITE.palette(),
            scripting: false,
            color_scheme: ColorScheme::Light,
            started: Duration::ZERO,
        },
    );
    for _ in 0..=128 {
        let commands = load.take_commands();
        if commands.is_empty() {
            if !load.applicable_is_settled() {
                return Err(format!("{path} has an unsettled stylesheet graph"));
            }
            if load.failed_resources() != 0 || load.external_disabled() {
                return Err(format!("{path} rejected a stylesheet resource"));
            }
            return Ok(load.force_render().painted);
        }
        for command in commands {
            let relative = local_url_path(&command.url)?;
            if !allowed.contains(relative.as_str()) {
                return Err(format!("{path} requested unlisted resource {relative}"));
            }
            if !relative.ends_with(".css") {
                return Err(format!(
                    "{path} requested unsupported resource type {relative}"
                ));
            }
            let resource_path = root.join(&relative);
            let body = fs::read(&resource_path)
                .map_err(|error| format!("resource {}: {error}", resource_path.display()))?;
            let delivered = load.deliver(
                command.resource_id,
                Ok(FetchResponse {
                    final_url: command.url,
                    status: 200,
                    body,
                    content_type: Some("text/css".to_string()),
                }),
            );
            if !delivered {
                return Err(format!("{path} refused resource {relative}"));
            }
        }
    }
    Err(format!("{path} exceeded the stylesheet command limit"))
}

#[cfg(feature = "vga")]
fn render_vga(manifest: &Manifest, path: &str, allowed: &HashSet<&str>) -> Result<(), String> {
    let _ = render_page(manifest, path, allowed, RenderMetrics::VGA)?;
    Ok(())
}

#[cfg(not(feature = "vga"))]
fn render_vga(_manifest: &Manifest, _path: &str, _allowed: &HashSet<&str>) -> Result<(), String> {
    Ok(())
}

fn first_difference_between(left: &VisualGrid, right: &VisualGrid) -> String {
    if left.columns != right.columns || left.rows != right.rows {
        return format!(
            "grid size {}x{} vs {}x{}",
            left.columns, left.rows, right.columns, right.rows
        );
    }
    left.cells
        .iter()
        .zip(&right.cells)
        .enumerate()
        .find_map(|(index, (actual, expected))| {
            (actual != expected).then(|| {
                let column = index % usize::from(left.columns);
                let row = index / usize::from(left.columns);
                format!(
                    "({column},{row}): {:?}/{:?} vs {:?}/{:?}",
                    actual.symbol(),
                    actual.style(),
                    expected.symbol(),
                    expected.style()
                )
            })
        })
        .unwrap_or_else(|| "unknown cell".to_string())
}

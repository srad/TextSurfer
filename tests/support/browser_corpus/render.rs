use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use textsurfer::core::form::FormState;
use textsurfer::core::geom::Size;
use textsurfer::core::image::{ImageDecoder, ImageResources};
use textsurfer::core::style::{CellMetric, RenderContext, RenderMetrics};
use textsurfer::css::ColorScheme;
use textsurfer::layout::{BoxTree, TaffyLayoutEngine};
use textsurfer::net::FetchResponse;
use textsurfer::pipeline::image::RasterImageDecoder;
use textsurfer::pipeline::page_load::{PageLoad, PageLoadOptions};
use textsurfer::pipeline::render::RenderedPage;
use textsurfer::ui::PAPER_WHITE;
use url::Url;

use super::manifest::{Manifest, Page, Renderer, Viewport, corpus_root, validate_relative_path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Bundle {
    schema: u8,
    slug: String,
    url: String,
    requested: String,
    captured_with: String,
    resources: Vec<Resource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Resource {
    url: String,
    status: u16,
    mime_type: String,
    file: Option<String>,
    sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Reference {
    pub cols: u16,
    pub rows: u16,
    pub words: Vec<BrowserWord>,
    pub document_url: String,
    pub document_height: f64,
}

#[derive(Deserialize)]
pub struct BrowserWord(
    pub String,
    pub f64,
    pub f64,
    pub f64,
    pub f64,
    pub u8,
    pub u8,
);

pub struct CaseRender {
    pub reference: Reference,
    pub tree: BoxTree,
}

pub fn verify_integrity(manifest: &Manifest) -> Result<(), String> {
    let root = corpus_root();
    let actual_pages = fs::read_dir(&root)
        .map_err(|error| format!("corpus root {}: {error}", root.display()))?
        .map(|entry| {
            let entry = entry.map_err(|error| format!("corpus entry: {error}"))?;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| format!("corpus metadata {}: {error}", entry.path().display()))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(format!("invalid corpus page {}", entry.path().display()));
            }
            Ok(entry.file_name().to_string_lossy().into_owned())
        })
        .collect::<Result<BTreeSet<_>, String>>()?;
    let expected_pages = manifest
        .pages
        .iter()
        .map(|page| page.slug.clone())
        .collect::<BTreeSet<_>>();
    if actual_pages != expected_pages {
        return Err(format!(
            "corpus page membership differs: expected={expected_pages:?} actual={actual_pages:?}"
        ));
    }
    for page in &manifest.pages {
        verify_page(manifest, page)?;
    }
    Ok(())
}

fn verify_page(manifest: &Manifest, page: &Page) -> Result<(), String> {
    let directory = corpus_root().join(&page.slug);
    let bundle_path = directory.join("bundle.json");
    let bundle_bytes = fs::read(&bundle_path)
        .map_err(|error| format!("bundle {}: {error}", bundle_path.display()))?;
    verify_hash(
        &bundle_bytes,
        &page.bundle_sha256,
        &format!("{} bundle", page.slug),
    )?;
    let bundle: Bundle = serde_json::from_slice(&bundle_bytes)
        .map_err(|error| format!("bundle {}: {error}", bundle_path.display()))?;
    if bundle.schema != 1
        || bundle.slug != page.slug
        || bundle.requested != page.requested_url
        || bundle.captured_with != "chromium"
    {
        return Err(format!("{} bundle identity drifted", page.slug));
    }
    let canonical =
        Url::parse(&bundle.url).map_err(|error| format!("{} canonical URL: {error}", page.slug))?;
    if canonical.scheme() != "https" || canonical.host_str().is_none() {
        return Err(format!("{} has a non-HTTPS canonical URL", page.slug));
    }
    let mut expected_files = BTreeSet::from(["bundle.json".to_string()]);
    let mut urls = HashSet::new();
    let mut files = HashSet::new();
    for resource in &bundle.resources {
        Url::parse(&resource.url)
            .map_err(|error| format!("{} resource URL: {error}", page.slug))?;
        if !urls.insert(resource.url.as_str()) {
            return Err(format!("{} repeats resource {}", page.slug, resource.url));
        }
        match (&resource.file, &resource.sha256) {
            (Some(file), Some(expected)) => {
                validate_relative_path(file)?;
                if !files.insert(file.as_str()) {
                    return Err(format!("{} repeats file {file}", page.slug));
                }
                let bytes = fs::read(directory.join(file))
                    .map_err(|error| format!("{} resource {file}: {error}", page.slug))?;
                verify_hash(&bytes, expected, &format!("{} resource {file}", page.slug))?;
                expected_files.insert(file.clone());
            }
            (None, None) => {}
            _ => return Err(format!("{} resource file/hash mismatch", page.slug)),
        }
    }
    for reference in &page.references {
        let viewport = manifest
            .viewport(&reference.viewport)
            .ok_or_else(|| format!("unknown viewport {}", reference.viewport))?;
        let file = format!("reference-{}.json", viewport.id);
        let bytes = fs::read(directory.join(&file))
            .map_err(|error| format!("{} reference {file}: {error}", page.slug))?;
        verify_hash(
            &bytes,
            &reference.sha256,
            &format!("{} reference {file}", page.slug),
        )?;
        let parsed: Reference = serde_json::from_slice(&bytes)
            .map_err(|error| format!("{} reference {file}: {error}", page.slug))?;
        if parsed.cols != viewport.columns
            || parsed.rows != viewport.rows
            || parsed.document_url != bundle.url
            || !parsed.document_height.is_finite()
            || parsed.document_height < 0.0
        {
            return Err(format!("{} reference {file} metadata drifted", page.slug));
        }
        expected_files.insert(file);
    }
    let actual_files = fs::read_dir(&directory)
        .map_err(|error| format!("corpus page {}: {error}", directory.display()))?
        .map(|entry| {
            let entry = entry.map_err(|error| format!("corpus file: {error}"))?;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| format!("corpus metadata {}: {error}", entry.path().display()))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(format!("invalid corpus file {}", entry.path().display()));
            }
            Ok(entry.file_name().to_string_lossy().into_owned())
        })
        .collect::<Result<BTreeSet<_>, String>>()?;
    if actual_files != expected_files {
        return Err(format!(
            "{} file membership differs: expected={expected_files:?} actual={actual_files:?}",
            page.slug
        ));
    }
    Ok(())
}

pub fn render_case(
    manifest: &Manifest,
    page: &Page,
    viewport: &Viewport,
    renderer: Renderer,
) -> Result<CaseRender, String> {
    let directory = corpus_root().join(&page.slug);
    let bundle_source = fs::read(directory.join("bundle.json"))
        .map_err(|error| format!("{} bundle: {error}", page.slug))?;
    let bundle: Bundle = serde_json::from_slice(&bundle_source)
        .map_err(|error| format!("{} bundle: {error}", page.slug))?;
    let reference_source = fs::read(directory.join(format!("reference-{}.json", viewport.id)))
        .map_err(|error| format!("{} {} reference: {error}", page.slug, viewport.id))?;
    let reference: Reference = serde_json::from_slice(&reference_source)
        .map_err(|error| format!("{} {} reference: {error}", page.slug, viewport.id))?;
    let rendered = render_page(page, &bundle, viewport, renderer)?;
    if !rendered.painted.limits.is_clear() {
        return Err(format!(
            "{} {} {} hit layout limits {:?}",
            page.slug,
            renderer.name(),
            viewport.id,
            rendered.painted.limits
        ));
    }
    let tree = comparison_layout(
        &rendered,
        Size {
            cols: viewport.columns,
            rows: viewport.rows,
        },
    );
    let _ = manifest;
    Ok(CaseRender { reference, tree })
}

fn render_page(
    page: &Page,
    bundle: &Bundle,
    viewport: &Viewport,
    renderer: Renderer,
) -> Result<RenderedPage, String> {
    let directory = corpus_root().join(&page.slug);
    let by_url = bundle
        .resources
        .iter()
        .map(|resource| (resource.url.as_str(), resource))
        .collect::<HashMap<_, _>>();
    let document = bundle
        .resources
        .iter()
        .find(|resource| resource.url == bundle.url)
        .and_then(|resource| resource.file.as_ref())
        .ok_or_else(|| format!("{} has no captured document body", page.slug))?;
    let source = fs::read_to_string(directory.join(document))
        .map_err(|error| format!("{} document: {error}", page.slug))?;
    let size = Size {
        cols: viewport.columns,
        rows: viewport.rows,
    };
    let metrics = match renderer {
        Renderer::Terminal => RenderMetrics::TERMINAL,
        Renderer::Vga => RenderMetrics::VGA,
    };
    let mut load = PageLoad::new(
        &source,
        Url::parse(&bundle.url).map_err(|error| format!("document URL: {error}"))?,
        encoding_rs::UTF_8,
        PageLoadOptions {
            render: RenderContext {
                viewport: size,
                metrics,
            },
            palette: PAPER_WHITE.palette(),
            scripting: false,
            color_scheme: ColorScheme::Light,
            started: std::time::Duration::ZERO,
        },
    );
    let mut settled = false;
    for _ in 0..=128 {
        let commands = load.take_commands();
        let decodes = load.take_image_decode_commands();
        if commands.is_empty() && decodes.is_empty() {
            settled = true;
            break;
        }
        for command in commands {
            let captured = by_url
                .get(command.url.as_str())
                .filter(|resource| resource.file.is_some());
            let response = match captured {
                Some(resource) => {
                    let file = resource.file.as_ref().expect("captured file");
                    FetchResponse {
                        final_url: command.url,
                        status: resource.status,
                        body: fs::read(directory.join(file))
                            .map_err(|error| format!("{} resource {file}: {error}", page.slug))?,
                        content_type: Some(resource.mime_type.clone()),
                    }
                }
                None => FetchResponse {
                    final_url: command.url,
                    status: 404,
                    body: Vec::new(),
                    content_type: None,
                },
            };
            load.deliver(command.resource_id, Ok(response));
        }
        for request in decodes {
            let asset_id = request.asset_id;
            let revision = request.revision;
            let result = RasterImageDecoder.decode(request);
            load.deliver_image_decode(asset_id, revision, result);
        }
    }
    if !settled {
        return Err(format!("{} resource graph did not settle", page.slug));
    }
    if load.external_disabled() {
        return Err(format!("{} discarded its stylesheets", page.slug));
    }
    Ok(load.force_render())
}

fn comparison_layout(page: &RenderedPage, viewport: Size) -> BoxTree {
    let mut images = ImageResources::default();
    for placement in &page.painted.images {
        if let Some(image) = page.painted.image_assets.get(&placement.asset_id) {
            images.insert(placement.node, image.clone());
        }
    }
    TaffyLayoutEngine.layout_with_images(
        &page.document.borrow(),
        &page.styles,
        viewport,
        FormState::empty(),
        &images,
        CellMetric::DEFAULT,
    )
}

pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn verify_hash(bytes: &[u8], expected: &str, label: &str) -> Result<(), String> {
    let actual = digest(bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{label} hash drift: {actual} != {expected}"))
    }
}

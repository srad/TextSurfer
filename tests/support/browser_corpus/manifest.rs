use std::collections::{BTreeSet, HashSet};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Manifest {
    pub schema: u8,
    pub viewports: Vec<Viewport>,
    pub renderers: Vec<Renderer>,
    pub required_shapes: Vec<String>,
    pub pages: Vec<Page>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub id: String,
    pub columns: u16,
    pub rows: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Renderer {
    Terminal,
    Vga,
}

impl Renderer {
    pub fn name(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Vga => "vga",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Page {
    pub slug: String,
    pub requested_url: String,
    pub shapes: Vec<String>,
    pub bundle_sha256: String,
    pub references: Vec<ReferenceHash>,
    pub expectations: Vec<Expectation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceHash {
    pub viewport: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    pub renderer: Renderer,
    pub viewport: String,
    #[serde(default)]
    pub xfails: Vec<ExpectedFindings>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedFindings {
    pub relation: Relation,
    pub count: usize,
    pub sha256: String,
    pub owner: String,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    A1Missing,
    A1HiddenLeak,
    A2Order,
}

impl Relation {
    pub fn name(self) -> &'static str {
        match self {
            Self::A1Missing => "a1-missing",
            Self::A1HiddenLeak => "a1-hidden-leak",
            Self::A2Order => "a2-order",
        }
    }
}

impl Manifest {
    pub fn load() -> Result<Self, String> {
        let path = manifest_path();
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("manifest {}: {error}", path.display()))?;
        Self::parse(&source)
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        let manifest: Self =
            serde_json::from_str(source).map_err(|error| format!("manifest JSON: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 {
            return Err(format!("unsupported manifest schema {}", self.schema));
        }
        let expected_viewports = [("40x30", 40, 30), ("100x38", 100, 38), ("160x45", 160, 45)];
        if self.viewports.len() != expected_viewports.len()
            || expected_viewports.iter().any(|(id, columns, rows)| {
                !self.viewports.iter().any(|viewport| {
                    viewport.id == *id && viewport.columns == *columns && viewport.rows == *rows
                })
            })
        {
            return Err("browser-reference viewports drifted".to_string());
        }
        let renderers = self.renderers.iter().copied().collect::<BTreeSet<_>>();
        if renderers != BTreeSet::from([Renderer::Terminal, Renderer::Vga])
            || self.renderers.len() != 2
        {
            return Err("browser-reference renderers drifted".to_string());
        }
        let required = self.required_shapes.iter().collect::<HashSet<_>>();
        if required.len() != self.required_shapes.len() || required.is_empty() {
            return Err("required shapes are empty or duplicated".to_string());
        }
        let mut slugs = HashSet::new();
        let mut covered_shapes = HashSet::new();
        for page in &self.pages {
            validate_slug(&page.slug)?;
            if !slugs.insert(page.slug.as_str()) {
                return Err(format!("duplicate page {}", page.slug));
            }
            let url = Url::parse(&page.requested_url)
                .map_err(|error| format!("{} requested URL: {error}", page.slug))?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
            {
                return Err(format!("{} has an unsafe requested URL", page.slug));
            }
            validate_hash(&page.bundle_sha256, &format!("{} bundle", page.slug))?;
            if page.shapes.is_empty() {
                return Err(format!("{} has no corpus shape", page.slug));
            }
            let mut page_shapes = HashSet::new();
            for shape in &page.shapes {
                if !required.contains(shape) {
                    return Err(format!("{} has unknown shape {shape}", page.slug));
                }
                if !page_shapes.insert(shape) {
                    return Err(format!("{} repeats shape {shape}", page.slug));
                }
                covered_shapes.insert(shape);
            }
            let mut references = HashSet::new();
            for reference in &page.references {
                if !self
                    .viewports
                    .iter()
                    .any(|viewport| viewport.id == reference.viewport)
                {
                    return Err(format!(
                        "{} has unknown reference viewport {}",
                        page.slug, reference.viewport
                    ));
                }
                if !references.insert(reference.viewport.as_str()) {
                    return Err(format!(
                        "{} repeats reference viewport {}",
                        page.slug, reference.viewport
                    ));
                }
                validate_hash(
                    &reference.sha256,
                    &format!("{} reference {}", page.slug, reference.viewport),
                )?;
            }
            if references.len() != self.viewports.len() {
                return Err(format!("{} has an incomplete reference matrix", page.slug));
            }
            let mut expectations = HashSet::new();
            for expectation in &page.expectations {
                if !self
                    .viewports
                    .iter()
                    .any(|viewport| viewport.id == expectation.viewport)
                {
                    return Err(format!(
                        "{} has unknown expectation viewport {}",
                        page.slug, expectation.viewport
                    ));
                }
                if !renderers.contains(&expectation.renderer)
                    || !expectations.insert((expectation.renderer, expectation.viewport.as_str()))
                {
                    return Err(format!(
                        "{} repeats or misstates an expectation profile",
                        page.slug
                    ));
                }
                let mut relations = HashSet::new();
                for xfail in &expectation.xfails {
                    if !relations.insert(xfail.relation) {
                        return Err(format!(
                            "{} {} {} repeats {}",
                            page.slug,
                            expectation.renderer.name(),
                            expectation.viewport,
                            xfail.relation.name()
                        ));
                    }
                    if xfail.count == 0
                        || xfail.owner.trim().is_empty()
                        || xfail.reason.trim().is_empty()
                    {
                        return Err(format!("{} has an unowned xfail", page.slug));
                    }
                    validate_hash(&xfail.sha256, "finding digest")?;
                }
            }
            if expectations.len() != self.viewports.len() * renderers.len() {
                return Err(format!(
                    "{} has an incomplete expectation matrix",
                    page.slug
                ));
            }
        }
        if covered_shapes != required {
            return Err("required corpus shapes are not covered exactly".to_string());
        }
        Ok(())
    }

    pub fn page(&self, slug: &str) -> Option<&Page> {
        self.pages.iter().find(|page| page.slug == slug)
    }

    pub fn viewport(&self, id: &str) -> Option<&Viewport> {
        self.viewports.iter().find(|viewport| viewport.id == id)
    }

    pub fn expectation<'a>(
        &self,
        page: &'a Page,
        renderer: Renderer,
        viewport: &str,
    ) -> Option<&'a Expectation> {
        page.expectations
            .iter()
            .find(|value| value.renderer == renderer && value.viewport == viewport)
    }
}

pub fn manifest_path() -> PathBuf {
    std::env::var_os("TEXTSURFER_BROWSER_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/browser-reference.json")
        })
}

pub fn corpus_root() -> PathBuf {
    std::env::var_os("TEXTSURFER_BROWSER_CORPUS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/browser-corpus"))
}

pub fn validate_relative_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains('\\')
        || Path::new(path).is_absolute()
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("unsafe corpus path {path}"));
    }
    Ok(())
}

fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty()
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || slug.starts_with('-')
        || slug.ends_with('-')
    {
        return Err(format!("unsafe corpus slug {slug}"));
    }
    Ok(())
}

fn validate_hash(hash: &str, label: &str) -> Result<(), String> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{label} has an invalid SHA-256"));
    }
    Ok(())
}

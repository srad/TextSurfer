use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use textsurfer::core::dom::{AttrNs, Document, Node, NodeId};
use textsurfer::html::{Html5everParser, HtmlParser};
use url::Url;

pub const PINNED_COMMIT: &str = "797589c8452b14ba448ba77819427ff3d743e37f";
const REPOSITORY: &str = "https://github.com/web-platform-tests/wpt";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u8,
    pub upstream: Upstream,
    pub profile: Profile,
    pub vga_profile: Profile,
    pub cases: Vec<Case>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upstream {
    pub repository: String,
    pub commit: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub columns: u16,
    pub rows: u16,
    pub metrics: String,
    pub appearance: String,
    pub theme: String,
    pub cell: CellProfile,
    pub palette: PaletteProfile,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellProfile {
    pub column_px: u16,
    pub row_px: u16,
    pub root_font_px: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaletteProfile {
    pub text: [u8; 3],
    pub background: [u8; 3],
    pub link: [u8; 3],
    pub link_hover: [u8; 3],
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub path: String,
    #[serde(default)]
    pub profile: OracleProfile,
    pub kind: CaseKind,
    pub status: ExpectedStatus,
    pub reason: Option<String>,
    pub capabilities: Vec<String>,
    pub references: Vec<Reference>,
    pub resources: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub enum OracleProfile {
    #[default]
    #[serde(rename = "terminal-cell-v1")]
    TerminalCellV1,
    #[serde(rename = "vga-pixel-v1")]
    VgaPixelV1,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CaseKind {
    Reftest,
    Crashtest,
    Testharness,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExpectedStatus {
    Run,
    Skip,
    Xfail,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub relation: Relation,
    pub path: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Relation {
    Match,
    Mismatch,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum CaseOutcome {
    Pass,
    AssertionMismatch(String),
    HarnessError(String),
    Crash(String),
    Timeout(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageMetadata {
    pub references: Vec<Reference>,
    pub has_script: bool,
    pub has_reftest_wait: bool,
    pub has_fuzzy: bool,
}

impl Manifest {
    pub fn load() -> Result<Self, String> {
        let path = manifest_path();
        let source = fs::read_to_string(&path)
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
        if self.upstream.repository != REPOSITORY || self.upstream.commit != PINNED_COMMIT {
            return Err("manifest provenance differs from the audited WPT pin".to_string());
        }
        if self.profile.id != "terminal-cell-v1"
            || self.profile.columns != 100
            || self.profile.rows != 38
            || self.profile.metrics != "terminal"
            || self.profile.appearance != "light"
            || self.profile.theme != "paper-white"
            || self.profile.cell.column_px != 8
            || self.profile.cell.row_px != 16
            || self.profile.cell.root_font_px != 16
            || self.profile.palette.text != [16, 16, 16]
            || self.profile.palette.background != [232, 228, 216]
            || self.profile.palette.link != [0, 71, 171]
            || self.profile.palette.link_hover != [139, 26, 26]
        {
            return Err("terminal-cell-v1 profile drifted".to_string());
        }
        if self.vga_profile.id != "vga-pixel-v1"
            || self.vga_profile.columns != 100
            || self.vga_profile.rows != 38
            || self.vga_profile.metrics != "vga"
            || self.vga_profile.appearance != "light"
            || self.vga_profile.theme != "paper-white"
            || self.vga_profile.cell.column_px != 8
            || self.vga_profile.cell.row_px != 16
            || self.vga_profile.cell.root_font_px != 16
            || self.vga_profile.palette.text != [16, 16, 16]
            || self.vga_profile.palette.background != [232, 228, 216]
            || self.vga_profile.palette.link != [0, 71, 171]
            || self.vga_profile.palette.link_hover != [139, 26, 26]
        {
            return Err("vga-pixel-v1 profile drifted".to_string());
        }
        let mut paths = HashSet::new();
        for case in &self.cases {
            validate_path(&case.path)?;
            if !paths.insert(case.path.as_str()) {
                return Err(format!("duplicate case {}", case.path));
            }
            if case.capabilities.is_empty() {
                return Err(format!("{} has no capability classification", case.path));
            }
            let capability_count = case.capabilities.iter().collect::<HashSet<_>>().len();
            if capability_count != case.capabilities.len() {
                return Err(format!("{} repeats a capability", case.path));
            }
            let reason = case.reason.as_deref().unwrap_or_default().trim();
            match case.status {
                ExpectedStatus::Run if !reason.is_empty() => {
                    return Err(format!("run case {} carries a reason", case.path));
                }
                ExpectedStatus::Skip | ExpectedStatus::Xfail if reason.is_empty() => {
                    return Err(format!("{} requires an exact reason", case.path));
                }
                _ => {}
            }
            if case.kind == CaseKind::Crashtest && case.status == ExpectedStatus::Xfail {
                return Err(format!("crashtest {} cannot xfail", case.path));
            }
            if case.kind == CaseKind::Testharness && case.status != ExpectedStatus::Skip {
                return Err(format!("testharness case {} must be skipped", case.path));
            }
            if case.kind == CaseKind::Reftest && case.status != ExpectedStatus::Skip {
                if case.references.is_empty()
                    || !case
                        .references
                        .iter()
                        .any(|reference| reference.relation == Relation::Match)
                {
                    return Err(format!("reftest {} needs a match reference", case.path));
                }
            } else if !case.references.is_empty() {
                return Err(format!(
                    "non-running reftest {} carries references",
                    case.path
                ));
            }
            let mut relations = BTreeSet::new();
            for reference in &case.references {
                validate_path(&reference.path)?;
                if !relations.insert((reference.relation, reference.path.as_str())) {
                    return Err(format!("{} repeats a reference", case.path));
                }
            }
            let mut resources = HashSet::new();
            for resource in &case.resources {
                validate_path(resource)?;
                if !resources.insert(resource.as_str()) {
                    return Err(format!("{} repeats resource {resource}", case.path));
                }
            }
        }
        self.validate_reference_cycles()
    }

    fn validate_reference_cycles(&self) -> Result<(), String> {
        let graph: BTreeMap<&str, Vec<&str>> = self
            .cases
            .iter()
            .filter(|case| case.status != ExpectedStatus::Skip)
            .map(|case| {
                (
                    case.path.as_str(),
                    case.references
                        .iter()
                        .map(|reference| reference.path.as_str())
                        .collect(),
                )
            })
            .collect();
        fn visit<'a>(
            node: &'a str,
            graph: &BTreeMap<&'a str, Vec<&'a str>>,
            visiting: &mut HashSet<&'a str>,
            visited: &mut HashSet<&'a str>,
        ) -> Result<(), String> {
            if visited.contains(node) || !graph.contains_key(node) {
                return Ok(());
            }
            if !visiting.insert(node) {
                return Err(format!("reference cycle at {node}"));
            }
            for child in &graph[node] {
                visit(child, graph, visiting, visited)?;
            }
            visiting.remove(node);
            visited.insert(node);
            Ok(())
        }
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        for node in graph.keys() {
            visit(node, &graph, &mut visiting, &mut visited)?;
        }
        Ok(())
    }

    pub fn runnable_cases(&self, profile: OracleProfile) -> Vec<&Case> {
        let mut cases: Vec<_> = self
            .cases
            .iter()
            .filter(|case| case.status != ExpectedStatus::Skip && case.profile == profile)
            .collect();
        cases.sort_by(|left, right| left.path.cmp(&right.path));
        cases
    }

    pub fn case(&self, path: &str) -> Option<&Case> {
        self.cases.iter().find(|case| case.path == path)
    }

    pub fn vendored_files(&self) -> BTreeSet<String> {
        let mut files = BTreeSet::from(["LICENSE.md".to_string()]);
        for case in self
            .cases
            .iter()
            .filter(|case| case.status != ExpectedStatus::Skip)
        {
            files.insert(case.path.clone());
            files.extend(
                case.references
                    .iter()
                    .map(|reference| reference.path.clone()),
            );
            files.extend(case.resources.iter().cloned());
        }
        files
    }
}

pub fn manifest_path() -> PathBuf {
    std::env::var_os("TEXTSURFER_WPT_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/wpt-rendering.json"))
}

pub fn corpus_root() -> PathBuf {
    std::env::var_os("TEXTSURFER_WPT_CORPUS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/wpt"))
}

pub fn validate_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains('\\')
        || path.contains(':')
        || path.contains('?')
        || path.contains('#')
        || Path::new(path).is_absolute()
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("unsafe corpus path {path}"));
    }
    Ok(())
}

pub fn discover_metadata(path: &str, source: &str) -> Result<PageMetadata, String> {
    let outcome = Html5everParser::new(false).parse_document(source);
    let document = outcome.document.borrow();
    let base = Url::parse("https://wpt.test/")
        .expect("fixed origin")
        .join(path)
        .map_err(|error| format!("base URL for {path}: {error}"))?;
    let mut metadata = PageMetadata::default();
    for root in document.roots() {
        inspect_node(&document, *root, &base, &mut metadata)?;
    }
    metadata.references.sort();
    if metadata
        .references
        .windows(2)
        .any(|pair| pair[0] == pair[1])
    {
        return Err(format!("{path} repeats reference metadata"));
    }
    Ok(metadata)
}

fn inspect_node(
    document: &Document,
    id: NodeId,
    base: &Url,
    metadata: &mut PageMetadata,
) -> Result<(), String> {
    if let Some(Node::Element { name, attrs, .. }) = document.node(id) {
        if name == "script" {
            metadata.has_script = true;
        }
        let attr = |target: &str| {
            attrs
                .iter()
                .find(|value| value.ns == AttrNs::None && value.name.eq_ignore_ascii_case(target))
                .map(|value| value.value.as_str())
        };
        if attr("class").is_some_and(|value| {
            value
                .split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("reftest-wait"))
        }) {
            metadata.has_reftest_wait = true;
        }
        if name == "meta" && attr("name").is_some_and(|value| value.eq_ignore_ascii_case("fuzzy")) {
            metadata.has_fuzzy = true;
        }
        if name == "link" {
            let relation = attr("rel").and_then(|value| {
                value.split_ascii_whitespace().find_map(|token| {
                    match token.to_ascii_lowercase().as_str() {
                        "match" => Some(Relation::Match),
                        "mismatch" => Some(Relation::Mismatch),
                        _ => None,
                    }
                })
            });
            if let (Some(relation), Some(href)) = (relation, attr("href")) {
                let url = base
                    .join(href)
                    .map_err(|error| format!("reference URL {href}: {error}"))?;
                let path = local_url_path(&url)?;
                metadata.references.push(Reference { relation, path });
            }
        }
    }
    for child in document.children(id) {
        inspect_node(document, child, base, metadata)?;
    }
    Ok(())
}

pub fn local_url_path(url: &Url) -> Result<String, String> {
    if url.scheme() != "https"
        || url.host_str() != Some("wpt.test")
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!("non-hermetic URL {url}"));
    }
    let path = url.path().trim_start_matches('/').to_string();
    validate_path(&path)?;
    Ok(path)
}

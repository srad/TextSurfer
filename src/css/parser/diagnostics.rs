use cssparser::SourceLocation;

pub(super) const MAX_RETAINED_DIAGNOSTICS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CssSourcePosition {
    pub line: u32,
    pub column: u32,
}

impl From<SourceLocation> for CssSourcePosition {
    fn from(location: SourceLocation) -> Self {
        Self {
            line: location.line,
            column: location.column,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssDiagnosticKind {
    IgnoredImport,
    InvalidImportForm,
    InvalidMediaRuleForm,
    UnsupportedMediaQuery,
    NestingLimitExceeded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CssDiagnostic {
    pub kind: CssDiagnosticKind,
    pub position: CssSourcePosition,
}

#[derive(Clone, Debug, Default)]
pub struct CssDiagnostics {
    total: usize,
    entries: Vec<CssDiagnostic>,
}

impl CssDiagnostics {
    pub fn total(&self) -> usize {
        self.total
    }

    pub fn entries(&self) -> &[CssDiagnostic] {
        &self.entries
    }

    pub fn truncated(&self) -> bool {
        self.total > self.entries.len()
    }

    pub(super) fn record(&mut self, kind: CssDiagnosticKind, location: SourceLocation) {
        self.total = self.total.saturating_add(1);
        if self.entries.len() < MAX_RETAINED_DIAGNOSTICS {
            self.entries.push(CssDiagnostic {
                kind,
                position: location.into(),
            });
        }
    }
}

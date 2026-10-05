//! Diagnostics: every problem the engine finds, with a code, a severity and a source span.

use std::fmt;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Something was accepted and ignored, or approximated, and the output may differ from
    /// GregorioTeX's.
    Info,
    /// Input the engine doesn't support or that looks like a mistake; output was recovered.
    Warning,
    /// Input that can't be read as written.
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Byte range in the source.
    pub span: Range<usize>,
    /// A stable code such as `gabc::hyphen-in-syllable`.
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{level}[{}] at {}..{}: {}", self.code, self.span.start, self.span.end, self.message)
    }
}

/// Collects diagnostics while a stage runs.
#[derive(Debug, Default)]
pub(crate) struct Sink {
    pub items: Vec<Diagnostic>,
}

impl Sink {
    pub fn push(&mut self, severity: Severity, span: Range<usize>, code: &'static str, message: impl Into<String>) {
        self.items.push(Diagnostic { severity, span, code, message: message.into() });
    }
    pub fn info(&mut self, span: Range<usize>, code: &'static str, message: impl Into<String>) {
        self.push(Severity::Info, span, code, message);
    }
    pub fn warn(&mut self, span: Range<usize>, code: &'static str, message: impl Into<String>) {
        self.push(Severity::Warning, span, code, message);
    }
    pub fn error(&mut self, span: Range<usize>, code: &'static str, message: impl Into<String>) {
        self.push(Severity::Error, span, code, message);
    }
}

/// Line and column (both 1-based, column in chars) of a byte offset, for printing.
pub fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(src.len());
    let before = &src[..src.floor_char_boundary(offset)];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

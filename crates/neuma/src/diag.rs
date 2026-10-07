//! Diagnostics: every problem the engine finds, with a code, a severity and a source span,
//! and an edit that fixes it when there is only one sensible edit. `docs/diagnostics.md`
//! lists every code.

use std::fmt;
use std::ops::Range;

/// How serious a [`Diagnostic`] is. This set is complete: it won't grow.
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

/// A problem found in the source, with where it is and, when there is one sensible edit, the
/// edit that fixes it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    pub severity: Severity,
    /// Byte range in the source.
    pub span: Range<usize>,
    /// A stable code such as `gabc::hyphen-in-syllable`.
    pub code: &'static str,
    /// What is wrong, in a sentence for people.
    pub message: String,
    /// An edit that fixes the problem, where there is only one sensible edit.
    pub fix: Option<Fix>,
}

impl Diagnostic {
    /// A diagnostic without a fix.
    #[must_use]
    pub fn new(severity: Severity, span: Range<usize>, code: &'static str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity,
            span,
            code,
            message: message.into(),
            fix: None,
        }
    }

    /// This diagnostic with `fix`.
    #[must_use]
    pub fn with_fix(mut self, fix: Fix) -> Diagnostic {
        self.fix = Some(fix);
        self
    }
}

/// A source edit: replace the bytes in `span` (empty to insert) with `replacement`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Fix {
    /// Byte range in the source.
    pub span: Range<usize>,
    /// The text to put in its place.
    pub replacement: String,
    /// What the edit does, for a quick-fix menu: "Insert `)`".
    pub title: String,
}

impl Fix {
    /// An edit replacing `span` with `replacement`, described by `title`.
    #[must_use]
    pub fn new(span: Range<usize>, replacement: impl Into<String>, title: impl Into<String>) -> Fix {
        Fix {
            span,
            replacement: replacement.into(),
            title: title.into(),
        }
    }

    /// `src` with the fix applied. `None` if the span isn't on character boundaries in `src`.
    #[must_use]
    pub fn apply(&self, src: &str) -> Option<String> {
        let before = src.get(..self.span.start)?;
        let after = src.get(self.span.end..)?;
        Some([before, &self.replacement, after].concat())
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(
            f,
            "{level}[{}] at {}..{}: {}",
            self.code, self.span.start, self.span.end, self.message
        )
    }
}

/// Collects diagnostics while a stage runs.
#[derive(Debug, Default)]
pub(crate) struct Sink {
    pub items: Vec<Diagnostic>,
}

impl Sink {
    pub fn push(&mut self, severity: Severity, span: Range<usize>, code: &'static str, message: impl Into<String>) {
        self.items.push(Diagnostic::new(severity, span, code, message));
    }
    /// Attaches a fix to the diagnostic pushed last.
    pub fn fix(&mut self, fix: Fix) {
        if let Some(d) = self.items.last_mut() {
            d.fix = Some(fix);
        }
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
#[must_use]
pub fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(src.len());
    let before = &src[..src.floor_char_boundary(offset)];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

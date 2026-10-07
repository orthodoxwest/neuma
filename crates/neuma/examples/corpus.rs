//! Runs a directory of GABC files through the whole pipeline: parse, engrave, lay out at
//! several widths, then the display list, note map, SVG and a GABC round trip. It fails on a
//! panic, a file slower than `--max-ms`, or a round trip that doesn't reach a fixed point,
//! and stops at once on a file still running after `--hang-secs`. It counts diagnostics by
//! code and can compare them with an earlier run.
//!
//! ```sh
//! cargo run --release -p neuma --example corpus -- DIR... [--counts OUT.tsv]
//!     [--baseline IN.tsv] [--markdown OUT.md] [--max-ms 5000] [--hang-secs 60]
//! ```
//!
//! The counts file has one line per diagnostic code: severity, code, diagnostics, files.
//! Panic messages in the report are cut short, so they don't copy a score's text into it.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use neuma::{Font, LayoutOptions, MetricsTable, Severity, StyleOptions, SvgOptions, Weights};

const WIDTHS: [f32; 3] = [320.0, 720.0, 1400.0];

struct Options {
    dirs: Vec<PathBuf>,
    counts: Option<PathBuf>,
    baseline: Option<PathBuf>,
    markdown: Option<PathBuf>,
    max_ms: u64,
    hang_secs: u64,
}

fn options() -> Result<Options, String> {
    let mut o = Options {
        dirs: Vec::new(),
        counts: None,
        baseline: None,
        markdown: None,
        max_ms: 5000,
        hang_secs: 60,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("{a} takes a value"));
        match a.as_str() {
            "--counts" => o.counts = Some(value()?.into()),
            "--baseline" => o.baseline = Some(value()?.into()),
            "--markdown" => o.markdown = Some(value()?.into()),
            "--max-ms" => o.max_ms = value()?.parse().map_err(|e| format!("--max-ms: {e}"))?,
            "--hang-secs" => o.hang_secs = value()?.parse().map_err(|e| format!("--hang-secs: {e}"))?,
            _ if a.starts_with("--") => return Err(format!("unknown option {a}")),
            _ => o.dirs.push(a.into()),
        }
    }
    if o.dirs.is_empty() {
        return Err("give at least one directory of .gabc files".into());
    }
    Ok(o)
}

fn gabc_files(dirs: &[PathBuf]) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for dir in dirs {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "gabc") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

#[derive(Default)]
struct Count {
    severity: String,
    diagnostics: u64,
    files: u64,
}

/// What one file did, short of a panic.
struct Outcome {
    codes: BTreeMap<&'static str, (Severity, u64)>,
    lines: usize,
    stable: bool,
}

fn run(src: &str, metrics: &MetricsTable) -> Outcome {
    let parsed = neuma::parse(src);
    let engraving = parsed.score.engrave(metrics, &StyleOptions::default());
    let mut codes = BTreeMap::new();
    for d in parsed.diagnostics.iter().chain(&engraving.diagnostics) {
        codes.entry(d.code).or_insert((d.severity, 0)).1 += 1;
    }
    let mut lines = 0;
    for width in WIDTHS {
        let layout = engraving.layout(width, &LayoutOptions::default());
        lines = layout.line_count();
        let _ = layout.display();
        let _ = layout.notes(&Weights::SOLESMES);
        let _ = layout.svg(&SvgOptions::default());
    }
    let once = parsed.score.to_gabc();
    let twice = neuma::parse(&once).score.to_gabc();
    Outcome {
        codes,
        lines,
        stable: once == twice,
    }
}

/// A panic message for the report: its first line, at most 120 characters, and none of what
/// follows a `:` or a quote, which is where an assertion or a `{:?}` prints the score's text.
/// The report is uploaded, so it must not carry the chants themselves.
fn short(msg: &str) -> String {
    let line = msg.lines().next().unwrap_or("");
    let cut = line.find([':', '"', '`', '\'']).unwrap_or(line.len());
    let mut out: String = line[..cut].chars().take(120).collect();
    if out.len() < line.len() {
        out.push('…');
    }
    out
}

fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

fn read_counts(path: &Path) -> Result<BTreeMap<String, Count>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        let [sev, code, n, files] = f[..] else {
            return Err(format!("{}: bad line {line:?}", path.display()));
        };
        let num = |s: &str| s.parse::<u64>().map_err(|e| format!("{}: {line:?}: {e}", path.display()));
        out.insert(
            code.to_string(),
            Count {
                severity: sev.to_string(),
                diagnostics: num(n)?,
                files: num(files)?,
            },
        );
    }
    Ok(out)
}

fn main() -> ExitCode {
    let o = match options() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("corpus: {e}");
            return ExitCode::from(2);
        }
    };
    let files = match gabc_files(&o.dirs) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("corpus: {e}");
            return ExitCode::from(2);
        }
    };
    let metrics = MetricsTable::from_bytes(Font::Google.table_bytes()).expect("built-in metrics");

    // A panic's message, kept for the report instead of printed as it happens.
    let last_panic: Arc<Mutex<String>> = Arc::default();
    {
        let last_panic = last_panic.clone();
        panic::set_hook(Box::new(move |info| {
            let msg = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_default();
            let place = info.location().map(|l| format!(" at {l}")).unwrap_or_default();
            *last_panic.lock().unwrap() = format!("{}{place}", short(&msg));
        }));
    }

    // A watchdog: a file still running after `hang_secs` stops the run, since a hung thread
    // can't be interrupted.
    let started = Instant::now();
    let current = Arc::new(AtomicUsize::new(usize::MAX));
    let since = Arc::new(AtomicU64::new(0));
    {
        let (current, since, files) = (current.clone(), since.clone(), files.clone());
        let limit = Duration::from_secs(o.hang_secs);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let i = current.load(Ordering::SeqCst);
                let t = Duration::from_millis(since.load(Ordering::SeqCst));
                if i != usize::MAX && started.elapsed().saturating_sub(t) > limit {
                    eprintln!("HANG {}: still running after {}s", files[i].display(), limit.as_secs());
                    std::process::exit(3);
                }
            }
        });
    }

    let mut counts: BTreeMap<String, Count> = BTreeMap::new();
    let (mut panics, mut slow, mut unstable) = (Vec::new(), Vec::new(), Vec::new());
    let mut times: Vec<(Duration, usize)> = Vec::new();
    let mut lines = 0usize;
    for (i, path) in files.iter().enumerate() {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("corpus: {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        let src = String::from_utf8_lossy(&bytes);
        since.store(started.elapsed().as_millis() as u64, Ordering::SeqCst);
        current.store(i, Ordering::SeqCst);
        let t = Instant::now();
        let result = panic::catch_unwind(AssertUnwindSafe(|| run(&src, &metrics)));
        let took = t.elapsed();
        current.store(usize::MAX, Ordering::SeqCst);
        times.push((took, i));
        match result {
            Err(_) => {
                let msg = last_panic.lock().unwrap().clone();
                eprintln!("PANIC {}: {msg}", path.display());
                panics.push((i, msg));
            }
            Ok(out) => {
                lines += out.lines;
                if !out.stable {
                    eprintln!("UNSTABLE {}: writing it as GABC twice gives different text", path.display());
                    unstable.push(i);
                }
                for (code, (sev, n)) in out.codes {
                    let c = counts.entry(code.to_string()).or_default();
                    c.severity = severity(sev).to_string();
                    c.diagnostics += n;
                    c.files += 1;
                }
            }
        }
        if took > Duration::from_millis(o.max_ms) {
            eprintln!("SLOW {}: {} ms", path.display(), took.as_millis());
            slow.push((i, took));
        }
    }

    times.sort();
    let total: Duration = times.iter().map(|t| t.0).sum();
    let pct = |p: f64| {
        times
            .get(((times.len() as f64 - 1.0) * p).round() as usize)
            .map_or(0, |t| t.0.as_millis())
    };
    let errors: u64 = counts.values().filter(|c| c.severity == "error").map(|c| c.files).sum();
    let mut report = String::new();
    let _ = writeln!(report, "## Corpus run\n");
    let _ = writeln!(
        report,
        "{} files, {} lines at {} units, in {:.1} s (median {} ms, 99th percentile {} ms, slowest {} ms{}).\n",
        files.len(),
        lines,
        WIDTHS[WIDTHS.len() - 1],
        total.as_secs_f64(),
        pct(0.5),
        pct(0.99),
        pct(1.0),
        times.last().map_or(String::new(), |t| format!(
            ", {}",
            files[t.1].file_name().unwrap_or_default().to_string_lossy()
        )),
    );
    let _ = writeln!(
        report,
        "- Panics: {}\n- Slower than {} ms: {}\n- Unstable GABC round trips: {}\n- Files with an error diagnostic: {}\n",
        panics.len(),
        o.max_ms,
        slow.len(),
        unstable.len(),
        errors
    );
    for (i, msg) in &panics {
        let _ = writeln!(report, "- panic in `{}`: {msg}", files[*i].display());
    }
    for (i, took) in &slow {
        let _ = writeln!(report, "- `{}` took {} ms", files[*i].display(), took.as_millis());
    }
    for i in &unstable {
        let _ = writeln!(
            report,
            "- `{}` doesn't reach a fixed point when written as GABC",
            files[*i].display()
        );
    }

    let baseline = match o.baseline.as_deref().map(read_counts).transpose() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("corpus: {e}");
            return ExitCode::from(2);
        }
    };
    let _ = writeln!(report, "\n### Diagnostics\n");
    let _ = writeln!(
        report,
        "| Severity | Code | Diagnostics | Files |{}\n| --- | --- | ---: | ---: |{}",
        if baseline.is_some() { " Files before |" } else { "" },
        if baseline.is_some() { " ---: |" } else { "" }
    );
    let mut codes: Vec<&String> = counts.keys().collect();
    if let Some(b) = &baseline {
        codes.extend(b.keys().filter(|k| !counts.contains_key(*k)));
    }
    codes.sort();
    for code in codes {
        let empty = Count::default();
        let c = counts.get(code).unwrap_or(&empty);
        let sev = if c.severity.is_empty() {
            baseline.as_ref().and_then(|b| b.get(code)).map_or("", |b| b.severity.as_str())
        } else {
            c.severity.as_str()
        };
        let _ = write!(report, "| {sev} | `{code}` | {} | {} |", c.diagnostics, c.files);
        if let Some(b) = &baseline {
            let before = b.get(code).map_or(0, |b| b.files);
            let mark = if before == c.files {
                String::new()
            } else {
                format!(" ({:+})", c.files as i64 - before as i64)
            };
            let _ = write!(report, " {before}{mark} |");
        }
        report.push('\n');
    }
    print!("{report}");
    if let Some(path) = &o.markdown
        && let Err(e) = fs::write(path, &report)
    {
        eprintln!("corpus: {}: {e}", path.display());
        return ExitCode::from(2);
    }
    if let Some(path) = &o.counts {
        let mut tsv = String::from("# severity\tcode\tdiagnostics\tfiles\n");
        for (code, c) in &counts {
            let _ = writeln!(tsv, "{}\t{code}\t{}\t{}", c.severity, c.diagnostics, c.files);
        }
        if let Err(e) = fs::write(path, tsv) {
            eprintln!("corpus: {}: {e}", path.display());
            return ExitCode::from(2);
        }
    }
    if panics.is_empty() && slow.is_empty() && unstable.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

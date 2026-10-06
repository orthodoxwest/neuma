//! `neuma render | check | notes`: the engine from the command line.

use std::io::{Read as _, Write as _};
use std::process::ExitCode;

use neuma::{Initial, LayoutOptions, MetricsTable, Severity, StyleOptions, SvgOptions, Weights};
use neuma_wasm::{Chant, ChantOptions, Font};

const USAGE: &str = "usage: neuma <render|check|notes> [--width PX] [--scale PX] [--initial LINES] [--font FONT]
                     [--max-lines N] [FILE|-]
       neuma info [FILE...]

  render   write SVG to stdout
  check    print diagnostics; exit 1 on errors
  notes    print the layout and playback timeline as JSON, as the browser package does
  info     print each score's catalogue entry as one line of JSON, with its file name;
           the layout options don't apply

  --initial LINES   drop-cap height in staves, 0 to 4; 0 for none (default 1)
  --max-lines N     keep only the first N lines, as broken for the whole score (an incipit);
                    a taller initial keeps its full size
  --font FONT       the EB Garamond the lyrics are measured for: google (Google Fonts,
                    the default) or eb-garamond-12";

/// Prints a line to stdout, ignoring a closed pipe (`neuma info *.gabc | head`).
macro_rules! out {
    ($($arg:tt)*) => {
        let _ = writeln!(std::io::stdout(), $($arg)*);
    };
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().cloned() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let mut width = 800.0f32;
    let mut scale = LayoutOptions::default().scale;
    let mut style = StyleOptions::default();
    let mut initial_lines = 1u8;
    let mut font = Font::Google;
    let mut max_lines = 0usize;
    let mut files = Vec::new();
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--width" => width = it.next().and_then(|v| v.parse().ok()).unwrap_or(width),
            "--scale" => scale = it.next().and_then(|v| v.parse().ok()).unwrap_or(scale),
            "--initial" => match it.next().and_then(|v| v.parse::<u8>().ok()).filter(|n| *n <= 4) {
                Some(n) => {
                    initial_lines = n;
                    style.initial = if n == 0 { Initial::None } else { Initial::Lines(n) };
                }
                None => {
                    eprintln!("neuma: --initial takes a number of staves from 0 to 4\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--font" => match it.next().map(String::as_str) {
                Some("google") => font = Font::Google,
                Some("eb-garamond-12") => font = Font::Garamond12,
                _ => {
                    eprintln!("neuma: --font takes google or eb-garamond-12\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "-h" | "--help" => {
                out!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--max-lines" => match it.next().and_then(|v| v.parse().ok()) {
                Some(n) => max_lines = n,
                None => {
                    eprintln!("neuma: --max-lines takes a number of lines\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            f => files.push(f.to_string()),
        }
    }
    if cmd == "info" {
        if files.is_empty() {
            files.push("-".to_string());
        }
        let mut status = ExitCode::SUCCESS;
        for path in &files {
            let Some(src) = read(path) else {
                status = ExitCode::from(2);
                continue;
            };
            // The entry's object, with the file name as its first field.
            let mut line = String::from("{\"file\":");
            neuma_wasm::json::string(&mut line, path);
            let mut entry = String::new();
            neuma_wasm::json::summary(&mut entry, &neuma::summarize(&src));
            line.push(',');
            line.push_str(&entry[1..]);
            out!("{line}");
        }
        return status;
    }
    if files.len() > 1 {
        eprintln!("neuma: {cmd} takes one file\n{USAGE}");
        return ExitCode::from(2);
    }
    let Some(src) = read(files.first().map_or("-", String::as_str)) else {
        return ExitCode::from(2);
    };
    let parsed = neuma::parse(&src);
    let metrics = match MetricsTable::from_bytes(font.table_bytes()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("neuma: built-in metrics: {e}");
            return ExitCode::FAILURE;
        }
    };
    let engraving = parsed.score.engrave(&metrics, &style);
    let layout = engraving.layout(
        width,
        &LayoutOptions {
            scale,
            max_lines,
            ..LayoutOptions::default()
        },
    );
    match cmd.as_str() {
        "render" => {
            out!("{}", layout.svg(&SvgOptions::default()));
            ExitCode::SUCCESS
        }
        "check" => {
            let mut errors = false;
            for d in parsed.diagnostics.iter().chain(&engraving.diagnostics) {
                let (line, col) = neuma::diag::line_col(&src, d.span.start);
                out!("{line}:{col}: {d}");
                errors |= d.severity == Severity::Error;
            }
            if errors { ExitCode::FAILURE } else { ExitCode::SUCCESS }
        }
        "notes" => {
            // The browser package's layout JSON, without the SVG, with the same note ids.
            let mut chant = Chant::new(
                &src,
                ChantOptions {
                    initial: initial_lines,
                    font,
                    ..ChantOptions::default()
                },
            );
            let opts = LayoutOptions {
                scale,
                max_lines,
                ..LayoutOptions::default()
            };
            chant.layout(width, &opts, &Weights::SOLESMES, &SvgOptions::default());
            out!("{}", chant.layout_json());
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// The file at `path`, or stdin for `-`; reports a failure and returns `None`.
fn read(path: &str) -> Option<String> {
    if path == "-" {
        let mut s = String::new();
        if std::io::stdin().read_to_string(&mut s).is_err() {
            eprintln!("neuma: can't read stdin");
            return None;
        }
        return Some(s);
    }
    match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("neuma: {path}: {e}");
            None
        }
    }
}

//! `neuma render | check | notes`: the engine from the command line.

use std::io::Read as _;
use std::process::ExitCode;

use neuma::{Initial, LayoutOptions, MetricsTable, Severity, StyleOptions, SvgOptions, Weights};

/// Metrics for EB Garamond 12, the lyric face the SVG output asks for.
const EB_GARAMOND: &[u8] = include_bytes!("../../neuma-metrics/tables/eb-garamond-12.bin");

const USAGE: &str = "usage: neuma <render|check|notes> [--width PX] [--scale PX] [--initial LINES] [FILE|-]

  render   write SVG to stdout
  check    print diagnostics; exit 1 on errors
  notes    print the note map as JSON lines

  --initial LINES   drop-cap height in staves, 0 to 4; 0 for none (default 1)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().cloned() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let mut width = 800.0f32;
    let mut scale = LayoutOptions::default().scale;
    let mut style = StyleOptions::default();
    let mut file = None;
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--width" => width = it.next().and_then(|v| v.parse().ok()).unwrap_or(width),
            "--scale" => scale = it.next().and_then(|v| v.parse().ok()).unwrap_or(scale),
            "--initial" => match it.next().and_then(|v| v.parse::<u8>().ok()).filter(|n| *n <= 4) {
                Some(n) => style.initial = if n == 0 { Initial::None } else { Initial::Lines(n) },
                None => {
                    eprintln!("neuma: --initial takes a number of staves from 0 to 4\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            f => file = Some(f.to_string()),
        }
    }
    let src = match file.as_deref() {
        None | Some("-") => {
            let mut s = String::new();
            if std::io::stdin().read_to_string(&mut s).is_err() {
                eprintln!("neuma: can't read stdin");
                return ExitCode::from(2);
            }
            s
        }
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("neuma: {path}: {e}");
                return ExitCode::from(2);
            }
        },
    };
    let parsed = neuma::parse(&src);
    let metrics = match MetricsTable::from_bytes(EB_GARAMOND) {
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
            ..LayoutOptions::default()
        },
    );
    match cmd.as_str() {
        "render" => {
            println!("{}", layout.svg(&SvgOptions::default()));
            ExitCode::SUCCESS
        }
        "check" => {
            let mut errors = false;
            for d in parsed.diagnostics.iter().chain(&engraving.diagnostics) {
                let (line, col) = neuma::diag::line_col(&src, d.span.start);
                println!("{line}:{col}: {d}");
                errors |= d.severity == Severity::Error;
            }
            if errors { ExitCode::FAILURE } else { ExitCode::SUCCESS }
        }
        "notes" => {
            for n in layout.notes(&Weights::SOLESMES).notes {
                println!(
                    "{{\"id\":{},\"syllable\":{},\"line\":{},\"x\":{:.2},\"y\":{:.2},\"staff_position\":{},\"semitones\":{},\"weight\":{},\"text\":{:?}}}",
                    n.id, n.syllable, n.line, n.x, n.y, n.staff_position, n.semitones, n.weight, n.syllable_text
                );
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

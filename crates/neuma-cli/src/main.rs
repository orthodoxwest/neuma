//! `neuma render | check | notes | info | psalm | point | tones`: the engine from the command line.

use std::io::{Read as _, Write as _};
use std::process::ExitCode;

use neuma::{Initial, LayoutOptions, MetricsTable, Severity, StyleOptions, SvgOptions, Weights};
use neuma_wasm::{Chant, ChantOptions, Font};

const USAGE: &str = "usage: neuma <render|check|notes> [--width PX] [--scale PX] [--initial LINES] [--font FONT]
                     [--max-lines N] [FILE|-]
       neuma info [FILE...]
       neuma psalm --tone TONE [--tone-file FILE] [--intone first|every|never] [--name NAME] [--no-point] [FILE|-]
       neuma point --tone TONE [--tone-file FILE] [FILE|-]
       neuma tones

  render   write SVG to stdout
  check    print diagnostics; exit 1 on errors
  notes    print the layout and playback timeline as JSON, as the browser package does
  info     print each score's catalogue entry as one line of JSON, with its file name;
           the layout options don't apply
  psalm    set psalm text (a verse per line, the mediant marked `*`) to a tone and print it
           as GABC. Half-verses without pointing marks are pointed automatically, unless
           --no-point. Problems go to stderr. Pipe it to `neuma render -` to see it.
  point    print the text with pointing marks added for the tone; half-verses the pointer
           is unsure of are listed on stderr
  tones    list the built-in psalm tones

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
    let mut tone_name: Option<String> = None;
    let mut tone_file: Option<String> = None;
    let mut psalm = neuma_tones::Options::default();
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
            "--tone" => tone_name = it.next().cloned(),
            "--tone-file" => tone_file = it.next().cloned(),
            "--name" => psalm.name = it.next().cloned(),
            "--no-point" => psalm.no_auto_point = true,
            "--intone" => match it.next().map(String::as_str) {
                Some("first") => psalm.intone = neuma_tones::Intone::FirstVerse,
                Some("every") => psalm.intone = neuma_tones::Intone::EveryVerse,
                Some("never") => psalm.intone = neuma_tones::Intone::Never,
                _ => {
                    eprintln!("neuma: --intone takes first, every or never\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            f => files.push(f.to_string()),
        }
    }
    if cmd == "tones" {
        for t in neuma_tones::Tone::builtin() {
            out!(
                "{:<6} {}  mediant: {}  termination: {}",
                t.name,
                t.clef_gabc(),
                t.mediant,
                t.termination
            );
        }
        return ExitCode::SUCCESS;
    }
    if cmd == "psalm" || cmd == "point" {
        let tone = match resolve_tone(tone_name, tone_file) {
            Ok(t) => t,
            Err(code) => return code,
        };
        let path = files.first().map_or("-", String::as_str);
        let Some(src) = read(path) else { return ExitCode::from(2) };
        return if cmd == "psalm" {
            psalm_command(&src, &tone, &psalm)
        } else {
            point_command(&src, &tone)
        };
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

/// The tone `--tone` names, from `--tone-file` when given, else built in.
fn resolve_tone(name: Option<String>, file: Option<String>) -> Result<neuma_tones::Tone, ExitCode> {
    let custom = match &file {
        Some(f) => {
            let src = read(f).ok_or(ExitCode::from(2))?;
            neuma_tones::Tone::parse_all(&src).map_err(|e| {
                eprintln!("neuma: {f}: {e}");
                ExitCode::from(2)
            })?
        }
        None => Vec::new(),
    };
    let found = match (&name, &file) {
        // A name not in the file falls back to the built-in tones.
        (Some(n), _) => neuma_tones::Tone::find(&custom, n).or_else(|| neuma_tones::Tone::named(n)),
        (None, Some(_)) => custom.first(),
        (None, None) => {
            eprintln!("neuma: this needs --tone or --tone-file\n{USAGE}");
            return Err(ExitCode::from(2));
        }
    };
    found.cloned().ok_or_else(|| {
        match (&name, &file) {
            (Some(n), Some(f)) => eprintln!("neuma: no tone {n} in {f} or built in"),
            (Some(n), None) => eprintln!("neuma: no built-in tone {n}; `neuma tones` lists them"),
            (None, _) => eprintln!("neuma: {} has no tones", file.as_deref().unwrap_or("")),
        }
        ExitCode::from(2)
    })
}

/// `neuma psalm`: psalm text and a tone to GABC.
fn psalm_command(src: &str, tone: &neuma_tones::Tone, options: &neuma_tones::Options) -> ExitCode {
    let setting = neuma_tones::apply_text(tone, src, options);
    let mut errors = false;
    for d in &setting.diagnostics {
        let (line, col) = neuma::diag::line_col(src, d.span.start);
        eprintln!("{line}:{col}: {d}");
        errors |= d.severity == Severity::Error;
    }
    out!("{}", setting.gabc);
    if errors { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// `neuma point`: psalm text with pointing marks added.
fn point_command(src: &str, tone: &neuma_tones::Tone) -> ExitCode {
    let p = neuma_tones::point_text(tone, src);
    let mut errors = false;
    for d in &p.pointed.diagnostics {
        let (line, col) = neuma::diag::line_col(src, d.span.start);
        eprintln!("{line}:{col}: {d}");
        errors |= d.severity == Severity::Error;
    }
    for h in p.halves.iter().filter(|h| !h.kept && h.confidence < neuma_tones::apply::UNSURE) {
        let verse = &p.pointed.verses[h.verse];
        let (line, _) = neuma::diag::line_col(src, verse.span.start);
        let part = match h.part {
            neuma_tones::PartKind::Flex => "flex",
            neuma_tones::PartKind::Mediant => "first half",
            neuma_tones::PartKind::Termination => "second half",
        };
        eprintln!("{line}: check the {part}: {:.0}% sure", h.confidence * 100.0);
    }
    out!("{}", p.text().trim_end());
    if errors { ExitCode::FAILURE } else { ExitCode::SUCCESS }
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

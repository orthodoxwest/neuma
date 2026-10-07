//! `neuma render | check | notes | info | psalm | point | tones | book`: the engine from the command line.

use std::io::{Read as _, Write as _};
use std::process::ExitCode;

use neuma::{Chant, ChantOptions, Initial, LayoutOptions, LyricFont, Severity};

const USAGE: &str = "usage: neuma <render|check|notes> [--width PX] [--scale PX] [--initial LINES] [--font FONT]
                     [--max-lines N] [FILE|-]
       neuma info [FILE...]
       neuma psalm --tone TONE [--tone-file FILE] [--intone first|every|never] [--name NAME] [--no-auto-point] [FILE|-]
       neuma point --tone TONE [--tone-file FILE] [FILE|-]
       neuma tones
       neuma book FILE.book [-o OUT.pdf] [--svg DIR] [--text-as-paths]

  render   write SVG to stdout
  check    print diagnostics; exit 1 on errors
  notes    print the layout and playback timeline as JSON, as the browser package does
  info     print each score's library entry as one line of JSON, with its file name;
           the layout options don't apply
  psalm    set psalm text (a verse per line, the mediant marked `*`) to a tone and print it
           as GABC. Half-verses without pointing marks are pointed automatically, unless
           --no-auto-point. Problems go to stderr. Pipe it to `neuma render -` to see it.
  point    print the text with pointing marks added for the tone; half-verses the pointer
           is unsure of are listed on stderr
  tones    list the built-in psalm tones
  book     set a booklet (an ordered list of scores, psalms, rubrics and text; see
           crates/neuma-book/README.md) on pages, and write a PDF (-o, default FILE.pdf)
           and, with --svg DIR, one SVG per page. Problems go to stderr.

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
    // `book` has flags of its own; the rendering flags below don't apply to it.
    if cmd == "book" {
        return book_command(&args[1..]);
    }
    let mut width = 800.0f32;
    let mut layout = LayoutOptions::default();
    let mut options = ChantOptions::default();
    let mut files = Vec::new();
    let mut tone_name: Option<String> = None;
    let mut tone_file: Option<String> = None;
    let mut psalm = neuma_tones::PsalmOptions::default();
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--width" => width = it.next().and_then(|v| v.parse().ok()).unwrap_or(width),
            "--scale" => {
                if let Some(scale) = it.next().and_then(|v| v.parse().ok()) {
                    layout = layout.with_scale(scale);
                }
            }
            "--initial" => match it.next().and_then(|v| v.parse::<u8>().ok()).filter(|n| *n <= 4) {
                Some(n) => options = options.with_initial(Initial::from_staves(n.into())),
                None => {
                    eprintln!("neuma: --initial takes a number of staves from 0 to 4\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--font" => match it.next().map(String::as_str) {
                Some("google") => options = options.with_font(LyricFont::Google),
                Some("eb-garamond-12") => options = options.with_font(LyricFont::Garamond12),
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
                Some(n) => layout = layout.with_max_lines(n),
                None => {
                    eprintln!("neuma: --max-lines takes a number of lines\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "--tone" => tone_name = it.next().cloned(),
            "--tone-file" => tone_file = it.next().cloned(),
            "--name" => psalm.name = it.next().cloned(),
            "--no-auto-point" => psalm.auto_point = false,
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
            neuma::json::string(&mut line, path);
            let mut entry = String::new();
            neuma::json::summary(&mut entry, &neuma::summarize(&src));
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
    let chant = Chant::with_options(&src, options);
    match cmd.as_str() {
        "render" => {
            out!("{}", chant.layout_with(width, &layout).svg());
            ExitCode::SUCCESS
        }
        "check" => {
            let mut errors = false;
            for d in chant.diagnostics() {
                let (line, col) = neuma::diag::line_col(&src, d.span.start);
                out!("{line}:{col}: {d}");
                if let Some(fix) = &d.fix {
                    out!("    fix: {}", fix.title);
                }
                errors |= d.severity == Severity::Error;
            }
            if errors { ExitCode::FAILURE } else { ExitCode::SUCCESS }
        }
        "notes" => {
            // The browser package's layout JSON, without the SVG, with the same note ids.
            let placed = chant.layout_with(width, &layout);
            let timeline = placed.timeline();
            let mut json = String::new();
            neuma::json::layout(&mut json, placed.size(), Some(&timeline), Some(chant.utf16()));
            out!("{json}");
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
        (Some(n), _) => neuma_tones::Tone::find(&custom, n).or_else(|| neuma_tones::Tone::named(n).ok()),
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
fn psalm_command(src: &str, tone: &neuma_tones::Tone, options: &neuma_tones::PsalmOptions) -> ExitCode {
    let setting = neuma_tones::psalm(src, tone, options);
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
    let p = neuma_tones::point(src, tone);
    let mut errors = false;
    for d in &p.diagnostics {
        let (line, col) = neuma::diag::line_col(src, d.span.start);
        eprintln!("{line}:{col}: {d}");
        errors |= d.severity == Severity::Error;
    }
    for h in p.halves.iter().filter(|h| !h.kept && h.confidence < neuma_tones::UNSURE) {
        let (line, _) = neuma::diag::line_col(src, h.span.start);
        let part = match h.part {
            neuma_tones::VersePart::Flex => "flex",
            neuma_tones::VersePart::Mediant => "first half",
            neuma_tones::VersePart::Termination => "second half",
        };
        eprintln!("{line}: check the {part}: {:.0}% sure", h.confidence * 100.0);
    }
    out!("{}", p.text.trim_end());
    if errors { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// `neuma book`: a `.book` file to a PDF and, optionally, SVG pages.
fn book_command(args: &[String]) -> ExitCode {
    let mut file = None;
    let mut pdf_out = None;
    let mut svg_dir = None;
    let mut paths = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" | "--output" => pdf_out = it.next().cloned(),
            "--svg" => svg_dir = it.next().cloned(),
            "--text-as-paths" => paths = true,
            "-h" | "--help" => {
                out!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            f if file.is_none() && !f.starts_with('-') => file = Some(f.to_string()),
            other => {
                eprintln!("neuma: book: unexpected `{other}`\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(file) = file else {
        eprintln!("neuma: book needs a .book file\n{USAGE}");
        return ExitCode::from(2);
    };
    let Some(src) = read(&file) else { return ExitCode::from(2) };
    let path = std::path::Path::new(&file);
    let mut book = match neuma_book::Book::parse(&src) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{file}: {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = book.resolve(path.parent().unwrap_or(std::path::Path::new("."))) {
        eprintln!("{file}: {e}");
        return ExitCode::from(2);
    }
    let files = match neuma_book::font_files(&book.settings) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("neuma: {e}");
            return ExitCode::from(2);
        }
    };
    let fonts = neuma_book::Fonts::new(&files);
    if fonts.is_standard() {
        eprintln!("neuma: no text font: set `font:` in the book, or install EB Garamond 12; using the PDF's standard Times");
    }
    let mut doc = neuma_book::typeset(&book, &fonts);
    if paths {
        doc.set_text_as_paths(true);
    }
    let mut errors = false;
    for p in &doc.problems {
        let d = &p.diagnostic;
        errors |= d.severity == Severity::Error;
        if d.severity != Severity::Info || d.code == "point::unsure" {
            eprintln!("{file}: piece {}: {d}", p.piece + 1);
        }
    }
    let missing = fonts.missing();
    if !missing.is_empty() {
        let list: String = missing.iter().map(|c| format!(" {c} (U+{:04X})", *c as u32)).collect();
        eprintln!("neuma: the text font has no glyph for{list}");
    }
    let pdf_out = pdf_out.unwrap_or_else(|| path.with_extension("pdf").to_string_lossy().into_owned());
    if let Err(e) = std::fs::write(&pdf_out, doc.pdf(&fonts)) {
        eprintln!("neuma: {pdf_out}: {e}");
        return ExitCode::FAILURE;
    }
    if let Some(dir) = svg_dir {
        let dir = std::path::Path::new(&dir);
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("neuma: {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
        for i in 0..doc.pages.len() {
            let out = dir.join(format!("page-{:03}.svg", i + 1));
            if let Err(e) = std::fs::write(&out, doc.svg(i, &fonts).unwrap_or_default()) {
                eprintln!("neuma: {}: {e}", out.display());
                return ExitCode::FAILURE;
            }
        }
    }
    eprintln!("{pdf_out}: {} pages", doc.pages.len());
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

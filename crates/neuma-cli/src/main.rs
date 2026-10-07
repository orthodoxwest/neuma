//! `neuma render | check | notes | info | psalm | point | tones | book`: the engine from the command line.

use std::io::{Read as _, Write as _};
use std::process::ExitCode;

use neuma::{Chant, ChantOptions, Diagnostic, Initial, LayoutOptions, LyricFont, Severity};

const USAGE: &str = "usage: neuma render [--width PX] [--scale PX] [--initial LINES] [--font FONT] [--max-lines N]
                    [FILE|-]
       neuma notes  [--width PX] [--scale PX] [--initial LINES] [--font FONT] [--max-lines N]
                    [FILE|-]
       neuma check  [--initial LINES] [--font FONT] [FILE...]
       neuma info   [FILE...]
       neuma psalm  --tone TONE [--tone-file FILE] [--intone first|every|never] [--name NAME]
                    [--no-auto-point] [FILE|-]
       neuma point  --tone TONE [--tone-file FILE] [FILE|-]
       neuma tones
       neuma book   FILE.book|- [-o OUT.pdf] [--svg DIR] [--text-as-paths]
       neuma --help | --version

  render   write SVG to stdout
  notes    print the layout and playback timeline as JSON, as the browser package does
  check    print each file's diagnostics, in order of position, and the fix for each that has
           one; exit 1 if any file has errors
  info     print each score's library entry as one line of JSON, with its file name
  psalm    set psalm text (a verse per line, the mediant marked `*`) to a tone and print it
           as GABC. Half-verses without pointing marks are pointed automatically, unless
           --no-auto-point. Problems go to stderr. Pipe it to `neuma render -` to see it.
  point    print the text with pointing marks added for the tone; half-verses the pointer
           is unsure of are listed on stderr
  tones    list the built-in psalm tones
  book     set a booklet (an ordered list of scores, psalms, rubrics and text; see
           crates/neuma-book/README.md) on pages, and write a PDF (-o, default FILE.pdf;
           needed when the book is read from stdin) and, with --svg DIR, one SVG per page.
           Problems go to stderr.

  --width PX        the column width in pixels, above 0 and at most 1000000 (default 800)
  --scale PX        pixels per staff space, above 0 and at most 1000 (default 6)
  --initial LINES   drop-cap height in staves, 0 to 4; 0 for none (default 1)
  --max-lines N     keep only the first N lines, as broken for the whole score (an incipit);
                    0 keeps them all; a taller initial keeps its full size
  --font FONT       the EB Garamond the lyrics are measured for: google (Google Fonts,
                    the default) or eb-garamond-12
  --                end the options: what follows is a file, even if it starts with -

  A FILE of - is stdin, which is also read when no FILE is given (but not by book).
  Diagnostics, from check on stdout and from psalm, point and book on stderr, are lines of
  FILE:LINE:COL: SEVERITY: CODE: MESSAGE. FILE is <stdin> for stdin, or a book's piece.
  LINE and COL count from 1, COL in characters (Unicode scalar values; a tab is one), not
  counting a byte-order mark at the start of the file.
  Exit status: 0 on success, 1 when there are errors in the input, 2 for a usage error or
  a file that can't be read or written.";

/// The commands, besides `book`, which parses its own arguments.
const COMMANDS: &[&str] = &["render", "check", "notes", "info", "psalm", "point", "tones"];

/// The largest `--width`: the widest column a layout lays out.
const MAX_WIDTH: f32 = 1.0e6;

/// The largest `--scale`.
const MAX_SCALE: f32 = 1000.0;

/// The flags each command takes, each with whether it takes a value.
fn flags(cmd: &str) -> &'static [(&'static str, bool)] {
    const LAYOUT: &[(&str, bool)] = &[
        ("--width", true),
        ("--scale", true),
        ("--initial", true),
        ("--font", true),
        ("--max-lines", true),
    ];
    match cmd {
        "render" | "notes" => LAYOUT,
        "check" => &[("--initial", true), ("--font", true)],
        "psalm" => &[
            ("--tone", true),
            ("--tone-file", true),
            ("--intone", true),
            ("--name", true),
            ("--no-auto-point", false),
        ],
        "point" => &[("--tone", true), ("--tone-file", true)],
        "book" => &[("-o", true), ("--output", true), ("--svg", true), ("--text-as-paths", false)],
        _ => &[],
    }
}

/// Whether `cmd` takes `flag`.
fn takes(cmd: &str, flag: &str) -> bool {
    flags(cmd).iter().any(|(f, _)| *f == flag)
}

/// Stdout was closed by its reader (`neuma check *.gabc | head`): the output stops quietly.
struct Closed;

/// Writes a line to stdout. A closed pipe returns [`Closed`]; any other write error (a full
/// disk) is reported and exits with status 2.
fn emit(args: std::fmt::Arguments<'_>) -> Result<(), Closed> {
    match writeln!(std::io::stdout(), "{args}") {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Err(Closed),
        Err(e) => {
            eprintln!("neuma: can't write to stdout: {e}");
            std::process::exit(2);
        }
    }
}

/// Prints a line to stdout; see [`emit`].
macro_rules! out {
    ($($arg:tt)*) => {
        emit(format_args!($($arg)*))
    };
}

/// Reports a usage error, with the usage, and returns the exit code for one.
fn usage_error(message: &str) -> ExitCode {
    eprintln!("neuma: {message}\n{USAGE}");
    ExitCode::from(2)
}

/// `neuma --help`.
fn help() -> ExitCode {
    let _ = out!("{USAGE}");
    ExitCode::SUCCESS
}

/// `neuma --version`.
fn version() -> ExitCode {
    let _ = out!("neuma {}", env!("CARGO_PKG_VERSION"));
    ExitCode::SUCCESS
}

/// `--help` or `--version` among `cmd`'s arguments, whichever comes first, skipping the
/// values of its flags and stopping at `--`. Either answers at once, before anything is read.
fn help_or_version(cmd: &str, args: &[String]) -> Option<ExitCode> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--" => break,
            "-h" | "--help" => return Some(help()),
            "--version" => return Some(version()),
            f if flags(cmd).contains(&(f, true)) => {
                it.next();
            }
            _ => {}
        }
    }
    None
}

/// The value after `flag`, or a usage error if there is none.
fn value<'a>(it: &mut std::slice::Iter<'a, String>, flag: &str) -> Result<&'a str, ExitCode> {
    it.next()
        .map(String::as_str)
        .ok_or_else(|| usage_error(&format!("{flag} needs a value")))
}

/// `v` as a number of pixels for `flag`, above 0 and at most `max`, or a usage error.
fn pixels(flag: &str, v: &str, max: f32) -> Result<f32, ExitCode> {
    match v.parse::<f32>() {
        Ok(x) if x > 0.0 && x <= max => Ok(x),
        _ => Err(usage_error(&format!(
            "{flag} takes a number of pixels above 0 and at most {max}, not `{v}`"
        ))),
    }
}

fn main() -> ExitCode {
    let mut args = Vec::new();
    for a in std::env::args_os().skip(1) {
        match a.into_string() {
            Ok(a) => args.push(a),
            Err(a) => return usage_error(&format!("`{}` isn't valid UTF-8", a.to_string_lossy())),
        }
    }
    match args.first().map(String::as_str) {
        None => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Some("-h" | "--help") => help(),
        Some("--version") => version(),
        Some(cmd) if cmd == "book" || COMMANDS.contains(&cmd) => {
            if let Some(code) = help_or_version(cmd, &args[1..]) {
                return code;
            }
            let result = if cmd == "book" {
                book_command(&args[1..])
            } else {
                run(cmd, &args[1..])
            };
            match result {
                Ok(code) | Err(code) => code,
            }
        }
        Some(other) if other.starts_with('-') => usage_error(&format!("unknown option `{other}`; a command comes first")),
        Some(other) => usage_error(&format!("unknown command `{other}`")),
    }
}

/// Every command but `book`.
fn run(cmd: &str, args: &[String]) -> Result<ExitCode, ExitCode> {
    let mut width = 800.0f32;
    let mut layout = LayoutOptions::default();
    let mut options = ChantOptions::default();
    let mut files = Vec::new();
    let mut tone_name: Option<String> = None;
    let mut tone_file: Option<String> = None;
    let mut psalm = neuma_tones::PsalmOptions::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let a = a.as_str();
        if a == "--" {
            files.extend(it.by_ref().cloned());
            break;
        }
        if a.starts_with('-') && a != "-" && !takes(cmd, a) {
            return Err(usage_error(&format!("{cmd}: unknown option `{a}`")));
        }
        match a {
            "--width" => width = pixels(a, value(&mut it, a)?, MAX_WIDTH)?,
            "--scale" => layout = layout.with_scale(pixels(a, value(&mut it, a)?, MAX_SCALE)?),
            "--initial" => {
                let v = value(&mut it, a)?;
                match v.parse::<u8>().ok().filter(|n| *n <= 4) {
                    Some(n) => options = options.with_initial(Initial::from_staves(n.into())),
                    None => return Err(usage_error(&format!("--initial takes a number of staves from 0 to 4, not `{v}`"))),
                }
            }
            "--font" => match value(&mut it, a)? {
                "google" => options = options.with_font(LyricFont::Google),
                "eb-garamond-12" => options = options.with_font(LyricFont::Garamond12),
                v => return Err(usage_error(&format!("--font takes google or eb-garamond-12, not `{v}`"))),
            },
            "--max-lines" => {
                let v = value(&mut it, a)?;
                match v.parse() {
                    Ok(n) => layout = layout.with_max_lines(n),
                    Err(_) => return Err(usage_error(&format!("--max-lines takes a number of lines, 0 for all, not `{v}`"))),
                }
            }
            "--tone" => tone_name = Some(value(&mut it, a)?.to_string()),
            "--tone-file" => tone_file = Some(value(&mut it, a)?.to_string()),
            "--name" => psalm.name = Some(value(&mut it, a)?.to_string()),
            "--no-auto-point" => psalm.auto_point = false,
            "--intone" => match value(&mut it, a)? {
                "first" => psalm.intone = neuma_tones::Intone::FirstVerse,
                "every" => psalm.intone = neuma_tones::Intone::EveryVerse,
                "never" => psalm.intone = neuma_tones::Intone::Never,
                v => return Err(usage_error(&format!("--intone takes first, every or never, not `{v}`"))),
            },
            f => files.push(f.to_string()),
        }
    }
    let many = matches!(cmd, "check" | "info");
    if cmd == "tones" && !files.is_empty() {
        return Err(usage_error("tones takes no arguments"));
    }
    if !many && files.len() > 1 {
        return Err(usage_error(&format!("{cmd} takes one file")));
    }
    if files.is_empty() {
        files.push("-".to_string());
    }
    if cmd == "tones" {
        for t in neuma_tones::Tone::builtin() {
            let line = out!(
                "{:<6} {}  mediant: {}  termination: {}",
                t.name,
                t.clef_gabc(),
                t.mediant,
                t.termination
            );
            if line.is_err() {
                break;
            }
        }
        return Ok(ExitCode::SUCCESS);
    }
    if cmd == "psalm" || cmd == "point" {
        let tone = resolve_tone(tone_name, tone_file)?;
        let src = read(&files[0]).ok_or(ExitCode::from(2))?;
        let name = display_name(&files[0]);
        return Ok(if cmd == "psalm" {
            psalm_command(name, &src, &tone, &psalm)
        } else {
            point_command(name, &src, &tone)
        });
    }
    if cmd == "info" {
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
            if out!("{line}").is_err() {
                break;
            }
        }
        return Ok(status);
    }
    if cmd == "check" {
        return Ok(check_command(&files, &options));
    }
    let src = read(&files[0]).ok_or(ExitCode::from(2))?;
    let chant = Chant::with_options(&src, options);
    if cmd == "render" {
        let _ = out!("{}", chant.layout_with(width, &layout).svg());
    } else {
        // `notes`: the browser package's layout JSON, without the SVG, with the same note ids.
        let placed = chant.layout_with(width, &layout);
        let timeline = placed.timeline();
        let mut json = String::new();
        neuma::json::layout(&mut json, placed.size(), Some(&timeline), Some(chant.utf16()));
        let _ = out!("{json}");
    }
    Ok(ExitCode::SUCCESS)
}

/// The name diagnostics give the file at `path`: `<stdin>` for `-`.
fn display_name(path: &str) -> &str {
    if path == "-" { "<stdin>" } else { path }
}

/// The 1-based line and column of byte `offset` in `src`, the column in characters, not
/// counting a byte-order mark at the start, as editors don't.
fn position(src: &str, offset: usize) -> (usize, usize) {
    let (line, col) = neuma::diag::line_col(src, offset);
    let bom = '\u{feff}'.len_utf8();
    if line == 1 && src.starts_with('\u{feff}') && offset >= bom {
        (line, col - 1)
    } else {
        (line, col)
    }
}

/// `d` as `FILE:LINE:COL: SEVERITY: CODE: MESSAGE`, with its fix, if it has one, on the
/// next line.
fn diagnostic_lines(name: &str, src: &str, d: &Diagnostic) -> String {
    let (line, col) = position(src, d.span.start);
    let severity = match d.severity {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
    };
    let mut s = format!("{name}:{line}:{col}: {severity}: {}: {}", d.code, d.message);
    if let Some(fix) = &d.fix {
        s.push_str("\n    fix: ");
        s.push_str(&fix.title);
    }
    s
}

/// `neuma check`: each file's diagnostics, in order of position. Exits 1 if any file has
/// errors, and 2 if any can't be read.
fn check_command(files: &[String], options: &ChantOptions) -> ExitCode {
    let mut errors = false;
    let mut unreadable = false;
    'files: for path in files {
        let Some(src) = read(path) else {
            unreadable = true;
            continue;
        };
        let chant = Chant::with_options(&src, options.clone());
        let mut diagnostics: Vec<&Diagnostic> = chant.diagnostics().iter().collect();
        diagnostics.sort_by_key(|d| d.span.start);
        for d in diagnostics {
            errors |= d.severity == Severity::Error;
            if out!("{}", diagnostic_lines(display_name(path), &src, d)).is_err() {
                break 'files;
            }
        }
    }
    if unreadable {
        ExitCode::from(2)
    } else if errors {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
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
            return Err(usage_error("this needs --tone or --tone-file"));
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
fn psalm_command(name: &str, src: &str, tone: &neuma_tones::Tone, options: &neuma_tones::PsalmOptions) -> ExitCode {
    let setting = neuma_tones::psalm(src, tone, options);
    let mut errors = false;
    for d in &setting.diagnostics {
        eprintln!("{}", diagnostic_lines(name, src, d));
        errors |= d.severity == Severity::Error;
    }
    let _ = out!("{}", setting.gabc);
    if errors { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// `neuma point`: psalm text with pointing marks added.
fn point_command(name: &str, src: &str, tone: &neuma_tones::Tone) -> ExitCode {
    let p = neuma_tones::point(src, tone);
    let mut errors = false;
    for d in &p.diagnostics {
        eprintln!("{}", diagnostic_lines(name, src, d));
        errors |= d.severity == Severity::Error;
    }
    for h in p.halves.iter().filter(|h| !h.kept && h.confidence < neuma_tones::UNSURE) {
        let part = match h.part {
            neuma_tones::VersePart::Flex => "flex",
            neuma_tones::VersePart::Mediant => "first half",
            neuma_tones::VersePart::Termination => "second half",
        };
        let message = format!("check the {part}: {:.0}% sure", h.confidence * 100.0);
        let d = Diagnostic::new(Severity::Info, h.span.clone(), "point::unsure", message);
        eprintln!("{}", diagnostic_lines(name, src, &d));
    }
    let _ = out!("{}", p.text.trim_end());
    if errors { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// `neuma book`: a `.book` file to a PDF and, optionally, SVG pages.
fn book_command(args: &[String]) -> Result<ExitCode, ExitCode> {
    let mut file = None;
    let mut pdf_out = None;
    let mut svg_dir = None;
    let mut paths = false;
    let mut it = args.iter();
    let mut positional = Vec::new();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--" => {
                positional.extend(it.by_ref());
                break;
            }
            flag @ ("-o" | "--output") => pdf_out = Some(value(&mut it, flag)?.to_string()),
            flag @ "--svg" => svg_dir = Some(value(&mut it, flag)?.to_string()),
            "--text-as-paths" => paths = true,
            f if f.starts_with('-') && f != "-" => return Err(usage_error(&format!("book: unknown option `{f}`"))),
            _ => positional.push(a),
        }
    }
    for f in positional {
        if file.is_some() {
            return Err(usage_error(&format!("book: unexpected `{f}`; it takes one book")));
        }
        file = Some(f.clone());
    }
    let Some(file) = file else {
        return Err(usage_error("book needs a .book file"));
    };
    let stdin = file == "-";
    if stdin && pdf_out.is_none() {
        return Err(usage_error("book needs -o OUT.pdf to read the book from stdin"));
    }
    let name = display_name(&file);
    let src = read(&file).ok_or(ExitCode::from(2))?;
    let path = std::path::Path::new(&file);
    let mut book = neuma_book::Book::parse(&src).map_err(|e| {
        eprintln!("{name}: {e}");
        ExitCode::from(2)
    })?;
    // Pieces resolve from the book's folder; a book from stdin from the current one.
    let base = if stdin {
        std::path::Path::new("")
    } else {
        path.parent().unwrap_or(std::path::Path::new("."))
    };
    // What each piece's diagnostics are named: its file, or the book and the piece's number.
    let labels: Vec<String> = book
        .pieces
        .iter()
        .enumerate()
        .map(|(i, piece)| match piece_source(piece) {
            Some(neuma_book::Source::Path(p)) => base.join(p).display().to_string(),
            _ => format!("{name} (piece {})", i + 1),
        })
        .collect();
    book.resolve(base).map_err(|e| {
        eprintln!("{name}: {e}");
        ExitCode::from(2)
    })?;
    let files = neuma_book::font_files(&book.settings).map_err(|e| {
        eprintln!("neuma: {e}");
        ExitCode::from(2)
    })?;
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
            let text = match piece_source(&book.pieces[p.piece]) {
                Some(neuma_book::Source::Inline(t)) => t.as_str(),
                _ => "",
            };
            eprintln!("{}", diagnostic_lines(&labels[p.piece], text, d));
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
        return Err(ExitCode::from(2));
    }
    if let Some(dir) = svg_dir {
        let dir = std::path::Path::new(&dir);
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("neuma: {}: {e}", dir.display());
            return Err(ExitCode::from(2));
        }
        for i in 0..doc.pages.len() {
            let out = dir.join(format!("page-{:03}.svg", i + 1));
            if let Err(e) = std::fs::write(&out, doc.svg(i, &fonts).unwrap_or_default()) {
                eprintln!("neuma: {}: {e}", out.display());
                return Err(ExitCode::from(2));
            }
        }
    }
    eprintln!("{pdf_out}: {} pages", doc.pages.len());
    Ok(if errors { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

/// The source of a book's score or psalm.
fn piece_source(piece: &neuma_book::Piece) -> Option<&neuma_book::Source> {
    match piece {
        neuma_book::Piece::Score { source, .. } => Some(source),
        neuma_book::Piece::Psalm(ps) => Some(&ps.source),
        _ => None,
    }
}

/// The file at `path`, or stdin for `-`; reports a failure and returns `None`.
fn read(path: &str) -> Option<String> {
    if path == "-" {
        let mut s = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut s) {
            eprintln!("neuma: can't read stdin: {e}");
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

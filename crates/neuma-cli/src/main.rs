//! `neuma render | check | notes | info | psalm | point | tones | book`: the engine from the command line.

use std::io::{Read as _, Write as _};
use std::process::ExitCode;

use neuma::{Chant, ChantOptions, Initial, LayoutOptions, LyricFont, Severity};

const USAGE: &str = "usage: neuma <render|notes> [--width PX] [--scale PX] [--initial LINES] [--font FONT]
                    [--max-lines N] [FILE|-]
       neuma check [the options above] [FILE...]
       neuma info [FILE...]
       neuma psalm --tone TONE [--tone-file FILE] [--intone first|every|never] [--name NAME] [--no-auto-point] [FILE|-]
       neuma point --tone TONE [--tone-file FILE] [FILE|-]
       neuma tones
       neuma book FILE.book [-o OUT.pdf] [--svg DIR] [--text-as-paths]
       neuma --help | --version

  render   write SVG to stdout
  check    print each file's diagnostics as FILE:LINE:COL: SEVERITY: CODE: MESSAGE (stdin is
           <stdin>); exit 1 if any file has errors, 2 if one can't be read
  notes    print the layout and playback timeline as JSON, as the browser package does
  info     print each score's library entry as one line of JSON, with its file name
  psalm    set psalm text (a verse per line, the mediant marked `*`) to a tone and print it
           as GABC. Half-verses without pointing marks are pointed automatically, unless
           --no-auto-point. Problems go to stderr. Pipe it to `neuma render -` to see it.
  point    print the text with pointing marks added for the tone; half-verses the pointer
           is unsure of are listed on stderr
  tones    list the built-in psalm tones
  book     set a booklet (an ordered list of scores, psalms, rubrics and text; see
           crates/neuma-book/README.md) on pages, and write a PDF (-o, default FILE.pdf)
           and, with --svg DIR, one SVG per page. Problems go to stderr.

  With no FILE, render, check, notes, info, psalm and point read stdin.

  --width PX        the column width in pixels, a positive number (default 800)
  --scale PX        pixels per staff space, a positive number (default 6)
  --initial LINES   drop-cap height in staves, 0 to 4; 0 for none (default 1)
  --max-lines N     keep only the first N lines, as broken for the whole score (an incipit);
                    0 keeps them all; a taller initial keeps its full size
  --font FONT       the EB Garamond the lyrics are measured for: google (Google Fonts,
                    the default) or eb-garamond-12";

/// The commands, besides `book`, which parses its own arguments.
const COMMANDS: &[&str] = &["render", "check", "notes", "info", "psalm", "point", "tones"];

/// The flags that take a value, so that a value such as `--name -h` isn't read as a flag.
const VALUE_FLAGS: &[&str] = &[
    "--width",
    "--scale",
    "--initial",
    "--font",
    "--max-lines",
    "--tone",
    "--tone-file",
    "--name",
    "--intone",
];

/// The flags each command takes.
fn takes(cmd: &str, flag: &str) -> bool {
    match cmd {
        "render" | "check" | "notes" => matches!(flag, "--width" | "--scale" | "--initial" | "--font" | "--max-lines"),
        "psalm" => matches!(flag, "--tone" | "--tone-file" | "--intone" | "--name" | "--no-auto-point"),
        "point" => matches!(flag, "--tone" | "--tone-file"),
        _ => false,
    }
}

/// Prints a line to stdout, ignoring a closed pipe (`neuma info *.gabc | head`).
macro_rules! out {
    ($($arg:tt)*) => {
        let _ = writeln!(std::io::stdout(), $($arg)*);
    };
}

/// Reports a usage error, with the usage, and returns the exit code for one.
fn usage_error(message: &str) -> ExitCode {
    eprintln!("neuma: {message}\n{USAGE}");
    ExitCode::from(2)
}

/// `neuma --help`.
fn help() -> ExitCode {
    out!("{USAGE}");
    ExitCode::SUCCESS
}

/// `neuma --version`.
fn version() -> ExitCode {
    out!("neuma {}", env!("CARGO_PKG_VERSION"));
    ExitCode::SUCCESS
}

/// `--help` or `--version` among `args`, whichever comes first, skipping the values of
/// `value_flags`. Either answers at once, before anything is read.
fn help_or_version(args: &[String], value_flags: &[&str]) -> Option<ExitCode> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => return Some(help()),
            "--version" => return Some(version()),
            f if value_flags.contains(&f) => {
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

/// `v` as a positive, finite number of pixels for `flag`, or a usage error.
fn pixels(flag: &str, v: &str) -> Result<f32, ExitCode> {
    match v.parse::<f32>() {
        Ok(x) if x.is_finite() && x > 0.0 => Ok(x),
        _ => Err(usage_error(&format!("{flag} takes a positive number of pixels, not `{v}`"))),
    }
}

fn main() -> ExitCode {
    let mut args = Vec::new();
    for a in std::env::args_os().skip(1) {
        match a.into_string() {
            Ok(a) => args.push(a),
            Err(a) => {
                eprintln!("neuma: `{}` isn't valid UTF-8", a.to_string_lossy());
                return ExitCode::from(2);
            }
        }
    }
    match args.first().map(String::as_str) {
        None => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Some("-h" | "--help") => help(),
        Some("--version") => version(),
        // `book` has flags of its own; the rendering flags below don't apply to it.
        Some("book") => book_command(&args[1..]),
        Some(cmd) if COMMANDS.contains(&cmd) => match run(cmd, &args[1..]) {
            Ok(code) | Err(code) => code,
        },
        Some(other) if other.starts_with('-') => usage_error(&format!("unknown option `{other}`; a command comes first")),
        Some(other) => usage_error(&format!("unknown command `{other}`")),
    }
}

/// Every command but `book`.
fn run(cmd: &str, args: &[String]) -> Result<ExitCode, ExitCode> {
    if let Some(code) = help_or_version(args, VALUE_FLAGS) {
        return Ok(code);
    }
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
        if a.starts_with('-') && a != "-" && !takes(cmd, a) {
            return Err(usage_error(&format!("{cmd}: unknown option `{a}`")));
        }
        match a {
            "--width" => width = pixels(a, value(&mut it, a)?)?,
            "--scale" => layout = layout.with_scale(pixels(a, value(&mut it, a)?)?),
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
            out!(
                "{:<6} {}  mediant: {}  termination: {}",
                t.name,
                t.clef_gabc(),
                t.mediant,
                t.termination
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    if cmd == "psalm" || cmd == "point" {
        let tone = resolve_tone(tone_name, tone_file)?;
        let src = read(&files[0]).ok_or(ExitCode::from(2))?;
        return Ok(if cmd == "psalm" {
            psalm_command(&src, &tone, &psalm)
        } else {
            point_command(&src, &tone)
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
            out!("{line}");
        }
        return Ok(status);
    }
    if cmd == "check" {
        return Ok(check_command(&files, &options));
    }
    let src = read(&files[0]).ok_or(ExitCode::from(2))?;
    let chant = Chant::with_options(&src, options);
    if cmd == "render" {
        out!("{}", chant.layout_with(width, &layout).svg());
    } else {
        // `notes`: the browser package's layout JSON, without the SVG, with the same note ids.
        let placed = chant.layout_with(width, &layout);
        let timeline = placed.timeline();
        let mut json = String::new();
        neuma::json::layout(&mut json, placed.size(), Some(&timeline), Some(chant.utf16()));
        out!("{json}");
    }
    Ok(ExitCode::SUCCESS)
}

/// `neuma check`: each file's diagnostics, a line each, as `path:line:col: severity: code:
/// message`. Exits 1 if any file has errors, and 2 if any can't be read.
fn check_command(files: &[String], options: &ChantOptions) -> ExitCode {
    let mut errors = false;
    let mut unreadable = false;
    for path in files {
        let Some(src) = read(path) else {
            unreadable = true;
            continue;
        };
        let name = if path == "-" { "<stdin>" } else { path.as_str() };
        let chant = Chant::with_options(&src, options.clone());
        for d in chant.diagnostics() {
            let (line, col) = neuma::diag::line_col(&src, d.span.start);
            let severity = match d.severity {
                Severity::Info => "info",
                Severity::Warning => "warning",
                Severity::Error => "error",
            };
            out!("{name}:{line}:{col}: {severity}: {}: {}", d.code, d.message);
            if let Some(fix) = &d.fix {
                out!("    fix: {}", fix.title);
            }
            errors |= d.severity == Severity::Error;
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
    if let Some(code) = help_or_version(args, &["-o", "--output", "--svg"]) {
        return code;
    }
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            flag @ ("-o" | "--output" | "--svg") => match value(&mut it, flag) {
                Ok(v) if flag == "--svg" => svg_dir = Some(v.to_string()),
                Ok(v) => pdf_out = Some(v.to_string()),
                Err(code) => return code,
            },
            "--text-as-paths" => paths = true,
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

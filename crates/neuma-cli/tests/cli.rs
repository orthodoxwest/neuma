//! The `neuma` command's arguments: help, unknown commands and flags, numeric flags, and
//! `neuma check` over several files.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn neuma() -> Command {
    Command::new(env!("CARGO_BIN_EXE_neuma"))
}

/// Runs `neuma args` with `stdin` as its input (closed after writing). A command that exits
/// without reading it, as a usage error does, may close the pipe first; that is no failure.
fn run(args: &[&str], stdin: &str) -> Output {
    let mut child = neuma()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Err(e) = child.stdin.take().unwrap().write_all(stdin.as_bytes()) {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe, "{e}");
    }
    child.wait_with_output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("neuma-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn golden(name: &str) -> String {
    format!("{}/../neuma/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Help and the version answer at once, with stdin left open and never written to: a command
/// that read stdin first would wait for it forever.
#[test]
fn help_and_version_never_read_stdin() {
    let cases: &[&[&str]] = &[
        &["--help"],
        &["-h"],
        &["--version"],
        &["render", "--help"],
        &["check", "-h"],
        &["notes", "--version"],
        &["info", "--help"],
        &["psalm", "--help"],
        &["point", "--version"],
        &["tones", "--help"],
        &["book", "--help"],
        &["book", "--version"],
        // Help wins over a bad flag before it.
        &["render", "--width", "abc", "--help"],
    ];
    for args in cases {
        let mut child = neuma()
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let _stdin = child.stdin.take(); // held open until the end of this iteration
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if start.elapsed() > Duration::from_secs(20) {
                child.kill().ok();
                panic!("neuma {args:?} waited for stdin");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "neuma {args:?}: {}", stderr(&out));
        let text = stdout(&out);
        if args.contains(&"--version") {
            assert_eq!(text, format!("neuma {}\n", env!("CARGO_PKG_VERSION")), "neuma {args:?}");
        } else {
            assert!(text.starts_with("usage: neuma"), "neuma {args:?}: {text}");
        }
    }
}

#[test]
fn unknown_commands_and_flags_are_usage_errors() {
    let cases: &[(&[&str], &str)] = &[
        (&["frobnicate"], "unknown command `frobnicate`"),
        (&["--bogus"], "unknown option `--bogus`"),
        (&["render", "--bogus"], "render: unknown option `--bogus`"),
        (&["check", "-x"], "check: unknown option `-x`"),
        // Flags that belong to another command.
        (&["info", "--width", "500"], "info: unknown option `--width`"),
        (&["render", "--tone", "8.G"], "render: unknown option `--tone`"),
        (&["point", "--intone", "every"], "point: unknown option `--intone`"),
        (&["tones", "extra"], "tones takes no arguments"),
        (&["render", "a.gabc", "b.gabc"], "render takes one file"),
        (&["psalm", "--tone", "8.G", "a.txt", "b.txt"], "psalm takes one file"),
    ];
    for (args, message) in cases {
        let out = run(args, "");
        assert_eq!(out.status.code(), Some(2), "neuma {args:?}");
        let err = stderr(&out);
        assert!(err.contains(message), "neuma {args:?}: {err}");
        assert!(err.contains("usage: neuma"), "neuma {args:?}: {err}");
        assert!(out.stdout.is_empty(), "neuma {args:?}");
    }
}

#[test]
fn numeric_flags_are_validated() {
    let cases: &[&[&str]] = &[
        &["render", "--width", "abc"],
        &["render", "--width", "-5"],
        &["render", "--width", "0"],
        &["render", "--width", "NaN"],
        &["render", "--width", "inf"],
        &["notes", "--scale", "0"],
        &["notes", "--scale", "-2"],
        &["notes", "--scale", "nan"],
        &["render", "--max-lines", "-1"],
        &["render", "--max-lines", "two"],
        &["render", "--initial", "5"],
        &["render", "--initial", "-1"],
        &["check", "--width", "wide"],
    ];
    let gabc = "(c4) a(f)";
    for args in cases {
        let out = run(args, gabc);
        assert_eq!(out.status.code(), Some(2), "neuma {args:?}");
        let err = stderr(&out);
        let (flag, v) = (args[1], args[2]);
        assert!(
            err.contains(&format!("neuma: {flag} takes")) && err.contains(&format!("`{v}`")),
            "neuma {args:?}: {err}"
        );
        assert!(out.stdout.is_empty(), "neuma {args:?}");
    }
}

#[test]
fn flags_without_a_value_are_usage_errors() {
    for flag in ["--width", "--scale", "--initial", "--font", "--max-lines"] {
        let out = run(&["render", flag], "(c4) a(f)");
        assert_eq!(out.status.code(), Some(2), "{flag}");
        assert!(
            stderr(&out).contains(&format!("neuma: {flag} needs a value")),
            "{flag}: {}",
            stderr(&out)
        );
    }
    for flag in ["--tone", "--tone-file", "--name", "--intone"] {
        let out = run(&["psalm", flag], "");
        assert_eq!(out.status.code(), Some(2), "{flag}");
        assert!(
            stderr(&out).contains(&format!("neuma: {flag} needs a value")),
            "{flag}: {}",
            stderr(&out)
        );
    }
    for flag in ["-o", "--svg"] {
        let out = run(&["book", "a.book", flag], "");
        assert_eq!(out.status.code(), Some(2), "{flag}");
        assert!(
            stderr(&out).contains(&format!("neuma: {flag} needs a value")),
            "{flag}: {}",
            stderr(&out)
        );
    }
}

/// Valid flags still take effect, from a file or from stdin.
#[test]
fn valid_flags_render() {
    let file = golden("adoro-te.gabc");
    let src = std::fs::read_to_string(&file).unwrap();
    let narrow = run(
        &[
            "render",
            "--width",
            "400",
            "--scale",
            "8",
            "--initial",
            "2",
            "--max-lines",
            "1",
            &file,
        ],
        "",
    );
    assert!(narrow.status.success(), "{}", stderr(&narrow));
    assert!(stdout(&narrow).starts_with("<svg"));
    let piped = run(
        &[
            "render",
            "--width",
            "400",
            "--scale",
            "8",
            "--initial",
            "2",
            "--max-lines",
            "1",
            "-",
        ],
        &src,
    );
    assert_eq!(narrow.stdout, piped.stdout);
    let default = run(&["render"], &src);
    assert!(default.status.success());
    assert_ne!(narrow.stdout, default.stdout);
}

#[test]
fn check_takes_several_files() {
    let dir = scratch("check");
    let warn = dir.join("warn.gabc");
    let bad = dir.join("bad.gabc");
    let clean = dir.join("clean.gabc");
    std::fs::write(&warn, "name: A;\n%%\n(c4) Al-(f)le(gf)lú(gh)ia.(g.) (::)\n").unwrap();
    std::fs::write(&bad, "(c4) a(f\n").unwrap();
    std::fs::write(&clean, "(c4) a(f) (::)\n").unwrap();
    let (warn, bad, clean) = (warn.to_str().unwrap(), bad.to_str().unwrap(), clean.to_str().unwrap());

    let out = run(&["check", warn, clean], "");
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stdout(&out),
        format!(
            "{warn}:3:8: warning: gabc::hyphen-in-syllable: a hyphen at the end of a syllable prints in addition to the hyphen the engine draws; remove it\n    fix: Remove the hyphen\n"
        )
    );

    let out = run(&["check", warn, bad, clean], "");
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains(&format!("{warn}:3:8: warning: gabc::hyphen-in-syllable: ")), "{text}");
    assert!(text.contains(&format!("{bad}:1:7: error: gabc::unclosed-notes: ")), "{text}");
    assert!(text.find(warn).unwrap() < text.find(bad).unwrap(), "in the order given: {text}");

    // A file that can't be read is reported, the others are still checked, and the exit is 2.
    let missing = dir.join("missing.gabc");
    let missing = missing.to_str().unwrap();
    let out = run(&["check", missing, bad], "");
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains(missing));
    assert!(stdout(&out).contains(&format!("{bad}:1:7: error: ")));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn check_reads_stdin_without_files() {
    let out = run(&["check"], "(c4) a(f\n");
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stdout(&out).starts_with("<stdin>:1:7: error: gabc::unclosed-notes: "),
        "{}",
        stdout(&out)
    );
    let out = run(&["check", "-"], "(c4) a(f) (::)\n");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stdout(&out), "");
}

/// An argument that isn't UTF-8 is an error, not a panic.
#[cfg(unix)]
#[test]
fn non_utf8_arguments_are_errors() {
    use std::os::unix::ffi::OsStrExt as _;
    let out = neuma()
        .arg("render")
        .arg(std::ffi::OsStr::from_bytes(b"\xff.gabc"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = stderr(&out);
    assert!(err.contains("isn't valid UTF-8") && !err.contains("panicked"), "{err}");
}

//! `neuma book` from the command line.

use std::process::Command;

fn neuma() -> Command {
    Command::new(env!("CARGO_BIN_EXE_neuma"))
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("neuma-cli-book-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn writes_a_pdf_and_svg_pages() {
    let dir = scratch("ok");
    let book = dir.join("a.book");
    std::fs::write(&book, "title: Test\ntext: Hello.\nbreak\ntext: Again.\n").unwrap();
    let out = neuma()
        .args(["book", book.to_str().unwrap(), "-o"])
        .arg(dir.join("a.pdf"))
        .arg("--svg")
        .arg(dir.join("svg"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(std::fs::read(dir.join("a.pdf")).unwrap().starts_with(b"%PDF"));
    assert!(dir.join("svg/page-002.svg").is_file());
    std::fs::remove_dir_all(&dir).ok();
}

/// The rendering flags belong to render, check and notes; `book` reports them as unknown
/// options instead of the general flag parser taking them (or `-h` printing the
/// general usage and exiting before the book is read).
#[test]
fn book_arguments_are_its_own() {
    let dir = scratch("flags");
    let book = dir.join("a.book");
    std::fs::write(&book, "text: Hello.\n").unwrap();
    let out = neuma().args(["book", book.to_str().unwrap(), "--initial", "9"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("book: unknown option `--initial`"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

/// A piece's diagnostics are named by its file, at a line and column in it, as `check` does.
#[test]
fn diagnostics_name_the_piece() {
    let dir = scratch("diag");
    std::fs::write(dir.join("bad.gabc"), "(c4) a(f\n").unwrap();
    let book = dir.join("a.book");
    std::fs::write(&book, "score: bad.gabc\n").unwrap();
    let out = neuma()
        .args(["book", book.to_str().unwrap(), "-o"])
        .arg(dir.join("a.pdf"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    let expected = format!("{}:1:7: error: gabc::unclosed-notes: ", dir.join("bad.gabc").display());
    assert!(err.contains(&expected), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

/// `-` reads the book from stdin, its files from the current folder; it needs `-o`.
#[test]
fn reads_a_book_from_stdin() {
    use std::io::Write as _;
    let dir = scratch("stdin");
    let out = neuma()
        .current_dir(&dir)
        .args(["book", "-"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("needs -o"));
    std::fs::write(dir.join("a.gabc"), "(c4) a(f) (::)\n").unwrap();
    let mut child = neuma()
        .current_dir(&dir)
        .args(["book", "-", "-o", "out.pdf"])
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"text: Hello.\nscore: a.gabc\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(std::fs::read(dir.join("out.pdf")).unwrap().starts_with(b"%PDF"));
    std::fs::remove_dir_all(&dir).ok();
}

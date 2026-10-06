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

/// The rendering flags belong to render, check and notes; `book` reports them as its own
/// unknown arguments instead of the general flag parser taking them (or `-h` printing the
/// general usage and exiting before the book is read).
#[test]
fn book_arguments_are_its_own() {
    let dir = scratch("flags");
    let book = dir.join("a.book");
    std::fs::write(&book, "text: Hello.\n").unwrap();
    let out = neuma().args(["book", book.to_str().unwrap(), "--initial", "9"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("book: unexpected `--initial`"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

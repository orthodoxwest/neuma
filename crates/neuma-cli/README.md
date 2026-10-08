# neuma-cli

The `neuma` command, for the [neuma](https://github.com/orthodoxwest/neuma) chant engine: it
engraves [GABC](https://gregorio-project.github.io/gabc/) as SVG, checks scores, prints their
timelines and library entries, sets English psalm text to the psalm tones, and typesets
booklets as PDF.

```sh
cargo install neuma-cli

neuma render --width 720 score.gabc > score.svg
neuma check *.gabc                # diagnostics, with fixes; exits 1 on errors
```

```text
neuma render [--width PX] [--scale PX] [--initial LINES] [--font FONT] [--max-lines N] FILE|-
neuma notes FILE                    the layout and timeline as JSON
neuma check FILE...                 diagnostics, with fixes; exits 1 on errors
neuma info FILE...                  one library entry per file, as JSON lines
neuma tones                         the built-in psalm tones
neuma point --tone TONE FILE        psalm text with pointing marks added
neuma psalm --tone TONE FILE        psalm text set to a tone, as GABC
neuma book FILE.book [-o OUT.pdf] [--svg DIR] [--text-as-paths]
neuma --help | --version
```

A file named `-` is stdin, and without a file every command but `tones` and `book` reads
stdin, so commands chain: `neuma psalm --tone 8.G psalm.txt | neuma render - > psalm.svg`.
`neuma COMMAND --help` lists a command's options.

Diagnostics come one to a line as `FILE:LINE:COL: SEVERITY: CODE: MESSAGE`, with the fix, if
there is one, on the next line:

```console
$ neuma check alleluia.gabc
alleluia.gabc:3:8: warning: gabc::hyphen-in-syllable: a hyphen at the end of a syllable prints in addition to the hyphen the engine draws; remove it
    fix: Remove the hyphen
```

The exit status is 1 when the input has errors and 2 for a usage error or a file that can't be
read or written. Every diagnostic code is listed in
[docs/diagnostics.md](https://github.com/orthodoxwest/neuma/blob/main/docs/diagnostics.md), and the `.book` format in
[neuma-book's README](https://crates.io/crates/neuma-book).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/orthodoxwest/neuma/blob/main/LICENSE-APACHE))
- MIT license ([LICENSE-MIT](https://github.com/orthodoxwest/neuma/blob/main/LICENSE-MIT))

at your option. [NOTICE](https://github.com/orthodoxwest/neuma/blob/main/NOTICE) credits the work neuma builds on, with its licenses.

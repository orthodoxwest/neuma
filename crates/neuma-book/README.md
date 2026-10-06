# neuma-book

Print booklets: an ordered list of pieces (chant scores, psalms set to a tone, rubrics and
text) set on fixed-size pages, as a PDF and as one SVG per page. It is meant for the chant of
one office on one day, with no hand assembly in a word processor.

```sh
neuma book examples/compline/compline.book -o compline.pdf --svg pages/
```

`-o` names the PDF (by default the book's name with `.pdf`), `--svg DIR` also writes
`page-001.svg` … , and `--text-as-paths` draws all text as outlines. Problems in the pieces
(GABC diagnostics, unknown tones, pointing the automatic pointer is unsure of) go to stderr;
the command fails only on errors.

From Rust:

```rust,no_run
use neuma_book::{Book, Fonts, font_files, typeset};

let mut book = Book::parse(&std::fs::read_to_string("compline.book")?)?;
book.resolve(std::path::Path::new("."))?;
let files = font_files(&book.settings)?;
let fonts = Fonts::new(&files);
let doc = typeset(&book, &fonts);
std::fs::write("compline.pdf", doc.pdf(&fonts))?;
let first_page_svg = doc.svg(0, &fonts);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## The `.book` format

A plain text file, one entry per line: `name: value`, or `name options: value` for pieces
that take options. Lines indented under an entry continue its value. `#` starts a comment
line. File names are relative to the book.

```text
page: a5
header: Compline

title: Compline
rubric: The Reader begins.
text:
    Brethren, be sober, be vigilant; because your adversary the devil,
    as a roaring lion, walketh about, seeking whom he may devour.
text lines:
    V/ O God, make speed to save us.
    R/ O Lord, make haste to help us.
heading: The Psalms
score: have-mercy.gabc
psalm tone=8.G gloria: psalm-4.txt
score: have-mercy.gabc
break
```

### Settings

Settings come before the first piece. Lengths take `pt` (the default), `mm`, `cm`, `in` or
`pc`.

| Setting | Default | Meaning |
|---|---|---|
| `page` | `a5` | `a4`, `a5`, `a6`, `b5`, `letter`, `half-letter`, `legal`, any of them followed by `landscape`, or a width and a height (`150mm 210mm`) |
| `margins` | `16mm 14mm 18mm 14mm` | One to four lengths, as in CSS: top, right, bottom, left |
| `mirror` | `no` | `yes` swaps the left and right margins on even pages, for printing both sides |
| `font` | EB Garamond 12, if installed | The text face's regular TrueType or OpenType file |
| `font-italic`, `font-bold`, `font-bold-italic` | | The other faces. A missing face falls back to the nearest one given |
| `text-size` | `11pt` | Body text, and lyrics unless `lyric-size` says otherwise |
| `lyric-size` | the text size | Lyrics under the notes |
| `staff-size` | `6.5mm` | Height of the staff, top line to bottom line |
| `leading` | `1.3` | Line spacing, as a multiple of the text size |
| `initial` | `1` | Drop-cap height for scores, in staves, `0` to `4` |
| `psalms` | `pointed` | How psalms are set: `pointed`, `first` or `chant` (below) |
| `page-numbers` | `bottom` | `bottom` (centred), `outer` (the outside top corner) or `none` |
| `text-as-paths` | `no` | `yes` draws text as outlines, so the files look the same without the font |
| `red` | `#a3211c` | The colour of rubrics and marks |

### Pieces

| Piece | Meaning |
|---|---|
| `title: TEXT` | A large centred title. It also becomes the running header, and its page has none. |
| `heading: TEXT` | A centred red heading. It stays on the page with what follows. |
| `rubric: TEXT` | Red italic directions. They stay with what follows. |
| `text: TEXT` | Justified paragraphs; a blank line between indented lines starts a new one. Options: `dropcap` (a two-line initial), `center`, and `lines` (keep the line breaks, as for verse or versicles; the lines stay together). |
| `score: FILE.gabc` | A chant score, from a file or from GABC on the indented lines below. Option `initial=N` sets its drop cap. |
| `psalm tone=NAME: FILE` | A psalm, one verse per line with the mediant marked `*` (the format `neuma psalm` reads), from a file or the indented lines below. |
| `header: TEXT` | The running header from here on: on this page if nothing is on it yet, else from the next. |
| `space: LENGTH` | Vertical space. |
| `break` | A new page. |

Text pieces take a little markup: `<i>…</i>`, `<b>…</b>`, `<sc>…</sc>` (small capitals),
`<r>…</r>` or `<c>…</c>` (red), and `V/` and `R/` for the versicle and response signs ℣ and
℟. The marks `*`, `†`, `‡`, `+` and `✠` are always red.

### Psalms

Options:

- `tone=NAME`: a built-in tone (`neuma tones` lists them), or one from `tone-file=FILE`.
- `gloria`: add the Gloria Patri (Book of Common Prayer wording) as two more verses.
- `intone=first|every|never`: when the intonation is sung; `first` by default, `every` for
  the Gospel canticles.
- `set=pointed|first|chant`: overrides the book's `psalms:` setting.

The three settings:

- **`pointed`** (the default): the tone is printed once as a small score labelled with its
  name, and the verses follow as text with their verse numbers, pointed as hand-pointed
  psalters print them: `·` where the cadence starts, an acute on each accented syllable, `*`
  at the mediant and `†` at the flex, with the marks in red. It is compact and the usual way
  a choir sings psalms from a book. Verses without marks are pointed automatically by
  neuma-tones; the ones it is unsure of are reported.
- **`first`**: the first verse as chant, so the choir sees the intonation and both cadences
  with words, then the rest pointed.
- **`chant`**: every verse as chant, one score per verse. Good for short canticles and for
  choirs that don't read pointing. The accents that placed the cadences are not printed under
  the notes.

## Layout and pagination

Each staff line and each line of text is a block. A block can keep with the next one, and a
run of blocks that keep together moves to a new page as a whole when it doesn't fit (a run
taller than a page breaks where it must). The rules:

- A title, heading or rubric stays with what follows.
- An antiphon (a score just before a psalm) stays with the psalm's tone and first verse.
- A score never leaves one staff alone at the top or bottom of a page: its first two lines
  stay together, and so do its last two. A tall drop cap keeps the lines it spans together.
- A paragraph keeps its first two lines and its last two together; a verse of a pointed
  psalm stays whole.

Running headers are centred in red italic in the top margin; page numbers are centred at
the bottom, or at the outside top corner with `page-numbers: outer`.

Lyrics hang a little lower below the staff than in neuma's screen layout (the text face's
ascent is padded by a quarter em), since capitals and ascenders otherwise come close to the
bottom line in print.

## Output

**PDF.** neuma-book writes PDF 1.7 itself. Neume glyphs are vector form objects. Text uses
the text face embedded in the file: a TrueType face is subset (the glyphs the booklet doesn't use
are emptied); an OpenType face with CFF outlines is embedded whole.
Text keeps a ToUnicode map, so it can be searched and copied. With `text-as-paths`, text is
drawn as outlines instead and no font is embedded.

Without any font file (no `font:` and no EB Garamond 12 installed), the PDF uses the
standard Times faces every PDF viewer has, measured only approximately, and the command
warns. Install EB Garamond 12 (`fonts-ebgaramond` on Debian and Ubuntu) or name a font for
real output; any TrueType or OpenType face works.

**SVG.** One file per page, sized in points, with a white background. Text is set in
`<text>` elements naming the font's family, each run at the position it was measured for, so
the font must be installed where the SVG is viewed; with `text-as-paths` the text is
outlines and the SVG needs no font.

The PDF is the file to print. To check pages as images, rasterize it with Poppler:
`pdftoppm -r 150 -png compline.pdf page`. `rsvg-convert` renders the SVG pages, with small
differences in kerning since it shapes text itself.

Pages are written one per sheet in reading order. Imposing them for a folded booklet
(saddle stitching) is left to the printer's driver or a tool such as `pdfjam --booklet true`.

## Dependencies

[rustybuzz](https://github.com/harfbuzz/rustybuzz) (MIT) shapes text, so kerning and small
capitals in the PDF match the widths the layout measured, and its `ttf-parser` reads glyph
outlines. It is already in the workspace through neuma-metrics. Everything else, the `.book`
parser, the PDF writer and the TrueType subsetter, is in this crate.

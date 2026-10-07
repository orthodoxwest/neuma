# Diagnostics

Every problem neuma finds is a `Diagnostic` with a severity, a source span, a code, a
message and, when only one edit makes sense, a fix. Parsing and engraving never fail: the
engine renders what it can and says what it did.

- **Codes are stable.** Tools may match on them, hide them or translate them. A code is
  never reused for a different problem. If a problem goes away (because the engine learns to
  draw something), its code stops appearing, and the code is listed below as retired. New
  problems get new codes.
- **Messages are not stable.** They are English prose for people and may be reworded.
- **Spans** are UTF-8 byte ranges in the source. The bindings also give UTF-16 offsets for
  editors (`from` and `to` in JavaScript, `utf16Start` and `utf16End` on mobile). A span is
  empty only where a problem has no place in the source, such as an empty psalm text.
- **Fixes** replace the span `fix.span` (empty for an insertion) with `fix.replacement`, and
  carry a short `title` for a quick-fix menu. A fix is offered only where there is one
  sensible edit, usually the one that makes the source say what the engine drew. Applying a
  fix removes that diagnostic.
- **Severities:** `error` is input that can't be read as written; `warning` is input that
  is unsupported or looks like a mistake, recovered from; `info` is input accepted and
  ignored or approximated, where the drawing may differ from GregorioTeX's.

## GABC (`gabc::`)

| Code | Severity | Span | Fix | Meaning |
|---|---|---|---|---|
| `gabc::no-separator` | warning | the header-like lines at the top | insert the `%%` line after them | The source looks like it has a header but no `%%` line; everything is read as notes. |
| `gabc::unterminated-header` | warning | the field, name to value | insert `;` after the value | A header value has no closing `;` or `;;`; it ends at the next field or at `%%`. |
| `gabc::bad-header` | warning | the line | | A header line has no `:`. It is skipped. |
| `gabc::macro-ignored` | info | the header field | | A `def-m` header defines TeX, which neuma doesn't run. |
| `gabc::legacy-oriscus` | warning | the header field | | `oriscus-orientation: legacy` isn't supported. |
| `gabc::staff-lines` | warning | the header field | | Only four-line staves are drawn. |
| `gabc::nabc` | warning | the `nabc-lines` field, or the first `\|` in the notes | | NABC is skipped. Reported once per score. |
| `gabc::no-clef` | warning | the first note | insert `(c4) ` before the first syllable | Notes come before any clef; they are read in `c4`. |
| `gabc::unclosed-notes` | error | from the `(` to the end | insert `)` at the end | Notes opened with `(` never close. |
| `gabc::trailing-text` | warning | the text | | Text after the last notes has no notes and is dropped. |
| `gabc::hyphen-in-syllable` | warning | the hyphen | remove it | A syllable starts or ends with `-`, which prints in addition to the hyphen the engine draws. |
| `gabc::unclosed-tag` | warning | the opening tag | insert the closing tag at the end of the syllable | A style tag (`<i>`, `<b>`, `<sc>`, `<ul>`, `<c>`, `<e>`) is never closed and styles the rest of the score, or a `<sp>`, `<v>` or `<alt>` has no closer in its syllable and runs to the syllable's end. |
| `gabc::unknown-tag` | warning | the tag | | An unknown `<tag>` is set as text. |
| `gabc::unknown-special` | warning | the text inside `<sp>` | | `<sp>…</sp>` names no known character. |
| `gabc::verbatim-dropped` | warning | the `<v>` element | | Verbatim TeX holds a command neuma doesn't run (spacing such as `\hspace`, or one it doesn't know); the text in and around it is kept. The common style and colour commands (`\textit`, `\textcolor`), accents (`\'a`) and symbols (`\ae`, `\P`) are followed and give no warning. |
| `gabc::translation-ignored` | info | the `[…]` | | Translation text isn't drawn yet. |
| `gabc::above-lines-text` | info | the `<alt>` element | | Above-lines text isn't drawn yet. |
| `gabc::lyric-tie` | info | the `~` | | Lyric ties are drawn as a space. |
| `gabc::teletype` | info | the tag | | Teletype text is set in the lyric face. |
| `gabc::unknown-notation` | warning | the character | | A character that isn't GABC notation is skipped. |
| `gabc::pitch-range` | warning | the note | | A pitch above a four-line staff. |
| `gabc::clef-line` | warning | the clef | | A clef on a line the staff doesn't have; line 3 is used. |
| `gabc::double-clef` | warning | the second clef | | Double clefs aren't supported; the first is used. |
| `gabc::bad-space` | warning | the space | | `/[…]` without a number is a small space. |
| `gabc::bar-sign` | warning | the sign | | Signs on bars are skipped. |
| `gabc::dominican-bar` | warning | the bar | | `;7` and `;8` have no line on a four-line staff; drawn above it. |
| `gabc::fusion` | info | the fusion | | Neume fusion is drawn unfused. |
| `gabc::zero-width` | info | the `{…}` | | Notes in `{…}` take their own width. Reported once per score. |
| `gabc::lined-note` | warning | the note | | Notes surrounded by lines are drawn without the lines. |
| `gabc::linea` | warning | the note | | The linea is drawn as a punctum. |
| `gabc::above-sign` | info | the sign | | Signs above the staff (`r1`–`r8`) aren't drawn yet. |
| `gabc::tuning-ignored` | info | the tag | | A placement fine-tuning tag; the default placement is used. |
| `gabc::unsupported-tag` | warning | the tag | | A `[name:…]` tag that isn't supported is skipped. |

## Engraving (`engrave::`, `text::`)

| Code | Severity | Span | Fix | Meaning |
|---|---|---|---|---|
| `engrave::vowel-rules` | info | the `language` header field | | No vowel rules for the language; the Latin rules center the lyrics. |
| `engrave::wide-porrectus` | info | the two notes | | No porrectus glyph spans more than a fifth; drawn as two puncta. |
| `text::synthetic-face` | info | the first such syllable | | No font face for a style (such as bold); measured with the regular face, widened. Reported once per score. |

## Psalm tones (`neuma-tones`: `pointed::`, `apply::`, `point::`)

Spans count bytes of the psalm text.

| Code | Severity | Meaning |
|---|---|---|
| `pointed::two-mediants` | error | A verse has more than one mediant `*`. |
| `pointed::late-flex` | error | The flex `†` comes after the mediant. |
| `pointed::two-flexes` | error | A verse has more than one flex. |
| `pointed::open-rubric` | error | A rubric `[` has no `]`. |
| `pointed::empty-part` | error | Nothing to sing before a mark or after the mediant. |
| `pointed::no-mediant` | warning | A verse without a mediant is sung as one half-verse. |
| `pointed::two-cadence-marks` | warning | A half-verse has more than one `·`; the cadence starts at the last. |
| `pointed::dangling-mark` | warning | A `·` has no syllable after it in its half-verse. |
| `pointed::empty-syllable` | warning | A mark has no syllable after it in its word. |
| `apply::empty` | warning | There is no verse to sing (empty span). |
| `apply::no-accent` | warning | No accented syllable is marked in a cadence; the last syllables take it. |
| `apply::missing-accent` | warning | Fewer accents are marked than the tone has. |
| `apply::few-preparatory` | warning | Fewer preparatory syllables are marked than the tone has. |
| `apply::needless-hold` | warning | A `–` hold where syllables come between the accents. |
| `apply::no-cadence-mark` | info | No `·` in a half-verse; its cadence is counted from the accents. |
| `apply::extra-accents` | info | More accents are marked than the tone uses; the last are used. |
| `apply::short-intonation` | info | The half-verse is too short for the whole intonation. |
| `apply::extra-preparatory` | info | Syllables before the preparatory notes stay on the tenor. |
| `apply::implied-hold` | info | Two accents with nothing between: the first is held. |
| `point::unsure` | info | The automatic pointing of a half-verse is unsure; check it. |

## Retired codes

- `engrave::final-break`: was to say a line break at the end of the score is dropped, but
  no score could reach it; a break written last applies to the segment before it, as anywhere.

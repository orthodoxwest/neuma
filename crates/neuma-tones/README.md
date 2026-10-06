# neuma-tones

Sets pointed English psalm text to a psalm tone and returns the chant as a neuma `Score` and
GABC, with each note's place in the tone (intonation, tenor, preparatory, accent or ending)
for practice tools.

```rust
use neuma_tones::{Options, Tone, apply_text};

let tone = Tone::named("8.G").unwrap();
let setting = apply_text(tone, "1 The Lord is King, and hath put on glorious ap·pá-rel; * \
    the Lord hath put on his apparel, and gird·ed him-sélf with strength.", &Options::default());
// setting.gabc:    "(c4) The(g) Lord(h) is(j) King,(j) … ap(j)pá(k)rel;(j) *(:) …"
// setting.notes[i] describes note i of the engraved score.
```

From the command line: `neuma psalm --tone 8.G psalm.txt | neuma render - > psalm.svg`.

## Pointed text

One verse per line, in the marks a hand-pointed psalter prints:

| Mark | Meaning |
|---|---|
| `*` | The mediant: the end of the first half-verse. |
| `†` | The flex, before the mediant in a long first half. |
| `·` | The cadence starts at the next syllable. Inside a word it also splits it (`e·ver`). |
| acute (`á`) | An accented syllable: it takes one of the cadence's accents. |
| `–` (en dash) | A cadence note with no syllable of its own: the syllable before is held (`thou · árt – mý God`, `re·member Dávid, – – *`). Before a half's first syllable it leaves a note out instead (`* – · – – práise the Lord`). |
| `-` in a word | A sung syllable split (`judg-ed`). `\-` is a hyphen that is only spelling. |
| `[…]` | A rubric such as a posture cue: kept, never sung. |
| a leading number | The verse number. |

The `·` and accents place the cadence; everything before it is sung on the tenor, after the
intonation on the first verse. Unmarked words are split into sung syllables by an English
syllabifier ("e-ver", "peo-ple", "wick-ed-ness"); mark a split with `-` where a psalter sings
one the spelling hides ("bless-ed").

Problems are diagnostics with spans in the pointed text, such as a cadence with fewer
preparatory syllables than the tone, or a two-accent cadence with one accent marked.

## Tones

`Tone::builtin()` holds the Solesmes tones with their usual endings (`1.D` … `8.c` and the
tonus peregrinus), from the Liber Usualis. `Tone::named` also takes `VIII.G`, `8g` or
`tone 8 G`. A tone of your own is a short block:

```text
name: 8.G
clef: c4
mediant: g h jr 'k jr j
termination: jr i j 'h gr g
flex: jr 'j hr h
```

Each formula lists one GABC neume per syllable. The first neume ending `r` is the tenor;
neumes before it are the intonation, and the fixed neumes after it, up to the first accent,
take the preparatory syllables. A neume after `'` takes an accented syllable. A neume ending
`r` takes any number of unaccented syllables, and the neumes after the last accent take the
last syllables. When the accent is the last syllable, its ending is sung on it (`súre(kj)`).
Without a `flex:` line, the flex drops a step from the tenor, or a minor third from do or fa.

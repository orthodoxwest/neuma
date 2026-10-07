# Golden snapshots

`tests/golden.rs` engraves each reference score with the built-in metrics, lays it out at
360 and 720 units wide, and compares its display list (one line per glyph, rectangle and
text run, to two decimal places) with `snapshots/<name>.txt`.

The reference scores are the `.gabc` files here, the ones in `../corpus`, and the Compline
example in `examples/compline`. The ones here were written for these tests and are under
the project's license:

- Plainchant settings of public-domain texts and melodies (antiphons, hymns, Kyriale
  pieces, versicles and a psalm tone). They were transcribed for testing and not checked
  against a printed edition, so don't use them as performing editions.
- Feature exercises, with made-up text: `neume-shapes`, `special-notes`, `markings`,
  `text-styles`, `bars-and-spaces`, `clef-changes`, `fa-clef-flats`, `long-melisma`,
  `annotations-initial` and `edge-cases`.

When an engraving change is intended, rewrite the snapshots and review the diff:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p neuma --test golden
git diff crates/neuma/tests/golden/snapshots
```

To add a reference score, put its `.gabc` file here and run the same command.

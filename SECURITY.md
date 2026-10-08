# Security

## Reporting a problem

Report it privately: on the repository's **Security** tab choose **Report a vulnerability**
([direct link](https://github.com/orthodoxwest/neuma/security/advisories/new)). Please don't
open a public issue or pull request for it first. If the form isn't available, open an issue
that asks for a private contact and leaves out the details.

A useful report has:

- the input that triggers it (GABC, psalm text, a booklet file, or the options), shrunk if
  you can;
- where it runs: the Rust crates, the browser module, the mobile bindings or the `neuma`
  command, and the version or commit;
- what happens: the panic message, how long it runs, how much memory it takes.

We will confirm the problem, fix it on `main` and credit you in the advisory unless you would
rather not be named. neuma is at 0.1 and has no maintained release branches: fixes go into the
next release.

## Scope

neuma is built to read untrusted input. Apps and websites hand it GABC that users typed or
pasted or that came from a public collection, so problems that input can cause are security
problems. In scope, for any input through any public entry point (the `neuma`, `neuma-tones`
and `neuma-book` APIs, the browser module, the mobile bindings and the command line):

- **A panic.** The parser and engraver report bad input as diagnostics and never panic. In the
  browser module a panic stops the engine, and every call throws until the page starts a new
  one with `init()`.
- **A hang**, or time that grows much faster than the input: a short file that takes seconds.
- **A blowup**: memory, layout or output (SVG, JSON, PDF) far out of proportion to the input.
- **Escaping the output's context**: text from the input that ends up as markup or attributes
  in the SVG, or breaks the JSON or PDF, rather than appearing as text.
- **Memory unsafety.** Apart from the browser module's exports, the crates have no `unsafe`
  code of their own: the compiler forbids it in the other crates, and denies it in the browser
  module and in the mobile bindings, whose FFI code UniFFI generates. A way to misuse the
  exports or the generated bindings, from Rust, JavaScript, Swift or Kotlin, is in scope.

Out of scope: engraving that is wrong but harmless (open an ordinary issue), output that is
large because the input is (a whole antiphonary in one file), the fonts or pages that an app
pairs with neuma, and problems in dependencies that neuma's use of them doesn't cause (report
those upstream).

The parser, layout and GABC round trip are fuzzed, and a nightly run puts every score in
GregoBase that isn't marked as under copyright through the whole pipeline with time limits;
[CONTRIBUTING.md](CONTRIBUTING.md) describes both. Adding the input that found a bug to those
tests is part of every fix.

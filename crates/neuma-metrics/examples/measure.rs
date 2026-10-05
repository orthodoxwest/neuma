//! Prints the table's advance for each argument, in ems: `measure TABLE [--italic] WORD...`.
use neuma::TextMeasure;
use neuma::score::TextStyle;
fn main() {
    let mut args = std::env::args().skip(1);
    let table = neuma::MetricsTable::from_bytes(&std::fs::read(args.next().unwrap()).unwrap()).unwrap();
    let mut style = TextStyle::REGULAR;
    for w in args {
        if w == "--italic" {
            style.italic = true;
            continue;
        }
        println!("{w}\t{:.4}", table.advance(&w, style));
    }
}

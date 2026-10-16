//! Training data for the pointer's weights, from a hand-pointed psalter: reads lines of
//! `ID<TAB>TONE<TAB>VERSE` (TONE a built-in name, or a tone's definition with its newlines
//! written `\n`) and prints a JSON line per half-verse the pointer would point:
//! `{"id", "chosen", "candidates": [[feature, ...], ...]}`. `scripts/fit_pointing.py` fits the
//! weights from it.
use std::io::{BufRead, Write};

use neuma_tones::{Tone, fit_halves};

fn main() {
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("stdin");
        let mut fields = line.splitn(3, '\t');
        let (Some(id), Some(tone), Some(verse)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        let tone = match Tone::named(tone) {
            Ok(t) => t.clone(),
            Err(_) => Tone::parse(&tone.replace("\\n", "\n")).expect("tone"),
        };
        for half in fit_halves(verse, &tone) {
            let candidates: Vec<String> = half
                .candidates
                .iter()
                .map(|c| format!("[{}]", c.iter().map(|f| json(f)).collect::<Vec<_>>().join(",")))
                .collect();
            let chosen = half.chosen.map_or("null".to_string(), |c| c.to_string());
            writeln!(
                out,
                "{{\"id\":{},\"chosen\":{chosen},\"candidates\":[{}]}}",
                json(id),
                candidates.join(",")
            )
            .expect("stdout");
        }
    }
}

fn json(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

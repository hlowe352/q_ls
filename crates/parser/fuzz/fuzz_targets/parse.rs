#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz the parser. Two invariants:
// 1. `parse` never panics on any input.
// 2. The CST is lossless: concatenating all token text equals the input.
fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else { return };
    let parse = q_parser::parse(input);
    let roundtrip = parse.syntax().text().to_string();
    assert_eq!(roundtrip, input, "lossless invariant violated");
});

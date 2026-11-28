#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz the server: building a document and running every editor feature at
// every token must never panic, and positions must round-trip.
fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        q_ls::exercise(text);
    }
});

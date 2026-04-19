//! Test helper: a tiny Brain stub that prints the contents of the file
//! named in its single CLI argument to stdout, then exits 0.
//!
//! The contract test (`tests/contract.rs`) uses this as the subprocess
//! transport target. We ship it as an `examples/` binary so the test can
//! invoke it cross-platform via `cargo run --example` — no Windows/Unix
//! shell gymnastics, no tempfile path quoting foot-guns.
//!
//! Why an `example` and not a `[[bin]]` target? Examples don't ship in
//! release artifacts, which keeps `cargo install` clean. We only need
//! this during testing.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("stub_child_brain: missing path argument");
            return ExitCode::from(2);
        }
    };
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("stub_child_brain: could not read {path:?}: {e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = io::stdout().write_all(&bytes) {
        eprintln!("stub_child_brain: write failed: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

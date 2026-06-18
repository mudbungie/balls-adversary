//! The `adversary` plugin binary: a thin shell over [`adversary::run`].
//!
//! All logic lives in the library (unit-tested); `main` only adapts the
//! process boundary — reads argv and exits with the library's code. The
//! integration test (`tests/cli.rs`) exercises this built binary so tarpaulin
//! sees the shell covered.

use std::env;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    exit(adversary::run(&args));
}

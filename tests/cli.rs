//! Exercise the built `adversary` binary at the process boundary — covering the
//! `main` shell that unit tests can't reach. tarpaulin counts `src/` only, so
//! this file is coverage-neutral; running the binary registers the shell's
//! coverage.

use std::process::Command;

#[test]
fn protocol_subcommand_prints_self_description_and_exits_zero() {
    let out = Command::new(env!("CARGO_BIN_EXE_adversary"))
        .arg("protocol")
        .output()
        .expect("run the adversary binary");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("close"));
}

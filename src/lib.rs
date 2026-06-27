//! The `adversary` plugin: a balls `close.pre` review gate.
//!
//! balls' plugin protocol IS a gate protocol — core spawns `<bin> <op> <phase>`
//! and a non-zero exit aborts the op and rolls prior plugins back in reverse
//! (`docs/architecture.md` §6 in the balls repo). This plugin, wired into
//! `close.pre` (prepended, before `bl-delivery` squashes), asks a single-pass
//! non-interactive `claude` to review the task's change against its ball and
//! exits 0 (pass) or non-zero (fail → close aborts, task stays claimed).
//!
//! The pure gate logic lives in [`review`] (fake-driven, unit-tested); the two
//! real subprocess spawns live in [`spawn`] (covered by `tests/cli.rs` running
//! the built binary). `run` only routes the one op-phase the gate acts on —
//! `close pre` — and ABSTAINS (exits 0) on every other invocation, so wiring the
//! plugin in never blocks an op it has no opinion about. See `docs/design.md`.

mod config;
mod review;
mod spawn;
mod wire;

use std::io::Read;
use wire::Wire;

/// The plugin's protocol self-description, emitted on `<bin> protocol`
/// (balls §6). balls never persists it; it is read once to validate wiring.
pub const PROTOCOL: &str = r#"{"protocol":1,"ops":["close"]}"#;

/// Dispatch one balls plugin invocation. `args` is argv minus the binary name;
/// the return value is the process exit code: `0` = ok (the gate passes / the op
/// proceeds), non-zero = abort. Only `close pre` runs the gate; every other
/// op-phase abstains (the capability is one routed arm, not a flipped default).
pub fn run(args: &[String]) -> i32 {
    match (args.first().map(String::as_str), args.get(1).map(String::as_str)) {
        (Some("protocol"), _) => {
            println!("{PROTOCOL}");
            0
        }
        (Some("close"), Some("pre")) => close_pre(),
        _ => 0,
    }
}

/// The gate proper: read the §7 payload on stdin, resolve owner config from the
/// landing, and run [`review::review`] over the real `git diff` and `claude`
/// seams, returning its exit code. A failed stdin read is non-fatal — the parse
/// is total, so a degenerate payload reviews against the safe defaults rather
/// than wedging the close.
fn close_pre() -> i32 {
    let mut payload = String::new();
    let _ = std::io::stdin().read_to_string(&mut payload);
    let wire = Wire::parse(&payload);
    let config = config::load_config(&wire.binding.landing);
    let ball_intent = wire.ball_intent();
    let mut diff = spawn::git_diff;
    let mut claude = |prompt: &str| spawn::spawn_claude(&config, prompt);
    review::review(&ball_intent, &config, &mut diff, &mut claude)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_arg_passes() {
        assert_eq!(run(&["protocol".to_string()]), 0);
    }

    #[test]
    fn non_close_op_phase_abstains() {
        assert_eq!(run(&["claim".to_string(), "post".to_string()]), 0);
    }

    #[test]
    fn close_without_pre_abstains() {
        assert_eq!(run(&["close".to_string(), "post".to_string()]), 0);
    }

    #[test]
    fn no_args_abstains() {
        assert_eq!(run(&[]), 0);
    }

    #[test]
    fn protocol_self_description_names_close() {
        assert!(PROTOCOL.contains("\"close\""));
    }
}

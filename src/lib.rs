//! The `adversary` plugin: a balls `close.pre` review gate.
//!
//! balls' plugin protocol IS a gate protocol — core spawns `<bin> <op> <phase>`
//! and a non-zero exit aborts the op and rolls prior plugins back in reverse
//! (`docs/architecture.md` §6 in the balls repo). This plugin, wired into
//! `close.pre` (prepended, before `bl-delivery` squashes), asks a single-pass
//! non-interactive `claude` to review the task's change against its ball and
//! exits 0 (pass) or non-zero (fail → close aborts, task stays claimed).
//!
//! All logic lives here and is unit-tested; the `main` shell only adapts the
//! process boundary. See `docs/design.md` for the full design — this is the
//! scaffold; the gate itself is the tracked work.

/// The plugin's protocol self-description, emitted on `<bin> protocol`
/// (balls §6). balls never persists it; it is read once to validate wiring.
pub const PROTOCOL: &str = r#"{"protocol":1,"ops":["close"]}"#;

/// Dispatch one balls plugin invocation. `args` is argv minus the binary name;
/// the return value is the process exit code: `0` = ok (the gate passes / the
/// op proceeds), non-zero = abort.
///
/// Until the review gate is implemented (see `docs/design.md`) every op-phase
/// invocation ABSTAINS (returns 0), so wiring the plugin in never blocks a
/// close — the capability is added by replacing the `_` arm, not by flipping a
/// default.
pub fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("protocol") => {
            println!("{PROTOCOL}");
            0
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_arg_passes() {
        assert_eq!(run(&["protocol".to_string()]), 0);
    }

    #[test]
    fn op_phase_invocation_abstains() {
        assert_eq!(run(&["close".to_string(), "pre".to_string()]), 0);
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

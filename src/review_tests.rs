//! Unit tests for the gate logic (`review.rs`), driven entirely by injected
//! fakes — no real `claude`, no real `git`. The `#[path]` sidecar is a child of
//! the `review` module, so `use super::*` resolves `review`'s own items
//! (including the private helpers); `Config`, which `review` only imports
//! privately, needs an explicit `use`.

use super::*;
use crate::config::Config;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};

/// A fake `claude` seam that always returns `outcome`, ignoring the prompt.
fn answered(text: &str) -> ClaudeOutcome {
    ClaudeOutcome::Answered(text.to_string())
}

/// Build a `process::Output` with a raw wait-status, stdout, and stderr. On
/// Unix the raw status encodes a clean exit code `N` as `N << 8`; a bare small
/// value reads as death-by-signal (no exit code).
fn output(raw_status: i32, stdout: &str, stderr: &str) -> Output {
    Output {
        status: ExitStatus::from_raw(raw_status),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

// ---- review(): the whole gate, given its two seams -------------------------

#[test]
fn review_passes_when_model_says_pass() {
    let cfg = Config::default();
    let mut diff = || Ok("a real change".to_string());
    let mut claude = |_p: &str| answered("looks good\nREVIEW: PASS");
    assert_eq!(review("intent", &cfg, &mut diff, &mut claude), 0);
}

#[test]
fn review_aborts_when_model_says_fail() {
    let cfg = Config::default();
    let mut diff = || Ok("a real change".to_string());
    let mut claude = |_p: &str| answered("this is wrong\nREVIEW: FAIL");
    assert_eq!(review("intent", &cfg, &mut diff, &mut claude), ABORT);
}

#[test]
fn empty_diff_passes_without_consulting_claude() {
    let cfg = Config::default();
    let mut diff = || Ok("   \n  \n".to_string());
    let mut claude = |_p: &str| panic!("claude must not be called on an empty diff");
    assert_eq!(review("intent", &cfg, &mut diff, &mut claude), 0);
}

#[test]
fn diff_error_aborts() {
    let cfg = Config::default();
    let mut diff = || Err(io::Error::other("git is gone"));
    let mut claude = |_p: &str| panic!("claude must not be called when the diff errs");
    assert_eq!(review("intent", &cfg, &mut diff, &mut claude), ABORT);
}

#[test]
fn unreachable_fails_closed_by_default() {
    let cfg = Config::default(); // fail_open == false
    let mut diff = || Ok("a real change".to_string());
    let mut claude = |_p: &str| ClaudeOutcome::Unreachable("offline".to_string());
    assert_eq!(review("intent", &cfg, &mut diff, &mut claude), ABORT);
}

#[test]
fn unreachable_passes_when_fail_open_configured() {
    let cfg = Config {
        fail_open: true,
        ..Config::default()
    };
    let mut diff = || Ok("a real change".to_string());
    let mut claude = |_p: &str| ClaudeOutcome::Unreachable("offline".to_string());
    assert_eq!(review("intent", &cfg, &mut diff, &mut claude), 0);
}

// ---- classify(): each branch of the raw subprocess result ------------------

#[test]
fn classify_spawn_error_is_unreachable() {
    let why = match classify(Err(io::Error::other("boom"))) {
        ClaudeOutcome::Unreachable(why) => why,
        ClaudeOutcome::Answered(_) => unreachable!(),
    };
    assert!(why.contains("could not run claude"));
}

#[test]
fn classify_nonzero_exit_with_code_is_unreachable() {
    let why = match classify(Ok(output(1 << 8, "", "auth failed"))) {
        ClaudeOutcome::Unreachable(why) => why,
        ClaudeOutcome::Answered(_) => unreachable!(),
    };
    assert!(why.contains("exited (1)"));
    assert!(why.contains("auth failed"));
}

#[test]
fn classify_signal_death_has_no_code() {
    let why = match classify(Ok(output(9, "", ""))) {
        ClaudeOutcome::Unreachable(why) => why,
        ClaudeOutcome::Answered(_) => unreachable!(),
    };
    assert!(why.contains("signal"));
}

#[test]
fn classify_empty_stdout_is_unreachable() {
    let why = match classify(Ok(output(0, "   \n", ""))) {
        ClaudeOutcome::Unreachable(why) => why,
        ClaudeOutcome::Answered(_) => unreachable!(),
    };
    assert!(why.contains("no output"));
}

#[test]
fn classify_nonempty_success_is_answered() {
    let text = match classify(Ok(output(0, "REVIEW: PASS", ""))) {
        ClaudeOutcome::Answered(text) => text,
        ClaudeOutcome::Unreachable(_) => unreachable!(),
    };
    assert_eq!(text.trim(), PASS_SENTINEL);
}

// ---- passes(): default-deny verdict parse ----------------------------------

#[test]
fn passes_only_on_exact_final_token() {
    assert!(passes("some reasoning\n\nREVIEW: PASS"));
    assert!(passes("REVIEW: PASS\n\n   \n")); // trailing blank lines ignored
    assert!(!passes("REVIEW: FAIL"));
    assert!(!passes("REVIEW: PASS\nbut actually I changed my mind")); // chatter
    assert!(!passes("no verdict token anywhere"));
    assert!(!passes("")); // no non-empty line at all
}

// ---- cap_diff(): bounded, char-boundary-safe truncation --------------------

#[test]
fn cap_diff_returns_short_input_verbatim() {
    assert_eq!(cap_diff("short diff", 100), "short diff");
}

#[test]
fn cap_diff_truncates_oversized_input_with_a_marker() {
    let big = "x".repeat(500);
    let capped = cap_diff(&big, 100);
    assert!(capped.contains("truncated"));
    assert!(capped.len() < big.len() + 100);
}

#[test]
fn cap_diff_backs_off_to_a_char_boundary() {
    // "é" is two bytes; a cap of 1 lands inside it, so the loop must back off
    // to byte 0 — the result must stay valid UTF-8 (no panic on the slice).
    let s = "é-tail-tail-tail";
    let capped = cap_diff(s, 1);
    assert!(capped.contains("truncated"));
}

// ---- build_prompt(): self-contained single-pass prompt ---------------------

#[test]
fn build_prompt_carries_rubric_ball_diff_and_verdict_contract() {
    let prompt = build_prompt("MY-RUBRIC", "MY-BALL", "MY-DIFF");
    assert!(prompt.contains("MY-RUBRIC"));
    assert!(prompt.contains("MY-BALL"));
    assert!(prompt.contains("MY-DIFF"));
    assert!(prompt.contains(PASS_SENTINEL));
    assert!(prompt.contains("REVIEW: FAIL"));
}

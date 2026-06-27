//! The gate logic: build the single-pass prompt, cap the diff, consult claude
//! through an injected seam, parse the verdict, and turn (verdict, reachability,
//! fail-mode) into an exit code (§6: 0 = the close proceeds, non-zero = abort).
//!
//! Everything here is pure or seam-injected, so it is fully unit-tested with a
//! fake claude — the only real subprocess spawn lives in `spawn` (covered by the
//! integration test). The seam is two `&mut dyn FnMut` (not generic `F`, whose
//! monomorphization reads as uncovered): `diff` yields the change, `claude`
//! returns the model's answer or why it could not.

use crate::config::Config;
use std::io;

/// Exit code for "abort the close" — any non-zero aborts (§6); 1 is the
/// conventional refusal.
pub(crate) const ABORT: i32 = 1;

/// The one verdict token that passes. The NARROWEST parse that cannot
/// false-pass: the gate passes ONLY if the model's final non-empty line is
/// EXACTLY this (default-deny). Named by both the prompt and the parser, so it
/// is the single source of truth for the wire between them.
pub(crate) const PASS_SENTINEL: &str = "REVIEW: PASS";

/// What the claude seam returned. `Answered` carries the model's stdout (the
/// verdict is parsed from it); `Unreachable` carries why we never got a usable
/// answer (spawn failure, non-zero exit, timeout, empty output) — the only case
/// fail-mode governs. A reachable model that does not say PASS is a definite
/// FAIL, not unreachability, and aborts regardless of fail-mode.
pub(crate) enum ClaudeOutcome {
    Answered(String),
    Unreachable(String),
}

/// Run the gate. Pure given its two seams. Returns the process exit code.
pub(crate) fn review(
    ball_intent: &str,
    config: &Config,
    diff: &mut dyn FnMut() -> io::Result<String>,
    claude: &mut dyn FnMut(&str) -> ClaudeOutcome,
) -> i32 {
    let raw = match diff() {
        Ok(text) => text,
        Err(err) => {
            eprintln!("adversary: cannot compute the change diff: {err}");
            return ABORT;
        }
    };
    if raw.trim().is_empty() {
        // Empty diff = the abandon case (unclaim then close delivers an empty
        // worktree): nothing to review (`design.md` §4). The general path with
        // empty input, not a special case.
        println!("adversary: empty change, nothing to review");
        return 0;
    }
    let capped = cap_diff(&raw, config.max_diff_bytes);
    let prompt = build_prompt(&config.rubric, ball_intent, &capped);
    match claude(&prompt) {
        ClaudeOutcome::Answered(text) => verdict_exit(&text),
        ClaudeOutcome::Unreachable(why) => unreachable_exit(config, &why),
    }
}

/// Classify the raw result of the claude subprocess. Pure, so every branch
/// (spawn error / non-zero exit / signal / empty output / answered) is unit-
/// tested without a real process; `spawn` provides the `io::Result`.
pub(crate) fn classify(result: io::Result<std::process::Output>) -> ClaudeOutcome {
    let out = match result {
        Ok(out) => out,
        Err(err) => return ClaudeOutcome::Unreachable(format!("could not run claude: {err}")),
    };
    if !out.status.success() {
        let code = out
            .status
            .code()
            .map_or_else(|| "signal".to_string(), |c| c.to_string());
        let stderr = String::from_utf8_lossy(&out.stderr);
        return ClaudeOutcome::Unreachable(format!("claude exited ({code}): {}", stderr.trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if text.trim().is_empty() {
        return ClaudeOutcome::Unreachable("claude produced no output".to_string());
    }
    ClaudeOutcome::Answered(text)
}

/// True iff the model's final non-empty line is EXACTLY the pass sentinel.
/// Default-deny: a missing token, `REVIEW: FAIL`, trailing chatter after the
/// token, or empty output all read as false.
fn passes(text: &str) -> bool {
    text.lines().rev().map(str::trim).find(|line| !line.is_empty()) == Some(PASS_SENTINEL)
}

/// Turn a model answer into an exit code, logging the full rationale to stderr
/// (balls envelopes it into the op log, §6) and a one-line verdict to stdout.
fn verdict_exit(text: &str) -> i32 {
    eprintln!("{}", text.trim_end());
    if passes(text) {
        println!("adversary: review PASSED");
        0
    } else {
        println!("adversary: review FAILED — close aborted (rationale in the log)");
        ABORT
    }
}

/// Decide the unreachable case by fail-mode. Default (fail-closed) aborts; owner
/// config may flip to fail-open to keep balls usable when claude is offline.
fn unreachable_exit(config: &Config, why: &str) -> i32 {
    if config.fail_open {
        eprintln!("adversary: claude unreachable ({why}); fail-open configured — allowing close");
        0
    } else {
        eprintln!("adversary: claude unreachable ({why}); fail-closed — aborting close");
        ABORT
    }
}

/// Cap the diff at `max` bytes so one model call stays bounded. Truncates on a
/// UTF-8 char boundary and appends a marker so the model knows the change shown
/// is partial.
fn cap_diff(diff: &str, max: usize) -> String {
    if diff.len() <= max {
        return diff.to_string();
    }
    let mut end = max;
    while !diff.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n\n[diff truncated to {end} of {} bytes — review what is shown]",
        &diff[..end],
        diff.len()
    )
}

/// Assemble the single-pass prompt: the owner rubric, the ball intent, and the
/// (capped) diff, plus the verdict contract. Self-contained — single-pass means
/// the prompt carries all the context.
fn build_prompt(rubric: &str, ball: &str, diff: &str) -> String {
    format!(
        "You are an independent review gate for a code change about to be \
delivered by the `balls` task tracker. A separate, deterministic pre-commit \
hook already proves the change builds, passes tests, and meets coverage and \
line-length limits; do not re-judge those.\n\n\
# Review rubric (owner-supplied)\n{rubric}\n\n\
# The task being delivered (the ball)\n{ball}\n\n\
# The change under review (git diff main...HEAD)\n```diff\n{diff}\n```\n\n\
# Verdict\nReason briefly, then end your reply with EXACTLY one final line — \
either `{PASS_SENTINEL}` if the change should be delivered, or `REVIEW: FAIL` \
otherwise. Put nothing after that line.\n"
    )
}

#[cfg(test)]
#[path = "review_tests.rs"]
mod tests;

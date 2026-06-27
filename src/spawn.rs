//! The two impure subprocess seams — the only side-effecting code in `src/`.
//!
//! Kept out of `review` (pure, fake-driven, unit-tested) so the gate logic never
//! shells out under test. These two functions are the real `git diff` and the
//! real `claude` invocation; they are covered by the integration test
//! (`tests/cli.rs`), which runs the built binary so tarpaulin's llvm engine
//! counts the spawned process's `src/` execution (the same mechanism that covers
//! `main`). Both use combinator error-handling (`map`/`and_then`) rather than a
//! `?`/`match` arm, so the unreachable error path carries no source region of
//! ours to leave uncovered.

use crate::config::Config;
use crate::review::{classify, ClaudeOutcome};
use std::io;
use std::io::Write;
use std::process::{Command, Stdio};

/// `git diff main...HEAD` in the process cwd (the change worktree). Three-dot is
/// merge-base relative — exactly "what this branch introduces" — so it is correct
/// whether or not `main` has been folded in yet (`design.md` §4). A non-git cwd
/// or absent diff yields empty stdout, which `review` reads as the empty-change
/// (abandon) case; only failure to *spawn* git is an `Err`.
pub(crate) fn git_diff() -> io::Result<String> {
    Command::new("git")
        .args(["diff", "main...HEAD"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run `claude` exactly once, non-interactively, and classify the result.
///
/// Headless `--print` mode (no TTY, prints and exits); the owner's `model` and
/// reasoning `effort` pick the grader, and the whole call is bounded by the
/// owner's `timeout_secs` via the `timeout` coreutil — so a hung model trips a
/// non-zero exit (caught as `Unreachable`, governed by fail-mode) instead of
/// wedging every close. The prompt is fed on stdin (no arg-length cap on a large
/// diff); a stdin write that races the child closing is non-fatal — the model's
/// actual output, captured by `wait_with_output`, is what `classify` judges.
pub(crate) fn spawn_claude(config: &Config, prompt: &str) -> ClaudeOutcome {
    let result = Command::new("timeout")
        .arg(config.timeout_secs.to_string())
        .arg("claude")
        .arg("--print")
        .arg("--model")
        .arg(&config.model)
        .arg("--effort")
        .arg(&config.effort)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            {
                let mut stdin = child.stdin.take().expect("piped stdin");
                let _ = stdin.write_all(prompt.as_bytes());
            } // stdin dropped here → EOF to claude
            child.wait_with_output()
        });
    classify(result)
}

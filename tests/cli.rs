//! Exercise the built `adversary` binary at the process boundary. tarpaulin
//! counts `src/` only and (with `--engine llvm`) credits the *spawned*
//! instrumented binary's execution, so running the binary here is how the `main`
//! shell, the `close pre` dispatch, and the two impure seams (`git_diff`,
//! `spawn_claude`) get their coverage — none is reachable from an in-process unit
//! test without shelling out for real.
//!
//! Each `close pre` case builds a throwaway git repo (a `main` base plus an
//! optional divergent HEAD so `git diff main...HEAD` is non-empty) and a STUB
//! `claude` on a per-test `PATH` passed via `Command::env` (never the global
//! env — that would race sibling tests). The stub stands in for the model so the
//! verdict is deterministic; `git` and `timeout` still resolve from the inherited
//! `PATH`.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

/// A §7 `close pre` payload: empty landing → owner config defaults (fail-closed),
/// plus a ball whose intent reaches the prompt.
const PAYLOAD: &str = r#"{"op":"close","phase":"pre","binding":{"landing":""},
    "current_state":{"title":"deliver the gate","tags":["bl-d014"]}}"#;

fn unique_dir(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "adversary-it-{tag}-{}-{n}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A `main` base commit plus, when `divergent`, a feature HEAD one commit ahead
/// so `git diff main...HEAD` shows a real change; otherwise HEAD == main and the
/// diff is empty (the abandon case).
fn init_repo(repo: &Path, divergent: bool) {
    git(repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("file.txt"), "base\n").expect("write base");
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "base"]);
    if divergent {
        git(repo, &["checkout", "-q", "-b", "work"]);
        fs::write(repo.join("file.txt"), "changed\n").expect("write change");
        git(repo, &["add", "."]);
        git(repo, &["commit", "-q", "-m", "change"]);
    }
}

/// Drop an executable stub `claude` in `dir`. fs::write closes the file before we
/// chmod and (later) exec it, so there is no open writer to trip ETXTBSY; the
/// repo setup between here and the spawn adds further margin.
fn write_claude_stub(dir: &Path, body: &str) {
    let path = dir.join("claude");
    fs::write(&path, body).expect("write stub");
    let mut perms = fs::metadata(&path).expect("stat stub").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub");
}

/// Run the built binary `close pre` in `repo`, with `stub_dir` prepended to the
/// inherited `PATH` (so the stub shadows any real `claude`), feeding `PAYLOAD` on
/// stdin. Inherits the rest of the env — crucially tarpaulin's `LLVM_PROFILE_FILE`
/// — so the spawned binary's coverage is recorded.
fn run_gate(repo: &Path, stub_dir: &Path) -> Output {
    let inherited = std::env::var("PATH").unwrap_or_default();
    let path = format!("{}:{inherited}", stub_dir.display());
    let mut child = Command::new(env!("CARGO_BIN_EXE_adversary"))
        .args(["close", "pre"])
        .current_dir(repo)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn adversary");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(PAYLOAD.as_bytes())
        .expect("write payload");
    child.wait_with_output().expect("wait adversary")
}

#[test]
fn protocol_subcommand_prints_self_description_and_exits_zero() {
    let out = Command::new(env!("CARGO_BIN_EXE_adversary"))
        .arg("protocol")
        .output()
        .expect("run the adversary binary");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("close"));
}

#[test]
fn close_pre_passes_when_review_passes() {
    let repo = unique_dir("pass-repo");
    let stub = unique_dir("pass-stub");
    write_claude_stub(&stub, "#!/bin/sh\ncat >/dev/null 2>&1\necho reasoning\necho 'REVIEW: PASS'\n");
    init_repo(&repo, true);
    let out = run_gate(&repo, &stub);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn close_pre_aborts_when_review_fails() {
    let repo = unique_dir("fail-repo");
    let stub = unique_dir("fail-stub");
    write_claude_stub(&stub, "#!/bin/sh\ncat >/dev/null 2>&1\necho reasoning\necho 'REVIEW: FAIL'\n");
    init_repo(&repo, true);
    let out = run_gate(&repo, &stub);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn close_pre_fails_closed_when_claude_unreachable() {
    let repo = unique_dir("unreach-repo");
    let stub = unique_dir("unreach-stub");
    // A claude that exits non-zero is "unreachable"; default config is
    // fail-closed, so the close must abort.
    write_claude_stub(&stub, "#!/bin/sh\ncat >/dev/null 2>&1\necho boom >&2\nexit 1\n");
    init_repo(&repo, true);
    let out = run_gate(&repo, &stub);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn close_pre_passes_on_empty_diff() {
    let repo = unique_dir("empty-repo");
    let stub = unique_dir("empty-stub");
    // HEAD == main → empty diff → pass without ever consulting the stub.
    write_claude_stub(&stub, "#!/bin/sh\nexit 7\n");
    init_repo(&repo, false);
    let out = run_gate(&repo, &stub);
    assert_eq!(out.status.code(), Some(0));
}

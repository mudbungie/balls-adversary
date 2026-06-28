#!/usr/bin/env bash
#
# smoke-test.sh — MANUAL end-to-end smoke test of the adversary close.pre gate
# against the REAL `claude` CLI.
#
# WHY THIS IS NOT AN AUTOMATED TEST. The unit tests (src/review_tests.rs) and the
# integration test (tests/cli.rs) drive the gate with a STUB `claude`, so they
# prove the wiring and the verdict parse against a script we control — but never
# against a real model. They cannot: a real claude needs network, auth, and a
# paid model that no CI has, and tarpaulin counts only src/ (tests/ and shell are
# coverage-neutral). This script closes the one gap they leave: does a real model,
# given the gate's own prompt, actually end its reply with EXACTLY `REVIEW: PASS`
# / `REVIEW: FAIL` (nothing after) so review.rs::passes — "the last non-empty line
# equals REVIEW: PASS" — fires? The gate is FAIL-CLOSED, so a first real use that
# mis-parses a genuine pass would silently abort the user's close.
#
# HERMETIC / SAFE. This NEVER touches your real `bl` config (it does not run
# `bl conf prepend close.pre adversary`, which would gate every future close
# fail-closed). It builds the binary and invokes it DIRECTLY, exactly as balls
# would at close.pre: argv `close pre`, the §7 payload on stdin, cwd = a THROWAWAY
# git repo under a fresh mktemp dir. Each run uses unique temp dirs and cleans up
# after itself, so it is idempotent and leaves nothing behind.
#
# REQUIRES: cargo, git, and a real `claude` on PATH (the gate shells
# `timeout <secs> claude --print --model <m> --effort <e>` with the prompt on
# stdin). Uses the gate's DEFAULT owner config (empty landing) — i.e. the model
# and effort a real, unconfigured install would use — so it exercises the actual
# shipped gate, not a cheaper stand-in.
#
# USAGE:   scripts/smoke-test.sh
# EXIT:    0 = both cases behaved (PASS-diff passed, FAIL-diff aborted); else 1.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN="${ADVERSARY_BIN:-$REPO_ROOT/target/release/adversary}"

note() { printf '\n=== %s ===\n' "$*"; }

# --- Preconditions ----------------------------------------------------------
command -v git    >/dev/null || { echo "smoke-test: git not on PATH" >&2; exit 1; }
command -v claude >/dev/null || {
    echo "smoke-test: real \`claude\` not on PATH — this test needs it" >&2
    exit 1
}

if [ -n "${ADVERSARY_BIN:-}" ]; then
    [ -x "$BIN" ] || { echo "smoke-test: ADVERSARY_BIN=$BIN is not executable" >&2; exit 1; }
else
    command -v cargo >/dev/null || { echo "smoke-test: cargo not on PATH" >&2; exit 1; }
    note "building release binary"
    ( cd "$REPO_ROOT" && cargo build --release ) || { echo "smoke-test: build failed" >&2; exit 1; }
fi
echo "smoke-test: gate binary = $BIN"

# A scratch root all cases live under; removed on exit (idempotent, stub-free).
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/adversary-smoke.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT

# Deterministic identity so `git commit` never blocks on missing user config.
export GIT_AUTHOR_NAME=smoke GIT_AUTHOR_EMAIL=smoke@test \
       GIT_COMMITTER_NAME=smoke GIT_COMMITTER_EMAIL=smoke@test

# make_repo <dir> <base-file-content> <head-file-content>
# A `main` base commit plus a divergent HEAD on `work`, so the gate's
# `git diff main...HEAD` (run in <dir>) is non-empty — the change under review.
make_repo() {
    local dir="$1" base="$2" head="$3"
    git -C "$dir" init -q -b main
    printf '%s' "$base" > "$dir/code.py"
    git -C "$dir" add -A && git -C "$dir" commit -qm base
    git -C "$dir" checkout -q -b work
    printf '%s' "$head" > "$dir/code.py"
    git -C "$dir" add -A && git -C "$dir" commit -qm change
}

# run_case <label> <ball-title> <base> <head> <expected-exit>
# Build a throwaway repo, craft a §7 payload carrying the ball title, run the gate
# in that repo with the payload on stdin, and assert the exit code. Prints the
# real review write-up (the gate's stderr) and the parsed last non-empty line so
# you can SEE the real model output shape the verdict parse depends on.
run_case() {
    local label="$1" title="$2" base="$3" head="$4" want="$5"
    local dir; dir="$(mktemp -d "$SCRATCH/$label.XXXXXX")"
    make_repo "$dir" "$base" "$head" >/dev/null 2>&1

    # §7 close.pre payload: empty landing → the gate's DEFAULT owner config
    # (real model, fail-closed); current_state carries the ball intent. The
    # titles below contain no JSON-special chars (no " or \), so they embed
    # directly — keeping the script dependency-free.
    local payload
    payload=$(printf '{"op":"close","phase":"pre","binding":{"landing":""},"current_state":{"title":"%s","tags":["smoke"]}}' "$title")

    note "$label case — ball: $title"
    echo "--- diff main...HEAD (what the gate reviews) ---"
    git -C "$dir" --no-pager diff main...HEAD
    echo "--- consulting real claude (this calls the model) ---"
    local err="$dir/review.stderr"
    echo "$payload" | ( cd "$dir" && "$BIN" close pre ) >/dev/null 2>"$err"
    local got=$?
    echo "--- gate stderr (the real review write-up) ---"
    cat "$err"
    echo "--- parsed verdict (gate's last non-empty stderr line) ---"
    grep -v '^[[:space:]]*$' "$err" | tail -n1

    if [ "$got" -eq "$want" ]; then
        echo "OK: $label exited $got as expected"
        return 0
    fi
    echo "FAIL: $label exited $got, expected $want" >&2
    return 1
}

fails=0

# PASS case: a small diff that genuinely delivers the trivial stated ball.
run_case pass \
    "Add a greet(name) function that returns a 'Hello, <name>!' string" \
$'def add(a, b):\n    return a + b\n' \
$'def add(a, b):\n    return a + b\n\n\ndef greet(name):\n    """Return a greeting for the given name."""\n    return f"Hello, {name}!"\n' \
    0 || fails=$((fails + 1))

# FAIL case: a diff that does NOT deliver its ball and is broken (the stated
# validation is absent; the only change is an unrelated, dangling reference).
run_case fail \
    "Add input validation to login() to reject empty usernames and passwords" \
$'def login(username, password):\n    return authenticate(username, password)\n' \
$'def login(username, password):\n    return authenticate(username, password)\n\n\n# TODO: footer later\nFOOTER_COLOR = undefined_color_constant\n' \
    1 || fails=$((fails + 1))

note "result"
if [ "$fails" -eq 0 ]; then
    echo "smoke-test PASSED: real round-trip verified — PASS-diff passed (exit 0),"
    echo "FAIL-diff aborted (exit 1), and the REVIEW: PASS/FAIL sentinel parsed."
    exit 0
fi
echo "smoke-test FAILED: $fails case(s) did not behave; inspect the write-ups above." >&2
echo "If the model emitted trailing text or wrapped the verdict, tighten the" >&2
echo "verdict instruction in src/review.rs::build_prompt and re-run." >&2
exit 1

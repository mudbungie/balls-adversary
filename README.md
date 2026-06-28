# adversary

A [balls](https://github.com/mudbungie/balls) `close.pre` review gate: before a
task delivers, run a **single-pass, non-interactive `claude`** over the task's
change and its ball, and **abort the close** if the review fails.

It is an ordinary, third-party balls plugin — a single binary named `adversary`,
wired into the `close.pre` hook ahead of `bl-delivery`. balls' plugin protocol
is itself a gate protocol: a non-zero exit aborts the op. So the gate is just
"exit non-zero to refuse delivery"; the review write-up rides stderr (which
balls envelopes into its log), and core parses nothing back.

> Status: **working gate.** On `close pre` it reads the §7 payload, runs a
> single-pass `claude` over `git diff main...HEAD` and the ball against the
> owner's rubric, and exits 0 (pass) or non-zero (abort). Every other op-phase
> abstains (exits 0). The verdict is default-deny — pass only if the model's
> final line is exactly `REVIEW: PASS` — and an unreachable model fails **closed**
> by default (owner-configurable). See [`docs/design.md`](docs/design.md).

## Install

```sh
make install          # builds release, drops the binary beside `bl`
bl conf prepend close.pre adversary   # wire it ahead of bl-delivery
```

Remove it cleanly with `bl conf remove close.pre adversary` — it is severable:
deleting it removes config, not code.

## Develop

```sh
make install-hooks    # clippy + 300-line cap + tests + 100% coverage gate
make test
make coverage         # needs cargo-tarpaulin
```

## Verify against real claude (manual ritual)

The unit tests and `tests/cli.rs` drive the gate with a **stub** `claude`, so
they prove the wiring and the verdict parse against a script we control — but a
real model is never consulted. They cannot be: a real `claude` needs network,
auth, and a paid model no CI has, and `tarpaulin` counts only `src/` (so `tests/`
and shell are coverage-neutral; staying at 100% does not depend on this). That
leaves one thing unproven — does a **real** model, given the gate's own prompt,
end its reply with EXACTLY `REVIEW: PASS` / `REVIEW: FAIL` (nothing after) so
`review.rs::passes` (last non-empty line == `REVIEW: PASS`) fires? Because the
gate is **fail-closed**, a first real use that mis-parses a genuine pass would
silently abort the user's close.

`scripts/smoke-test.sh` closes that gap. It is a **manual** ritual — run it by
hand when touching `build_prompt` or the verdict parse, or to sanity-check a new
model. It is **hermetic and safe**: it NEVER touches your real `bl` config (it
does not `bl conf prepend close.pre adversary`, which would gate every future
close). It builds the binary and invokes it **directly**, exactly as balls would
at `close.pre` — argv `close pre`, the §7 payload on stdin, cwd = a **throwaway**
git repo under a fresh `mktemp` dir — for two cases against the real model:

- a small diff that **genuinely delivers** a trivial stated ball → expect exit
  `0` and a final `REVIEW: PASS`;
- a diff that **does not deliver** its ball (and is broken) → expect a non-zero
  exit and a final `REVIEW: FAIL`.

```sh
scripts/smoke-test.sh          # needs cargo, git, and a real `claude` on PATH
```

It prints each real review write-up and the parsed verdict line, then exits `0`
only if both cases behaved. It uses the gate's **default** owner config (empty
landing → the shipped model/effort, fail-closed), so it exercises the actual
gate. Each run uses unique temp dirs and cleans up after itself (idempotent).
If a model wraps the verdict or adds trailing text, the script fails loudly —
tighten the verdict instruction in `src/review.rs::build_prompt` and re-run.

> Verified end-to-end against real `claude` (Opus, `--effort high`): both cases
> behaved and the `REVIEW: PASS`/`REVIEW: FAIL` sentinel parsed cleanly with no
> trailing wrapper — no prompt tuning was required (ball bl-9d85).

## Why a separate repo

balls ships exactly two first-party plugins — `tracker` (the only thing that
talks to a remote) and `bl-delivery` (the only thing that touches code). Those
are *constitutive*: the base can't sync or deliver without them. An LLM review
gate is *policy* layered on the primitives, and it carries a heavy external
dependency (`claude` itself). So it lives out of tree, behind the plugin seam.
The `bl-` prefix is reserved for balls' first-party plugins; this one is named
for itself. See [`docs/design.md`](docs/design.md) for the full rationale.

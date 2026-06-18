# adversary

A [balls](https://github.com/mudbungie/balls) `close.pre` review gate: before a
task delivers, run a **single-pass, non-interactive `claude`** over the task's
change and its ball, and **abort the close** if the review fails.

It is an ordinary, third-party balls plugin — a single binary named `adversary`,
wired into the `close.pre` hook ahead of `bl-delivery`. balls' plugin protocol
is itself a gate protocol: a non-zero exit aborts the op. So the gate is just
"exit non-zero to refuse delivery"; the review write-up rides stderr (which
balls envelopes into its log), and core parses nothing back.

> Status: **scaffold.** The binary currently abstains (always exits 0) on every
> op-phase, so wiring it in is safe but inert. The gate itself is the tracked
> work — see [`docs/design.md`](docs/design.md).

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

## Why a separate repo

balls ships exactly two first-party plugins — `tracker` (the only thing that
talks to a remote) and `bl-delivery` (the only thing that touches code). Those
are *constitutive*: the base can't sync or deliver without them. An LLM review
gate is *policy* layered on the primitives, and it carries a heavy external
dependency (`claude` itself). So it lives out of tree, behind the plugin seam.
The `bl-` prefix is reserved for balls' first-party plugins; this one is named
for itself. See [`docs/design.md`](docs/design.md) for the full rationale.

# adversary — design

> Living document. The ball records the work; this file is the artifact. Edit
> it like code as the design is attacked. References to `§N` are sections of the
> balls spec (`docs/architecture.md` in the balls repo).
>
> This file is the **gate mechanics**. For the rubric-agnostic reframe
> (completion vs quality is config) and the surrounding system the plugin does
> not own — the autonomous loop, handoff, and the two grading holes — see
> [`completion-gate.md`](completion-gate.md) (ball bl-c005).

## 1. What this is

An **adversarial review gate** for balls: at task close, before the change is
delivered, run a **single-pass, non-interactive `claude`** that reviews the
task's diff against the original ball, and **abort the close** if the review
fails. The agent that wrote the change does not get to deliver it unchecked.

## 2. Decision: it is a balls `close.pre` plugin

Not a new mechanism, and not a new kind of thing. balls §0 already states the
shape: *"review before close, sign-offs, build gates are emergent."* The
recognized pattern is a **`close.pre` plugin** — the same pattern `tracker` and
`bl-delivery` use — and the adversary is one instance of it.

The fit is exact because **balls' plugin protocol IS a gate protocol** (§6):

```
<bin> <op> <phase>
  cwd:    the CHANGE worktree (mutating ops)
  stdin:  the §7 payload (carries bl-id + ball metadata)
  stdout: user-facing, forwarded verbatim; core PARSES NOTHING back
  stderr: diagnostic; balls envelopes each line into its unified log
  exit:   0 = ok; non-zero = abort + roll prior plugins back in reverse
```

So the only signal core reads back is the **exit code**. That is precisely a
gate verdict: exit non-zero ⇒ the close aborts, the task stays claimed, the
worktree stays up for the fix. The review's rationale goes to **stderr** (logged
by balls), not stdout. Nothing about the abort path needs building — it's the
contract.

### Why not the two cheaper-looking options

- **Fold it into the repo's `pre-commit` hook** (which `bl-delivery::gate()`
  already runs at close, post-fold). Rejected: that hook fires on *every* git
  commit in the work worktree, including WIP commits during work, and it gets
  **no `bl-id`** and no ball piped in (`gate()` runs `Command::new(&hook)
  .current_dir(path)` with inherited env only). Each defect would force a core
  change just to recover what the plugin wire hands us for free.
- **A close-blocker subtask** (`--blocks close`). Rejected: that is the *async
  human-review* gate — a dependency edge resolved out of band. An adversarial
  check is a *synchronous procedure* run at the moment of close. That's a
  plugin, not an edge.

## 3. Why a separate repo / third-party / no `bl-` prefix

**Constitutive vs policy.** balls ships exactly two first-party plugins because
each is the reference implementation of one of balls' own irreducible jobs:
`tracker` is the only thing that talks to a remote, `bl-delivery` the only thing
that touches code. Strip them and the base can't sync or deliver. The adversary
is the opposite: balls is fully functional without it, it encodes a *specific
opinion* about review, and it carries a heavy external dependency — `claude`
itself, with its auth and cost model. Baking that into a git-native task tracker
is exactly the coupling §0 warns against. So it lives out of tree, behind the
plugin seam.

**Naming.** The `bl-` prefix is reserved for balls' first-party plugins,
mirroring the §5 commit-trailer reservation ("`bl-` is RESERVED to core; plugins
prefix with their own name"). `tracker` is grandfathered unprefixed; the rule is
declared forward, not retrofitted. This plugin is third-party, so its binary is
named `adversary` — no prefix.

**Why not rename `bl-delivery` → `delivery` for symmetry?** It would break every
deployed install. The plugin *name* is the contract, and it lives in each user's
**committed landing config** (`config/plugins.toml` on `balls/config`),
referenced six times for delivery (`claim.post`, `prime.post`, `unclaim.post`,
`show`, `close.pre`, `close.post`). Upgrading the `bl` binary does not rewrite
that committed config: seed-prune runs only when *founding* a fresh substrate
(`src/seed.rs::seed`), while an established landing uses `rebind()`, which
"never prunes or rewrites the committed `plugins.toml`"; and config never syncs
(§12). So after a rename the committed hooks still name `bl-delivery`, the
shipped binary is `delivery`, the name no longer binds, and the next `bl claim`
fails with a clean "plugin referenced but not installed here" dispatch error —
delivery dead until the user hand-rewrites all six entries. The only
non-breaking rename path is shipping `bl-delivery` as a deprecated alias for a
major cycle — cruft for a cosmetic win. Don't.

## 4. How it works

Wire it **prepended** to `close.pre`: `bl conf prepend close.pre adversary`.
Per §6 the irreversible plugins run last (`bl-delivery`'s squash), so a gate
belongs *ahead* of `bl-delivery` — its non-zero exit aborts before anything is
squashed.

On a `close pre` invocation the binary:

1. Reads the **§7 payload** on stdin → the `bl-id` (and ball metadata; or shells
   `bl show <id> --json` for the authoritative ball).
2. Computes the change with `git diff main...HEAD` in cwd (the change worktree).
   Three-dot / merge-base is "what this branch introduces" — the right diff to
   review, and it is correct whether or not `main` has been folded in yet (see
   below).
3. Makes **one** `claude` call (single-pass, non-interactive) with the ball + the
   diff, asking for a pass/fail verdict.
4. Exits **0** (pass → close proceeds) or **non-zero** (fail → close aborts,
   task stays claimed). The review write-up goes to **stderr**.

### Edges that fall out for free

- **Pre-fold vs post-fold tree.** `close.pre` runs *before* `bl-delivery` folds
  `main` into `work/<id>`. But `git diff main...HEAD` is merge-base-relative, so
  it shows exactly what the branch introduces regardless of the fold. Correct
  as-is; no need to reproduce the fold.
- **Empty diff** (abandon = `unclaim` then `close` delivers an empty worktree):
  nothing to review → pass. Not a special case — the general path with empty
  input.

### One optional knob

Universal-at-close vs selective: the plugin can early-exit (pass) unless the
ball carries a tag/field it reads from the payload. That is the plugin's own
policy reading the ball — never a core field. Default: review every close.

## 5. Open questions (attack these before building)

- **Verdict wire.** Core reads only the exit code. How does `claude`'s answer
  become an exit code — a trailing `PASS`/`FAIL` token, a small JSON envelope,
  a tool/stop-reason contract? Must be robust to a chatty model. (Pick the
  narrowest parse that can't false-pass.)
- **Model / effort / prompt.** Which model, what review rubric, how much of the
  ball (body vs full history) and diff (size cap?) to feed. Single-pass means
  one shot — the prompt carries all the context.
- **Failure-mode policy.** If `claude` is unreachable / unauthenticated / times
  out, does the gate fail-open (pass, log a warning) or fail-closed (abort)?
  Fail-open keeps balls usable offline; fail-closed is a stricter gate. Likely a
  config knob in the plugin's *own* territory (`config/plugins/adversary/`, §1),
  never a core field.
- **Cost / determinism.** One model call per close. Acceptable; note it.

## 6. Wiring summary

```sh
make install                          # binary beside `bl`
bl conf prepend close.pre adversary   # ahead of bl-delivery
# ... later ...
bl conf remove close.pre adversary    # fully severable
```

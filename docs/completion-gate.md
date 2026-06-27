# completion gate & the autonomous loop — design

> Living document. The ball (bl-c005, child of bl-d014) records the work; this
> file is the artifact. Edit it like code as the design is attacked. `§N` are
> sections of the balls spec (`docs/architecture.md` in the balls repo). The
> gate *mechanics* live in `design.md` (this repo); this file is the **reframe**
> that sits on top of them, plus the surrounding system the plugin does not own.

## 0. Provenance

This is the converged output of a design dialogue that asked: *how does balls
compare to Codex `/goal` mode, and what would it take to get goal-mode outcomes
on top of balls?* The short answer: a generalization of the adversary gate
(`design.md`) **plus** a harness loop. Most of what `/goal` does, balls already
does better (multi-agent claim/occupancy, a dependency graph, git-native shared
state, gated delivery). The two things `/goal` has that balls leaves to others
are the **autonomy loop** and a **completion audit** — and both land cleaner in
balls than in `/goal`, because balls gives the loop a *deterministic* stop
condition and the audit a *separate process*.

## 1. The reframe: the gate is rubric-agnostic

`design.md` describes an **adversarial review gate**. That framing is too narrow.
The mechanism — a `close.pre` plugin that runs a single-pass `claude` over the
diff + the ball and returns a verdict as an exit code — is not inherently about
*review quality*. It is a gate that checks the delivered change against a
**rubric the repo owner supplies**.

So "completion vs quality" is **not a core opinion and not a plugin opinion** —
it is the owner's rubric, config. Valid rubrics:

- *completion*: "does this diff actually achieve the ball's stated intent?"
- *quality*: "is this change good — clear, safe, minimal?"
- a mix, or anything else the owner writes.

The plugin stays unopinionated about *what* to judge. It is opinionated about
exactly one thing — §3 below — and that is a principle, not a policy.

**Where the rubric lives.** It must be repo-scoped *and* shared, or two clones
grade differently. So it is a **landing-committed plugin config** (the plugin's
own `config/plugins/adversary/` territory, §1/§12), travelling on `bl install`
like any capability policy — never XDG (per-machine, drifts) and never a core
field. Severable: delete the plugin, delete its config, core untouched.

## 2. The gate is a pure verdict; disposition lives in the consumer

The gate's contract is a **pure function**:

```
(diff, ball-intent, rubric) -> { pass: bool, reasons, unmet[] }
```

It returns; it does not act. It does **not** unclaim, create follow-ups, deliver
partials, or decide "fix now vs hand off." Those are *disposition*, and the same
`fail` verdict means different things in different workflows (solo loop: keep
iterating; pool: release for a fresh agent; human-in-loop: surface). Baking one
disposition into the gate is the non-severable smell — it welds one workflow
into a component that should serve all of them.

Two consequences fall out for free from the plugin seam (§6):

- **Fresh context by construction.** A `close.pre` plugin is a separate process;
  it does not inherit the maker's conversation. So "the maker grades its own
  work" is solved *structurally*, not by configuration. There is no
  same-context-vs-fresh-context knob — same-context isn't even expressible
  without going out of the way to pipe the maker's transcript in, which is the
  contamination, not a feature. (Contrast Codex `/goal`, whose audit rides the
  same degradable thread; its own tracker, openai/codex#19910, reports the audit
  instruction being lost to mid-turn compaction — a failure that is *impossible*
  if the verifier is a separate context.)
- **The verdict surfaces, it does not return into core.** Core reads only the
  exit code (§6); `reasons`/`unmet[]` ride stderr (logged by balls) and are read
  by the *loop*, not core. Emit `unmet[]` machine-readable so the loop can seed a
  handoff note or follow-up from it (§5).

## 3. Hole 1 — deterministic vs judgmental (the one principle the gate enforces)

The axis that matters is **not** completion-vs-quality. It is
**deterministic-vs-judgmental**, and balls already owns a deterministic gate.

"Tests passed / coverage is 100% / no file over the line cap / it builds" are
facts with definitive answers, and `bl close` *already runs them
deterministically* — that is the repo's **pre-commit hook**, which
`bl-delivery::gate()` runs on the folded tree at close. An LLM asked to attest a
checkable fact can be fooled or hallucinate "yes": you would be paying inference
for a *less* reliable answer than `make test` gives for free.

So the principle — modelled by the plugin's **sample rubric**, not hardcoded in
its dispatch:

- Let the **hook** prove facts.
- Ask the **LLM** only for what *only* judgment can settle.
- Where the rubric must touch a fact, make it **cite the artifact** (the
  test-output line, the diff hunk) rather than assert. *Evidence, not
  confidence* — the exact rule Codex states and then violates by riding it on
  degradable context.

Unopinionated about *what* to judge; opinionated about not delegating arithmetic
to a poet.

## 4. Hole 2 — grade criterion → intent, not just diff → criterion

Fresh context (§2) fixes the **grader**, not the **criterion-writer**. If the
maker also authors the success criterion, it can write a weak criterion it
trivially clears, and an honest fresh grader still passes it — garbage rubric,
clean grade.

This is the balls analog of the failure underneath openai/codex#19910: *the
local task loses the global goal.* The fix: the grader must not only check
`diff -> criterion`; it must check `criterion -> intent` — is this success
criterion **adequate for the ball's stated title/purpose**? The title/intent is
the durable, author-set anchor (harder to weaken after the fact); the criterion
is derived and suspect. **Grade the whole chain, not the last link.**

Consequence: the ball's **intent must reach the plugin**, not just the diff. The
§7 payload already carries the `bl-id` and ball metadata (`design.md` §4.1), and
the plugin can shell `bl show <id> --json` for the authoritative ball — so the
input surface exists. Confirm it carries body/title intent, not only the change.

**Partial mitigation balls already affords:** in a multi-agent flow the ball's
author is often *not* its claimant, so the criterion-writer is not the maker.
Weak-rubric gaming then needs two agents to collude rather than one to
rationalize. Cross-tracking (this repo is now a satellite of the balls center
store) makes that handoff the normal case.

## 5. The autonomous loop is the harness — not balls, not the plugin

balls is **inert**: `bl skill` exhorts "an agent that claims and walks away has
not finished its job," but balls has no mechanism to *enforce* it — it is CLI
verbs over git and cannot make a model take another turn. "Keep going until
done" is irreducibly a **harness** feature (Codex `/goal` is exactly that: a
continuation check at idle boundaries). The balls analog is a harness loop
(`/loop`, or a Ralph-style external `while`).

Do **not** copy `/goal`'s stop condition. `/goal` stops on an in-context
self-audit — the fragile part. balls gives the loop a **deterministic, external**
stop condition instead:

```
loop:  claim -> work -> bl close
stop:  bl list -s claimed --json is empty   (the work is delivered)
```

The stop is git-observable state, not a model's self-assessment — "make it a
query, not a field" applied to termination. `bl close` going green *is*
completion, because the close gate (pre-commit hook §3 + this gate §1) is what
makes "claimed → closed" mean "done and independently verified."

**The runaway guard — the dangerous half of "keep going."** "Until done" plus an
unsatisfiable ball (gate never greens) is an infinite, token-burning loop.
`/goal`'s `budget-limited` state is the one piece worth lifting wholesale: the
loop must also stop on **budget** (max iterations/tokens) and on **stuck** (no
state change — same `work/<id>` HEAD and same gate failure across N rounds).
Both are queryable; neither is a judgment.

## 6. Handoff = unclaim + a note (no new mechanism)

The "valuable but incomplete, not delivering" outcome does **not** need a new
task. "Hand off to a new agent" ≠ "cut a new task":

- `bl unclaim` already hands the ball to whatever agent claims next, **with the
  committed WIP preserved on `work/<id>`**; a later claim resumes the branch and
  close eventually delivers it. The remainder lives where remainders live: the
  **body** (living state — "done X, still need Y") plus a `-m` journal line.
- A **new follow-up ball** is earned in exactly one case: the remaining work is a
  genuinely **separable unit** (or the partial is itself independently
  deliverable and the remainder is distinct). "I didn't finish" is same-ball
  unclaim, not a split.

**Discipline:** commit WIP *before* unclaim, or the worktree teardown drops
uncommitted work — the handoff only preserves what is on the branch.

**Fix-now vs hand-off is a loop threshold, not a judgment.** Don't push
grade-the-grader up a level by asking the maker (or the gate) "should I keep
trying?" Make it the runaway-guard tripping: N failed closes → release. That is
deterministic and lives in the loop (§5).

## 7. Structure enforcement is an optional, severable plugin

There is a real, separate pattern: *mandate that a ball carries a checkable
success criterion* (the four-clause goal contract — achieve / don't-change /
validate / stop). It has consumers. It is **not core** — balls works without it.

It is the **gate's input contract**: a mandated structure nobody consumes is
ceremony; it earns its keep only because this gate reads it. So fold the
"require a checkable criterion" into the *gate's own* enforcement at close (the
one point it is consumed), and keep any create/claim-time prompt to a **nudge**
(seed the body template at `claim.post`), never a refusal — defining success
*after* the work is the rationalization anti-pattern. Single enforcement point;
filing stays cheap.

balls already answers two of the four clauses, so don't rebuild them:
**when-to-stop** = the close gate greening (universal); **how-to-validate
(floor)** = the pre-commit hook (§3); **blast radius** = the worktree diff. The
only bespoke delta a ball must carry is the **success criterion + the
don't-regress constraints**.

## 8. Open tensions (attack before committing)

- **Criterion → intent leaks quality into a completion-only gate.** Asking "is
  this criterion adequate?" (§4) is itself a quality judgment. So "completion and
  quality are cleanly separable, it's just config" (§1) may not survive contact —
  the adequacy check smuggles quality back in. Decide: wall it off, or admit the
  gate is always a little of both.
- **Splitting-to-pass is §4's gaming in a follow-up costume.** "Shrink the
  criterion to the increment, push the rest to a follow-up, now the gate is
  green" lowers the bar to fit the work. Legitimate *only* as a deliberate,
  logged decision by someone who is not the maker — never a silent move the loop
  or gate makes to escape a red gate.
- **False-fail on the delivery critical path.** A nondeterministic gate trades
  `/goal`'s false-pass (premature "done") for a false-fail (good work it won't
  let you close). Bound it: a refusal must **cite the specific unmet criterion**
  (so a vague "I'm unsure" can't wedge delivery), plus a human override path —
  "retry `bl close`" doesn't help when the gate reliably dislikes correct work.

## 9. Relationship to `design.md` and bl-d014

`design.md` stays the artifact for the **gate mechanics** (why a `close.pre`
plugin, the wire, pre-fold/empty-diff edges, the §5 open questions on verdict
wire / model / failure-mode). This document is the **reframe + ecosystem**: the
gate is rubric-agnostic (§1), the two holes (§3–§4), and the loop/handoff/
structure scope the plugin does not own (§5–§7). Where this doc resolves a
`design.md` §5 open question — e.g. "review rubric" becomes "owner-supplied
config rubric" — `design.md` should be amended to point here rather than
duplicate. Single source of truth.

+++
type = "grilling"
mode = "hitl"
status = "closed"
blocked_by = ["t02-no-timeout-in-a-moving-line"]
repos = ["amont"]
+++
# What a check does when its wait ends

## Question

Under `onBusy wait` (ADR-0009) a check whose wait times out runs anyway, outside
the queue: 29 such runs since 2026-11-21 18:00, some at load above 400, which
adds load to a host that is already jammed. The person sometimes wanted exactly
that (no wait). Once t02 decides when a wait ends, what should happen then: run
unqueued as now, defer to CI, or run unqueued only below a load ceiling? And is
"do not wait for this one" a choice the person or agent makes up front instead?

## Resolution

The person decided this on 2026-11-22.

- **Stalled hook check: defers to CI.** A wait ends under t02's rule: 300 s
  with no admission and no position change, or the ceiling. A hook check then
  defers to CI, the same way `onBusy defer` does today.
- **Stalled `slot run`: exits 75.** Nothing runs outside the queue as a side
  effect of a timeout.
- **Skipping the wait is a choice made up front.** It is never what a timeout
  leaves behind. It is an explicit flag on the run, such as `--now`, or the
  equivalent setting for a hook check. Every use is journalled in `waits.jsonl`
  with its outcome (`now`).
- **ADR-0009 is amended.** Its "wait, then run unqueued" becomes "wait while
  the line moves, then defer". The build ([t11](t11-build-admission-and-waits.md))
  records the amendment through the decisions corpus.
- **Why:** 38 unqueued fallbacks in a week, some started at load 400–563. They
  added load to a host that was already jammed. The person sometimes wants no
  wait, and keeps that as an explicit choice.

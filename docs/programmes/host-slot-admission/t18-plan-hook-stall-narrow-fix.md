+++
type = "task"
mode = "hitl"
status = "closed"
blocked_by = []
repos = ["amont"]
+++
# Plan greengate 5.0.1: no stalled-line deferral on the hook path

## Question

[t17](t17-count-after-5-0-install.md) found 141.8 hook deferrals per 12 h
after the 5.0 install, against a baseline of 31.8 (4.5×, past the plan's 2×
rollback rule). The person chose a narrow fix over a revert. Which change to
the hook path's wait (`wait_for_slot` in
`crates/greengate-runtime/src/host_slots.rs`, from #516) stops the 75
`"end":"stalled"` deferrals while keeping `slot run`'s stall end, `--now`,
`holders`, and relais on crate 5?

## Look at

- t17's `## Interim count`: deferrals by end, name and session.
- The plan `docs/plans/2026-11-23-admission-counts-holders-stalled-waits.md`,
  the hook rows of its wait table, and ADR-0022.
- The 83 `"end":"ceiling"` hook deferrals. Before 5.0 they ran unqueued at
  the ceiling. State whether 5.0.1 keeps them deferred or brings back
  unqueued runs, against `heavy-build-unqueued` (greenbelay 6.15).

## Answer looks like

A plan, reviewed by the panel, that amends ADR-0022 for the hook path and is
released as greengate 5.0.1 through `tag-release`. Its Verification repeats
t17's count 24 h after the 5.0.1 install, against the 31.8-per-12 h
baseline, and that count is the first half of t08.

## Resolution

2026-11-26. Plan `docs/plans/2026-11-26-hook-waits-no-stall-end.md`, reviewed
by the panel, built on branch `fix/hsa-t18`, and shipping as **greengate
5.1.1**. The ticket said 5.0.1, but main already carried 5.1.0 (#525, the
fleet hook's greentopo change, installed on this machine), so the narrow fix
is the next patch above it.

The change: `wait_plan` and `sibling_bounds` in
`crates/greengate-runtime/src/host_slots.rs` pass no stall window on the hook
path, so a hook check waits to its ceiling and defers there. The crate stays
at 5, so `slot run`'s stall end and exit 75, `--now`,
`GREENGATE_ON_BUSY=now`, `holders` and relais are untouched. The roll-up shows
each command under its label, so the wait command, now up to
`greengate.timeout`, copies whole in 80 columns. ADR-0022 carries the
amendment.

The 83 ceiling deferrals stay deferred. Counted again over all of the journal
since the 5.0 install (428 hook deferrals to 2026-11-26 09:14 UTC: 167 stalled,
261 ceiling), the ceiling rows are 229 at the 60 s `slotWait` of `onBusy
defer` (a deferral since ADR-0013, so no regression), 18 at 0 s and 14 at the
600 s `warnSlotWait`; none ended at `greengate.timeout`. Only the 14 were
unqueued runs before 5.0. Bringing them back would break the Destination
("nothing heavy runs outside the queue") and greenbelay's
`heavy-build-unqueued`, so they stay deferred.

The fog, made precise:

- Removing the stall end will not by itself bring the count under the plan's
  2× rule. The ceiling rows alone ran at about 69 per 12 h (2.2× the 31.8
  baseline), mostly the `onBusy defer` `slotWait` path on a host whose hook
  wait rows doubled (112 to 227 per 12 h) and whose median load went from 11
  to 17. The baseline is load-dependent. The count is
  [t19](t19-count-after-5-1-1-install.md).
- A stalled wait that used to defer after 300 s now holds a commit or a
  push up to 3600 s, or until a person presses Ctrl-C. A Ctrl-C leaves no
  `waits.jsonl` row, and `greengate slots --json` lists no waiter pids, so
  the aborted waits cannot be counted. The proxy is hook wait rows per 12 h
  against 227, and a `joined` journal row is the follow-up if it matters.
- Resolving needs the scope: `greentopo resolve hooks.host-concurrency`
  answers `undecided` with no scope, and `--scope hook-binary` answers
  ADR-0022 (the key declares scope `hook-binary`, not the default). Both aval
  and greentopo agree once the scope is named.

Verification: unit and integration tests (see the plan), then t19's count 24 h
after the install.

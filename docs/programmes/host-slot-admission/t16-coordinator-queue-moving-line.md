+++
type = "task"
mode = "afk"
status = "open"
blocked_by = ["t11-build-admission-and-waits"]
repos = ["relais"]
+++
# relais's coordinator queue: does a moving line still time out?

## Question

t11 applies the stall rule to relais's host-slot wait only. relais's
coordinator queue for model work (`runner/launch.rs:133`, and `heavy.rs:779`
for a heavy command's coordinator lease) also charges
`admission_wait_seconds`, and it still ends a wait when that budget is spent,
even while the line is moving. That queue writes no `waits.jsonl` line.

Two questions. First, do runs end `admission_wait_exhausted` in the
coordinator queue while other dispatches are being granted? Second, if they
do, should that queue take the same stall rule?

## Look at

- `crates/relais/src/runner/launch.rs` (the `gate.admit` loop) and
  `crates/relais/src/verify/heavy.rs` (`Queue::charging`, the coordinator
  lease).
- The relais ledger: runs blocked `admission_wait_exhausted`, and whether
  their last event names the coordinator queue or the host slot
  (`admission_queue_ended`, "still queued at position N").
- relais's own record, `execution.resource-budgets`, as t11's Phase 5
  amends it.

## Answer looks like

A count, over 7 days after the relais t11 release, of coordinator-queue
exhaustions during which a grant happened, with the rule to apply. That is
either the crate's `Stall` on the coordinator's position and grants, or
"no change", with the reason.

+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# The promote off switch's unset line, caller doc and error text need fixes

## Question

The promote off switch (t23, #541) has four gaps that its callers and docs
will hit: the unset command, the caller contract, the error text, and a
duplicated plan. Which of them does one change fix, and does t24 depend on
the caller contract?

## Look at

- `CHANGELOG.md` and `docs/host-slots.md`: both give only
  `git config --global --unset greengate.hostPromote`. A value set with
  `--system` needs `--system --unset`.
- The `promote_in_waiting` doc comment: it should say that callers check the
  switch first. t24 needs this stated.
- `PromoteError`'s Display: it no longer prints its cause, so callers should
  use `one_line()`.
- `docs/plans/2026-11-27-promote-off-switch.md`: its duplicated sections need
  repairing.

## Answer looks like

Each of the four fixed in one merged change, or named `deliberate: <reason>`:
the docs show both unset forms, the caller contract is written on
`promote_in_waiting`, the Display output carries the cause through
`one_line()`, and the plan has no duplicated sections.

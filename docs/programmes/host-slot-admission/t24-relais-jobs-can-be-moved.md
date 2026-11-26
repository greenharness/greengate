+++
type = "task"
mode = "afk"
status = "open"
blocked_by = ["t12-build-relais-setup-lease"]
repos = ["relais", "amont"]
+++
# Can an agent move its own relais job ahead?

## Question

relais links `greenharness-slots` 5.0/5.1. Its waiters write empty queue
tickets, which `slot promote` can never move and which read one more job
ahead when a job passes them. Even after a bump to crate 5.2, relais tags its
jobs with its run id, not the Claude session that started the run, so an
agent cannot move its own relais job under t06's own-session rule. Two
questions: does relais bump to crate 5.2 so its tickets carry an owner, and
whose session does a relais job belong to (the run's, or the Claude session
that dispatched it)? The answer also decides how a move shows in relais's
pane. It waits for [t12](t12-build-relais-setup-lease.md), which changes the
same relais slot code (`verify/heavy.rs`, `verify/mod.rs`).

## Look at

- [t06](t06-who-may-reorder.md) (own session only) and
  [t13](t13-build-reorder.md)'s Resolution.
- relais `verify/heavy.rs` (`slots::take`, the session it passes) and the
  pane code that lists queued runs.
- `crates/greenharness-slots/src/reorder.rs`: what a ticket records.

## Answer looks like

A decision on the session a relais job belongs to, with its reason, then a
merged and released relais change on crate 5.2 whose test moves a relais
waiter ahead from its owning session, or a recorded reason to leave relais
jobs unmovable.

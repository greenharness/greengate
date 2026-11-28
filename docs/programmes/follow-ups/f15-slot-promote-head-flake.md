+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# `slot_run::a_session_moves_its_own_waiting_job_to_the_head` is flaky on macOS CI

## Question

On host-slot-admission t15's phase-1 PR (#536), the first attempt of the CI
run failed `hooks (macos-latest)` on
`slot_run::a_session_moves_its_own_waiting_job_to_the_head`
(`crates/greengate/tests/it/slot_run.rs`); the rerun passed, and the branch
did not touch `slot promote`. Does the test race the waiter it promotes (the
waiter not yet in line, or already granted, when `slot promote` runs), and
what does it wait on instead of a fixed pause?

## Look at

- Run 45959725785, attempt 1, job 140374603641 (2026-11-27).
- The test, and how it learns that its waiter has joined the queue before it
  promotes it.

## Answer looks like

A merged change after which the test passes 50 runs in a row under load, or
the race named and fixed.

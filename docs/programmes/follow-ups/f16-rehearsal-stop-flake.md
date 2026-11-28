+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# `rehearsal::stop_cancels_the_worker_and_removes_its_snapshot` is flaky under load

## Question

On host-slot-admission t15's phase-2 branch (`fix/hsa-t15-p2`), the local
commit gate's `cargo-test` failed
`rehearsal::stop_cancels_the_worker_and_removes_its_snapshot`
(`crates/greengate/tests/it/rehearsal.rs:550`): `rehearse --stop` answered
"no rehearsal is running in this worktree" instead of "stopped the
rehearsal". The suite took 1,342 s (821 passed, 1 failed), so the host was
loaded; the branch did not touch the rehearsal. The test's gated suite waits
2 s (`Atomics.wait … 2000`), and the test sends `--stop` only after
`wait_state` sees `phase=running`. Under load, can the 2 s pass between that
read and the stop, so the worker has already finished? What should the test
hold the worker on instead of a fixed 2 s (a file it waits for, released
after the stop)?

## Look at

- The log `amont/.git/worktrees/amont-wt-hsa-t15-p2/greengate/logs/cargo-test.46205.log`
  (2026-11-28 08:04), lines 1036-1060; a worktree sweep removes it, and the
  panic line above is its substance.
- The test, `gated_repo`, `wait_state`, and how `--stop` decides that no
  rehearsal is running.

## Answer looks like

A merged change after which the test passes 50 runs in a row under load, or
the race named and fixed.

+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# `cluster_gates::concurrent_gates_each_get_their_own_job` is flaky on CI

## Question

On release PR #534 (only a version bump and a CHANGELOG move), `hooks
(ubuntu-latest)` failed `cluster_gates::concurrent_gates_each_get_their_own_job`
(`crates/greengate/tests/it/cluster_gates.rs:790`): gate `a` fell back to a
local run because "greengate-gate-slot-0 is not the Job this run created";
the rerun passed. Do two concurrent gates race for the same fake Job name, and
should the slot name carry the run's id?

## Look at

- Run 45901494242, job 140188252433 (2026-11-26).
- The fake kubectl in the test and how it names `greengate-gate-slot-<n>`.

## Answer looks like

A merged change after which the test passes 50 runs in a row under load,
or the race named and fixed.

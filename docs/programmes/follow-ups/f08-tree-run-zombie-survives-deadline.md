+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# A reaped-late grandchild fails the tree_run deadline test under load

## Question

`greengate-runtime` `tree_run::tests::tree_run_timed_out_leaves_no_child_alive`
failed in a land's pre-stage on 2026-11-23, at load 40–60, on a branch that does
not touch `tree_run.rs`. The message was "grandchild 56132 survived the
deadline", 100 ms after the kill (`tree_run.rs:479`). The hypothesis: on macOS,
`kill(pid, 0)` still succeeds on a grandchild that was reparented and is a
zombie not yet reaped, so the test reads a dead process as alive. Is the
grandchild really alive at that point, or only not yet reaped? And how should
the test tell the two apart?

## Look at

- `crates/greengate-runtime/src/tree_run.rs`: the test near line 479 and the
  liveness check it uses.
- `ps -o stat= -p <pid>` (a state `Z` is a zombie) against `kill(pid, 0)` on
  macOS, with the test running under load (for example `stress-ng` or a
  parallel `cargo test`).

## Answer looks like

A red run under load shows whether the pid is a zombie or running. Then the test
either counts state `Z` as gone or polls for a few seconds until the pid is
gone, and 20 runs at load 40 or more pass.

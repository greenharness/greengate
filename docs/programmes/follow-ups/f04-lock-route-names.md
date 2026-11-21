+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# Weak names in the uv.lock audit route

## Question

Implementation review of PR #498 ruled these names against `names.reveal-intent`, and left them out of that push:
- the `ctx` parameter of `python_lock_run` in `crates/greengate-runtime/src/hooks/audit.rs`. Every check takes `ctx` for the registry `Ctx`, so renaming one alone would be inconsistent;
- the `tmp` scratch directories in the lock-route tests of `crates/greengate/tests/it/audit.rs`.

## Look at

- How many checks name the registry parameter `ctx`, and whether a fleet-wide rename is wanted.
- The `tmp` locals and parameters in `crates/greengate/tests/it/audit.rs`, in `scratch_tmp`, `leftovers` and the lock-route tests.

## Answer looks like

The test `tmp` names become `scratch_dir`. `ctx` is renamed everywhere or kept, with that decision in Decisions so far. `adr-lint check` reports no `names.reveal-intent` candidate on those lines.

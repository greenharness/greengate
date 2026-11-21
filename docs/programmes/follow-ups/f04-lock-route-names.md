+++
type = "task"
mode = "afk"
status = "closed"
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

## Resolution

- `ctx` is kept. All 15 functions that take the registry `Ctx` name it
  `ctx`, `python_lock_run` included: it is the one name for that type
  across the checks, so a reader meets it once and knows it everywhere.
  Renaming one would break that; renaming 15 buys nothing.
- The scratch-directory `tmp` locals and parameters in
  `crates/greengate/tests/it/audit.rs` (`push_check_in`, `scratch_tmp`,
  `leftovers` and the three lock-route tests) are now `scratch_dir`.

## Verification

- `grep -c '\btmp\b'` on the test file: 0 (was 18).
- `adr-lint check` on the test file: no finding.
- `cargo test -p greengate --test it audit::`: passes (pre-commit gate).

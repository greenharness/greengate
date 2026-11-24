+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# Two linter integration tests fail under load and pass alone

## Question

In a land's pre-push run on 2026-11-24, at load about 48, two tests in
`crates/greengate/tests/it` failed on a branch that touches neither:
`linters::ruff_fixes_what_it_can_and_still_blocks_on_the_rest` and
`tool_tables::opted_in_a_clean_input_passes`. The second had been running for
more than 60 s before it failed. Run together at load 9, both passed in 35 s.
The full log went away with the push worktree, so the panic text is not known.
Is it a timeout inside the test or the gate (a silence or deadline budget that
load exceeds), or something else?

## Look at

- `crates/greengate/tests/it/linters.rs` and `crates/greengate/tests/it/tool_tables.rs`:
  any deadline, budget or timeout the tests set or inherit.
- Reproduce under load: run both tests in a loop while a parallel `cargo test`
  keeps the load above 40, and keep the full output this time.

## Answer looks like

A red run under load, with its panic text, names the bound that breaks. That
bound is then scaled to the load, or removed when the test does not need it.
20 runs at load 40 or more pass.

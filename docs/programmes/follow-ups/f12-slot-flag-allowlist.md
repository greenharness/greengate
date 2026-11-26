+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# greengate's top-level flag allowlist hides new `slot` flags

## Question

Before it dispatches, greengate checks each subcommand's flags against a
top-level allowlist. When `slot promote` added `--reason` and `--to`
(host-slot-admission t13, #530), the allowlist refused them before the
subcommand saw them. Only the end-to-end test on the built binary caught it;
the unit tests passed. Should each subcommand own its flag list (so the
dispatcher cannot drift from it), or should a test assert that every flag a
subcommand parses is on the allowlist?

## Look at

- The dispatcher's allowlist in greengate's `main` / CLI module, and its
  `slot` entry.
- The `slot promote` end-to-end test that found it (PR #530).

## Answer looks like

A merged change that makes a new subcommand flag either need no allowlist
edit, or fail a unit test until it is listed.

Filed from host-slot-admission t13's report (2026-11-26).

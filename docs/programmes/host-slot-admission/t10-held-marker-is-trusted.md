+++
type = "research"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont", "greenbelay"]
+++
# A hand-set held marker skips the queue

## Question

`GREENHARNESS_HOST_SLOT` with any non-empty value makes `slot run` and the hooks
treat the process as already holding a slot (`host_slots.rs`, `slot_cmd.rs`).
A child of a real holder needs that. But `GREENHARNESS_HOST_SLOT=1 cargo test`,
typed by hand, also skips the queue, and the greenbelay rule stays silent
whenever the variable is set. Should the marker carry something only a real
holder can give, such as the holder's slot file and pid, checked to be alive?
And how often is it set by hand?

## Look at

- `crates/greenharness-slots/src/lib.rs` (`HELD_ENV`), `host_slots.rs`
  (`nested` test), `slot_cmd.rs` (`Launch::Held`, the export to children).
- `greenbelay-guard/src/rules/heavy_build_unqueued.rs` (the `confirm` docs).
- Claude transcripts under `~/.claude/projects/` from the last 7 days, scanned
  for `GREENHARNESS_HOST_SLOT=` in Bash commands (command strings only).

## Answer looks like

The count of hand-set uses; a marker format a real holder writes and a reader
checks (the slot it names is locked, and its holder pid is an ancestor or
alive), with the cost of the check; and what a stale or forged marker does: it
queues, with one line saying why.

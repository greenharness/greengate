+++
type = "research"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# How heavy work gets past the queue

## Question

The person has seen agents run heavy work outside the queue. Which ways exist,
and how often is each used: a bare `cargo test`/`cargo build` the greenbelay
`heavy-build-unqueued` rule only advises on; an inherited
`GREENHARNESS_HOST_SLOT=held`; the unqueued fallback (t03); a
`GREENGATE_ON_BUSY` override; anything else?

## Look at

- greenbelay's journal for `heavy-build-unqueued` (advise count, and whether
  the agent followed it) and the rule's source in the greenbelay repository.
- `waits.jsonl` `unqueued` rows by name and session.
- `crates/greenharness-slots/src/lib.rs` and `host_slots.rs`: what
  `GREENHARNESS_HOST_SLOT` set by a parent does to a child.
- Claude transcripts under `~/.claude/projects/` for `cargo test`, `pnpm test`
  and similar commands run with no `slot run` in front.

## Answer looks like

A table: route, count over the last 7 days, sessions involved, and a proposed
response per route (deny, advise, leave). This only informs the build; changing
the greenbelay rule from advise to deny is that repository's own decision.

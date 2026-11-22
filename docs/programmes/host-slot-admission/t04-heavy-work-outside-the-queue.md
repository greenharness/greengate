+++
type = "research"
mode = "afk"
status = "closed"
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

## Resolution

Research run 2026-11-22 (Explore agent over the greenbelay journal, `waits.jsonl`,
greengate source and 7 days of Claude transcripts, scanned for command strings
only).

| Route | 7-day count | Sessions | Response |
|---|---|---|---|
| A bare heavy command in a Bash call (`cargo test/build/clippy`, `pnpm/npm test`, `pytest`) | 148 greenbelay `heavy-build-unqueued` advise rows; the transcripts give an upper bound of 272 | 69 (journal) | deny, once the rule's confirm check is shown to hold |
| The unqueued fallback after a wait times out | 38 rows: `adr-lint-cli-probe` 22, `cargo-test` 8, `clippy` 6, `run-tests-js` 1, `go-test` 1 | 13 / 6 / 3 / 1 / 1 | decided in [t03](t03-unqueued-fallback.md) |
| `GREENHARNESS_HOST_SLOT` set by a parent | not measured | — | leave; a child of a real holder must not queue again |
| The same variable set by hand to any non-empty value | not measured | — | [t10](t10-held-marker-is-trusted.md) |
| A `GREENGATE_ON_BUSY` override | no production code reads it to skip the queue | — | leave |
| `slot run --on-timeout=run` | not separated from the first row | — | advise (the rule already counts it as unqueued) |

**What is known about each route.** The greenbelay rule
(`greenbelay-guard/src/rules/heavy_build_unqueued.rs`) is declared with a
default and maximum stance of advise, so making it deny is a code change in
greenbelay. That repository decides it; this programme hands it the number. The
journal does not record whether the agent followed the advice.

**Accepted gaps.** The transcript count is an upper bound: a matching command
may only be text inside a `grep` or `echo`. Hand-set environment variables were
not counted.

---
canonical: relais:docs/plans/2026-11-27-relais-waits-through-greengate-loop.md
phases: [1]
status: done
---
Part of [relais waits through greengate's wait loop](https://github.com/fredericrous/relais/blob/main/docs/plans/2026-11-27-relais-waits-through-greengate-loop.md)
(programme relais-modularise, ticket t05): Phase 1, `wait_in_line_paced` in
`greenharness-slots`, released as the next minor.

## Code review

- amont: backend, lang:rust, architect (unix, tui left out: no flag, output or exit-code change)

## Decision log

- 2026-11-27 — `Pause` and `Ended` are exhaustive, not `#[non_exhaustive]`
  as the plan's panel had it: the decided rule `types.exhaustive-matching`
  wins (code review lang:rust, architect), so a new variant is a major
  change the compiler flags in every caller, never a wildcard arm.
- 2026-11-27 — the crate README is left as it is: it has 8 lines and no API
  list; `docs/host-slots.md` and the rustdoc name `wait_in_line_paced`.

## Verification

- Phase 1: `cargo test -p greenharness-slots` → existing `wait_in_line_with`
  tests pass unedited (only `run_line`'s closures changed), new
  Break/Retrying/order tests pass → 106 passed.
- Phase 1: `cargo +1.74.0 check -p greenharness-slots -p greengate-runtime`
  (no lockfile, as CI's `msrv` job) → builds → OK.
- Phase 1: `cargo test -p greengate-runtime --lib -- host_slots slot_cmd`
  (the two `wait_in_line` callers) → pass → 17 passed.
- Phase 1: built `greengate slot run -- sleep 1` behind a `sleep 4` holder
  in a private slot dir, one slot → waits, runs → queued ~3 s, rc 0.
- Phase 1: `greengate slot run -- sh -c 'exit 7'` behind a holder → prints 7
  → rc 7 after a 5.2 s queue; `waits.jsonl` rows read `admitted`.

## Implementation review

- implementation-review: approve-with-changes → approve on delta; fixed Verification record, README decision, CHANGELOG tense; candidate fine (test panic); 86k+34k, 97 s.
- backend: approve-with-changes → approve; wildcard rule moot (enums exhaustive), wrapper evidence recorded; 60k+27k, 78 s.
- lang:rust: approve-with-changes → approve; `#[non_exhaustive]` removed from `Pause`/`Ended`, variants documented, pause count asserted; 49k+23k, 57 s.
- architect: approve → approve; crate doc names no caller (ADR-0075); 49k+23k, 48 s.
- implementation-review delta 2 (CI fix, HEADS.md as greentopo 2.0.0 writes it): approve, no findings; 32k, 23 s.

---
status: active
branch: prog/host-slot-admission/t20-queued-heartbeat-names-ceiling
repos: [amont]
adrs: [ADR-0022]
plan-id: p-01M8FH118P0RWR9B68M6X6SM0Z
---
# The queued heartbeat names the ceiling

## Review panel
👉 **Decide:** none — approve if the heartbeat naming its ceiling and the skip variable, and the roll-up label carrying the resolved length, is the whole change.
📍 amont · 1 phase · next: Phase 1. Panel: backend, lang:rust, tui, unix (opus); codex: not installed.
**Changed by review:** wording `defers to CI`; skip names the variable only (`on_push` cannot name the command run); one shared `wait_ceiling_secs`.
**Verdicts:** 2 approve, 2 approve-with-changes after round 2; 5 carried into implementation (listed in the reviews); words 629 → 894 (×1.42, fixes to wrong statements exempt); remove 0, merge 0.
📄 Full reviews: [2026-11-27-queued-heartbeat-names-ceiling.reviews.md](2026-11-27-queued-heartbeat-names-ceiling.reviews.md)

## Picture

```
 before: queued 1m00s for a host slot (greengate.hostSlots 2)
 after : queued 1m00s for a host slot (greengate.hostSlots 2),
         defers to CI in at most 59m00s (greengate.timeout);
         skip the queue: rerun with GREENGATE_ON_BUSY=now
```

The picture shows the one line that changes: the queued heartbeat.

## Goal

A hook check can wait up to `greengate.timeout` (3600 s) in a host-slot line,
and its heartbeat says only `queued 1m00s for a host slot (greengate.hostSlots 2)`.
The person cannot tell when the wait ends nor how to skip it. The heartbeat
should name both, and the deferral roll-up label should carry the resolved
length instead of the bare setting name (t20, carried from t18's tui review).

## Data: how long hook waits really are

Window: `/tmp/greenharness-slots-501/waits.jsonl` from 2026-11-26 16:24 CET
(the 5.1.1 install) to its last row, 2026-11-27 14:23 CET (about 22 h); hook
rows only (`tool = "greengate"`, `released` lines left out): 35 waits.
t19 (due 16:24) has not run; this is not its verdict.

| group | n | p50 | p95 | max | over 60 s | over 300 s |
| --- | --- | --- | --- | --- | --- | --- |
| all hook waits | 35 | 106 s | 896 s | 1104 s | 27 | 14 |
| admitted | 20 | 367 s | 965 s | 1104 s | 16 | 12 |
| deferred (all `end: ceiling`) | 11 | 60 s | 600 s | 600 s | 11 | 2 |
| handed (a free slot, no queue) | 4 | 0 s | 0 s | 0 s | 0 | 0 |

Admitted hook checks waited a median 6 minutes and up to 18 minutes in a
line that shows no end. So the message is needed.

## Non-goals

- The 80-column region row (`· queued 42s (slots 2/2)`): unchanged.
- The wait itself (ceilings, stall, `slotWait`, `warnSlotWait`), the journal.
- A per-check exact end: a `warn` check ends at `warnSlotWait` (600 s) and an
  `onBusy defer` check at `slotWait` (60 s), both earlier than the ceiling.
  The text says "at most", which is true for every one. Cost, carried: 11 of
  the 35 waits were bounded at 60 s, and their first heartbeat reads "at most
  59m00s" as they defer. Carrying each check's own bound into the live row
  means changing `host_slots`; left to a follow-up.

## Code review
- amont: backend, lang:rust, unix, tui (matches `greenbelay plan-panel --code-review`: a CLI repository, one Rust phase; no UI, platform or React profile applies)

## Behaviour

The heartbeat (a log line, after its unchanged prefix) becomes
`… queued 1m00s for a host slot (greengate.hostSlots 2), defers to CI in at most 59m00s (greengate.timeout); skip the queue: rerun with GREENGATE_ON_BUSY=now`.
The added text is plain, with no
`highlight()` and no escape bytes: the heartbeat is a log.

- "in at most" is `ceiling - queued secs` (never below 0), the ceiling being
  `greengate.timeout`, or 3600 when it is 0, resolved by one new
  `host_slots::wait_ceiling_secs(timeout)` that `host_slots.rs` (where it is
  inline now), `beat_line` and both `announce_deferred` call sites share.
  `Budgets.ceiling` keeps 0 = off (the 80% cliff note reads it).
- The skip names the variable only. `on_push` is false for a pre-push check
  in a land rehearsal (live.rs:985) and true under `greengate run --raw`, and
  stage names carry `pre-push-` either way, so no field names the command that
  was run; the variable is true on every path. Rerunning is the only way, as
  the choice is made up front (ADR-0022). The roll-up already prints the whole
  command for the deferred checks.
- The roll-up label reads `wait for a slot, up to 1h00m (greengate.timeout):`
  with the resolved ceiling, within 80 columns.

## Phases

- [ ] Phase 1 — heartbeat and roll-up: `crates/greengate-runtime/src/live.rs`
  (`beat_line` queued branch, test `a_queued_check_says_it_waits_for_a_host_slot`
  rewritten to the new exact line, extra cases with `Budgets { ceiling: …, ..B }`),
  `crates/greengate-runtime/src/host_slots.rs` (`wait_ceiling_secs`),
  `crates/greengate-runtime/src/dispatch.rs` (`announce_deferred` and
  `deferral_advice` take the ceiling; their tests),
  `crates/greengate/tests/it/busy_defers.rs` (label assertion); one commit.
No release is part of this plan: a message change only, shipped with the next greengate release.

## Decision log
- 2026-11-27 — the roll-up label is changed too, because the ticket's Look-at
  asks for it and the ceiling is already resolved at both call sites.

## Verification
- Phase 1: unit test on `beat_line` with ceiling 3600 and 60 s queued → the
  exact line above; with ceiling 0 → 3600 used; 3590 s queued → `10s`; ceiling
  spent → `0s`, never negative; the same text for `on_push` true and false; no `\x1b` in
  the line, and the line's prefix is unchanged → pass.
- Roll-up: `deferral_advice` tests assert the exact label
  `  wait for a slot, up to 1h00m (greengate.timeout):` (replacing the
  `contains("up to greengate.timeout")` at dispatch.rs:3063), one case with
  timeout 0 through `wait_ceiling_secs`; the `busy_defers` integration test
  sees it on its own line, under 80 columns.
- Live pilot: a hook check queued behind a held slot for over 60 s (as
  `busy_defers` does) → its stderr heartbeat shows the line → pasted in Outcome.
- fmt, `clippy -D warnings`, `cargo test -p greengate-runtime -p greengate`.

<!-- panel: repos=amont reviewers=backend,lang:rust,tui,unix body-sha=8ef6f3170ea2 -->
